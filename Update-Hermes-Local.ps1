[CmdletBinding(SupportsShouldProcess, ConfirmImpact = 'High')]
param(
    [ValidateSet('Check', 'Compatibility', 'Apply', 'Rollback')]
    [string] $Mode = 'Check',

    [ValidateSet(
        'All', 'HermesAgent', 'Launcher', 'LlamaCpp', 'Model',
        'PythonLock', 'NodeLock', 'BrowserBinaries', 'OptionalTools'
    )]
    [string] $Component = 'All',

    [ValidateSet('Cli', 'Desktop', 'Installer', 'Recovery')]
    [string] $Caller = 'Cli',

    [ValidatePattern('^[0-9a-fA-F]{40}$')]
    [string] $TargetCommit,

    [ValidatePattern('^[A-Za-z0-9._/-]+$')]
    [string] $TargetBranch,

    [switch] $NonInteractive
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

Import-Module (Join-Path $PSScriptRoot 'scripts\Common-Hermes.psm1') -Force
Import-Module (Join-Path $PSScriptRoot 'scripts\Hermes-UpdateOrchestrator.psm1') -Force

try {
    Assert-HermesRoot
    Initialize-HermesLayout
    Set-HermesProcessEnvironment

    if ($Mode -in @('Apply', 'Rollback') -and -not $NonInteractive) {
        if (-not $PSCmdlet.ShouldProcess("Hermes Local $Component", $Mode)) {
            Write-Host "$Mode cancelled."
            exit 2
        }
    }

    $inputRecord = @{}
    if ($TargetCommit) {
        $inputRecord.TargetCommit = $TargetCommit.ToLowerInvariant()
    }
    if ($TargetBranch) {
        $inputRecord.TargetBranch = $TargetBranch
    }
    $desktopTaskId = [Environment]::GetEnvironmentVariable('HERMES_LOCAL_TASK_ID')
    if ($desktopTaskId) {
        if ($desktopTaskId -notmatch '^[0-9a-fA-F-]{16,64}$') {
            throw 'HERMES_LOCAL_TASK_ID contains an invalid task identity.'
        }
        $inputRecord.TaskId = $desktopTaskId
    }

    $result = Invoke-HermesUpdateOperation `
        -Mode $Mode `
        -Component $Component `
        -Caller $Caller `
        -Input $inputRecord

    $result | ConvertTo-Json -Depth 64

    if ($result.status -eq 'succeeded') {
        Write-Host "Update operation $($result.operationId) completed. State: $($result.statePath)"
        exit 0
    }

    if ($result.status -eq 'rolled-back') {
        Write-Host "Update operation $($result.operationId) failed and was rolled back. State: $($result.statePath)" -ForegroundColor Yellow
        exit 1
    }

    Write-Host "Update operation $($result.operationId) failed. State: $($result.statePath)" -ForegroundColor Red
    exit 1
} catch {
    try {
        Write-HermesLog -Component update -Level ERROR -Message $_.Exception.ToString()
    } catch {
        Write-Warning "Could not write the update failure log: $($_.Exception.Message)"
    }
    Write-Host "Hermes Local update $Mode failed: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
}
