function Get-HermesDesktopLauncherBrowserProcesses {
    [CmdletBinding()]
    param()

    $rootPrefix = [IO.Path]::GetFullPath($root).TrimEnd('\', '/') +
        [IO.Path]::DirectorySeparatorChar

    @(
        Get-CimInstance Win32_Process -ErrorAction SilentlyContinue |
            Where-Object {
                if ([string]$_.Name -ne 'Hermes Launcher.exe') {
                    return $false
                }

                $executablePath = [string]$_.ExecutablePath
                if (-not $executablePath) {
                    return $false
                }

                $resolvedExecutable = try {
                    [IO.Path]::GetFullPath($executablePath)
                } catch {
                    return $false
                }

                if (-not $resolvedExecutable.StartsWith(
                    $rootPrefix,
                    [StringComparison]::OrdinalIgnoreCase
                )) {
                    return $false
                }

                # Electron renderer, GPU and utility children share the same
                # executable but carry --type=. Only browser processes own the
                # single-instance lock that can redirect a newly promoted dist
                # launcher back into an old activation-backup executable.
                [string]$_.CommandLine -notmatch '(?i)(?:^|\s)--type='
            }
    )
}

function Test-HermesDesktopLauncherRunning {
    [CmdletBinding()]
    param()

    @(Get-HermesDesktopLauncherBrowserProcesses).Count -gt 0
}

function Wait-HermesDesktopLauncherExit {
    [CmdletBinding()]
    param([Parameter(Mandatory)][object] $Plan)

    $processId = [int](Get-HermesDesktopObjectValue `
        -InputObject $Plan `
        -Name parentPid `
        -Default 0
    )
    $startedAt = [string](Get-HermesDesktopObjectValue `
        -InputObject $Plan `
        -Name parentStartedAt `
        -Default ''
    )

    # First wait for the exact browser process that initiated staging. This
    # avoids confusing short-lived Electron children with the owning process.
    while (
        Test-HermesDesktopProcessIdentity `
            -ProcessId $processId `
            -StartedAt $startedAt
    ) {
        Start-Sleep -Milliseconds 250
    }

    # A backup-path browser can be opened during handoff and still own
    # Electron's single-instance lock. Give every Hermes Local browser process
    # a brief graceful-exit window, then terminate only remaining browser
    # processes under this installation root before promotion continues.
    $deadline = (Get-Date).AddSeconds(5)
    while (
        @(Get-HermesDesktopLauncherBrowserProcesses).Count -gt 0 -and
        (Get-Date) -lt $deadline
    ) {
        Start-Sleep -Milliseconds 250
    }

    $remaining = @(Get-HermesDesktopLauncherBrowserProcesses)
    foreach ($process in $remaining) {
        Stop-Process `
            -Id ([int]$process.ProcessId) `
            -Force `
            -ErrorAction SilentlyContinue
    }

    $deadline = (Get-Date).AddSeconds(15)
    while (
        @(Get-HermesDesktopLauncherBrowserProcesses).Count -gt 0 -and
        (Get-Date) -lt $deadline
    ) {
        Start-Sleep -Milliseconds 250
    }

    $remaining = @(Get-HermesDesktopLauncherBrowserProcesses)
    if ($remaining.Count -gt 0) {
        $details = $remaining |
            ForEach-Object {
                "PID $($_.ProcessId): $($_.ExecutablePath)"
            }
        throw (
            'Hermes Launcher browser processes remained active after shutdown: ' +
            ($details -join '; ')
        )
    }

    # Let renderer/GPU children release their final file handles. Directory
    # promotion already retries transient Windows locks after this point.
    Start-Sleep -Milliseconds 500
}

function Restore-HermesDesktopActivationBackup {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $Dist,
        [Parameter(Mandatory)][string] $PendingDist,
        [Parameter(Mandatory)][string] $ActivationBackup
    )

    if (-not (Test-Path -LiteralPath $ActivationBackup -PathType Container)) {
        return
    }

    if (Test-Path -LiteralPath $Dist -PathType Container) {
        try {
            if (-not (Test-Path -LiteralPath $PendingDist)) {
                Move-Item -LiteralPath $Dist -Destination $PendingDist
            } else {
                Remove-Item -LiteralPath $Dist -Recurse -Force
            }
        } catch {
            Remove-Item -LiteralPath $Dist -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    if (-not (Test-Path -LiteralPath $Dist -PathType Container)) {
        # Keep the activation backup intact across retries. A failed restore must
        # never consume the only known-good launcher distribution.
        Copy-HermesDesktopDirectory `
            -Source $ActivationBackup `
            -Destination $Dist
    }
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
            -Message 'Update ready. Close Hermes Launcher; the updated launcher will reopen automatically.' `
            -Percent 95 `
            -Failure $null `
            -Result $null | Out-Null
    } catch {
    }

    # Give the staging process time to persist the promotion PID before a
    # command-line update with no running launcher can complete activation.
    Start-Sleep -Milliseconds 250
    Wait-HermesDesktopLauncherExit -Plan $Plan

    $lastError = $null
    $promoted = $false
    for ($attempt = 0; $attempt -lt 120; $attempt += 1) {
        try {
            if (-not (Test-Path -LiteralPath $activationBackup -PathType Container)) {
                if (Test-Path -LiteralPath $dist -PathType Container) {
                    Move-Item -LiteralPath $dist -Destination $activationBackup
                }
            } elseif (Test-Path -LiteralPath $dist -PathType Container) {
                # A prior attempt restored the active launcher from the backup.
                # Remove only that disposable copy; retain the backup itself.
                Remove-Item -LiteralPath $dist -Recurse -Force
            }

            if (
                -not (Test-Path `
                    -LiteralPath (Join-Path $pendingDist 'Hermes Launcher.exe') `
                    -PathType Leaf)
            ) {
                throw 'The deferred launcher payload was lost during promotion.'
            }

            Move-Item -LiteralPath $pendingDist -Destination $dist
            $launcher = Join-Path $dist 'Hermes Launcher.exe'
            if (-not (Test-Path -LiteralPath $launcher -PathType Leaf)) {
                throw 'Deferred launcher promotion did not produce the launcher executable.'
            }

            $promoted = $true
            break
        } catch {
            $lastError = $_
            try {
                Restore-HermesDesktopActivationBackup `
                    -Dist $dist `
                    -PendingDist $pendingDist `
                    -ActivationBackup $activationBackup
            } catch {
            }
            Start-Sleep -Milliseconds 500
        }
    }

    if (-not $promoted) {
        throw "Could not activate the staged launcher after the running process closed. $($lastError.Exception.Message)"
    }

    $launcher = Join-Path $dist 'Hermes Launcher.exe'
    try {
        if (Test-Path -LiteralPath $activationBackup) {
            Remove-Item -LiteralPath $activationBackup -Recurse -Force
        }
    } catch {
    }

    try {
        $pendingState = Get-HermesDesktopPendingUpdatePath
        if (Test-Path -LiteralPath $pendingState -PathType Leaf) {
            Remove-Item -LiteralPath $pendingState -Force
        }
    } catch {
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
}
