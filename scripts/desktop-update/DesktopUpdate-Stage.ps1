function Invoke-HermesDesktopUpdateStage {
    [CmdletBinding()]
    param([Parameter(Mandatory)][object] $Plan)

    $script:root = [IO.Path]::GetFullPath([string]$Plan.root)
    $null = Assert-HermesDesktopUpdatePath `
        -Root $root `
        -Path ([string]$Plan.stagingRoot) `
        -Description 'Staging root'
    $null = Assert-HermesDesktopUpdatePath `
        -Root $root `
        -Path ([string]$Plan.pendingDist) `
        -Description 'Pending launcher'
    $null = Assert-HermesDesktopUpdatePath `
        -Root $root `
        -Path ([string]$Plan.progressPath) `
        -Description 'Progress path'
    $null = Assert-HermesDesktopUpdatePath `
        -Root $root `
        -Path ([string]$Plan.resultPath) `
        -Description 'Result path'

    $lockPath = $null
    $failure = $null
    $stash = $null
    $sourceChanged = $false

    try {
        $lockPath = Enter-HermesDesktopUpdateLock `
            -Root $root `
            -OperationId ([string]$Plan.operationId)

        Write-HermesDesktopUpdateProgress `
            -Plan $Plan `
            -Stage preparing `
            -Status running `
            -Message 'Preparing the update in the background. Hermes Launcher will remain open.' `
            -Percent 5 `
            -Failure $null `
            -Result $null | Out-Null

        $dist = Join-Path $root 'dist'
        if (
            -not [bool]$Plan.rollbackOnly -and
            (Test-Path -LiteralPath $dist -PathType Container)
        ) {
            Copy-HermesDesktopDirectory `
                -Source $dist `
                -Destination ([string]$Plan.previousDist)
        }

        $changes = Get-HermesDesktopWorkingTreeChanges
        if ($changes) {
            Write-HermesDesktopUpdateProgress `
                -Plan $Plan `
                -Stage preserving-local-changes `
                -Status running `
                -Message 'Preserving local source changes before updating.' `
                -Percent 12 `
                -Failure $null `
                -Result $null | Out-Null
            $stash = Save-HermesDesktopWorkingTree -Plan $Plan
        }

        Write-HermesDesktopUpdateProgress `
            -Plan $Plan `
            -Stage installing `
            -Status running `
            -Message 'Pinning the trusted Hermes Local source revision.' `
            -Percent 20 `
            -Failure $null `
            -Result $null | Out-Null

        Invoke-HermesDesktopGit -Arguments @(
            'fetch', '--no-tags', 'origin', [string]$Plan.targetCommit
        ) | Out-Null

        if (-not [bool]$Plan.rollbackOnly) {
            $fastForward = Invoke-HermesDesktopGit -Arguments @(
                'merge-base', '--is-ancestor',
                [string]$Plan.previousCommit,
                [string]$Plan.targetCommit
            ) -AllowFailure
            if (
                $fastForward.ExitCode -ne 0 -and
                [string]$Plan.channel -ne 'pinned'
            ) {
                throw 'The selected update is not a fast-forward from the installed revision.'
            }
        }

        Invoke-HermesDesktopGit -Arguments @(
            'reset', '--hard', [string]$Plan.targetCommit
        ) | Out-Null
        $sourceChanged = $true

        Write-HermesDesktopUpdateProgress `
            -Plan $Plan `
            -Stage preparing `
            -Status running `
            -Message 'Synchronising the pinned Hermes Agent integration.' `
            -Percent 35 `
            -Failure $null `
            -Result $null | Out-Null
        Invoke-HermesDesktopSetup -Description 'Hermes Local source synchronisation'

        if ([bool]$Plan.rollbackOnly) {
            Write-HermesDesktopUpdateProgress `
                -Plan $Plan `
                -Stage staging-launcher `
                -Status running `
                -Message 'Preparing the previous known-good launcher for the next restart.' `
                -Percent 60 `
                -Failure $null `
                -Result $null | Out-Null
            Copy-HermesDesktopDirectory `
                -Source ([string]$Plan.previousDist) `
                -Destination ([string]$Plan.pendingDist)
        } else {
            Write-HermesDesktopUpdateProgress `
                -Plan $Plan `
                -Stage staging-launcher `
                -Status running `
                -Message 'Building and validating the update without replacing the running launcher.' `
                -Percent 55 `
                -Failure $null `
                -Result $null | Out-Null
            Invoke-HermesDesktopProcess -FilePath 'pwsh.exe' -Arguments @(
                '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
                '-File', (Join-Path $root 'Build-Hermes-Launcher.ps1'),
                '-DestinationDirectory', [string]$Plan.pendingDist,
                '-NonInteractive'
            ) -Description 'Hermes Local staged launcher build'
        }

        $pendingLauncher = Join-Path ([string]$Plan.pendingDist) 'Hermes Launcher.exe'
        if (-not (Test-Path -LiteralPath $pendingLauncher -PathType Leaf)) {
            throw 'The staged launcher was not produced.'
        }

        $null = Set-HermesDesktopPlanParent `
            -Plan $Plan `
            -ProcessId ([int]$Plan.parentPid)
        $pendingRecord = New-HermesDesktopPendingUpdateRecord -Plan $Plan
        Write-HermesDesktopPendingUpdate -Value $pendingRecord

        $restore = Restore-HermesDesktopWorkingTree `
            -Stash $stash `
            -Revision ([string]$Plan.targetCommit) `
            -Plan $Plan

        $promotionError = $null
        try {
            $pending = Start-HermesDesktopPromotionHelper `
                -Plan $Plan `
                -ProcessId ([int]$Plan.parentPid)
        } catch {
            $promotionError = $_.Exception.Message
            $pending = [pscustomobject]@{ promotionPid = 0 }
        }

        $parentStartedAt = if ($Plan.PSObject.Properties['parentStartedAt']) {
            [string]$Plan.parentStartedAt
        } else {
            ''
        }
        $result = [ordered]@{
            status = 'ready-to-restart'
            previousCommit = [string]$Plan.previousCommit
            currentCommit = [string]$Plan.targetCommit
            pendingLauncherPath = $pendingLauncher
            localChangesPreserved = [bool]$stash
            localChangesRestored = [bool]$restore.Restored
            retainedStashCommit = if (
                $restore.Retained -and -not $restore.Restored
            ) {
                [string]$restore.Commit
            } else {
                $null
            }
            activationDeferred = $true
            launcherStayedOpen = Test-HermesDesktopProcessIdentity `
                -ProcessId ([int]$Plan.parentPid) `
                -StartedAt $parentStartedAt
            restartRequired = $true
            promotionPid = [int]$pending.promotionPid
            promotionWarning = $promotionError
            relaunched = $false
        }

        $message = if ($restore.Restored) {
            'Update ready. Hermes Launcher will stay open; close and reopen it when convenient to activate the update.'
        } else {
            ([string]$restore.Message) + ' The launcher update is ready and will activate after you close Hermes Launcher.'
        }

        try {
            Write-HermesDesktopUpdateJson `
                -Path ([string]$Plan.resultPath) `
                -Value $result
            Write-HermesDesktopUpdateProgress `
                -Plan $Plan `
                -Stage ready-to-restart `
                -Status succeeded `
                -Message $message `
                -Percent 95 `
                -Failure $null `
                -Result $result | Out-Null
        } catch {
            $result['persistenceWarning'] = $_.Exception.Message
        }

        if ($stash -and $restore.Restored) {
            try {
                $null = Remove-HermesDesktopWorkingTreeStash -Stash $stash
            } catch {
                $result['stashCleanupWarning'] = $_.Exception.Message
            }
        }

        [pscustomobject]$result
    } catch {
        $failure = $_
        $rollbackFailure = $null

        try {
            Write-HermesDesktopUpdateProgress `
                -Plan $Plan `
                -Stage rolling-back `
                -Status running `
                -Message 'Discarding the staged update and restoring the previous source revision.' `
                -Percent 80 `
                -Failure $null `
                -Result $null | Out-Null

            if ($sourceChanged) {
                Invoke-HermesDesktopGit -Arguments @(
                    'reset', '--hard', [string]$Plan.previousCommit
                ) | Out-Null
                Invoke-HermesDesktopSetup `
                    -Description 'Hermes Local rollback source synchronisation'
            }

            if (Test-Path -LiteralPath ([string]$Plan.pendingDist)) {
                Remove-Item `
                    -LiteralPath ([string]$Plan.pendingDist) `
                    -Recurse `
                    -Force
            }
            $pendingStatePath = Get-HermesDesktopPendingUpdatePath
            if (Test-Path -LiteralPath $pendingStatePath -PathType Leaf) {
                Remove-Item -LiteralPath $pendingStatePath -Force
            }

            $restore = Restore-HermesDesktopWorkingTree `
                -Stash $stash `
                -Revision ([string]$Plan.previousCommit) `
                -Plan $Plan

            $result = [ordered]@{
                status = if ($sourceChanged) { 'rolled-back' } else { 'failed' }
                failedStage = 'desktop-self-update'
                previousCommit = [string]$Plan.previousCommit
                activeLauncherUntouched = $true
                localChangesPreserved = [bool]$stash
                localChangesRestored = [bool]$restore.Restored
                retainedStashCommit = if (
                    $restore.Retained -and -not $restore.Restored
                ) {
                    [string]$restore.Commit
                } else {
                    $null
                }
                relaunched = $false
            }

            Write-HermesDesktopUpdateJson `
                -Path ([string]$Plan.resultPath) `
                -Value $result

            $rollbackMessage = if ($sourceChanged) {
                'The staged update failed. The running launcher was not replaced and the previous source was restored.'
            } else {
                'The update stopped before changing the installed source. The running launcher was not replaced.'
            }
            if (-not $restore.Restored) {
                $rollbackMessage += " Local changes remain safe in Git stash $($restore.Commit)."
            }

            Write-HermesDesktopUpdateProgress `
                -Plan $Plan `
                -Stage rolled-back `
                -Status rolled-back `
                -Message $rollbackMessage `
                -Percent 100 `
                -Failure ([ordered]@{
                    code = 'desktop-update-rolled-back'
                    message = $failure.Exception.Message
                }) `
                -Result $result | Out-Null

            if ($stash -and $restore.Restored) {
                $null = Remove-HermesDesktopWorkingTreeStash -Stash $stash
            }
        } catch {
            $rollbackFailure = $_
        }

        if ($rollbackFailure) {
            try {
                Write-HermesDesktopUpdateProgress `
                    -Plan $Plan `
                    -Stage failed `
                    -Status failed `
                    -Message 'Update staging and automatic source rollback failed.' `
                    -Percent 100 `
                    -Failure ([ordered]@{
                        code = 'desktop-update-and-rollback-failed'
                        message = $failure.Exception.Message
                        rollback = $rollbackFailure.Exception.Message
                        retainedStashCommit = if ($stash) {
                            [string]$stash.commit
                        } else {
                            $null
                        }
                    }) `
                    -Result $null | Out-Null
            } catch {
            }
        }

        throw $failure
    } finally {
        Exit-HermesDesktopUpdateLock -LockPath $lockPath
    }
}

function New-HermesDesktopPreparedPlan {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $CurrentCommit,
        [Parameter(Mandatory)][string] $RequestedTargetCommit,
        [Parameter(Mandatory)][string] $RequestedChannel,
        [string] $CurrentBranch,
        [int] $LauncherPid,
        [string] $TaskId,
        [switch] $RollbackOnly,
        [string] $PreviousDist
    )

    $plan = New-HermesDesktopUpdatePlan `
        -Root $root `
        -CurrentCommit $CurrentCommit `
        -TargetCommit $RequestedTargetCommit `
        -Channel $RequestedChannel `
        -CurrentBranch $CurrentBranch `
        -ParentPid (Resolve-HermesDesktopParentPid -RequestedPid $LauncherPid) `
        -TaskId $TaskId `
        -RollbackOnly:$RollbackOnly

    $plan['planPath'] = Join-Path ([string]$plan.stagingRoot) 'plan.json'
    $plan['pendingDist'] = Join-Path ([string]$plan.stagingRoot) 'pending-dist'
    $plan['pendingStatePath'] = Get-HermesDesktopPendingUpdatePath
    $plan['parentStartedAt'] = Get-HermesDesktopProcessStartTime `
        -ProcessId ([int]$plan.parentPid)

    if ($PreviousDist) {
        $plan['previousDist'] = [IO.Path]::GetFullPath($PreviousDist)
    }

    [IO.Directory]::CreateDirectory([string]$plan.stagingRoot) | Out-Null
    $runtime = Copy-HermesDesktopUpdateRuntime -Plan $plan
    $plan['helperScript'] = $runtime.Script
    $plan['helperModule'] = $runtime.Module
    Write-HermesDesktopUpdateJson -Path ([string]$plan.planPath) -Value $plan
    $plan
}
