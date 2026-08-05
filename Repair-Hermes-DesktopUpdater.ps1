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
$script:root = $null
$script:recoveryRoot = $null
$script:logPath = $null
$script:stashCommit = $null
$script:git = $null
$script:pwsh = $null

function Write-RecoveryLog {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $Message,
        [ValidateSet('INFO', 'WARN', 'ERROR')][string] $Level = 'INFO'
    )

    $line = "[$((Get-Date).ToUniversalTime().ToString('o'))] [$Level] $Message"
    Write-Host $line
    if ($script:logPath) {
        [IO.File]::AppendAllText(
            $script:logPath,
            $line + [Environment]::NewLine,
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

    $startInfo = [Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $FilePath
    $startInfo.WorkingDirectory = if ($WorkingDirectory) {
        [IO.Path]::GetFullPath($WorkingDirectory)
    } else {
        $script:root
    }
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
            throw "Could not start '$FilePath'."
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
    if ($output -and $script:logPath) {
        [IO.File]::AppendAllText(
            $script:logPath,
            "${Stage}:$([Environment]::NewLine)$output$([Environment]::NewLine)",
            [Text.UTF8Encoding]::new($false)
        )
    }

    if ($exitCode -ne 0 -and -not $AllowFailure) {
        $tail = (@($output -split '\r?\n') | Select-Object -Last 80) -join [Environment]::NewLine
        throw (
            "$Stage failed with exit code $exitCode." +
            $(if ($tail) { "`n$tail" } else { '' }) +
            "`nRecovery log: $script:logPath"
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

function Get-RecoveryProtectedProcessIds {
    [CmdletBinding()]
    param()

    $ids = [System.Collections.Generic.HashSet[int]]::new()
    $candidate = [int]$PID
    for ($depth = 0; $depth -lt 16 -and $candidate -gt 0; $depth += 1) {
        $null = $ids.Add($candidate)
        $record = Get-CimInstance Win32_Process `
            -Filter "ProcessId = $candidate" `
            -ErrorAction SilentlyContinue
        if (-not $record) {
            break
        }
        $parent = [int]$record.ParentProcessId
        if ($parent -le 0 -or $ids.Contains($parent)) {
            break
        }
        $candidate = $parent
    }
    Write-Output -NoEnumerate $ids
}

function Get-HermesRecoveryProcesses {
    [CmdletBinding()]
    param()

    $protected = Get-RecoveryProtectedProcessIds
    $rootPrefix = $script:root.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    $knownNames = @(
        'Hermes Launcher.exe', 'pwsh.exe', 'powershell.exe',
        'python.exe', 'pythonw.exe', 'node.exe', 'npm.exe', 'npx.exe',
        'git.exe', 'git-lfs.exe', 'cmake.exe', 'ninja.exe',
        'llama-server.exe', 'cmd.exe', 'dotnet.exe'
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
                        $script:root,
                        [StringComparison]::OrdinalIgnoreCase
                    ) -ge 0

                $underRoot -or ($referencesRoot -and [string]$_.Name -in $knownNames)
            }
    )
}

function Stop-HermesRecoveryProcesses {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string] $Reason)

    $processes = @(Get-HermesRecoveryProcesses)
    foreach ($process in $processes) {
        Write-RecoveryLog (
            "Stopping PID $($process.ProcessId) for ${Reason}: " +
            "$($process.Name) $($process.ExecutablePath)"
        )
        Stop-Process -Id ([int]$process.ProcessId) -Force -ErrorAction SilentlyContinue
    }

    $deadline = (Get-Date).AddSeconds(30)
    while ((Get-Date) -lt $deadline) {
        $remaining = @(Get-HermesRecoveryProcesses)
        if ($remaining.Count -eq 0) {
            Start-Sleep -Milliseconds 1500
            return
        }
        Start-Sleep -Milliseconds 250
    }

    $details = @(Get-HermesRecoveryProcesses) |
        ForEach-Object {
            "PID $($_.ProcessId): $($_.Name) $($_.ExecutablePath) $($_.CommandLine)".Trim()
        }
    throw "Hermes-owned processes remained active during ${Reason}: $($details -join '; ')"
}

function Move-RecoveryItem {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $Source,
        [Parameter(Mandatory)][string] $Destination,
        [Parameter(Mandatory)][string] $Stage
    )

    $lastError = $null
    for ($attempt = 1; $attempt -le 120; $attempt += 1) {
        try {
            Move-Item -LiteralPath $Source -Destination $Destination -Force
            return
        } catch {
            $lastError = $_
            Start-Sleep -Milliseconds 500
        }
    }
    throw "$Stage failed moving '$Source' to '$Destination'. $($lastError.Exception.Message)"
}

function Archive-StaleUpdaterState {
    [CmdletBinding()]
    param()

    $destinationRoot = Join-Path $script:recoveryRoot 'runtime-state'
    [IO.Directory]::CreateDirectory($destinationRoot) | Out-Null
    foreach ($relative in @(
        'data\runtime\pending-desktop-update.json',
        'data\runtime\locks\desktop-self-update.json',
        'data\runtime\locks\desktop-activation-recovery.lock'
    )) {
        $source = Join-Path $script:root $relative
        if (-not (Test-Path -LiteralPath $source)) {
            continue
        }
        $name = $relative -replace '[\\/:*?"<>|]', '_'
        Move-Item -LiteralPath $source -Destination (Join-Path $destinationRoot $name) -Force
        Write-RecoveryLog "Archived stale updater state '$relative'."
    }
}

function Repair-RecoveryGitState {
    [CmdletBinding()]
    param()

    foreach ($arguments in @(
        @('am', '--abort'), @('rebase', '--abort'), @('merge', '--abort'),
        @('cherry-pick', '--abort'), @('revert', '--abort'), @('bisect', 'reset')
    )) {
        $null = Invoke-RecoveryGit `
            -Arguments $arguments `
            -Stage "Recovering Git operation '$($arguments -join ' ')'" `
            -AllowFailure
    }

    $gitDirectoryResult = Invoke-RecoveryGit `
        -Arguments @('rev-parse', '--absolute-git-dir') `
        -Stage 'Resolving Git metadata directory'
    $gitDirectory = [IO.Path]::GetFullPath($gitDirectoryResult.Output.Trim())
    foreach ($name in @('index.lock', 'HEAD.lock', 'packed-refs.lock', 'config.lock', 'shallow.lock')) {
        $source = Join-Path $gitDirectory $name
        if (Test-Path -LiteralPath $source -PathType Leaf) {
            Move-Item `
                -LiteralPath $source `
                -Destination (Join-Path $script:recoveryRoot "git-$name") `
                -Force
            Write-RecoveryLog "Archived stale Git lock '$source'."
        }
    }
}

function Remove-BootstrapOverrides {
    [CmdletBinding()]
    param()

    $paths = @(
        'scripts/Repair-Hermes-DesktopUpdateState.ps1',
        'scripts/desktop-update/DesktopUpdate-00-Context.ps1',
        'scripts/desktop-update/DesktopUpdate-Activation.ps1',
        'scripts/desktop-update/DesktopUpdate-Reliability-Platform.ps1',
        'scripts/desktop-update/DesktopUpdate-ZActivation.ps1'
    )
    $backupRoot = Join-Path $script:recoveryRoot 'bootstrap-working-copy'

    foreach ($relative in $paths) {
        $source = Join-Path $script:root $relative
        if (Test-Path -LiteralPath $source -PathType Leaf) {
            $destination = Join-Path $backupRoot $relative
            [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($destination)) | Out-Null
            Copy-Item -LiteralPath $source -Destination $destination -Force
        }

        $tracked = Invoke-RecoveryGit `
            -Arguments @('ls-files', '--error-unmatch', '--', $relative) `
            -Stage "Checking bootstrap path '$relative'" `
            -AllowFailure
        if ($tracked.ExitCode -eq 0) {
            $null = Invoke-RecoveryGit `
                -Arguments @('restore', '--source=HEAD', '--staged', '--worktree', '--', $relative) `
                -Stage "Resetting temporary bootstrap path '$relative'"
        } else {
            $null = Invoke-RecoveryGit `
                -Arguments @('rm', '--cached', '--ignore-unmatch', '--', $relative) `
                -Stage "Removing temporary bootstrap index entry '$relative'" `
                -AllowFailure
            Remove-Item -LiteralPath $source -Force -ErrorAction SilentlyContinue
        }
    }
}

function Preserve-RecoveryWorkingTree {
    [CmdletBinding()]
    param()

    $head = Invoke-RecoveryGit -Arguments @('rev-parse', 'HEAD') -Stage 'Resolving installed revision'
    $backupBranch = "recovery/desktop-updater-$script:stamp"
    $null = Invoke-RecoveryGit `
        -Arguments @('branch', $backupBranch, $head.Output.Trim()) `
        -Stage 'Creating recovery backup branch'
    Write-RecoveryLog "Created backup branch '$backupBranch'."

    Remove-BootstrapOverrides
    $status = Invoke-RecoveryGit -Arguments @('status', '--porcelain=v1') -Stage 'Inspecting local changes'
    if ([string]::IsNullOrWhiteSpace($status.Output)) {
        return
    }

    $null = Invoke-RecoveryGit `
        -Arguments @('stash', 'push', '--include-untracked', '--message', "desktop-updater-$script:stamp") `
        -Stage 'Preserving local source changes'
    $stash = Invoke-RecoveryGit -Arguments @('rev-parse', 'refs/stash') -Stage 'Recording source-change stash'
    $script:stashCommit = $stash.Output.Trim()
    Write-RecoveryLog "Preserved local changes in stash '$script:stashCommit'."
}

function Restore-RecoveryWorkingTree {
    [CmdletBinding()]
    param()

    if (-not $script:stashCommit) {
        return
    }
    $result = Invoke-RecoveryGit `
        -Arguments @('stash', 'apply', '--index', $script:stashCommit) `
        -Stage 'Restoring preserved local changes' `
        -AllowFailure
    if ($result.ExitCode -eq 0) {
        Write-RecoveryLog "Restored local changes; safety stash '$script:stashCommit' was retained."
    } else {
        Write-RecoveryLog (
            "Launcher recovery succeeded, but local changes remain only in safety stash " +
            "'$script:stashCommit'."
        ) -Level WARN
    }
}

function Promote-RecoveryLauncher {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string] $PendingDist)

    Stop-HermesRecoveryProcesses -Reason 'launcher promotion'
    $dist = Join-Path $script:root 'dist'
    $backup = Join-Path $script:recoveryRoot 'previous-dist'
    $promoted = $false
    try {
        if (Test-Path -LiteralPath $dist -PathType Container) {
            Move-RecoveryItem -Source $dist -Destination $backup -Stage 'Backing up active launcher'
        }
        Move-RecoveryItem -Source $PendingDist -Destination $dist -Stage 'Promoting repaired launcher'
        $promoted = $true
    } finally {
        if (-not $promoted -and
            -not (Test-Path -LiteralPath $dist -PathType Container) -and
            (Test-Path -LiteralPath $backup -PathType Container)) {
            Move-RecoveryItem -Source $backup -Destination $dist -Stage 'Restoring previous launcher'
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
        throw "Hermes Local Git checkout not found at '$script:root'."
    }

    $scriptPath = if ($PSCommandPath) { [IO.Path]::GetFullPath($PSCommandPath) } else { '' }
    $rootPrefix = $script:root.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    if (-not $env:HERMES_DESKTOP_RECOVERY_RELOCATED -and
        $scriptPath -and
        $scriptPath.StartsWith($rootPrefix, [StringComparison]::OrdinalIgnoreCase)) {
        $temporaryScript = Join-Path $env:TEMP (
            'Repair-Hermes-DesktopUpdater-' + [guid]::NewGuid().ToString('N') + '.ps1'
        )
        Copy-Item -LiteralPath $scriptPath -Destination $temporaryScript -Force
        $env:HERMES_DESKTOP_RECOVERY_RELOCATED = '1'
        try {
            & (Get-Command pwsh.exe -ErrorAction Stop).Source `
                -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass `
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
    $script:logPath = Join-Path $script:recoveryRoot 'recovery.log'
    $script:git = (Get-Command git.exe -ErrorAction Stop).Source
    $script:pwsh = (Get-Command pwsh.exe -ErrorAction Stop).Source

    Write-RecoveryLog "Starting standalone updater recovery for '$script:root'."
    Stop-HermesRecoveryProcesses -Reason 'source recovery'
    Archive-StaleUpdaterState
    Repair-RecoveryGitState
    Preserve-RecoveryWorkingTree

    $null = Invoke-RecoveryGit `
        -Arguments @('fetch', '--no-tags', $TargetRemote, $TargetBranch) `
        -Stage 'Fetching repaired source'
    $targetRef = "$TargetRemote/$TargetBranch"
    $null = Invoke-RecoveryGit `
        -Arguments @('checkout', '-B', $TargetBranch, $targetRef) `
        -Stage "Activating '$targetRef'"
    $null = Invoke-RecoveryGit `
        -Arguments @('reset', '--hard', $targetRef) `
        -Stage "Pinning '$targetRef'"

    $null = Invoke-RecoveryProcess `
        -FilePath $script:pwsh `
        -Arguments @(
            '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
            '-File', (Join-Path $script:root 'Setup-Hermes-Local.ps1'),
            '-SkipModel', '-SkipLlamaBuild', '-SkipLauncherBuild', '-NonInteractive'
        ) `
        -Stage 'Synchronising source and dependencies' `
        -WorkingDirectory $script:root

    $pendingDist = Join-Path $script:recoveryRoot 'pending-dist'
    $null = Invoke-RecoveryProcess `
        -FilePath $script:pwsh `
        -Arguments @(
            '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
            '-File', (Join-Path $script:root 'Build-Hermes-Launcher.ps1'),
            '-DestinationDirectory', $pendingDist, '-NonInteractive'
        ) `
        -Stage 'Building repaired launcher' `
        -WorkingDirectory $script:root

    if (-not (Test-Path -LiteralPath (Join-Path $pendingDist 'Hermes Launcher.exe') -PathType Leaf)) {
        throw "Repaired launcher build is incomplete at '$pendingDist'."
    }

    $launcher = Promote-RecoveryLauncher -PendingDist $pendingDist
    Restore-RecoveryWorkingTree

    $versionPath = Join-Path $script:root 'VERSION.json'
    $version = if (Test-Path -LiteralPath $versionPath -PathType Leaf) {
        [string](Get-Content -Raw -LiteralPath $versionPath | ConvertFrom-Json -Depth 32).product.version
    } else {
        '<unknown>'
    }
    if (-not $SkipLaunch) {
        Start-Process -FilePath $launcher -WorkingDirectory $script:root | Out-Null
    }

    Write-RecoveryLog "Desktop updater recovery completed successfully at version '$version'."
    Write-Host "Recovery evidence: $script:recoveryRoot"
    exit 0
} catch {
    if ($script:logPath) {
        [IO.File]::AppendAllText(
            $script:logPath,
            $_.Exception.ToString() + [Environment]::NewLine,
            [Text.UTF8Encoding]::new($false)
        )
    }
    Write-Host "Desktop updater recovery failed: $($_.Exception.Message)" -ForegroundColor Red
    if ($script:logPath) {
        Write-Host "Recovery log: $script:logPath" -ForegroundColor Yellow
    }
    exit 1
}
