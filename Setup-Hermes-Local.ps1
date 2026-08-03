[CmdletBinding()]
param(
    [switch] $SkipModel,
    [switch] $SkipLlamaBuild,
    [switch] $SkipHermesDependencies,
    [switch] $SkipLauncherBuild,
    [switch] $ReinstallDependencies,
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
        $null = Invoke-HermesPythonRuntimeMigration `
            -Runtime (Resolve-HermesPath 'runtimes\python\hermes') `
            -ManifestPath (Resolve-HermesPath 'VERSION.json')
    }

    $implementation = Join-Path $PSScriptRoot 'Setup-Hermes-Local.Impl.ps1'
    if (-not (Test-Path -LiteralPath $implementation -PathType Leaf)) {
        throw "Setup implementation is missing: $implementation"
    }

    $forwardedParameters = @{}
    foreach ($entry in $PSBoundParameters.GetEnumerator()) {
        $forwardedParameters[$entry.Key] = $entry.Value
    }

    & $implementation @forwardedParameters
    exit $LASTEXITCODE
} catch {
    try {
        Write-HermesLog -Component setup -Level ERROR -Message $_.Exception.ToString()
    } catch {
    }
    Write-Host "Hermes Local setup failed: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
}
