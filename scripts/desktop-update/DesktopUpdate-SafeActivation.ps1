function Invoke-HermesDesktopSetup {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $Description,
        [string] $WorkingRoot = $root,
        [string] $SharedCacheRoot
    )

    $setupRoot = [IO.Path]::GetFullPath($WorkingRoot)
    $cacheRoot = if ($SharedCacheRoot) {
        [IO.Path]::GetFullPath($SharedCacheRoot)
    } else {
        Join-Path $setupRoot 'cache'
    }

    # Source and launcher dependencies may be synchronized while the Launcher
    # remains open, but the active Python environment is immutable until
    # deferred activation after every Launcher process has exited.
    Invoke-HermesDesktopProcess -FilePath 'pwsh.exe' -Arguments @(
        '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
        '-File', (Join-Path $setupRoot 'Setup-Hermes-Local.ps1'),
        '-SkipModel',
        '-SkipLlamaBuild',
        '-SkipHermesDependencies',
        '-SkipLauncherBuild',
        '-NonInteractive'
    ) -Description $Description -WorkingDirectory $setupRoot

    $source = Join-Path $setupRoot 'source\hermes-agent'
    $packageLock = Join-Path $source 'package-lock.json'
    if (Test-Path -LiteralPath $packageLock -PathType Leaf) {
        Invoke-HermesDesktopProcess -FilePath 'npm.cmd' -Arguments @(
            '--prefix', $source,
            'ci',
            '--cache', (Join-Path $cacheRoot 'npm'),
            '--no-audit'
        ) -Description "$Description Node dependency synchronisation" `
          -WorkingDirectory $setupRoot
    }
}

function Invoke-HermesDesktopRuntimeSync {
    [CmdletBinding()]
    param()

    $scriptPath = Join-Path $root 'scripts\setup\Sync-HermesPythonRuntime.ps1'
    if (-not (Test-Path -LiteralPath $scriptPath -PathType Leaf)) {
        throw "Deferred Python runtime synchronizer is missing: $scriptPath"
    }

    Invoke-HermesDesktopProcess -FilePath 'pwsh.exe' -Arguments @(
        '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
        '-File', $scriptPath,
        '-NonInteractive'
    ) -Description 'Hermes Local deferred Python runtime synchronisation'
}

function Remove-HermesDesktopActivationDirectory {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $Path,
        [Parameter(Mandatory)][string] $Description
    )

    $lastError = $null
    for ($attempt = 0; $attempt -lt 120; $attempt += 1) {
        try {
            if (-not (Test-Path -LiteralPath $Path)) {
                return
            }

            Remove-Item `
                -LiteralPath $Path `
                -Recurse `
                -Force `
                -ErrorAction Stop

            if (Test-Path -LiteralPath $Path) {
                throw "$Description still exists after removal: $Path"
            }

            return
        } catch {
            $lastError = $_
            Start-Sleep -Milliseconds 500
        }
    }

    throw "Could not remove $Description. $($lastError.Exception.Message)"
}

function Move-HermesDesktopActivationDirectory {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $Source,
        [Parameter(Mandatory)][string] $Destination,
        [Parameter(Mandatory)][string] $Description
    )

    $lastError = $null
    for ($attempt = 0; $attempt -lt 120; $attempt += 1) {
        try {
            if (-not (Test-Path -LiteralPath $Source -PathType Container)) {
                throw "$Description source is missing: $Source"
            }
            if (Test-Path -LiteralPath $Destination) {
                throw "$Description destination already exists: $Destination"
            }

            Move-Item `
                -LiteralPath $Source `
                -Destination $Destination `
                -ErrorAction Stop

            if (-not (Test-Path -LiteralPath $Destination -PathType Container)) {
                throw "$Description destination was not created: $Destination"
            }

            return
        } catch {
            $lastError = $_
            Start-Sleep -Milliseconds 500
        }
    }

    throw "Could not complete $Description. $($lastError.Exception.Message)"
}

function Move-HermesDesktopActiveLauncherToActivationBackup {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $Dist,
        [Parameter(Mandatory)][string] $ActivationBackup
    )

    if (Test-Path -LiteralPath $ActivationBackup -PathType Container) {
        # A previous activation attempt may have restored the known-good backup
        # into dist. Keep the canonical backup and remove only the disposable
        # restored copy before retrying the prepared payload.
        if (Test-Path -LiteralPath $Dist) {
            Remove-HermesDesktopActivationDirectory `
                -Path $Dist `
                -Description 'restored active launcher copy'
        }
        return
    }

    if (-not (Test-Path -LiteralPath $Dist -PathType Container)) {
        throw "The active launcher directory is missing: $Dist"
    }

    Move-HermesDesktopActivationDirectory `
        -Source $Dist `
        -Destination $ActivationBackup `
        -Description 'active launcher backup reservation'
}

function Enter-HermesDesktopActivationMutex {
    [CmdletBinding()]
    param([Parameter(Mandatory)][object] $Plan)

    $operationId = [string](Get-HermesDesktopObjectValue `
        -InputObject $Plan `
        -Name operationId `
        -Default '')
    if (-not $operationId) {
        throw 'Deferred activation requires an operation identity.'
    }

    $safeOperationId = $operationId -replace '[^A-Za-z0-9_.-]', '_'
    $mutexName = "Local\HermesLocalDesktopActivation-$safeOperationId"
    $mutex = [Threading.Mutex]::new($false, $mutexName)
    $ownsMutex = $false

    try {
        try {
            $ownsMutex = $mutex.WaitOne([TimeSpan]::FromMinutes(30))
        } catch [Threading.AbandonedMutexException] {
            # The prior helper exited while holding the operation mutex. The
            # retained pending payload and activation backup are the source of
            # truth, so this helper may safely resume the same operation.
            $ownsMutex = $true
        }

        if (-not $ownsMutex) {
            throw "Timed out waiting for deferred activation operation $operationId."
        }

        [pscustomobject]@{
            Mutex = $mutex
            OwnsMutex = $true
        }
    } catch {
        $mutex.Dispose()
        throw
    }
}

function Exit-HermesDesktopActivationMutex {
    [CmdletBinding()]
    param([AllowNull()][object] $Lease)

    if (-not $Lease) {
        return
    }

    try {
        if ([bool](Get-HermesDesktopObjectValue `
            -InputObject $Lease `
            -Name OwnsMutex `
            -Default $false)) {
            $Lease.Mutex.ReleaseMutex()
        }
    } catch {
    } finally {
        try {
            $Lease.Mutex.Dispose()
        } catch {
        }
    }
}

function Get-HermesDesktopCompletedActivationResult {
    [CmdletBinding()]
    param([Parameter(Mandatory)][object] $Plan)

    $resultPath = [string](Get-HermesDesktopObjectValue `
        -InputObject $Plan `
        -Name resultPath `
        -Default '')
    if (-not $resultPath -or -not (Test-Path -LiteralPath $resultPath -PathType Leaf)) {
        return $null
    }

    $result = try {
        Get-Content -Raw -LiteralPath $resultPath | ConvertFrom-Json -Depth 64
    } catch {
        $null
    }
    if (-not $result) {
        return $null
    }

    $status = [string](Get-HermesDesktopObjectValue `
        -InputObject $result `
        -Name status `
        -Default '')
    $currentCommit = [string](Get-HermesDesktopObjectValue `
        -InputObject $result `
        -Name currentCommit `
        -Default '')
    $targetCommit = [string](Get-HermesDesktopObjectValue `
        -InputObject $Plan `
        -Name targetCommit `
        -Default '')
    $launcher = Join-Path $root 'dist\Hermes Launcher.exe'

    if (
        $status -eq 'activated' -and
        $currentCommit -eq $targetCommit -and
        (Test-Path -LiteralPath $launcher -PathType Leaf)
    ) {
        return $result
    }

    $null
}

function Promote-HermesDesktopPendingLauncher {
    [CmdletBinding()]
    param([Parameter(Mandatory)][object] $Plan)

    $lease = Enter-HermesDesktopActivationMutex -Plan $Plan
    try {
        $completed = Get-HermesDesktopCompletedActivationResult -Plan $Plan
        if ($completed) {
            return $completed
        }

        $null = Assert-HermesDesktopUpdatePath `
            -Root $root `
            -Path ([string]$Plan.stagingRoot) `
            -Description 'Staging root'
        $pendingDist = Assert-HermesDesktopUpdatePath `
            -Root $root `
            -Path ([string]$Plan.pendingDist) `
            -Description 'Pending launcher'
        $dist = Join-Path $root 'dist'
        $activationBackup = Join-Path ([string]$Plan.stagingRoot) 'active-dist-at-activation'
        $activeSource = Join-Path $root 'source\hermes-agent'
        $pendingSource = [string](Get-HermesDesktopObjectValue `
            -InputObject $Plan `
            -Name pendingSource `
            -Default '')
        $sourceActivationBackup = Join-Path `
            ([string]$Plan.stagingRoot) `
            'active-source-at-activation'
        $preserveNestedSource = [bool](Get-HermesDesktopObjectValue `
            -InputObject $Plan `
            -Name preserveNestedSource `
            -Default $false)
        $sourceReserved = $false
        $sourcePromoted = $false

        if (
            -not (Test-Path -LiteralPath (Join-Path $pendingDist 'Hermes Launcher.exe') -PathType Leaf)
        ) {
            throw 'The deferred launcher payload is missing or incomplete.'
        }

        try {
            Write-HermesDesktopUpdateProgress `
                -Plan $Plan `
                -Stage waiting-for-restart `
                -Status waiting `
                -Message 'Update ready. Waiting for the user to close Hermes Launcher.' `
                -Percent 95 `
                -Failure $null `
                -Result $null | Out-Null
        } catch {
        }

        Start-Sleep -Milliseconds 250
        Wait-HermesDesktopLauncherExit -Plan $Plan

        try {
            # Reserve the active dist path before dependency work begins. This
            # prevents the old Launcher from being reopened while the Python
            # environment is stopped, rebuilt and atomically activated.
            Move-HermesDesktopActiveLauncherToActivationBackup `
                -Dist $dist `
                -ActivationBackup $activationBackup

            try {
                Write-HermesDesktopUpdateProgress `
                    -Plan $Plan `
                    -Stage activating-runtime `
                    -Status running `
                    -Message 'Launcher closed. Stopping services and activating the prepared runtime.' `
                    -Percent 97 `
                    -Failure $null `
                    -Result $null | Out-Null
            } catch {
            }

            Invoke-HermesDesktopProcess -FilePath 'pwsh.exe' -Arguments @(
                '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
                '-File', (Join-Path $root 'Stop-Hermes-Local.ps1'),
                '-NonInteractive'
            ) -Description 'Hermes Local service shutdown before update activation'

            # Plans created before isolated-source staging did not carry a
            # pendingSource field; retain their already-synchronised checkout.
            if (-not $preserveNestedSource -and $pendingSource) {
                $null = Assert-HermesDesktopUpdatePath `
                    -Root $root `
                    -Path $pendingSource `
                    -Description 'Prepared Hermes Agent source'
                if (-not (Test-Path -LiteralPath (Join-Path $pendingSource '.git'))) {
                    throw 'The prepared Hermes Agent source checkout is missing or incomplete.'
                }
                if (Test-Path -LiteralPath $activeSource -PathType Container) {
                    Move-HermesDesktopActivationDirectory `
                        -Source $activeSource `
                        -Destination $sourceActivationBackup `
                        -Description 'active Hermes Agent source backup reservation'
                    $sourceReserved = $true
                }
                Move-HermesDesktopActivationDirectory `
                    -Source $pendingSource `
                    -Destination $activeSource `
                    -Description 'prepared Hermes Agent source promotion'
                $sourcePromoted = $true
            }

            Invoke-HermesDesktopRuntimeSync

            if (Test-HermesDesktopLauncherRunning) {
                throw 'Hermes Launcher restarted before update activation completed.'
            }

            # Runtime/setup recovery may recreate dist from the known-good backup.
            # That copy is disposable because activationBackup remains intact. Clear
            # it with retries, then move the prepared launcher into the active path.
            if (Test-Path -LiteralPath $dist) {
                Remove-HermesDesktopActivationDirectory `
                    -Path $dist `
                    -Description 'recreated active launcher path'
            }

            Move-HermesDesktopActivationDirectory `
                -Source $pendingDist `
                -Destination $dist `
                -Description 'prepared launcher promotion'

            $launcher = Join-Path $dist 'Hermes Launcher.exe'
            if (-not (Test-Path -LiteralPath $launcher -PathType Leaf)) {
                throw 'Deferred launcher promotion did not produce the launcher executable.'
            }

            if (Test-Path -LiteralPath $activationBackup) {
                Remove-HermesDesktopActivationDirectory `
                    -Path $activationBackup `
                    -Description 'completed activation backup'
            }
            if (Test-Path -LiteralPath $sourceActivationBackup) {
                Remove-HermesDesktopActivationDirectory `
                    -Path $sourceActivationBackup `
                    -Description 'completed Hermes Agent source activation backup'
            }

            $pendingState = Get-HermesDesktopPendingUpdatePath
            if (Test-Path -LiteralPath $pendingState -PathType Leaf) {
                Remove-Item -LiteralPath $pendingState -Force
            }

            $relaunched = $false
            $relaunchWarning = $null
            try {
                Start-Process `
                    -FilePath $launcher `
                    -WorkingDirectory $root
                $relaunched = $true
            } catch {
                $relaunchWarning = $_.Exception.Message
            }

            $result = [ordered]@{
                status = 'activated'
                previousCommit = [string]$Plan.previousCommit
                currentCommit = [string]$Plan.targetCommit
                launcherPath = $launcher
                activationDeferred = $true
                runtimeSynchronizedAfterExit = $true
                nestedSourcePromoted = $sourcePromoted
                nestedSourcePreserved = $preserveNestedSource
                activatedAt = (Get-Date).ToUniversalTime().ToString('o')
                relaunched = $relaunched
                relaunchWarning = $relaunchWarning
            }

            try {
                Write-HermesDesktopUpdateJson -Path ([string]$Plan.resultPath) -Value $result
                Write-HermesDesktopUpdateProgress `
                    -Plan $Plan `
                    -Stage activated `
                    -Status succeeded `
                    -Message $(if ($relaunched) {
                        'Update activated and the new Hermes Launcher was started.'
                    } else {
                        'Update activated. Start Hermes Launcher to use the new version.'
                    }) `
                    -Percent 100 `
                    -Failure $null `
                    -Result $result | Out-Null
            } catch {
            }

            [pscustomobject]$result
        } catch {
            try {
                if ($sourcePromoted -and (Test-Path -LiteralPath $activeSource)) {
                    Remove-HermesDesktopActivationDirectory `
                        -Path $activeSource `
                        -Description 'failed prepared Hermes Agent source'
                }
                if ($sourceReserved -and (Test-Path -LiteralPath $sourceActivationBackup)) {
                    Move-HermesDesktopActivationDirectory `
                        -Source $sourceActivationBackup `
                        -Destination $activeSource `
                        -Description 'Hermes Agent source rollback'
                }
            } catch {
            }
            try {
                Restore-HermesDesktopActivationBackup `
                    -Dist $dist `
                    -PendingDist $pendingDist `
                    -ActivationBackup $activationBackup
            } catch {
            }
            throw
        }
    } finally {
        Exit-HermesDesktopActivationMutex -Lease $lease
    }
}
