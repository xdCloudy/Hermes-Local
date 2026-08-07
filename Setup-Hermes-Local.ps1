[CmdletBinding()]
param(
    [switch] $SkipModel,
    [switch] $SkipLlamaBuild,
    [switch] $SkipHermesDependencies,
    [switch] $SkipLauncherBuild,
    [switch] $ReinstallDependencies,
    [ValidateSet('prebuilt', 'source')]
    [string] $LlamaRuntimeMode = 'prebuilt',
    [switch] $NonInteractive
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'scripts\Common-Hermes.psm1') -Force
. (Join-Path $PSScriptRoot 'scripts\setup\Python-RuntimeMigration.ps1')

try {
    Assert-HermesRoot
    Initialize-HermesLayout
    Set-HermesProcessEnvironment

    if (-not $SkipHermesDependencies) {
        $manifestPath = Resolve-HermesPath 'VERSION.json'
        $null = Sync-HermesConfiguredPythonVersion `
            -ManifestPath $manifestPath `
            -UserSettingsPath (Resolve-HermesPath 'config\launcher\user-settings.json')
        $null = Invoke-HermesPythonRuntimeMigration `
            -Runtime (Resolve-HermesPath 'runtimes\python\hermes') `
            -ManifestPath $manifestPath
    }

    $implementationName = if ($LlamaRuntimeMode -eq 'source') {
        'Setup-Hermes-Local.Impl.ps1'
    } else {
        'Setup-Hermes-Local.Prebuilt.ps1'
    }
    $implementation = Join-Path $PSScriptRoot $implementationName
    if (-not (Test-Path -LiteralPath $implementation -PathType Leaf)) {
        throw "Setup implementation is missing: $implementation"
    }

    $forwardedParameters = @{}
    foreach ($entry in $PSBoundParameters.GetEnumerator()) {
        if ($entry.Key -ne 'LlamaRuntimeMode') {
            $forwardedParameters[$entry.Key] = $entry.Value
        }
    }
    Write-HermesLog -Component setup -Message "Selected llama.cpp runtime mode: $LlamaRuntimeMode."

    $desktopSourceSynchronization = (
        $SkipModel -and
        $SkipLlamaBuild -and
        $SkipLauncherBuild -and
        $NonInteractive
    )

    if (-not $desktopSourceSynchronization) {
        & $implementation @forwardedParameters
        exit $LASTEXITCODE
    }

    $diagnosticScript = Join-Path $PSScriptRoot 'Test-Hermes-Local.ps1'
    $deferredDiagnosticScript = Resolve-HermesPath (
        "temp\Test-Hermes-Local.desktop-update-$([guid]::NewGuid().ToString('N')).ps1"
    )
    $diagnosticDeferred = $false
    $implementationExitCode = 1

    try {
        if (Test-Path -LiteralPath $diagnosticScript -PathType Leaf) {
            Move-Item -LiteralPath $diagnosticScript -Destination $deferredDiagnosticScript -Force
            $diagnosticDeferred = $true
            Write-HermesLog -Component setup -Message 'Deferred bootstrap diagnostics during Desktop updater source synchronisation.'
        }

        $hostExecutable = (Get-Process -Id $PID -ErrorAction Stop).Path
        if ([string]::IsNullOrWhiteSpace($hostExecutable)) {
            throw 'Unable to resolve the current PowerShell host executable.'
        }

        $implementationArguments = [System.Collections.Generic.List[string]]::new()
        foreach ($argument in @(
            '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
            '-File', $implementation
        )) {
            $implementationArguments.Add([string]$argument)
        }
        foreach ($entry in $forwardedParameters.GetEnumerator()) {
            if ([bool]$entry.Value) {
                $implementationArguments.Add("-$($entry.Key)")
            }
        }

        & $hostExecutable @($implementationArguments.ToArray())
        $implementationExitCode = $LASTEXITCODE
    } finally {
        if ($diagnosticDeferred) {
            Move-Item -LiteralPath $deferredDiagnosticScript -Destination $diagnosticScript -Force
            Write-HermesLog -Component setup -Message 'Restored bootstrap diagnostics after Desktop updater source synchronisation.'
        }
    }

    exit $implementationExitCode
} catch {
    try {
        Write-HermesLog -Component setup -Level ERROR -Message $_.Exception.ToString()
    } catch {
    }
    Write-Host "Hermes Local setup failed: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
}
