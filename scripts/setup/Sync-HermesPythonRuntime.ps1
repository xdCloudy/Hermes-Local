[CmdletBinding()]
param(
    [switch] $Reinstall,
    [switch] $NonInteractive
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

Import-Module (Join-Path $PSScriptRoot '..\Common-Hermes.psm1') -Force
. (Join-Path $PSScriptRoot 'Python-RuntimeMigration.ps1')

try {
    Assert-HermesRoot
    Initialize-HermesLayout
    Set-HermesProcessEnvironment

    $root = Get-HermesRoot
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

    $runtimePython = Join-Path $runtime 'Scripts\python.exe'
    $runtimeVersion = Get-HermesInstalledPythonMinorVersion -PythonExecutable $runtimePython
    $targetMinor = Get-HermesTargetPythonMinorVersion -ManifestPath $manifestPath
    if (
        (Test-Path -LiteralPath $runtime -PathType Container) -and
        $runtimeVersion -ne $targetMinor
    ) {
        $null = Invoke-HermesPythonRuntimeMigration `
            -Runtime $runtime `
            -ManifestPath $manifestPath
    }

    if (-not (Test-Path -LiteralPath $runtimePython -PathType Leaf)) {
        Invoke-HermesProcess -FilePath 'uv.exe' -ArgumentList @(
            'venv', $runtime,
            '--python', $managedPython,
            '--managed-python',
            '--seed'
        ) -Environment $uvEnvironment -LogComponent setup
    }

    $createdVersion = (@(
        & $runtimePython -c 'import sys; print(".".join(map(str, sys.version_info[:3])))'
    ) -join [Environment]::NewLine).Trim()
    if ($LASTEXITCODE -ne 0 -or $createdVersion -ne $targetVersion) {
        throw "Hermes runtime uses Python '$createdVersion'; expected '$targetVersion'."
    }

    $syncArguments = @(
        'sync',
        '--project', $source,
        '--locked',
        '--active',
        '--python', $managedPython,
        '--managed-python',
        '--extra', 'all',
        '--extra', 'dev',
        '--extra', 'voice',
        '--extra', 'edge-tts',
        '--extra', 'messaging'
    )
    if ($Reinstall) {
        $syncArguments += '--reinstall'
    }

    Invoke-HermesProcess -FilePath 'uv.exe' -ArgumentList $syncArguments -Environment @{
        VIRTUAL_ENV = $runtime
        UV_PROJECT_ENVIRONMENT = $runtime
        UV_PYTHON_INSTALL_DIR = $managedRoot
        UV_PYTHON = $managedPython
        UV_CACHE_DIR = $uvCache
    } -LogComponent setup

    $verification = (@(
        Invoke-HermesProcess -FilePath $runtimePython -ArgumentList @(
            '-c',
            'import sys, yaml, gateway; from gateway.config import Platform, load_gateway_config; print(sys.executable); print(sys.version); print(yaml.__file__); print(next(iter(gateway.__path__)))'
        ) -LogComponent setup -PassThruOutput
    ) -join [Environment]::NewLine).Trim()

    Write-HermesLog -Component setup -Message (
        "Hermes Python runtime synchronized with Python $targetVersion. $verification"
    )
    Write-Host "Hermes Python runtime synchronized with Python $targetVersion."
    exit 0
} catch {
    try {
        Write-HermesLog -Component setup -Level ERROR -Message $_.Exception.ToString()
    } catch {
    }
    Write-Host "Hermes Python runtime synchronization failed: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
}
