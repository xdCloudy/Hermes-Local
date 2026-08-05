[CmdletBinding()]
param(
    [string] $RepositoryRoot = $PSScriptRoot,
    [string] $TargetRemote = 'origin',
    [string] $TargetBranch = 'main',
    [switch] $SkipLaunch,
    [switch] $NonInteractive
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Write-RecoveryMessage {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $Message,
        [ValidateSet('INFO', 'WARN', 'ERROR')][string] $Level = 'INFO'
    )

    $prefix = "[$((Get-Date).ToUniversalTime().ToString('o'))] [$Level]"
    Write-Host "$prefix $Message"
    if ($script:recoveryLogPath) {
        [IO.File]::AppendAllText(
            $script:recoveryLogPath,
            "$prefix $Message$([Environment]::NewLine)",
            [Text.UTF8Encoding]::new($false)
        )
    }
}

function Invoke-RecoveryProcess {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $FilePath,
        [Parameter(Mandatory)][string[]] $Arguments,
        [Parameter(Mandatory)][string] $Stage,
        [string] $WorkingDirectory,
        [switch] $AllowFailure
    )

    $resolvedWorkingDirectory = if ($WorkingDirectory) {
        [IO.Path]::GetFullPath($WorkingDirectory)
    } else {
        $script:root
    }

    $startInfo = [Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $FilePath
    $startInfo.WorkingDirectory = $resolvedWorkingDirectory
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    foreach ($argument in $Arguments) {
        $startInfo.ArgumentList.Add([string]$argument)
    }

    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $startInfo
    try {
        if (-not $process.Start()) {
            throw "Could not start $FilePath."
        }
        $stdoutTask = $process.StandardOutput.ReadToEndAsync()
        $stderrTask = $process.StandardError.ReadToEndAsync()
        $process.WaitForExit()
        $stdout = $stdoutTask.GetAwaiter().GetResult()
        $stderr = $stderrTask.GetAwaiter().GetResult()
        $exitCode = [int]$process.ExitCode
    } finally {
        $process.Dispose()
    }

    $output = (@($stdout.TrimEnd(), $stderr.TrimEnd()) |
        Where-Object { -not [string]::IsNullOrWhiteSpace($_) }) -join [Environment]::NewLine

    if ($output) {
        [IO.File]::AppendAllText(
            $script:recoveryLogPath,
            "$Stage output:$([Environment]::NewLine)$output$([Environment]::NewLine)",
            [Text.UTF8Encoding]::new($false)
        )
    }

    if ($exitCode -ne 0 -and -not $AllowFailure) {
        $tail = (@($output -split '\r?\n') | Select-Object -Last 80) -join [Environment]::NewLine
        throw (
            "$Stage failed with exit code $exitCode." +
            $(if ($tail) { "`n$tail" } else { '' }) +
            "`nRecovery log: $script:recoveryLogPath"
        )
    }

    [pscustomobject]@{
        ExitCode = $exitCode
        Output = $output
    }
}

function Invoke-RecoveryGit {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string[]] $Arguments,
        [Parameter(Mandatory)][string] $Stage,
        [switch] $AllowFailure
    )

    Invoke-RecoveryProcess `
        -FilePath $script:git `
        -Arguments (@('-C', $script:root) + $Arguments) `
        -Stage $Stage `
        -WorkingDirectory $script:root `
        -AllowFailure:$AllowFailure
}

function Get-ProtectedProcessIds {
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
    $protected
}

function Get-HermesOwnedProcesses {
    [CmdletBinding()]
    param()

    $rootPrefix = $script:root.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    $protected = Get-ProtectedProcessIds
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
        'llama-server.exe'
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
                    $commandLine.IndexOf($script:root, [StringComparison]::OrdinalIgnoreCase) -ge 0

                $underRoot -or (
                    $referencesRoot -and
                    [string]$_.Name -in $knownCommandProcesses
                )
            }
    )
}

function Stop-HermesOwnedProcesses {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string] $Reason)

    $candidates = @(Get-HermesOwnedProcesses)
    if ($candidates.Count -gt 0) {
        Write-RecoveryMessage "Stopping $($candidates.Count) Hermes-owned process(es) before $Reason."
    }

    foreach ($process in $candidates) {
        Write-RecoveryMessage (
            "Stopping PID $($process.ProcessId): $($process.Name) " +
            "[$([string]$process.ExecutablePath)]"
        )
        Stop-Process `
            -Id ([int]$process.ProcessId) `
            -Force `
            -ErrorAction SilentlyContinue
    }

    $deadline = (Get-Date).AddSeconds(30)
    do {
        $remaining = @(Get-HermesOwnedProcesses)
        if ($remaining.Count -eq 0) {
            Start-Sleep -Milliseconds 1000
            return
        }
        Start-Sleep -Milliseconds 250
    } while ((Get-Date) -lt $deadline)

    $details = @(Get-HermesOwnedProcesses) |
        ForEach-Object {
            "PID $($_.ProcessId): $($_.Name) $($_.ExecutablePath) $($_.CommandLine)".Trim()
        }
    throw (
        "Hermes-owned processes remained active during $Reason: " +
        ($details -join '; ')
    )
}

function Move-WithRetry {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $Source,
        [Parameter(Mandatory)][string] $Destination,
        [Parameter(Mandatory)][string] $Stage,
        [ValidateRange(1, 240)][int] $Attempts = 120
    )

    $lastError = $null
    for ($attempt = 1; $attempt -le $Attempts; $attempt += 1) {
        try {
            Move-Item -LiteralPath $Source -Destination $Destination -Force
            return
        } catch {
            $lastError = $_
            if ($attempt -lt $Attempts) {
                Start-Sleep -Milliseconds 500
            }
        }
    }

    $blockers = @(Get-HermesOwnedProcesses) |
        ForEach-Object {
            "PID $($_.ProcessId): $($_.Name) $($_.ExecutablePath)"
        }
    throw (
        "$Stage could not move '$Source' to '$Destination' after $Attempts attempts. " +
        "$($lastError.Exception.Message)" +
        $(if ($blockers.Count -gt 0) {
            " Remaining Hermes-owned processes: $($blockers -join '; ')"
        } else {
            ''
        })
    )
}

function Archive-RecoveryState {
    [CmdletBinding()]
    param()

    $stateDestination = Join-Path $script:recoveryRoot 'runtime-state'
    [IO.Directory]::CreateDirectory($stateDestination) | Out-Null
    foreach ($relative in @(
        'data\runtime\pending-desktop-update.json',
        'data\runtime\locks\desktop-self-update.json',
        'data\runtime\locks\desktop-activation-recovery.lock'
    )) {
        $path = Join-Path $script:root $relative
        if (-not (Test-Path -LiteralPath $path)) {
            continue
        }
        $safeName = $relative -replace '[\\/:*?"<>|]', '_'
        Move-Item `
            -LiteralPath $path `
            -Destination (Join-Path $stateDestination $safeName) `
            -Force
        Write-RecoveryMessage "Archived stale updater state: $relative"
    }
}

function Repair-GitState {
    [CmdletBinding()]
    param()

    foreach ($arguments in @(
        @('am', '--abort'),
        @('rebase', '--abort'),
        @('merge', '--abort'),
        @('cherry-pick', '--abort'),
        @('revert', '--abort'),
        @('bisect', 'reset')
    )) {
        $null = Invoke-RecoveryGit `
            -Arguments $arguments `
            -Stage "Recovering interrupted Git operation: git $($arguments -join ' ')" `
            -AllowFailure
    }

    $gitDirectoryResult = Invoke-RecoveryGit `
        -Arguments @('rev-parse', '--absolute-git-dir') `
        -Stage 'Resolving the Git metadata directory'
    $gitDirectory = [IO.Path]::GetFullPath($gitDirectoryResult.Output.Trim())
    foreach ($name in @(
        'index.lock', 'HEAD.lock', 'packed-refs.lock', 'config.lock', 'shallow.lock'
    )) {
        $lockPath = Join-Path $gitDirectory $name
        if (-not (Test-Path -LiteralPath $lockPath -PathType Leaf)) {
            continue
        }
        $destination = Join-Path $script:recoveryRoot "git-$name"
        Move-Item -LiteralPath $lockPath -Destination $destination -Force
        Write-RecoveryMessage "Archived stale Git lock: $lockPath"
    }
}

function Preserve-WorkingTree {
    [CmdletBinding()]
    param()

    $head = Invoke-RecoveryGit `
        -Arguments @('rev-parse', 'HEAD') `
        -Stage 'Resolving the installed revision'
    $backupBranch = "recovery/desktop-updater-$script:stamp"
    $null = Invoke-RecoveryGit `
        -Arguments @('branch', $backupBranch, $head.Output.Trim()) `
        -Stage 'Creating the recovery backup branch'
    Write-RecoveryMessage "Created backup branch $backupBranch at $($head.Output.Trim())."

    $bootstrapPaths = @(
        'scripts/Repair-Hermes-DesktopUpdateState.ps1',
        'scripts/desktop-update/DesktopUpdate-00-Context.ps1',
        'scripts/desktop-update/DesktopUpdate-Activation.ps1',
        'scripts/desktop-update/DesktopUpdate-Reliability-Platform.ps1',
        'scripts/desktop-update/DesktopUpdate-ZActivation.ps1'
    )
    $bootstrapBackup = Join-Path $script:recoveryRoot 'bootstrap-working-copy'
    foreach ($relative in $bootstrapPaths) {
        $path = Join-Path $script:root $relative
        if (Test-Path -LiteralPath $path -PathType Leaf) {
            $destination = Join-Path $bootstrapBackup $relative
            [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($destination)) | Out-Null
            Copy-Item -LiteralPath $path -Destination $destination -Force
        }
    }

    $status = Invoke-RecoveryGit `
        -Arguments @('status', '--porcelain=v1') `
        -Stage 'Inspecting local source changes'
    if (-not [string]::IsNullOrWhiteSpace($status.Output)) {
        $stash = Invoke-RecoveryGit `
            -Arguments @(
                'stash', 'push', '--include-untracked',
                '--message', "desktop-updater-recovery-$script:stamp"
            ) `
            -Stage 'Preserving local source changes'
        $stashRef = Invoke-RecoveryGit `
            -Arguments @('rev-parse', 'refs/stash') `
            -Stage 'Recording the preserved source-change stash'
        $script:retainedStash = $stashRef.Output.Trim()
        Write-RecoveryMessage "Preserved local changes in Git stash $script:retainedStash."
    }
}

function Restore-WorkingTree {
    [CmdletBinding()]
    param()

    if (-not $script:retainedStash) {
        return
    }

    $apply = Invoke-RecoveryGit `
        -Arguments @('stash', 'apply', '--index', $script:retainedStash) `
        -Stage 'Restoring preserved local source changes' `
        -AllowFailure
    if ($apply.ExitCode -eq 0) {
        Write-RecoveryMessage (
            "Restored local source changes. The safety stash $script:retainedStash " +
            'was retained intentionally.'
        )
    } else {
        Write-RecoveryMessage (
            "The launcher update succeeded, but local changes could not be applied cleanly. " +
            "They remain safe in Git stash $script:retainedStash."
        ) -Level WARN
    }
}

function Promote-RecoveredLauncher {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string] $PendingDist)

    $dist = Join-Path $script:root 'dist'
    $backup = Join-Path $script:recoveryRoot 'previous-dist'
    $promoted = $false
    try {
        Stop-HermesOwnedProcesses -Reason 'launcher promotion'
        if (Test-Path -LiteralPath $dist -PathType Container) {
            Move-WithRetry `
                -Source $dist `
                -Destination $backup `
                -Stage 'Backing up the active launcher distribution'
        }
        Move-WithRetry `
            -Source $PendingDist `
            -Destination $dist `
            -Stage 'Promoting the repaired launcher distribution'
        $promoted = $true
    } finally {
        if (-not $promoted -and
            -not (Test-Path -LiteralPath $dist -PathType Container) -and
            (Test-Path -LiteralPath $backup -PathType Container)) {
            Move-WithRetry `
                -Source $backup `
                -Destination $dist `
                -Stage 'Restoring the previous launcher distribution'
        }
    }

    $launcher = Join-Path $dist 'Hermes Launcher.exe'
    if (-not (Test-Path -LiteralPath $launcher -PathType Leaf)) {
        throw "Launcher promotion did not produce '$launcher'."
    }
    $launcher
}

try {
    $script:root = [IO.Path]::GetFullPath($RepositoryRoot)
    if (-not (Test-Path -LiteralPath (Join-Path $script:root '.git') -PathType Container)) {
        throw "Hermes Local Git checkout not found: $script:root"
    }

    $scriptPath = if ($PSCommandPath) { [IO.Path]::GetFullPath($PSCommandPath) } else { '' }
    $rootPrefix = $script:root.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    if (
        -not $env:HERMES_DESKTOP_RECOVERY_RELOCATED -and
        $scriptPath -and
        $scriptPath.StartsWith($rootPrefix, [StringComparison]::OrdinalIgnoreCase)
    ) {
        $temporaryScript = Join-Path $env:TEMP (
            'Repair-Hermes-DesktopUpdater-' + [guid]::NewGuid().ToString('N') + '.ps1'
        )
        Copy-Item -LiteralPath $scriptPath -Destination $temporaryScript -Force
        $env:HERMES_DESKTOP_RECOVERY_RELOCATED = '1'
        try {
            & (Get-Command pwsh.exe -ErrorAction Stop).Source `
                -NoLogo `
                -NoProfile `
                -NonInteractive `
                -ExecutionPolicy Bypass `
                -File $temporaryScript `
                -RepositoryRoot $script:root `
                -TargetRemote $TargetRemote `
                -TargetBranch $TargetBranch `
                -SkipLaunch:$SkipLaunch `
                -NonInteractive
            exit $LASTEXITCODE
        } finally {
            Remove-Item -LiteralPath $temporaryScript -Force -ErrorAction SilentlyContinue
            Remove-Item Env:HERMES_DESKTOP_RECOVERY_RELOCATED -ErrorAction SilentlyContinue
        }
    }

    $script:stamp = (Get-Date).ToUniversalTime().ToString('yyyyMMddTHHmmssfffZ')
    $script:recoveryRoot = Join-Path $script:root "build\updates\desktop-break-glass\$script:stamp"
    [IO.Directory]::CreateDirectory($script:recoveryRoot) | Out-Null
    $script:recoveryLogPath = Join-Path $script:recoveryRoot 'recovery.log'
    $script:retainedStash = $null
    $script:git = (Get-Command git.exe -ErrorAction Stop).Source
    $pwsh = (Get-Command pwsh.exe -ErrorAction Stop).Source

    Write-RecoveryMessage "Starting standalone Desktop updater recovery for $script:root."
    Stop-HermesOwnedProcesses -Reason 'source recovery'
    Archive-RecoveryState
    Repair-GitState
    Preserve-WorkingTree

    $null = Invoke-RecoveryGit `
        -Arguments @('fetch', '--no-tags', $TargetRemote, $TargetBranch) `
        -Stage 'Fetching the repaired Hermes Local source'
    $targetRef = "$TargetRemote/$TargetBranch"
    $null = Invoke-RecoveryGit `
        -Arguments @('checkout', '-B', $TargetBranch, $targetRef) `
        -Stage "Activating repaired source revision $targetRef"
    $null = Invoke-RecoveryGit `
        -Arguments @('reset', '--hard', $targetRef) `
        -Stage "Pinning repaired source revision $targetRef"

    $setup = Join-Path $script:root 'Setup-Hermes-Local.ps1'
    $null = Invoke-RecoveryProcess `
        -FilePath $pwsh `
        -Arguments @(
            '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
            '-File', $setup,
            '-SkipModel', '-SkipLlamaBuild', '-SkipLauncherBuild', '-NonInteractive'
        ) `
        -Stage 'Synchronising repaired Hermes Local source and dependencies' `
        -WorkingDirectory $script:root

    $pendingDist = Join-Path $script:recoveryRoot 'pending-dist'
    $build = Join-Path $script:root 'Build-Hermes-Launcher.ps1'
    $null = Invoke-RecoveryProcess `
        -FilePath $pwsh `
        -Arguments @(
            '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
            '-File', $build,
            '-DestinationDirectory', $pendingDist,
            '-NonInteractive'
        ) `
        -Stage 'Building the repaired Hermes Launcher' `
        -WorkingDirectory $script:root

    $pendingLauncher = Join-Path $pendingDist 'Hermes Launcher.exe'
    if (-not (Test-Path -LiteralPath $pendingLauncher -PathType Leaf)) {
        throw "The repaired launcher build did not produce '$pendingLauncher'."
    }

    $launcher = Promote-RecoveredLauncher -PendingDist $pendingDist
    Restore-WorkingTree

    $versionPath = Join-Path $script:root 'VERSION.json'
    $version = if (Test-Path -LiteralPath $versionPath -PathType Leaf) {
        (Get-Content -Raw -LiteralPath $versionPath | ConvertFrom-Json -Depth 32).product.version
    } else {
        '<unknown>'
    }

    if (-not $SkipLaunch) {
        Start-Process -FilePath $launcher -WorkingDirectory $script:root | Out-Null
    }

    Write-RecoveryMessage "Desktop updater recovery completed successfully. Hermes Launcher version: $version."
    Write-Host "Recovery evidence: $script:recoveryRoot"
    exit 0
} catch {
    $message = $_.Exception.ToString()
    if ($script:recoveryLogPath) {
        [IO.File]::AppendAllText(
            $script:recoveryLogPath,
            "$message$([Environment]::NewLine)",
            [Text.UTF8Encoding]::new($false)
        )
    }
    Write-Host "Desktop updater recovery failed: $($_.Exception.Message)" -ForegroundColor Red
    if ($script:recoveryLogPath) {
        Write-Host "Recovery log: $script:recoveryLogPath" -ForegroundColor Yellow
    }
    exit 1
}
