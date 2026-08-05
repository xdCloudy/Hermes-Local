[CmdletBinding()]
param([switch] $NonInteractive)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'scripts\Common-Hermes.psm1') -Force
Import-Module (Join-Path $PSScriptRoot 'scripts\Hermes-Configuration.psm1') -Force
Import-Module (Join-Path $PSScriptRoot 'scripts\Hermes-RuntimeManager.psm1') -Force

try {
    Assert-HermesRoot
    Initialize-HermesLayout
    $state = Restore-HermesLlamaRuntime
    Write-Host "Restored runtime: $($state.packageId). Integrity state: $($state.integrityState)."
    exit 0
} catch {
    Write-HermesLog -Component setup -Level ERROR -Message $_.Exception.ToString()
    Write-Host "Hermes runtime rollback failed: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
}
