[CmdletBinding()]
param(
    [switch] $Force,
    [switch] $NonInteractive
)

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
    if ($Force) {
        $options.Force = $true
    }
    $desktopTaskId = [Environment]::GetEnvironmentVariable('HERMES_LOCAL_TASK_ID')
    if ($desktopTaskId) {
        if ($desktopTaskId -notmatch '^[0-9a-fA-F-]{16,64}$') {
            throw 'HERMES_LOCAL_TASK_ID contains an invalid task identity.'
        }
        $options.TaskId = $desktopTaskId
    }
    $caller = if ($desktopTaskId) { 'Desktop' } else { 'Cli' }

    $result = Invoke-HermesUpdateOperation `
        -Mode Apply `
        -Component LlamaCpp `
        -Caller $caller `
        -Input $options

    if ($result.status -eq 'succeeded') {
        $identity = $result.stageResults.validate.identity
        Write-Host "Active runtime: $($identity.key) [$($identity.fingerprint)]."
        exit 0
    }
    if ($result.status -eq 'rolled-back') {
        Write-Host "Hermes runtime update failed and the previous runtime was restored. State: $($result.statePath)" -ForegroundColor Yellow
        exit 1
    }
    throw "Hermes runtime update failed. State: $($result.statePath)"
} catch {
    Write-HermesLog -Component setup -Level ERROR -Message $_.Exception.ToString()
    Write-Host "Hermes runtime update failed: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
}
