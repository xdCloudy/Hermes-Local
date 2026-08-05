[CmdletBinding(SupportsShouldProcess, ConfirmImpact = 'High')]
param(
    [Parameter(Mandatory)]
    [string] $BackupPath,

    [bool] $VerifyIntegrity = $true,

    [switch] $NonInteractive
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'scripts\Common-Hermes.psm1') -Force
Import-Module (Join-Path $PSScriptRoot 'scripts\Hermes-Configuration.psm1') -Force
. (Join-Path $PSScriptRoot 'scripts\restore\Restore-Common.ps1')
. (Join-Path $PSScriptRoot 'scripts\restore\Restore-Reliability.ps1')

$context = $null
$stagingRoot = $null
$journal = @()
$servicesStopped = $false
$exitCode = 1

try {
    Assert-HermesRoot
    Initialize-HermesLayout

    $root = Get-HermesRoot
    $resolvedBackup = Resolve-HermesRestoreBackupPath -Root $root -BackupPath $BackupPath
    $taskId = Get-HermesRestoreTaskId -RequestedTaskId $env:HERMES_LOCAL_TASK_ID
    $context = New-HermesRestoreContext `
        -Root $root `
        -TaskId $taskId `
        -BackupPath $resolvedBackup `
        -VerifyIntegrity $VerifyIntegrity

    Remove-HermesRestoreCancellationRequest -Context $context
    $context.PreviousState = Get-HermesRestorePreviousState -Root $root

    $null = Write-HermesRestoreProgress `
        -Context $context `
        -Stage 'validation' `
        -Message 'Validating the selected backup identity and restore policy.' `
        -Status 'running' `
        -Cancellable $true `
        -Indeterminate

    $plan = Get-HermesRestoreArchivePlan `
        -Root $root `
        -BackupPath $resolvedBackup `
        -VerifyIntegrity $VerifyIntegrity
    $context.Backup = $plan

    $null = Write-HermesRestoreProgress `
        -Context $context `
        -Stage 'archive-inspection' `
        -Message "Inspected $($plan.EntryCount) archive entries and validated the backup manifest." `
        -Status 'running' `
        -Cancellable $true `
        -CompletedUnits $plan.EntryCount `
        -TotalUnits $plan.EntryCount

    if (-not $NonInteractive) {
        $target = "Hermes Local configuration and user data from $($plan.RelativePath)"
        if (-not $PSCmdlet.ShouldProcess($target, 'Create a safety snapshot and perform a transactional restore')) {
            throw [OperationCanceledException]::new('Restore confirmation was declined.')
        }
    }

    Assert-HermesRestoreNotCancelled -Context $context

    $null = Write-HermesRestoreProgress `
        -Context $context `
        -Stage 'safety-snapshot' `
        -Message 'Creating a recoverable snapshot of the active installation before replacement.' `
        -Status 'running' `
        -Cancellable $true `
        -Indeterminate
    $context.SafetySnapshot = New-HermesRestoreSafetySnapshot -Context $context

    Assert-HermesRestoreNotCancelled -Context $context

    # From this boundary onward cancellation is deliberately disabled. The
    # transaction either validates the restored state or rolls the original
    # installation back before returning a terminal result.
    $context.Cancellable = $false
    $null = Write-HermesRestoreProgress `
        -Context $context `
        -Stage 'service-shutdown' `
        -Message 'Stopping owned Hermes Local services before the atomic data transaction.' `
        -Status 'running' `
        -Cancellable $false `
        -Indeterminate

    $null = Invoke-HermesRestoreProcess `
        -Context $context `
        -ScriptPath (Join-Path $root 'Stop-Hermes-Local.ps1') `
        -Arguments @('-NonInteractive') `
        -Description 'Hermes Local service shutdown'
    $servicesStopped = $true

    $stagingRoot = Join-Path $root "temp\restore-$taskId"
    $context.RollbackRoot = Join-Path $root "temp\restore-rollback-$taskId"
    $context.FailedStateRoot = Join-Path $root "temp\restore-failed-$taskId"

    $null = Write-HermesRestoreProgress `
        -Context $context `
        -Stage 'extraction' `
        -Message 'Extracting the validated archive into isolated restore staging.' `
        -Status 'running' `
        -Cancellable $false `
        -Indeterminate
    Expand-HermesRestoreArchive -Context $context -Plan $plan -Destination $stagingRoot

    $context.PromotionAttempted = $true
    $journal = @(
        Invoke-HermesRestorePromotion `
            -Context $context `
            -StagingRoot $stagingRoot `
            -RollbackRoot $context.RollbackRoot
    )
    $context.RestorePromoted = $true
    $context.ActiveState = 'restored-stopped'

    $null = Write-HermesRestoreProgress `
        -Context $context `
        -Stage 'configuration-migration' `
        -Message 'Loading the restored configuration through the current Hermes Local schema.' `
        -Status 'running' `
        -Cancellable $false `
        -Indeterminate
    $restoredProfile = Test-HermesRestorePromotedState -Context $context

    $null = Write-HermesRestoreProgress `
        -Context $context `
        -Stage 'validation-after-restore' `
        -Message 'Validated restored paths, configuration and declared data scope.' `
        -Status 'running' `
        -Cancellable $false `
        -CompletedUnits $script:HermesRestoreScopes.Count `
        -TotalUnits $script:HermesRestoreScopes.Count

    if ([bool]$context.PreviousState.WasRunning) {
        if ([string]::IsNullOrWhiteSpace($restoredProfile)) {
            $restoredProfile = [string]$plan.Profile
        }
        if ([string]::IsNullOrWhiteSpace($restoredProfile)) {
            throw 'The restored installation cannot identify a profile for service restart.'
        }

        $null = Write-HermesRestoreProgress `
            -Context $context `
            -Stage 'service-restart' `
            -Message "Restarting the restored installation with profile $restoredProfile." `
            -Status 'running' `
            -Cancellable $false `
            -Indeterminate
        $null = Invoke-HermesRestoreProcess `
            -Context $context `
            -ScriptPath (Join-Path $root 'Start-Hermes-Local.ps1') `
            -Arguments @('-Profile', $restoredProfile, '-NonInteractive') `
            -Description 'Restored Hermes Local service restart'

        $runtimePath = Join-Path $root 'data\runtime\status.json'
        $runtime = if (Test-Path -LiteralPath $runtimePath -PathType Leaf) {
            Get-Content -Raw -LiteralPath $runtimePath | ConvertFrom-Json -Depth 64
        } else {
            $null
        }
        $phase = [string](Get-HermesRestoreValue -Record $runtime -Name phase -Default '')
        $controllerPid = [int](Get-HermesRestoreValue -Record $runtime -Name controllerPid -Default 0)
        if ($phase -ne 'running' -or $controllerPid -le 0 -or -not (Get-Process -Id $controllerPid -ErrorAction SilentlyContinue)) {
            throw 'The restored installation did not return to a validated running state.'
        }
        $context.ActiveState = 'restored-running'
    }

    Complete-HermesRestore `
        -Context $context `
        -Status 'succeeded' `
        -Message "Hermes Local was restored from backup $($plan.Id)."

    if (Test-Path -LiteralPath $context.RollbackRoot) {
        Remove-Item -LiteralPath $context.RollbackRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
    Write-Host "Hermes Local restored from backup $($plan.Id)."
    Write-Host "Restore report: $($context.ReportPath)"
    $exitCode = 0
} catch [OperationCanceledException] {
    if ($context) {
        $context.ActiveState = 'original-active'
        Complete-HermesRestore `
            -Context $context `
            -Status 'cancelled' `
            -Message 'Restore cancelled safely before installation replacement.'
        Write-Host 'Hermes Local restore cancelled safely.' -ForegroundColor Yellow
    }
    $exitCode = 130
} catch {
    $failure = $_
    $failureMessage = Protect-HermesRestoreText -Text $failure.Exception.Message -Root $(if ($context) { $context.Root } else { $PSScriptRoot })

    if ($context) {
        Add-HermesRestoreLog -Context $context -Level ERROR -Message $failure.Exception.ToString()
        if ($journal.Count -eq 0 -and $context.PromotionJournal.Count -gt 0) {
            $journal = @($context.PromotionJournal)
        }

        if ($journal.Count -gt 0) {
            $context.RollbackAttempted = $true
            $null = Write-HermesRestoreProgress `
                -Context $context `
                -Stage 'rollback' `
                -Message 'Restore validation failed; restoring the previous installation transaction.' `
                -Status 'running' `
                -Cancellable $false `
                -Indeterminate

            try {
                $null = Invoke-HermesRestoreProcess `
                    -Context $context `
                    -ScriptPath (Join-Path $context.Root 'Stop-Hermes-Local.ps1') `
                    -Arguments @('-NonInteractive') `
                    -Description 'Rollback service shutdown'
            } catch {
                Add-HermesRestoreLog -Context $context -Level WARN -Message "Rollback continued after service-stop warning: $($_.Exception.Message)"
            }

            $rollback = Invoke-HermesRestoreRollback `
                -Context $context `
                -Journal $journal `
                -FailedStateRoot $context.FailedStateRoot
            $context.RollbackSucceeded = [bool]$rollback.Succeeded

            if ($rollback.Succeeded) {
                $context.RestorePromoted = $false
                $context.ActiveState = 'rollback-restored-original'
                if ([bool]$context.PreviousState.WasRunning -and -not [string]::IsNullOrWhiteSpace([string]$context.PreviousState.Profile)) {
                    try {
                        $null = Invoke-HermesRestoreProcess `
                            -Context $context `
                            -ScriptPath (Join-Path $context.Root 'Start-Hermes-Local.ps1') `
                            -Arguments @('-Profile', [string]$context.PreviousState.Profile, '-NonInteractive') `
                            -Description 'Original Hermes Local service restart after rollback'
                        $context.ActiveState = 'rollback-restored-original-running'
                    } catch {
                        $context.ActiveState = 'rollback-restored-original-stopped'
                        Add-HermesRestoreLog -Context $context -Level ERROR -Message "Original state was restored but restart failed: $($_.Exception.Message)"
                    }
                }
            } else {
                $context.ActiveState = 'partial-restore-preserved-for-recovery'
                Add-HermesRestoreLog -Context $context -Level ERROR -Message (
                    'Rollback did not fully complete: ' + ($rollback.Errors -join '; ')
                )
            }
        } elseif ($servicesStopped -and [bool]$context.PreviousState.WasRunning -and
            -not [string]::IsNullOrWhiteSpace([string]$context.PreviousState.Profile)) {
            try {
                $null = Invoke-HermesRestoreProcess `
                    -Context $context `
                    -ScriptPath (Join-Path $context.Root 'Start-Hermes-Local.ps1') `
                    -Arguments @('-Profile', [string]$context.PreviousState.Profile, '-NonInteractive') `
                    -Description 'Original Hermes Local service restart after pre-promotion failure'
                $context.ActiveState = 'original-active'
            } catch {
                $context.ActiveState = 'original-stopped-after-restore-failure'
                Add-HermesRestoreLog -Context $context -Level ERROR -Message "Original installation restart failed: $($_.Exception.Message)"
            }
        }

        Complete-HermesRestore `
            -Context $context `
            -Status 'failed' `
            -Message "Restore failed; active state is $($context.ActiveState)." `
            -FailureCode $(if ($context.RollbackAttempted -and $context.RollbackSucceeded -eq $false) {
                'restore-rollback-failed'
            } elseif ($context.RollbackAttempted) {
                'restore-failed-rollback-succeeded'
            } else {
                'restore-failed'
            }) `
            -FailureMessage $failureMessage
        Write-Host "Hermes Local restore failed: $failureMessage" -ForegroundColor Red
        Write-Host "Active state: $($context.ActiveState)" -ForegroundColor Yellow
        Write-Host "Restore report: $($context.ReportPath)"
    } else {
        Write-HermesLog -Component restore -Level ERROR -Message $failure.Exception.ToString()
        Write-Host "Hermes Local restore failed: $failureMessage" -ForegroundColor Red
    }
    $exitCode = 1
} finally {
    if ($context) {
        Remove-HermesRestoreCancellationRequest -Context $context
    }
    if ($stagingRoot -and (Test-Path -LiteralPath $stagingRoot)) {
        $stagingFull = [IO.Path]::GetFullPath($stagingRoot)
        $stagingPrefix = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot 'temp\restore-'))
        if ($stagingFull.StartsWith($stagingPrefix, [StringComparison]::OrdinalIgnoreCase)) {
            Remove-Item -LiteralPath $stagingFull -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}

exit $exitCode
