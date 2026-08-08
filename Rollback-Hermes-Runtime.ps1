[CmdletBinding()]
param([switch] $NonInteractive)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'scripts\Common-Hermes.psm1') -Force
Import-Module (Join-Path $PSScriptRoot 'scripts\Hermes-UpdateOrchestrator.psm1') -Force
Import-Module (Join-Path $PSScriptRoot 'scripts\Hermes-RuntimeUpdateAdapter.psm1') -Force

try {
    Assert-HermesRoot
    Initialize-HermesLayout
    Set-HermesProcessEnvironment

    $result = Invoke-HermesUpdateOperation `
        -Mode Rollback `
        -Component LlamaCpp `
        -Caller Recovery `
        -Input @{}
    if ($result.status -ne 'succeeded') {
        throw "Hermes runtime rollback failed. State: $($result.statePath)"
    }
    $identity = $result.stageResults.validate.identity
    Write-Host "Restored runtime: $($identity.key). Integrity state: verified."
    exit 0
} catch {
    Write-HermesLog -Component setup -Level ERROR -Message $_.Exception.ToString()
    Write-Host "Hermes runtime rollback failed: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
}
