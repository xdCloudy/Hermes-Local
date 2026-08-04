function Test-HermesDesktopLauncherRunning {
    [CmdletBinding()]
    param()

    $launcher = [IO.Path]::GetFullPath((Join-Path $root 'dist\Hermes Launcher.exe'))
    foreach ($process in @(Get-Process -Name 'Hermes Launcher' -ErrorAction SilentlyContinue)) {
        try {
            if (
                $process.Path -and
                [IO.Path]::GetFullPath($process.Path).Equals(
                    $launcher,
                    [StringComparison]::OrdinalIgnoreCase
                )
            ) {
                return $true
            }
        } catch {
        }
    }

    $false
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

    while ($true) {
        while (
            (Test-HermesDesktopProcessIdentity `
                -ProcessId $processId `
                -StartedAt $startedAt) -or
            (Test-HermesDesktopLauncherRunning)
        ) {
            Start-Sleep -Milliseconds 250
        }

        Start-Sleep -Milliseconds 100
        if (-not (Test-HermesDesktopLauncherRunning)) {
            return
        }
    }
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
            -Message 'Update ready. Waiting for the user to close Hermes Launcher.' `
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

    $result = [ordered]@{
        status = 'activated'
        previousCommit = [string]$Plan.previousCommit
        currentCommit = [string]$Plan.targetCommit
        launcherPath = $launcher
        activationDeferred = $true
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
}
