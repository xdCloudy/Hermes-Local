function Get-HermesDesktopUpdateActivePlan {
    [CmdletBinding()]
    param([AllowNull()][object] $Plan)

    if ($Plan) {
        return $Plan
    }

    $variable = Get-Variable `
        -Name HermesDesktopUpdateActivePlan `
        -Scope Script `
        -ErrorAction SilentlyContinue
    if ($variable) {
        return $variable.Value
    }

    $null
}

function Get-HermesDesktopUpdateLogPath {
    [CmdletBinding()]
    param([AllowNull()][object] $Plan)

    $resolvedPlan = Get-HermesDesktopUpdateActivePlan -Plan $Plan
    if ($resolvedPlan) {
        $configured = Get-HermesDesktopObjectValue `
            -InputObject $resolvedPlan `
            -Name logPath `
            -Default $null
        if ($configured) {
            return [IO.Path]::GetFullPath([string]$configured)
        }
    }

    [IO.Path]::GetFullPath((Join-Path $root 'logs\desktop-update\desktop-self-update.log'))
}

function Add-HermesDesktopUpdateLog {
    [CmdletBinding()]
    param(
        [AllowNull()][object] $Plan,
        [Parameter(Mandatory)][string] $Message
    )

    $path = Get-HermesDesktopUpdateLogPath -Plan $Plan
    [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($path)) | Out-Null
    [IO.File]::AppendAllText(
        $path,
        $Message + $(if ($Message.EndsWith([Environment]::NewLine)) {
            ''
        } else {
            [Environment]::NewLine
        }),
        [Text.UTF8Encoding]::new($false)
    )
    $path
}

function Get-HermesDesktopUpdateOutputTail {
    [CmdletBinding()]
    param(
        [AllowNull()][string] $Text,
        [ValidateRange(1, 200)][int] $Lines = 35
    )

    if ([string]::IsNullOrWhiteSpace($Text)) {
        return ''
    }

    (@($Text -split '\r?\n') | Select-Object -Last $Lines) -join [Environment]::NewLine
}

function Resolve-HermesDesktopUpdateExecutable {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string] $FilePath)

    if ([IO.Path]::IsPathFullyQualified($FilePath)) {
        if (-not (Test-Path -LiteralPath $FilePath -PathType Leaf)) {
            throw "Update executable is missing: $FilePath"
        }
        return [IO.Path]::GetFullPath($FilePath)
    }

    $command = Get-Command $FilePath -ErrorAction SilentlyContinue |
        Select-Object -First 1
    if (-not $command -or -not $command.Source) {
        throw "Update executable is unavailable: $FilePath"
    }

    [string]$command.Source
}

function Invoke-HermesDesktopProcess {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $FilePath,
        [Parameter(Mandatory)][string[]] $Arguments,
        [Parameter(Mandatory)][string] $Description,
        [AllowNull()][object] $Plan,
        [string] $WorkingDirectory,
        [hashtable] $Environment = @{},
        [ValidateRange(1, 5)][int] $MaxAttempts = 1,
        [ValidateRange(0, 30)][int] $RetryDelaySeconds = 2
    )

    $resolvedPlan = Get-HermesDesktopUpdateActivePlan -Plan $Plan
    $resolvedExecutable = Resolve-HermesDesktopUpdateExecutable -FilePath $FilePath
    $resolvedWorkingDirectory = if ($WorkingDirectory) {
        [IO.Path]::GetFullPath($WorkingDirectory)
    } else {
        [IO.Path]::GetFullPath($root)
    }
    if (-not (Test-Path -LiteralPath $resolvedWorkingDirectory -PathType Container)) {
        throw "Update working directory is missing: $resolvedWorkingDirectory"
    }

    $lastExitCode = -1
    $lastOutput = ''
    for ($attempt = 1; $attempt -le $MaxAttempts; $attempt += 1) {
        $startedAt = (Get-Date).ToUniversalTime()
        $commandText = @($resolvedExecutable) + @($Arguments)
        $header = @(
            ''
            ('=' * 88)
            "[$($startedAt.ToString('o'))] $Description — attempt $attempt/$MaxAttempts"
            "Working directory: $resolvedWorkingDirectory"
            "Command: $($commandText -join ' ')"
            ('-' * 88)
        ) -join [Environment]::NewLine
        $logPath = Add-HermesDesktopUpdateLog -Plan $resolvedPlan -Message $header

        $startInfo = [Diagnostics.ProcessStartInfo]::new()
        $startInfo.FileName = $resolvedExecutable
        $startInfo.WorkingDirectory = $resolvedWorkingDirectory
        $startInfo.UseShellExecute = $false
        $startInfo.CreateNoWindow = $true
        $startInfo.RedirectStandardOutput = $true
        $startInfo.RedirectStandardError = $true
        foreach ($argument in $Arguments) {
            $startInfo.ArgumentList.Add([string]$argument)
        }
        foreach ($entry in $Environment.GetEnumerator()) {
            $startInfo.Environment[[string]$entry.Key] = [string]$entry.Value
        }

        $process = [Diagnostics.Process]::new()
        $process.StartInfo = $startInfo
        try {
            if (-not $process.Start()) {
                throw "Could not start $Description."
            }
            $stdoutTask = $process.StandardOutput.ReadToEndAsync()
            $stderrTask = $process.StandardError.ReadToEndAsync()
            $process.WaitForExit()
            $stdout = $stdoutTask.GetAwaiter().GetResult()
            $stderr = $stderrTask.GetAwaiter().GetResult()
            $lastExitCode = $process.ExitCode
        } finally {
            $process.Dispose()
        }

        $lastOutput = (@($stdout.TrimEnd(), $stderr.TrimEnd()) |
            Where-Object { -not [string]::IsNullOrWhiteSpace($_) }) -join [Environment]::NewLine
        $footer = @(
            $lastOutput
            "[exit $lastExitCode; elapsed $([math]::Round(((Get-Date).ToUniversalTime() - $startedAt).TotalSeconds, 2))s]"
        ) -join [Environment]::NewLine
        $null = Add-HermesDesktopUpdateLog -Plan $resolvedPlan -Message $footer

        if ($lastExitCode -eq 0) {
            return [pscustomobject]@{
                ExitCode = 0
                Output = $lastOutput
                Attempts = $attempt
                LogPath = $logPath
            }
        }

        if ($attempt -lt $MaxAttempts) {
            $delay = [math]::Min(30, $RetryDelaySeconds * [math]::Pow(2, $attempt - 1))
            $null = Add-HermesDesktopUpdateLog `
                -Plan $resolvedPlan `
                -Message "Retrying $Description in $delay second(s)."
            Start-Sleep -Seconds $delay
        }
    }

    $tail = Get-HermesDesktopUpdateOutputTail -Text $lastOutput
    $detail = if ($tail) {
        "`n$tail"
    } else {
        ''
    }
    throw (
        "$Description failed with exit code $lastExitCode after $MaxAttempts attempt(s)." +
        "$detail`nFull update log: $(Get-HermesDesktopUpdateLogPath -Plan $resolvedPlan)"
    )
}

function Invoke-HermesDesktopGit {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string[]] $Arguments,
        [switch] $AllowFailure,
        [ValidateRange(1, 5)][int] $MaxAttempts = 0
    )

    $networkCommand = $Arguments.Count -gt 0 -and
        $Arguments[0] -in @('fetch', 'ls-remote')
    if ($MaxAttempts -le 0) {
        $MaxAttempts = if ($networkCommand) { 3 } else { 1 }
    }

    $lastExitCode = -1
    $lastText = ''
    for ($attempt = 1; $attempt -le $MaxAttempts; $attempt += 1) {
        Push-Location $root
        try {
            $output = @(& git @Arguments 2>&1 | ForEach-Object { [string]$_ })
            $lastExitCode = $LASTEXITCODE
        } finally {
            Pop-Location
        }
        $lastText = ($output -join [Environment]::NewLine).Trim()
        $null = Add-HermesDesktopUpdateLog -Plan $null -Message (
            "git $($Arguments -join ' ') [attempt $attempt/$MaxAttempts, exit $lastExitCode]" +
            $(if ($lastText) { "`n$lastText" } else { '' })
        )

        if ($lastExitCode -eq 0) {
            break
        }
        if ($attempt -lt $MaxAttempts) {
            Start-Sleep -Seconds ([math]::Min(12, [math]::Pow(2, $attempt)))
        }
    }

    if ($lastExitCode -ne 0 -and -not $AllowFailure) {
        throw (
            "git $($Arguments -join ' ') failed with exit code $lastExitCode after " +
            "$MaxAttempts attempt(s).`n$lastText`nFull update log: " +
            (Get-HermesDesktopUpdateLogPath -Plan $null)
        )
    }

    [pscustomobject]@{
        ExitCode = $lastExitCode
        Text = $lastText
        Attempts = $MaxAttempts
    }
}

function Invoke-HermesDesktopNestedSourceGit {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $Repository,
        [Parameter(Mandatory)][string[]] $Arguments,
        [switch] $AllowFailure,
        [ValidateRange(1, 5)][int] $MaxAttempts = 0
    )

    $networkCommand = $Arguments.Count -gt 0 -and
        $Arguments[0] -in @('fetch', 'ls-remote')
    if ($MaxAttempts -le 0) {
        $MaxAttempts = if ($networkCommand) { 3 } else { 1 }
    }

    $lastExitCode = -1
    $lastText = ''
    for ($attempt = 1; $attempt -le $MaxAttempts; $attempt += 1) {
        $output = @(
            & git -C $Repository @Arguments 2>&1 |
                ForEach-Object { [string]$_ }
        )
        $lastExitCode = $LASTEXITCODE
        $lastText = ($output -join [Environment]::NewLine).Trim()
        $null = Add-HermesDesktopUpdateLog -Plan $null -Message (
            "git -C $Repository $($Arguments -join ' ') " +
            "[attempt $attempt/$MaxAttempts, exit $lastExitCode]" +
            $(if ($lastText) { "`n$lastText" } else { '' })
        )
        if ($lastExitCode -eq 0) {
            break
        }
        if ($attempt -lt $MaxAttempts) {
            Start-Sleep -Seconds ([math]::Min(12, [math]::Pow(2, $attempt)))
        }
    }

    if ($lastExitCode -ne 0 -and -not $AllowFailure) {
        throw (
            "git -C $Repository $($Arguments -join ' ') failed with exit code " +
            "$lastExitCode after $MaxAttempts attempt(s).`n$lastText`nFull update log: " +
            (Get-HermesDesktopUpdateLogPath -Plan $null)
        )
    }

    [pscustomobject]@{
        ExitCode = $lastExitCode
        Text = $lastText
        Attempts = $MaxAttempts
    }
}

function Get-HermesDesktopGitDirectory {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string] $Repository)

    if (-not (Test-Path -LiteralPath $Repository -PathType Container)) {
        return $null
    }
    $resolved = Invoke-HermesDesktopNestedSourceGit `
        -Repository $Repository `
        -Arguments @('rev-parse', '--git-dir') `
        -AllowFailure
    if ($resolved.ExitCode -ne 0 -or -not $resolved.Text) {
        return $null
    }

    if ([IO.Path]::IsPathFullyQualified($resolved.Text)) {
        return [IO.Path]::GetFullPath($resolved.Text)
    }
    [IO.Path]::GetFullPath((Join-Path $Repository $resolved.Text))
}

function Test-HermesDesktopGitProcessActive {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string] $Repository)

    try {
        $needle = [IO.Path]::GetFullPath($Repository)
        foreach ($process in @(Get-CimInstance Win32_Process -ErrorAction Stop)) {
            if (
                [string]$process.Name -match '^(?:git|git-lfs)(?:\.exe)?$' -and
                [string]$process.CommandLine -and
                [string]$process.CommandLine -like "*$needle*"
            ) {
                return $true
            }
        }
    } catch {
        # If process inspection is unavailable, stale-age checks remain conservative.
    }
    $false
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
        @{ Marker = 'rebase-apply'; Commands = @(@('am', '--abort'), @('rebase', '--abort')) },
        @{ Marker = 'rebase-merge'; Commands = @(@('rebase', '--abort')) },
        @{ Marker = 'MERGE_HEAD'; Commands = @(@('merge', '--abort')) },
        @{ Marker = 'CHERRY_PICK_HEAD'; Commands = @(@('cherry-pick', '--abort')) },
        @{ Marker = 'REVERT_HEAD'; Commands = @(@('revert', '--abort')) },
        @{ Marker = 'BISECT_LOG'; Commands = @(@('bisect', 'reset')) }
    )

    foreach ($operation in $operations) {
        $markerPath = Join-Path $gitDirectory ([string]$operation.Marker)
        if (-not (Test-Path -LiteralPath $markerPath)) {
            continue
        }
        foreach ($command in @($operation.Commands)) {
            $result = Invoke-HermesDesktopNestedSourceGit `
                -Repository $Repository `
                -Arguments @($command) `
                -AllowFailure
            if ($result.ExitCode -eq 0 -and -not (Test-Path -LiteralPath $markerPath)) {
                $actions.Add("Recovered interrupted Git operation with: git $($command -join ' ')")
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

function Invoke-HermesDesktopSourceSetupProcess {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string] $Description)

    Invoke-HermesDesktopProcess -FilePath 'pwsh.exe' -Arguments @(
        '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
        '-File', (Join-Path $root 'Setup-Hermes-Local.ps1'),
        '-SkipModel',
        '-SkipLlamaBuild',
        '-SkipHermesDependencies',
        '-SkipLauncherBuild',
        '-NonInteractive'
    ) -Description $Description -Plan $null
}

function Invoke-HermesDesktopSetup {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string] $Description)

    $plan = Get-HermesDesktopUpdateActivePlan -Plan $null
    $source = Join-Path $root 'source\hermes-agent'
    $firstError = $null

    try {
        $null = Invoke-HermesDesktopSourceSetupProcess -Description $Description
    } catch {
        $firstError = $_
        if (Test-Path -LiteralPath (Join-Path $source '.git')) {
            $null = Repair-HermesDesktopGitOperationState `
                -Repository $source `
                -Description 'Hermes Agent source checkout'
        }
        $null = Add-HermesDesktopUpdateLog -Plan $plan -Message (
            "Retrying source synchronisation after checkout recovery. First failure: " +
            $firstError.Exception.Message
        )
        try {
            $null = Invoke-HermesDesktopSourceSetupProcess -Description $Description
            $firstError = $null
        } catch {
            $secondError = $_
            $stashState = if ($plan) {
                Join-Path ([string]$plan.stagingRoot) 'hermes-agent-working-tree-stash.json'
            } else {
                $null
            }
            $canReclone = (
                $plan -and
                (Test-Path -LiteralPath $source -PathType Container) -and
                -not ($stashState -and (Test-Path -LiteralPath $stashState -PathType Leaf))
            )
            if (-not $canReclone) {
                throw $secondError
            }

            $backup = Join-Path ([string]$plan.stagingRoot) (
                'hermes-agent-recovery-' + (Get-Date).ToUniversalTime().ToString('yyyyMMddTHHmmssfffZ')
            )
            $null = Add-HermesDesktopUpdateLog -Plan $plan -Message (
                "Preserving the unusable clean Hermes Agent checkout at $backup and retrying from a fresh clone."
            )
            Move-Item -LiteralPath $source -Destination $backup
            try {
                $null = Invoke-HermesDesktopSourceSetupProcess -Description $Description
                Set-HermesDesktopObjectValue `
                    -InputObject $plan `
                    -Name recoveredNestedSourceBackup `
                    -Value $backup
                Write-HermesDesktopUpdateJson -Path ([string]$plan.planPath) -Value $plan
            } catch {
                $failedFresh = if (Test-Path -LiteralPath $source) {
                    Join-Path ([string]$plan.stagingRoot) (
                        'hermes-agent-failed-fresh-' + (Get-Date).ToUniversalTime().ToString('yyyyMMddTHHmmssfffZ')
                    )
                } else {
                    $null
                }
                if ($failedFresh) {
                    Move-Item -LiteralPath $source -Destination $failedFresh -Force
                }
                if (-not (Test-Path -LiteralPath $source)) {
                    Move-Item -LiteralPath $backup -Destination $source
                }
                throw
            }
        }
    }

    $packageLock = Join-Path $source 'package-lock.json'
    if (Test-Path -LiteralPath $packageLock -PathType Leaf) {
        $null = Invoke-HermesDesktopProcess -FilePath 'npm.cmd' -Arguments @(
            '--prefix', $source,
            'ci',
            '--cache', (Join-Path $root 'cache\npm'),
            '--prefer-offline',
            '--no-audit',
            '--fund=false'
        ) -Description "$Description Node dependency synchronisation" `
          -Plan $plan `
          -WorkingDirectory $source `
          -MaxAttempts 3 `
          -RetryDelaySeconds 3
    }
}

function Invoke-HermesDesktopRuntimeSync {
    [CmdletBinding()]
    param()

    $scriptPath = Join-Path $root 'scripts\setup\Sync-HermesPythonRuntime.ps1'
    if (-not (Test-Path -LiteralPath $scriptPath -PathType Leaf)) {
        throw "Deferred Python runtime synchronizer is missing: $scriptPath"
    }

    $null = Invoke-HermesDesktopProcess -FilePath 'pwsh.exe' -Arguments @(
        '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
        '-File', $scriptPath,
        '-NonInteractive'
    ) -Description 'Hermes Local deferred Python runtime synchronisation' `
      -Plan $null `
      -MaxAttempts 3 `
      -RetryDelaySeconds 4
}

$reliableStageCoreVariable = Get-Variable `
    -Name HermesDesktopReliableStageCore `
    -Scope Script `
    -ErrorAction SilentlyContinue
if (-not $reliableStageCoreVariable) {
    Set-Variable `
        -Name HermesDesktopReliableStageCore `
        -Scope Script `
        -Value ${function:Invoke-HermesDesktopUpdateStage}
}

function Invoke-HermesDesktopUpdateStage {
    [CmdletBinding()]
    param([Parameter(Mandatory)][object] $Plan)

    $priorPlan = Get-HermesDesktopUpdateActivePlan -Plan $null
    $script:HermesDesktopUpdateActivePlan = $Plan
    try {
        $null = Add-HermesDesktopUpdateLog -Plan $Plan -Message (
            "Starting Desktop update operation $($Plan.operationId): " +
            "$($Plan.previousCommit) -> $($Plan.targetCommit)"
        )
        $null = Repair-HermesDesktopGitOperationState `
            -Repository ([string]$Plan.root) `
            -Description 'Hermes Local checkout'
        $nestedSource = Join-Path ([string]$Plan.root) 'source\hermes-agent'
        if (Test-Path -LiteralPath (Join-Path $nestedSource '.git')) {
            $null = Repair-HermesDesktopGitOperationState `
                -Repository $nestedSource `
                -Description 'Hermes Agent source checkout'
        }

        & $script:HermesDesktopReliableStageCore -Plan $Plan
    } finally {
        $script:HermesDesktopUpdateActivePlan = $priorPlan
    }
}

$reliablePromotionCoreVariable = Get-Variable `
    -Name HermesDesktopReliablePromotionCore `
    -Scope Script `
    -ErrorAction SilentlyContinue
if (-not $reliablePromotionCoreVariable) {
    Set-Variable `
        -Name HermesDesktopReliablePromotionCore `
        -Scope Script `
        -Value ${function:Promote-HermesDesktopPendingLauncher}
}

function Promote-HermesDesktopPendingLauncher {
    [CmdletBinding()]
    param([Parameter(Mandatory)][object] $Plan)

    $priorPlan = Get-HermesDesktopUpdateActivePlan -Plan $null
    $script:HermesDesktopUpdateActivePlan = $Plan
    try {
        & $script:HermesDesktopReliablePromotionCore -Plan $Plan
    } finally {
        $script:HermesDesktopUpdateActivePlan = $priorPlan
    }
}
