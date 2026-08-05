# Activation reliability overrides are loaded after the core Desktop updater
# parts. The original functions remain the source of staging and promotion
# semantics; this layer hardens process shutdown and persists activation state.

if (-not (Get-Variable -Name hermesDesktopOriginalPromotePendingLauncher -Scope Script -ErrorAction SilentlyContinue)) {
    $script:hermesDesktopOriginalPromotePendingLauncher =
        ${function:Promote-HermesDesktopPendingLauncher}
}
if (-not (Get-Variable -Name hermesDesktopOriginalStartPromotionHelper -Scope Script -ErrorAction SilentlyContinue)) {
    $script:hermesDesktopOriginalStartPromotionHelper =
        ${function:Start-HermesDesktopPromotionHelper}
}
if (-not (Get-Variable -Name hermesDesktopOriginalGetUpdateStatus -Scope Script -ErrorAction SilentlyContinue)) {
    $script:hermesDesktopOriginalGetUpdateStatus =
        ${function:Get-HermesDesktopUpdateStatus}
}

function Get-HermesDesktopLauncherProcesses {
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

                $resolvedExecutable.StartsWith(
                    $rootPrefix,
                    [StringComparison]::OrdinalIgnoreCase
                )
            }
    )
}

function Test-HermesDesktopLauncherRunning {
    [CmdletBinding()]
    param()

    @(Get-HermesDesktopLauncherProcesses).Count -gt 0
}

function Request-HermesDesktopLauncherClose {
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

    if (-not (Test-HermesDesktopProcessIdentity `
        -ProcessId $processId `
        -StartedAt $startedAt
    )) {
        return $false
    }

    try {
        $process = Get-Process -Id $processId -ErrorAction Stop
        $null = $process.CloseMainWindow()
        return $true
    } catch {
        return $false
    }
}

function Wait-HermesDesktopLauncherExit {
    [CmdletBinding()]
    param([Parameter(Mandatory)][object] $Plan)

    # An explicit Apply/Rollback is permission to restart the launcher. Ask the
    # owning browser process to close immediately rather than leaving a helper
    # blocked forever waiting for a manual restart.
    $null = Request-HermesDesktopLauncherClose -Plan $Plan

    $graceDeadline = (Get-Date).AddSeconds(12)
    while (
        @(Get-HermesDesktopLauncherProcesses).Count -gt 0 -and
        (Get-Date) -lt $graceDeadline
    ) {
        Start-Sleep -Milliseconds 250
    }

    # Electron renderer, GPU, utility and crashpad children can retain handles
    # after the browser process disappears. Drain every launcher executable
    # originating from this installation root, not only --type-less browsers.
    $remaining = @(Get-HermesDesktopLauncherProcesses)
    foreach ($process in $remaining) {
        Stop-Process `
            -Id ([int]$process.ProcessId) `
            -Force `
            -ErrorAction SilentlyContinue
    }

    $forceDeadline = (Get-Date).AddSeconds(30)
    while (
        @(Get-HermesDesktopLauncherProcesses).Count -gt 0 -and
        (Get-Date) -lt $forceDeadline
    ) {
        Start-Sleep -Milliseconds 250
    }

    $remaining = @(Get-HermesDesktopLauncherProcesses)
    if ($remaining.Count -gt 0) {
        $details = $remaining |
            ForEach-Object {
                $commandLine = [string]$_.CommandLine
                "PID $($_.ProcessId): $($_.ExecutablePath) $commandLine".Trim()
            }
        throw (
            'Hermes Launcher processes retained update file handles after shutdown: ' +
            ($details -join '; ')
        )
    }

    # File-system filters and antivirus can briefly retain handles after the
    # final process exits. Promotion already retries transient move failures;
    # this quiet period avoids racing the most common delayed release.
    Start-Sleep -Milliseconds 1000
}

function Set-HermesDesktopPendingActivationState {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][object] $Plan,
        [Parameter(Mandatory)]
        [ValidateSet('ready-to-restart', 'activating', 'activation-failed')]
        [string] $Status,
        [AllowNull()][string] $ErrorMessage
    )

    $pending = Read-HermesDesktopPendingUpdate
    if (-not $pending) {
        return $null
    }

    $pendingOperation = [string](Get-HermesDesktopObjectValue `
        -InputObject $pending `
        -Name operationId `
        -Default ''
    )
    if ($pendingOperation -ne [string]$Plan.operationId) {
        return $pending
    }

    $attempts = [int](Get-HermesDesktopObjectValue `
        -InputObject $pending `
        -Name activationAttempts `
        -Default 0
    )
    if ($Status -eq 'activating') {
        $attempts += 1
    }

    Set-HermesDesktopObjectValue -InputObject $pending -Name status -Value $Status
    Set-HermesDesktopObjectValue -InputObject $pending -Name activationAttempts -Value $attempts
    Set-HermesDesktopObjectValue -InputObject $pending -Name promotionPid -Value $(
        if ($Status -eq 'activating') { $PID } else { $null }
    )
    Set-HermesDesktopObjectValue -InputObject $pending -Name promotionStartedAt -Value $(
        if ($Status -eq 'activating') {
            Get-HermesDesktopProcessStartTime -ProcessId $PID
        } else {
            $null
        }
    )
    Set-HermesDesktopObjectValue -InputObject $pending -Name activationError -Value $ErrorMessage
    Set-HermesDesktopObjectValue -InputObject $pending -Name activationUpdatedAt -Value (
        (Get-Date).ToUniversalTime().ToString('o')
    )

    if ($Status -eq 'activation-failed') {
        Set-HermesDesktopObjectValue -InputObject $pending -Name activationFailedAt -Value (
            (Get-Date).ToUniversalTime().ToString('o')
        )
    }

    Write-HermesDesktopPendingUpdate -Value $pending
    $pending
}

function Start-HermesDesktopPromotionHelper {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][object] $Plan,
        [int] $ProcessId
    )

    $previous = Read-HermesDesktopPendingUpdate
    $previousAttempts = if (
        $previous -and
        [string](Get-HermesDesktopObjectValue -InputObject $previous -Name operationId -Default '') -eq
            [string]$Plan.operationId
    ) {
        [int](Get-HermesDesktopObjectValue `
            -InputObject $previous `
            -Name activationAttempts `
            -Default 0
        )
    } else {
        0
    }
    $previousError = if ($previous) {
        [string](Get-HermesDesktopObjectValue `
            -InputObject $previous `
            -Name activationError `
            -Default ''
        )
    } else {
        ''
    }

    $result = & $script:hermesDesktopOriginalStartPromotionHelper `
        -Plan $Plan `
        -ProcessId $ProcessId

    # Preserve retry evidence without overwriting a helper that has already
    # moved the state from ready-to-restart to activating.
    $current = Read-HermesDesktopPendingUpdate
    if (
        $current -and
        [string](Get-HermesDesktopObjectValue -InputObject $current -Name operationId -Default '') -eq
            [string]$Plan.operationId
    ) {
        $currentAttempts = [int](Get-HermesDesktopObjectValue `
            -InputObject $current `
            -Name activationAttempts `
            -Default 0
        )
        Set-HermesDesktopObjectValue `
            -InputObject $current `
            -Name activationAttempts `
            -Value ([math]::Max($currentAttempts, $previousAttempts))
        if ($previousError) {
            Set-HermesDesktopObjectValue `
                -InputObject $current `
                -Name previousActivationError `
                -Value $previousError
        }
        Write-HermesDesktopPendingUpdate -Value $current
        return $current
    }

    $result
}

function Promote-HermesDesktopPendingLauncher {
    [CmdletBinding()]
    param([Parameter(Mandatory)][object] $Plan)

    $null = Set-HermesDesktopPendingActivationState `
        -Plan $Plan `
        -Status activating `
        -ErrorMessage $null

    try {
        & $script:hermesDesktopOriginalPromotePendingLauncher -Plan $Plan
    } catch {
        $null = Set-HermesDesktopPendingActivationState `
            -Plan $Plan `
            -Status activation-failed `
            -ErrorMessage $_.Exception.Message
        throw
    }
}

function Get-HermesDesktopUpdateStatus {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $RequestedChannel,
        [string] $RequestedCommit,
        [int] $LauncherPid
    )

    $result = & $script:hermesDesktopOriginalGetUpdateStatus @PSBoundParameters
    $pending = Read-HermesDesktopPendingUpdate
    if (-not $pending) {
        return $result
    }

    $status = [string](Get-HermesDesktopObjectValue `
        -InputObject $pending `
        -Name status `
        -Default 'ready-to-restart'
    )
    $promotionPid = [int](Get-HermesDesktopObjectValue `
        -InputObject $pending `
        -Name promotionPid `
        -Default 0
    )
    $promotionStartedAt = [string](Get-HermesDesktopObjectValue `
        -InputObject $pending `
        -Name promotionStartedAt `
        -Default ''
    )
    $helperRunning = Test-HermesDesktopProcessIdentity `
        -ProcessId $promotionPid `
        -StartedAt $promotionStartedAt

    Set-HermesDesktopObjectValue -InputObject $result -Name activationStatus -Value $status
    Set-HermesDesktopObjectValue `
        -InputObject $result `
        -Name activationAttempts `
        -Value ([int](Get-HermesDesktopObjectValue `
            -InputObject $pending `
            -Name activationAttempts `
            -Default 0
        ))
    Set-HermesDesktopObjectValue -InputObject $result -Name promotionRunning -Value $helperRunning

    $message = switch ($status) {
        'activation-failed' {
            $errorMessage = [string](Get-HermesDesktopObjectValue `
                -InputObject $pending `
                -Name activationError `
                -Default 'Unknown activation failure.'
            )
            "The previous activation attempt failed, but the staged update is intact and automatic recovery is retrying. $errorMessage"
        }
        'activating' {
            'The update is activating now. Hermes Launcher will close and reopen automatically.'
        }
        default {
            'The update is ready and Hermes Launcher will restart automatically to activate it.'
        }
    }
    Set-HermesDesktopObjectValue -InputObject $result -Name message -Value $message
    $result
}
