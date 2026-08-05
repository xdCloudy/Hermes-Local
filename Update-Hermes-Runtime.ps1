[CmdletBinding()]
param(
    [switch] $Force,
    [switch] $NonInteractive
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'scripts\Common-Hermes.psm1') -Force
Import-Module (Join-Path $PSScriptRoot 'scripts\Hermes-Configuration.psm1') -Force
Import-Module (Join-Path $PSScriptRoot 'scripts\Hermes-RuntimeManager.psm1') -Force

try {
    Assert-HermesRoot
    Initialize-HermesLayout
    Set-HermesProcessEnvironment
    $configuration = Get-HermesConfiguration
    $requested = Get-HermesRequestedAcceleration -Configuration $configuration
    $hardware = Assert-HermesMachine -Acceleration $(if ($requested -eq 'cuda') { 'cuda' } else { 'auto' })
    $decision = Resolve-HermesLlamaRuntimePackage -Configuration $configuration -Hardware $hardware
    Write-Host "Selection: $($decision.SelectionState)"
    Write-Host "Reason: $($decision.Reason)"
    if (-not $decision.Package) {
        throw 'No compatible verified prebuilt runtime is available. Use Setup-Hermes-Local.ps1 -LlamaRuntimeMode source for a custom build.'
    }
    $manifest = Install-HermesLlamaRuntime -Decision $decision -Force:$Force
    Write-Host "Active runtime: $($manifest.packageId) ($($manifest.acceleration), source $($manifest.sourceCommit))."
    exit 0
} catch {
    Write-HermesLog -Component setup -Level ERROR -Message $_.Exception.ToString()
    Write-Host "Hermes runtime update failed: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
}
