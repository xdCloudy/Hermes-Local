function Invoke-HermesDesktopSetup {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string] $Description)

    # Source and launcher dependencies may be synchronized while the Launcher
    # remains open, but the active Python environment is immutable until
    # deferred activation after every Launcher process has exited.
    Invoke-HermesDesktopProcess -FilePath 'pwsh.exe' -Arguments @(
        '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
        '-File', (Join-Path $root 'Setup-Hermes-Local.ps1'),
        '-SkipModel',
        '-SkipLlamaBuild',
        '-SkipHermesDependencies',
        '-SkipLauncherBuild',
        '-NonInteractive'
    ) -Description $Description

    $source = Join-Path $root 'source\hermes-agent'
    $packageLock = Join-Path $source 'package-lock.json'
    if (Test-Path -LiteralPath $packageLock -PathType Leaf) {
        Invoke-HermesDesktopProcess -FilePath 'npm.cmd' -Arguments @(
            '--prefix', $source,
            'ci',
            '--cache', (Join-Path $root 'cache\npm'),
            '--no-audit'
        ) -Description "$Description Node dependency synchronisation"
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

function Promote-HermesDesktopPendingLauncher {
    [CmdletBinding()]
    param([Parameter(Mandatory)][object] $Plan)

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
            Restore-HermesDesktopActivationBackup `
                -Dist $dist `
                -PendingDist $pendingDist `
                -ActivationBackup $activationBackup
        } catch {
        }
        throw
    }
}
