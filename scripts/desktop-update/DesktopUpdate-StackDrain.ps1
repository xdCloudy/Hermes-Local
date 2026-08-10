if (-not (Get-Variable -Name hermesDesktopOriginalInvokeUpdateStage -Scope Script -ErrorAction SilentlyContinue)) {
    $script:hermesDesktopOriginalInvokeUpdateStage =
        ${function:Invoke-HermesDesktopUpdateStage}
}

function Get-HermesDesktopProtectedProcessIds {
    [CmdletBinding()]
    param()

    $protected = [System.Collections.Generic.HashSet[int]]::new()
    $candidate = [int]$PID
    for ($depth = 0; $depth -lt 16 -and $candidate -gt 0; $depth += 1) {
        $null = $protected.Add($candidate)
        $record = Get-CimInstance Win32_Process `
            -Filter "ProcessId = $candidate" `
            -ErrorAction SilentlyContinue
        if (-not $record) {
            break
        }

        $parent = [int]$record.ParentProcessId
        if ($parent -le 0 -or $protected.Contains($parent)) {
            break
        }
        $candidate = $parent
    }

    Write-Output -NoEnumerate $protected
}

function Get-HermesDesktopOwnedProcesses {
    [CmdletBinding()]
    param()

    $resolvedRoot = [IO.Path]::GetFullPath($root)
    $rootPrefix = $resolvedRoot.TrimEnd('\', '/') +
        [IO.Path]::DirectorySeparatorChar
    $protected = Get-HermesDesktopProtectedProcessIds
    $knownCommandProcesses = @(
        'Hermes Launcher.exe',
        'pwsh.exe',
        'powershell.exe',
        'python.exe',
        'pythonw.exe',
        'node.exe',
        'npm.exe',
        'npx.exe',
        'git.exe',
        'git-lfs.exe',
        'cmake.exe',
        'ninja.exe',
        'llama-server.exe',
        'cmd.exe',
        'dotnet.exe'
    )

    @(
        Get-CimInstance Win32_Process -ErrorAction SilentlyContinue |
            Where-Object {
                $processId = [int]$_.ProcessId
                if ($protected.Contains($processId)) {
                    return $false
                }

                $executablePath = [string]$_.ExecutablePath
                $commandLine = [string]$_.CommandLine
                $underRoot = $false
                if ($executablePath) {
                    try {
                        $underRoot = [IO.Path]::GetFullPath($executablePath).StartsWith(
                            $rootPrefix,
                            [StringComparison]::OrdinalIgnoreCase
                        )
                    } catch {
                        $underRoot = $false
                    }
                }

                $referencesRoot =
                    $commandLine -and
                    $commandLine.IndexOf(
                        $resolvedRoot,
                        [StringComparison]::OrdinalIgnoreCase
                    ) -ge 0

                $underRoot -or (
                    $referencesRoot -and
                    [string]$_.Name -in $knownCommandProcesses
                )
            }
    )
}

function Stop-HermesDesktopOwnedProcesses {
    [CmdletBinding()]
    param(
        [AllowNull()][object] $Plan,
        [Parameter(Mandatory)][string] $Reason,
        [ValidateRange(0, 30)][int] $GraceSeconds = 8
    )

    if ($Plan) {
        $null = Request-HermesDesktopLauncherClose -Plan $Plan
    }

    $graceDeadline = (Get-Date).AddSeconds($GraceSeconds)
    while (
        @(Get-HermesDesktopOwnedProcesses).Count -gt 0 -and
        (Get-Date) -lt $graceDeadline
    ) {
        Start-Sleep -Milliseconds 250
    }

    $remaining = @(Get-HermesDesktopOwnedProcesses)
    foreach ($process in $remaining) {
        try {
            $null = Add-HermesDesktopUpdateLog -Plan $Plan -Message (
                "Stopping Hermes-owned process for ${Reason}: PID $($process.ProcessId), " +
                "name $($process.Name), executable $($process.ExecutablePath), " +
                "command line $($process.CommandLine)"
            )
        } catch {
        }

        Stop-Process `
            -Id ([int]$process.ProcessId) `
            -Force `
            -ErrorAction SilentlyContinue
    }

    $forceDeadline = (Get-Date).AddSeconds(30)
    while (
        @(Get-HermesDesktopOwnedProcesses).Count -gt 0 -and
        (Get-Date) -lt $forceDeadline
    ) {
        Start-Sleep -Milliseconds 250
    }

    $remaining = @(Get-HermesDesktopOwnedProcesses)
    if ($remaining.Count -gt 0) {
        $details = $remaining |
            ForEach-Object {
                "PID $($_.ProcessId): $($_.Name) $($_.ExecutablePath) $($_.CommandLine)".Trim()
            }
        throw (
            "Hermes-owned processes retained update file handles during ${Reason}: " +
            ($details -join '; ')
        )
    }

    Start-Sleep -Milliseconds 1500
}

function Wait-HermesDesktopLauncherExit {
    [CmdletBinding()]
    param([Parameter(Mandatory)][object] $Plan)

    Stop-HermesDesktopOwnedProcesses `
        -Plan $Plan `
        -Reason 'launcher activation' `
        -GraceSeconds 12
}

function Get-HermesDesktopFailureEvidence {
    [CmdletBinding()]
    param([Parameter(Mandatory)][object] $Plan)

    $progressPath = [string](Get-HermesDesktopObjectValue `
        -InputObject $Plan `
        -Name progressPath `
        -Default ''
    )
    if (-not $progressPath -or
        -not (Test-Path -LiteralPath $progressPath -PathType Leaf)) {
        return $null
    }

    try {
        $progress = Get-Content -Raw -LiteralPath $progressPath |
            ConvertFrom-Json -Depth 64
        [pscustomobject]@{
            Stage = [string](Get-HermesDesktopObjectValue `
                -InputObject $progress `
                -Name stage `
                -Default 'unknown'
            )
            Message = [string](Get-HermesDesktopObjectValue `
                -InputObject $progress `
                -Name message `
                -Default ''
            )
            Failure = Get-HermesDesktopObjectValue `
                -InputObject $progress `
                -Name failure `
                -Default $null
        }
    } catch {
        $null
    }
}

function Invoke-HermesDesktopUpdateStage {
    [CmdletBinding()]
    param([Parameter(Mandatory)][object] $Plan)

    try {
        # Staging is deliberately isolated from the live checkout and launcher.
        # The full process drain belongs exclusively to deferred activation,
        # after the candidate is built, validated and recorded as pending. A
        # pre-stage drain kills both the Launcher parent and its updater child,
        # leaving no helper or pending state for the next launch to recover.
        & $script:hermesDesktopOriginalInvokeUpdateStage -Plan $Plan
    } catch {
        $evidence = Get-HermesDesktopFailureEvidence -Plan $Plan
        $stage = if ($evidence -and $evidence.Stage) {
            [string]$evidence.Stage
        } else {
            'unknown'
        }
        $recordedFailure = if (
            $evidence -and
            $evidence.Failure -and
            $evidence.Failure.PSObject.Properties['message']
        ) {
            [string]$evidence.Failure.message
        } else {
            ''
        }
        $recordedMessage = if ($evidence) { [string]$evidence.Message } else { '' }
        $detail = @(
            $recordedFailure
            $recordedMessage
            $_.Exception.Message
        ) |
            Where-Object { -not [string]::IsNullOrWhiteSpace($_) } |
            Select-Object -Unique

        $exception = [InvalidOperationException]::new(
            "Desktop update failed during stage '$stage'. $($detail -join ' ')",
            $_.Exception
        )
        throw $exception
    }
}
