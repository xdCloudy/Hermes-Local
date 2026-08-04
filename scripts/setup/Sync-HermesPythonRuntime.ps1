[CmdletBinding()]
param(
    [switch] $Reinstall,
    [switch] $NonInteractive
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

Import-Module (Join-Path $PSScriptRoot '..\Common-Hermes.psm1') -Force
. (Join-Path $PSScriptRoot 'Python-RuntimeMigration.ps1')

$candidateRuntime = $null
$rollbackRuntime = $null
$runtimeActivated = $false

function Get-HermesRuntimeSyncArguments {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $Source,
        [Parameter(Mandatory)][string] $ManagedPython,
        [switch] $ForceReinstall
    )

    $arguments = @(
        'sync',
        '--project', $Source,
        '--locked',
        '--active',
        '--python', $ManagedPython,
        '--managed-python',
        '--extra', 'all',
        '--extra', 'dev',
        '--extra', 'voice',
        '--extra', 'edge-tts',
        '--extra', 'messaging'
    )
    if ($ForceReinstall) {
        $arguments += '--reinstall'
    }

    $arguments
}

function Invoke-HermesRuntimeDependencySync {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $Runtime,
        [Parameter(Mandatory)][string] $Source,
        [Parameter(Mandatory)][string] $ManagedPython,
        [Parameter(Mandatory)][string] $ManagedRoot,
        [Parameter(Mandatory)][string] $UvCache,
        [switch] $ForceReinstall
    )

    Invoke-HermesProcess -FilePath 'uv.exe' -ArgumentList (
        Get-HermesRuntimeSyncArguments `
            -Source $Source `
            -ManagedPython $ManagedPython `
            -ForceReinstall:$ForceReinstall
    ) -Environment @{
        VIRTUAL_ENV = $Runtime
        UV_PROJECT_ENVIRONMENT = $Runtime
        UV_PYTHON_INSTALL_DIR = $ManagedRoot
        UV_PYTHON = $ManagedPython
        UV_CACHE_DIR = $UvCache
    } -LogComponent setup
}

try {
    Assert-HermesRoot
    Initialize-HermesLayout
    Set-HermesProcessEnvironment

    $manifestPath = Resolve-HermesPath 'VERSION.json'
    $manifest = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json -Depth 32
    $targetVersion = [string]$manifest.runtime.python
    if ($targetVersion -notmatch '^\d+\.\d+\.\d+$') {
        throw "VERSION.json runtime.python must be an exact Python version: '$targetVersion'."
    }

    $source = Resolve-HermesPath 'source\hermes-agent'
    foreach ($required in @(
        (Join-Path $source 'pyproject.toml'),
        (Join-Path $source 'uv.lock'),
        (Join-Path $source 'gateway\config.py')
    )) {
        if (-not (Test-Path -LiteralPath $required -PathType Leaf)) {
            throw "Required Hermes Agent source file is missing: $required"
        }
    }

    $runtime = Resolve-HermesPath 'runtimes\python\hermes'
    $runtimeParent = [IO.Path]::GetDirectoryName($runtime)
    $candidateRuntime = Join-Path $runtimeParent (
        'hermes-next-' + [guid]::NewGuid().ToString('N')
    )
    $managedRoot = Resolve-HermesPath 'runtimes\python\managed'
    $uvCache = Resolve-HermesPath 'cache\uv'
    [IO.Directory]::CreateDirectory($managedRoot) | Out-Null
    [IO.Directory]::CreateDirectory($uvCache) | Out-Null

    $uvEnvironment = @{
        UV_PYTHON_INSTALL_DIR = $managedRoot
        UV_CACHE_DIR = $uvCache
    }

    Invoke-HermesProcess -FilePath 'uv.exe' -ArgumentList @(
        'python', 'install', $targetVersion,
        '--install-dir', $managedRoot,
        '--no-registry',
        '--compile-bytecode'
    ) -Environment $uvEnvironment -LogComponent setup

    $managedPython = (@(
        Invoke-HermesProcess -FilePath 'uv.exe' -ArgumentList @(
            'python', 'find', '--managed-python', $targetVersion
        ) -Environment $uvEnvironment -LogComponent setup -PassThruOutput
    ) -join [Environment]::NewLine).Trim()

    if (
        [string]::IsNullOrWhiteSpace($managedPython) -or
        -not (Test-Path -LiteralPath $managedPython -PathType Leaf)
    ) {
        throw "uv could not resolve project-managed Python $targetVersion under $managedRoot."
    }

    Invoke-HermesProcess -FilePath 'uv.exe' -ArgumentList @(
        'venv', $candidateRuntime,
        '--python', $managedPython,
        '--managed-python',
        '--seed'
    ) -Environment $uvEnvironment -LogComponent setup

    $candidatePython = Join-Path $candidateRuntime 'Scripts\python.exe'
    $createdVersion = (@(
        & $candidatePython -c 'import sys; print(".".join(map(str, sys.version_info[:3])))'
    ) -join [Environment]::NewLine).Trim()
    if ($LASTEXITCODE -ne 0 -or $createdVersion -ne $targetVersion) {
        throw "Candidate Hermes runtime uses Python '$createdVersion'; expected '$targetVersion'."
    }

    Invoke-HermesRuntimeDependencySync `
        -Runtime $candidateRuntime `
        -Source $source `
        -ManagedPython $managedPython `
        -ManagedRoot $managedRoot `
        -UvCache $uvCache `
        -ForceReinstall:$Reinstall

    $candidateVerification = (@(
        Invoke-HermesProcess -FilePath $candidatePython -ArgumentList @(
            '-c',
            'import sys, yaml, gateway; from gateway.config import Platform, load_gateway_config; print(sys.executable); print(sys.version); print(yaml.__file__); print(next(iter(gateway.__path__)))'
        ) -LogComponent setup -PassThruOutput
    ) -join [Environment]::NewLine).Trim()

    if (Test-Path -LiteralPath $runtime -PathType Container) {
        Assert-HermesPythonRuntimeInactive -Runtime $runtime
        $activeVersion = Get-HermesInstalledPythonMinorVersion `
            -PythonExecutable (Join-Path $runtime 'Scripts\python.exe')
        $rollbackRuntime = New-HermesPythonRollbackPath `
            -Runtime $runtime `
            -RuntimeVersion $activeVersion
        [IO.Directory]::Move($runtime, $rollbackRuntime)
    }

    try {
        [IO.Directory]::Move($candidateRuntime, $runtime)
        $candidateRuntime = $null
        $runtimeActivated = $true
    } catch {
        if (
            $rollbackRuntime -and
            (Test-Path -LiteralPath $rollbackRuntime -PathType Container) -and
            -not (Test-Path -LiteralPath $runtime)
        ) {
            [IO.Directory]::Move($rollbackRuntime, $runtime)
            $rollbackRuntime = $null
        }
        throw
    }

    # uv's Windows console-script trampolines retain the absolute environment
    # path used when they are generated. The candidate runtime is deliberately
    # built in a sibling directory and atomically renamed, so regenerate every
    # installed entry point from the final active path before validating it.
    Invoke-HermesRuntimeDependencySync `
        -Runtime $runtime `
        -Source $source `
        -ManagedPython $managedPython `
        -ManagedRoot $managedRoot `
        -UvCache $uvCache `
        -ForceReinstall

    $runtimePython = Join-Path $runtime 'Scripts\python.exe'
    $activeVerification = (@(
        Invoke-HermesProcess -FilePath $runtimePython -ArgumentList @(
            '-c',
            'import sys, yaml, gateway; from gateway.config import Platform, load_gateway_config; print(sys.executable); print(sys.version); print(yaml.__file__); print(next(iter(gateway.__path__)))'
        ) -LogComponent setup -PassThruOutput
    ) -join [Environment]::NewLine).Trim()

    $runtimeHermes = Join-Path $runtime 'Scripts\hermes.exe'
    if (-not (Test-Path -LiteralPath $runtimeHermes -PathType Leaf)) {
        throw "Hermes runtime entry point is missing after activation: $runtimeHermes"
    }
    $entryPointVerification = (@(
        Invoke-HermesProcess -FilePath $runtimeHermes -ArgumentList @(
            '--help'
        ) -LogComponent setup -PassThruOutput
    ) -join [Environment]::NewLine).Trim()

    Write-HermesLog -Component setup -Message (
        "Hermes Python runtime synchronized with Python $targetVersion. " +
        "Candidate verification: $candidateVerification " +
        "Active verification: $activeVerification " +
        "Entry-point verification: $entryPointVerification"
    )
    Write-Host "Hermes Python runtime synchronized with Python $targetVersion."
    if ($rollbackRuntime) {
        Write-Host "Previous runtime preserved at: $rollbackRuntime"
    }
    exit 0
} catch {
    try {
        $runtime = Resolve-HermesPath 'runtimes\python\hermes'
        if (
            $runtimeActivated -and
            $rollbackRuntime -and
            (Test-Path -LiteralPath $rollbackRuntime -PathType Container)
        ) {
            if (Test-Path -LiteralPath $runtime -PathType Container) {
                $failedRuntime = Join-Path ([IO.Path]::GetDirectoryName($runtime)) (
                    'hermes-failed-' + [guid]::NewGuid().ToString('N')
                )
                [IO.Directory]::Move($runtime, $failedRuntime)
            }
            [IO.Directory]::Move($rollbackRuntime, $runtime)
            $rollbackRuntime = $null
            $runtimeActivated = $false
        } elseif (
            -not $runtimeActivated -and
            $rollbackRuntime -and
            (Test-Path -LiteralPath $rollbackRuntime -PathType Container) -and
            -not (Test-Path -LiteralPath $runtime)
        ) {
            [IO.Directory]::Move($rollbackRuntime, $runtime)
            $rollbackRuntime = $null
        } elseif (
            $runtimeActivated -and
            -not $rollbackRuntime -and
            (Test-Path -LiteralPath $runtime -PathType Container)
        ) {
            Remove-Item -LiteralPath $runtime -Recurse -Force
            $runtimeActivated = $false
        }
    } catch {
    }

    try {
        Write-HermesLog -Component setup -Level ERROR -Message $_.Exception.ToString()
    } catch {
    }
    Write-Host "Hermes Python runtime synchronization failed: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
} finally {
    if ($candidateRuntime -and (Test-Path -LiteralPath $candidateRuntime)) {
        Remove-Item -LiteralPath $candidateRuntime -Recurse -Force -ErrorAction SilentlyContinue
    }
}
