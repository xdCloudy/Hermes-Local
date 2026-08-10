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

    $options = @{}
    $desktopTaskId = [Environment]::GetEnvironmentVariable('HERMES_LOCAL_TASK_ID')
    if ($desktopTaskId) {
        if ($desktopTaskId -notmatch '^[0-9a-fA-F-]{16,64}$') {
            throw 'HERMES_LOCAL_TASK_ID contains an invalid task identity.'
        }
        $options.TaskId = $desktopTaskId
    }
    $caller = if ($desktopTaskId) { 'Desktop' } else { 'Recovery' }

    $result = Invoke-HermesUpdateOperation `
        -Mode Rollback `
        -Component LlamaCpp `
        -Caller $caller `
        -Input $options
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
