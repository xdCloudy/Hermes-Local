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

    $processId = if ($Plan.PSObject.Properties['parentPid']) {
        [int]$Plan.parentPid
    } else {
        0
    }
    $startedAt = if (
        $Plan.PSObject.Properties['parentStartedAt'] -and
        $Plan.parentStartedAt
    ) {
        [string]$Plan.parentStartedAt
    } else {
        $null
    }

    while (
        (Test-HermesDesktopProcessIdentity -ProcessId $processId -StartedAt $startedAt) -or
        (Test-HermesDesktopLauncherRunning)
    ) {
        Start-Sleep -Milliseconds 250
    }

    # Require a brief quiet period so Electron child processes can release files.
    Start-Sleep -Milliseconds 250
    if (Test-HermesDesktopLauncherRunning) {
        Wait-HermesDesktopLauncherExit -Plan $Plan
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

    Write-HermesDesktopUpdateProgress `
        -Plan $Plan `
        -Stage waiting-for-restart `
        -Status waiting `
        -Message 'Update ready. Waiting for the user to close Hermes Launcher.' `
        -Percent 95 `
        -Failure $null `
        -Result $null | Out-Null

    Wait-HermesDesktopLauncherExit -Plan $Plan

    $lastError = $null
    for ($attempt = 0; $attempt -lt 120; $attempt += 1) {
        try {
            if (Test-Path -LiteralPath $activationBackup) {
                Remove-Item -LiteralPath $activationBackup -Recurse -Force
            }

            if (Test-Path -LiteralPath $dist -PathType Container) {
                Move-Item -LiteralPath $dist -Destination $activationBackup
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
                activatedAt = (Get-Date).ToUniversalTime().ToString('o')
                relaunched = $false
            }
            Write-HermesDesktopUpdateJson -Path ([string]$Plan.resultPath) -Value $result
            Write-HermesDesktopUpdateProgress `
                -Plan $Plan `
                -Stage activated `
                -Status succeeded `
                -Message 'Update activated. Start Hermes Launcher to use the new version.' `
                -Percent 100 `
                -Failure $null `
                -Result $result | Out-Null
            return
        } catch {
            $lastError = $_
            try {
                if (
                    -not (Test-Path -LiteralPath $dist -PathType Container) -and
                    (Test-Path -LiteralPath $activationBackup -PathType Container)
                ) {
                    Move-Item -LiteralPath $activationBackup -Destination $dist
                }
            } catch {
            }
            Start-Sleep -Milliseconds 500
        }
    }

    throw "Could not activate the staged launcher after the running process closed. $($lastError.Exception.Message)"
}
