function Invoke-HermesDesktopGitAttempt {
    [CmdletBinding()]
    param(
        [AllowNull()][string] $Repository,
        [Parameter(Mandatory)][string[]] $Arguments
    )

    $gitExecutable = Resolve-HermesDesktopUpdateExecutable -FilePath 'git.exe'
    $workingDirectory = if ($Repository) {
        [IO.Path]::GetFullPath($Repository)
    } else {
        [IO.Path]::GetFullPath($root)
    }

    $startInfo = [Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $gitExecutable
    $startInfo.WorkingDirectory = $workingDirectory
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    if ($Repository) {
        $startInfo.ArgumentList.Add('-C')
        $startInfo.ArgumentList.Add($workingDirectory)
    }
    foreach ($argument in $Arguments) {
        $startInfo.ArgumentList.Add([string]$argument)
    }

    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $startInfo
    try {
        if (-not $process.Start()) {
            throw 'Could not start Git for the Desktop updater.'
        }
        $stdoutTask = $process.StandardOutput.ReadToEndAsync()
        $stderrTask = $process.StandardError.ReadToEndAsync()
        $process.WaitForExit()
        $stdout = $stdoutTask.GetAwaiter().GetResult()
        $stderr = $stderrTask.GetAwaiter().GetResult()
        $exitCode = $process.ExitCode
    } finally {
        $process.Dispose()
    }

    [pscustomobject]@{
        ExitCode = [int]$exitCode
        Output = (@($stdout.TrimEnd(), $stderr.TrimEnd()) |
            Where-Object { -not [string]::IsNullOrWhiteSpace($_) }) -join [Environment]::NewLine
    }
}

function Invoke-HermesDesktopGit {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string[]] $Arguments,
        [switch] $AllowFailure,
        [ValidateRange(0, 5)][int] $MaxAttempts = 0
    )

    $networkCommand = $Arguments.Count -gt 0 -and
        $Arguments[0] -in @('fetch', 'ls-remote')
    if ($MaxAttempts -le 0) {
        $MaxAttempts = if ($networkCommand) { 3 } else { 1 }
    }

    $lastResult = $null
    for ($attempt = 1; $attempt -le $MaxAttempts; $attempt += 1) {
        $lastResult = Invoke-HermesDesktopGitAttempt -Repository $null -Arguments $Arguments
        $null = Add-HermesDesktopUpdateLog -Plan $null -Message (
            "git $($Arguments -join ' ') [attempt $attempt/$MaxAttempts, exit $($lastResult.ExitCode)]" +
            $(if ($lastResult.Output) { "`n$($lastResult.Output)" } else { '' })
        )

        if ($lastResult.ExitCode -eq 0) {
            break
        }
        if ($attempt -lt $MaxAttempts) {
            Start-Sleep -Seconds ([math]::Min(12, [math]::Pow(2, $attempt)))
        }
    }

    if ($lastResult.ExitCode -ne 0 -and -not $AllowFailure) {
        throw (
            "git $($Arguments -join ' ') failed with exit code $($lastResult.ExitCode) after " +
            "$MaxAttempts attempt(s).`n$($lastResult.Output)`nFull update log: " +
            (Get-HermesDesktopUpdateLogPath -Plan $null)
        )
    }

    [pscustomobject]@{
        ExitCode = [int]$lastResult.ExitCode
        Text = [string]$lastResult.Output
        Attempts = $MaxAttempts
    }
}

function Invoke-HermesDesktopNestedSourceGit {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $Repository,
        [Parameter(Mandatory)][string[]] $Arguments,
        [switch] $AllowFailure,
        [ValidateRange(0, 5)][int] $MaxAttempts = 0
    )

    $repositoryPath = [IO.Path]::GetFullPath($Repository)
    $networkCommand = $Arguments.Count -gt 0 -and
        $Arguments[0] -in @('fetch', 'ls-remote')
    if ($MaxAttempts -le 0) {
        $MaxAttempts = if ($networkCommand) { 3 } else { 1 }
    }

    $lastResult = $null
    for ($attempt = 1; $attempt -le $MaxAttempts; $attempt += 1) {
        $lastResult = Invoke-HermesDesktopGitAttempt `
            -Repository $repositoryPath `
            -Arguments $Arguments
        $null = Add-HermesDesktopUpdateLog -Plan $null -Message (
            "git -C $repositoryPath $($Arguments -join ' ') " +
            "[attempt $attempt/$MaxAttempts, exit $($lastResult.ExitCode)]" +
            $(if ($lastResult.Output) { "`n$($lastResult.Output)" } else { '' })
        )

        if ($lastResult.ExitCode -eq 0) {
            break
        }
        if ($attempt -lt $MaxAttempts) {
            Start-Sleep -Seconds ([math]::Min(12, [math]::Pow(2, $attempt)))
        }
    }

    if ($lastResult.ExitCode -ne 0 -and -not $AllowFailure) {
        throw (
            "git -C $repositoryPath $($Arguments -join ' ') failed with exit code " +
            "$($lastResult.ExitCode) after $MaxAttempts attempt(s).`n$($lastResult.Output)" +
            "`nFull update log: $(Get-HermesDesktopUpdateLogPath -Plan $null)"
        )
    }

    [pscustomobject]@{
        ExitCode = [int]$lastResult.ExitCode
        Text = [string]$lastResult.Output
        Attempts = $MaxAttempts
    }
}

function Get-HermesDesktopGitDirectory {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string] $Repository)

    if (-not (Test-Path -LiteralPath $Repository -PathType Container)) {
        return $null
    }

    $repositoryPath = [IO.Path]::GetFullPath($Repository)
    $dotGit = Join-Path $repositoryPath '.git'
    if (Test-Path -LiteralPath $dotGit -PathType Container) {
        return [IO.Path]::GetFullPath($dotGit)
    }

    if (Test-Path -LiteralPath $dotGit -PathType Leaf) {
        $pointer = (Get-Content -Raw -LiteralPath $dotGit).Trim()
        if ($pointer -match '^gitdir:\s*(.+)$') {
            $target = $Matches[1].Trim()
            if (-not [IO.Path]::IsPathFullyQualified($target)) {
                $target = Join-Path $repositoryPath $target
            }
            $resolved = [IO.Path]::GetFullPath($target)
            if (Test-Path -LiteralPath $resolved -PathType Container) {
                return $resolved
            }
        }
    }

    $resolvedGitDirectory = Invoke-HermesDesktopNestedSourceGit `
        -Repository $repositoryPath `
        -Arguments @('rev-parse', '--absolute-git-dir') `
        -AllowFailure
    if (
        $resolvedGitDirectory.ExitCode -eq 0 -and
        -not [string]::IsNullOrWhiteSpace([string]$resolvedGitDirectory.Text)
    ) {
        $resolved = [IO.Path]::GetFullPath(([string]$resolvedGitDirectory.Text).Trim())
        if (Test-Path -LiteralPath $resolved -PathType Container) {
            return $resolved
        }
    }

    $null
}

function Repair-HermesDesktopGitOperationState {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $Repository,
        [string] $Description = 'Git checkout'
    )

    $gitDirectory = Get-HermesDesktopGitDirectory -Repository $Repository
    if (-not $gitDirectory) {
        return [pscustomobject]@{ Repaired = $false; Actions = @() }
    }

    $actions = [System.Collections.Generic.List[string]]::new()
    $operations = @(
        [pscustomobject]@{
            Marker = 'rebase-apply'
            Commands = @(
                [pscustomobject]@{ Arguments = [string[]]@('am', '--abort') },
                [pscustomobject]@{ Arguments = [string[]]@('rebase', '--abort') }
            )
        },
        [pscustomobject]@{
            Marker = 'rebase-merge'
            Commands = @(
                [pscustomobject]@{ Arguments = [string[]]@('rebase', '--abort') }
            )
        },
        [pscustomobject]@{
            Marker = 'MERGE_HEAD'
            Commands = @(
                [pscustomobject]@{ Arguments = [string[]]@('merge', '--abort') }
            )
        },
        [pscustomobject]@{
            Marker = 'CHERRY_PICK_HEAD'
            Commands = @(
                [pscustomobject]@{ Arguments = [string[]]@('cherry-pick', '--abort') }
            )
        },
        [pscustomobject]@{
            Marker = 'REVERT_HEAD'
            Commands = @(
                [pscustomobject]@{ Arguments = [string[]]@('revert', '--abort') }
            )
        },
        [pscustomobject]@{
            Marker = 'BISECT_LOG'
            Commands = @(
                [pscustomobject]@{ Arguments = [string[]]@('bisect', 'reset') }
            )
        }
    )

    foreach ($operation in $operations) {
        $markerPath = Join-Path $gitDirectory ([string]$operation.Marker)
        if (-not (Test-Path -LiteralPath $markerPath)) {
            continue
        }

        foreach ($command in @($operation.Commands)) {
            $arguments = [string[]]$command.Arguments
            $result = Invoke-HermesDesktopNestedSourceGit `
                -Repository $Repository `
                -Arguments $arguments `
                -AllowFailure
            if (-not (Test-Path -LiteralPath $markerPath)) {
                $actions.Add(
                    "Recovered interrupted Git operation with: git $($arguments -join ' ') " +
                    "(exit $($result.ExitCode))"
                )
                break
            }
        }

        if (Test-Path -LiteralPath $markerPath) {
            throw (
                "$Description contains an interrupted Git operation that could not be " +
                "recovered automatically: $markerPath"
            )
        }
    }

    foreach ($lockName in @(
        'index.lock', 'HEAD.lock', 'packed-refs.lock', 'config.lock', 'shallow.lock'
    )) {
        $lockPath = Join-Path $gitDirectory $lockName
        if (-not (Test-Path -LiteralPath $lockPath -PathType Leaf)) {
            continue
        }

        $age = (Get-Date).ToUniversalTime() - (Get-Item -LiteralPath $lockPath).LastWriteTimeUtc
        if ((Test-HermesDesktopGitProcessActive -Repository $Repository) -or $age.TotalSeconds -lt 45) {
            throw (
                "$Description is currently being modified by Git. Wait for it to finish and retry. " +
                "Lock: $lockPath"
            )
        }

        $stamp = (Get-Date).ToUniversalTime().ToString('yyyyMMddTHHmmssfffZ')
        $recoveredPath = "$lockPath.recovered-$stamp"
        Move-Item -LiteralPath $lockPath -Destination $recoveredPath -Force
        $actions.Add("Archived stale Git lock: $recoveredPath")
    }

    if ($actions.Count -gt 0) {
        $null = Add-HermesDesktopUpdateLog -Plan $null -Message (
            "$Description recovery:`n$($actions -join [Environment]::NewLine)"
        )
    }

    [pscustomobject]@{
        Repaired = $actions.Count -gt 0
        Actions = $actions.ToArray()
    }
}
