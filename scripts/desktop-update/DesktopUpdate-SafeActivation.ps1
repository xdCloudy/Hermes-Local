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

function Move-HermesDesktopActiveLauncherToActivationBackup {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $Dist,
        [Parameter(Mandatory)][string] $ActivationBackup
    )

    if (
        (Test-Path -LiteralPath $ActivationBackup -PathType Container) -and
        -not (Test-Path -LiteralPath $Dist)
    ) {
        return
    }

    if (
        (Test-Path -LiteralPath $ActivationBackup -PathType Container) -and
        (Test-Path -LiteralPath $Dist -PathType Container)
    ) {
        Remove-Item -LiteralPath $ActivationBackup -Recurse -Force
    }

    $lastError = $null
    for ($attempt = 0; $attempt -lt 120; $attempt += 1) {
        try {
            if (-not (Test-Path -LiteralPath $Dist -PathType Container)) {
                throw "The active launcher directory is missing: $Dist"
            }
            Move-Item -LiteralPath $Dist -Destination $ActivationBackup
            return
        } catch {
            $lastError = $_
            Start-Sleep -Milliseconds 500
        }
    }

    throw "Could not reserve the active launcher for deferred activation. $($lastError.Exception.Message)"
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
        if (Test-Path -LiteralPath $dist) {
            throw 'The active launcher path was recreated during deferred activation.'
        }

        Move-Item -LiteralPath $pendingDist -Destination $dist
        $launcher = Join-Path $dist 'Hermes Launcher.exe'
        if (-not (Test-Path -LiteralPath $launcher -PathType Leaf)) {
            throw 'Deferred launcher promotion did not produce the launcher executable.'
        }

        if (Test-Path -LiteralPath $activationBackup) {
            Remove-Item -LiteralPath $activationBackup -Recurse -Force
        }

        $pendingState = Get-HermesDesktopPendingUpdatePath
        if (Test-Path -LiteralPath $pendingState -PathType Leaf) {
            Remove-Item -LiteralPath $pendingState -Force
        }

        $result = [ordered]@{
            status = 'activated'
            previousCommit = [string]$Plan.previousCommit
            currentCommit = [string]$Plan.targetCommit
            launcherPath = $launcher
            activationDeferred = $true
            runtimeSynchronizedAfterExit = $true
            activatedAt = (Get-Date).ToUniversalTime().ToString('o')
            relaunched = $false
        }

        try {
            Write-HermesDesktopUpdateJson -Path ([string]$Plan.resultPath) -Value $result
            Write-HermesDesktopUpdateProgress `
                -Plan $Plan `
                -Stage activated `
                -Status succeeded `
                -Message 'Update activated. Start Hermes Launcher to use the new version.' `
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
