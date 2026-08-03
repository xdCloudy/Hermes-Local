[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidateSet('Check', 'Apply', 'Rollback', 'Helper')]
    [string] $Mode,

    [ValidateSet('development', 'stable', 'beta', 'pinned')]
    [string] $Channel = 'development',

    [ValidatePattern('^[0-9a-fA-F]{40}$')]
    [string] $TargetCommit,

    [ValidateRange(0, 2147483647)]
    [int] $ParentPid = 0,

    [string] $PlanPath,

    [switch] $NonInteractive
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$rootHint = if ($PlanPath -and (Test-Path -LiteralPath $PlanPath -PathType Leaf)) {
    [string](Get-Content -Raw -LiteralPath $PlanPath | ConvertFrom-Json -Depth 64).root
} elseif ($env:HERMES_LOCAL_ROOT) {
    [string]$env:HERMES_LOCAL_ROOT
} else {
    $PSScriptRoot
}
$root = [System.IO.Path]::GetFullPath($rootHint)

Import-Module (Join-Path $root 'scripts\Hermes-DesktopUpdate.psm1') -Force

function Invoke-HermesDesktopGit {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string[]] $Arguments,
        [switch] $AllowFailure
    )

    Push-Location $root
    try {
        $output = @(& git @Arguments 2>&1 | ForEach-Object { [string]$_ })
        $exitCode = $LASTEXITCODE
    } finally {
        Pop-Location
    }
    $text = ($output -join [Environment]::NewLine).Trim()
    if ($exitCode -ne 0 -and -not $AllowFailure) {
        throw "git $($Arguments -join ' ') failed with exit code $exitCode.`n$text"
    }
    [pscustomobject]@{ ExitCode = $exitCode; Text = $text }
}

function Get-HermesDesktopSemverTarget {
    [CmdletBinding()]
    param([Parameter(Mandatory)][ValidateSet('stable', 'beta')][string] $ReleaseChannel)

    $lines = (Invoke-HermesDesktopGit -Arguments @(
        'ls-remote', '--tags', '--refs', 'origin', 'refs/tags/v*'
    )).Text -split '\r?\n'
    $records = foreach ($line in $lines) {
        if ($line -notmatch '^([0-9a-fA-F]{40})\s+refs/tags/(v(\d+)\.(\d+)\.(\d+)([-+][A-Za-z0-9.-]+)?)$') {
            continue
        }
        $suffix = [string]$Matches[6]
        if ($ReleaseChannel -eq 'stable' -and $suffix) {
            continue
        }
        [pscustomobject]@{
            Commit = $Matches[1].ToLowerInvariant()
            Tag = $Matches[2]
            Major = [int]$Matches[3]
            Minor = [int]$Matches[4]
            Patch = [int]$Matches[5]
            Prerelease = [bool]$suffix
        }
    }
    $selected = $records |
        Sort-Object Major, Minor, Patch, @{ Expression = { if ($_.Prerelease) { 0 } else { 1 } } } -Descending |
        Select-Object -First 1
    if (-not $selected) {
        throw "No trusted $ReleaseChannel Hermes Local release tag is available."
    }
    $selected
}

function Get-HermesDesktopUpdateTarget {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $RequestedChannel,
        [string] $RequestedCommit
    )

    if ($RequestedChannel -eq 'pinned') {
        if (-not $RequestedCommit) {
            throw 'Pinned Hermes Local updates require -TargetCommit.'
        }
        return [pscustomobject]@{
            Branch = 'pinned'
            Commit = $RequestedCommit.ToLowerInvariant()
            Release = $null
        }
    }
    if ($RequestedChannel -eq 'development') {
        $line = (Invoke-HermesDesktopGit -Arguments @(
            'ls-remote', '--heads', 'origin', 'refs/heads/main'
        )).Text
        $commit = (($line -split '\s+')[0]).ToLowerInvariant()
        if ($commit -notmatch '^[0-9a-f]{40}$') {
            throw 'The trusted main branch did not resolve to a commit.'
        }
        return [pscustomobject]@{ Branch = 'main'; Commit = $commit; Release = $null }
    }

    $release = Get-HermesDesktopSemverTarget -ReleaseChannel $RequestedChannel
    [pscustomobject]@{ Branch = $release.Tag; Commit = $release.Commit; Release = $release.Tag }
}

function Get-HermesDesktopUpdateStatus {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $RequestedChannel,
        [string] $RequestedCommit
    )

    if (-not (Test-Path -LiteralPath (Join-Path $root '.git') -PathType Container)) {
        throw 'Hermes Local self-update requires a Git installation checkout.'
    }
    $origin = (Invoke-HermesDesktopGit -Arguments @('remote', 'get-url', 'origin')).Text
    if (-not (Test-HermesDesktopUpdateOrigin -Origin $origin)) {
        throw "Refusing updates from unexpected origin '$origin'."
    }

    $current = (Invoke-HermesDesktopGit -Arguments @('rev-parse', 'HEAD')).Text.ToLowerInvariant()
    $branch = (Invoke-HermesDesktopGit -Arguments @('branch', '--show-current') -AllowFailure).Text
    $dirty = (Invoke-HermesDesktopGit -Arguments @(
        'status', '--porcelain', '--untracked-files=no'
    ) -AllowFailure).Text
    $target = Get-HermesDesktopUpdateTarget -RequestedChannel $RequestedChannel -RequestedCommit $RequestedCommit

    $fetch = Invoke-HermesDesktopGit -Arguments @(
        'fetch', '--no-tags', 'origin', $target.Commit
    ) -AllowFailure
    if ($fetch.ExitCode -ne 0) {
        throw "Could not download update metadata for $($target.Commit). $($fetch.Text)"
    }

    $ancestor = Invoke-HermesDesktopGit -Arguments @(
        'merge-base', '--is-ancestor', $current, $target.Commit
    ) -AllowFailure
    $behind = if ($current -eq $target.Commit) {
        0
    } elseif ($ancestor.ExitCode -eq 0) {
        [int](Invoke-HermesDesktopGit -Arguments @(
            'rev-list', '--count', "$current..$($target.Commit)"
        )).Text
    } else {
        1
    }

    $versionPath = Join-Path $root 'VERSION.json'
    $version = if (Test-Path -LiteralPath $versionPath -PathType Leaf) {
        Get-Content -Raw -LiteralPath $versionPath | ConvertFrom-Json -Depth 32
    } else {
        $null
    }

    [ordered]@{
        supported = $true
        branch = $target.Branch
        localBranch = $branch
        channel = $RequestedChannel
        currentVersion = if ($version) { [string]$version.product.version } else { $current.Substring(0, 12) }
        currentSha = $current
        targetSha = $target.Commit
        behind = $behind
        updateAvailable = $current -ne $target.Commit
        dirty = [bool]$dirty
        restartRequired = $current -ne $target.Commit
        commits = @()
        releaseNotes = if ($target.Release) {
            "https://github.com/xdCloudy/Hermes-Local/releases/tag/$($target.Release)"
        } else {
            "https://github.com/xdCloudy/Hermes-Local/compare/$current...$($target.Commit)"
        }
        message = if ($current -eq $target.Commit) {
            'Hermes Local is up to date.'
        } elseif ($dirty) {
            'An update is available, but tracked or staged source changes must be committed or stashed first.'
        } else {
            'A Hermes Local application update is available.'
        }
        fetchedAt = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
    }
}

function Copy-HermesDesktopUpdateHelper {
    [CmdletBinding()]
    param([Parameter(Mandatory)][object] $Plan)

    $helperRoot = Join-Path ([System.IO.Path]::GetTempPath()) "Hermes-Local-Updater-$($Plan.operationId)"
    [System.IO.Directory]::CreateDirectory($helperRoot) | Out-Null
    $helperScript = Join-Path $helperRoot 'Invoke-Hermes-DesktopUpdate.ps1'
    $helperModule = Join-Path $helperRoot 'Hermes-DesktopUpdate.psm1'
    Copy-Item -LiteralPath $PSCommandPath -Destination $helperScript -Force
    Copy-Item -LiteralPath (Join-Path $root 'scripts\Hermes-DesktopUpdate.psm1') -Destination $helperModule -Force
    [pscustomobject]@{ Root = $helperRoot; Script = $helperScript; Module = $helperModule }
}

function Start-HermesDesktopUpdateHelper {
    [CmdletBinding()]
    param([Parameter(Mandatory)][object] $Plan)

    $helper = Copy-HermesDesktopUpdateHelper -Plan $Plan
    $pwsh = (Get-Process -Id $PID -ErrorAction Stop).Path
    $arguments = @(
        '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
        '-File', $helper.Script,
        '-Mode', 'Helper',
        '-PlanPath', [string]$Plan.planPath,
        '-NonInteractive'
    )
    $process = Start-Process -FilePath $pwsh -ArgumentList $arguments -WorkingDirectory $helper.Root -WindowStyle Hidden -PassThru
    [ordered]@{
        operationId = [string]$Plan.operationId
        pid = $process.Id
        planPath = [string]$Plan.planPath
        taskId = if ($Plan.taskId) { [string]$Plan.taskId } else { $null }
    }
}

function Invoke-HermesDesktopProcess {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $FilePath,
        [Parameter(Mandatory)][string[]] $Arguments,
        [Parameter(Mandatory)][string] $Description
    )

    & $FilePath @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$Description failed with exit code $LASTEXITCODE."
    }
}

function Restore-PreviousLauncher {
    [CmdletBinding()]
    param([Parameter(Mandatory)][object] $Plan)

    $dist = Join-Path ([string]$Plan.root) 'dist'
    if (-not (Test-Path -LiteralPath ([string]$Plan.previousDist) -PathType Container)) {
        throw 'The previous launcher snapshot is missing.'
    }
    [System.IO.Directory]::CreateDirectory($dist) | Out-Null
    Get-ChildItem -LiteralPath $dist -Force -ErrorAction SilentlyContinue |
        Remove-Item -Recurse -Force
    Get-ChildItem -LiteralPath ([string]$Plan.previousDist) -Force |
        Copy-Item -Destination $dist -Recurse -Force
}

function Start-HermesKnownGoodLauncher {
    [CmdletBinding()]
    param([Parameter(Mandatory)][object] $Plan)

    $launcher = Join-Path ([string]$Plan.root) 'dist\Hermes Launcher.exe'
    if (Test-Path -LiteralPath $launcher -PathType Leaf) {
        Start-Process -FilePath $launcher -WorkingDirectory ([string]$Plan.root) | Out-Null
        return $true
    }
    $false
}

function Invoke-HermesDesktopUpdateHelperMode {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string] $Path)

    $plan = Get-Content -Raw -LiteralPath $Path | ConvertFrom-Json -Depth 64
    $script:root = [System.IO.Path]::GetFullPath([string]$plan.root)
    $null = Assert-HermesDesktopUpdatePath -Root $root -Path ([string]$plan.stagingRoot) -Description 'Staging root'
    $null = Assert-HermesDesktopUpdatePath -Root $root -Path ([string]$plan.progressPath) -Description 'Progress path'
    $null = Assert-HermesDesktopUpdatePath -Root $root -Path ([string]$plan.resultPath) -Description 'Result path'
    $lockPath = $null
    $failure = $null
    try {
        $lockPath = Enter-HermesDesktopUpdateLock -Root $root -OperationId ([string]$plan.operationId)
        Write-HermesDesktopUpdateProgress -Plan $plan -Stage waiting-for-restart -Status running -Message 'Waiting for Hermes Launcher to close.' -Percent 5 -Failure $null -Result $null | Out-Null
        Wait-HermesDesktopUpdateParent -ParentPid ([int]$plan.parentPid) | Out-Null

        $dist = Join-Path $root 'dist'
        if (-not [bool]$plan.rollbackOnly) {
            [System.IO.Directory]::CreateDirectory([string]$plan.previousDist) | Out-Null
            if (Test-Path -LiteralPath $dist -PathType Container) {
                Get-ChildItem -LiteralPath $dist -Force |
                    Copy-Item -Destination ([string]$plan.previousDist) -Recurse -Force
            }
        }

        $trackedChanges = (Invoke-HermesDesktopGit -Arguments @(
            'status', '--porcelain', '--untracked-files=no'
        ) -AllowFailure).Text
        if ($trackedChanges) {
            throw 'Tracked or staged working-tree changes appeared after staging; the update was not applied.'
        }

        Write-HermesDesktopUpdateProgress -Plan $plan -Stage installing -Status running -Message 'Pinning the trusted Hermes Local source revision.' -Percent 20 -Failure $null -Result $null | Out-Null
        Invoke-HermesDesktopGit -Arguments @('fetch', '--no-tags', 'origin', [string]$plan.targetCommit) | Out-Null
        if (-not [bool]$plan.rollbackOnly) {
            $fastForward = Invoke-HermesDesktopGit -Arguments @(
                'merge-base', '--is-ancestor', [string]$plan.previousCommit, [string]$plan.targetCommit
            ) -AllowFailure
            if ($fastForward.ExitCode -ne 0 -and [string]$plan.channel -ne 'pinned') {
                throw 'The selected update is not a fast-forward from the installed revision.'
            }
        }
        Invoke-HermesDesktopGit -Arguments @('reset', '--hard', [string]$plan.targetCommit) | Out-Null

        Write-HermesDesktopUpdateProgress -Plan $plan -Stage preparing -Status running -Message 'Synchronising the pinned Hermes Agent integration.' -Percent 35 -Failure $null -Result $null | Out-Null
        Invoke-HermesDesktopProcess -FilePath 'pwsh.exe' -Arguments @(
            '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
            '-File', (Join-Path $root 'Setup-Hermes-Local.ps1'),
            '-SkipModel', '-SkipLlamaBuild', '-SkipLauncherBuild', '-NonInteractive'
        ) -Description 'Hermes Local source synchronisation'

        if ([bool]$plan.rollbackOnly) {
            Restore-PreviousLauncher -Plan $plan
        } else {
            Write-HermesDesktopUpdateProgress -Plan $plan -Stage installing -Status running -Message 'Building and validating the staged launcher.' -Percent 55 -Failure $null -Result $null | Out-Null
            # Authoritative command: Update-Hermes-Local.ps1 -Component Launcher
            Invoke-HermesDesktopProcess -FilePath 'pwsh.exe' -Arguments @(
                '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
                '-File', (Join-Path $root 'Update-Hermes-Local.ps1'),
                '-Mode', 'Apply', '-Component', 'Launcher', '-Caller', 'Desktop', '-NonInteractive'
            ) -Description 'Hermes Local launcher update'
        }

        $launcher = Join-Path $root 'dist\Hermes Launcher.exe'
        if (-not (Test-Path -LiteralPath $launcher -PathType Leaf)) {
            throw 'The updated launcher was not produced.'
        }

        $result = [ordered]@{
            status = 'succeeded'
            previousCommit = [string]$plan.previousCommit
            currentCommit = [string]$plan.targetCommit
            launcherPath = $launcher
            relaunched = Start-HermesKnownGoodLauncher -Plan $plan
        }
        Write-HermesDesktopUpdateJson -Path ([string]$plan.resultPath) -Value $result
        Write-HermesDesktopUpdateProgress -Plan $plan -Stage completed -Status succeeded -Message 'Hermes Local updated and relaunched successfully.' -Percent 100 -Failure $null -Result $result | Out-Null
        return
    } catch {
        $failure = $_
        try {
            Write-HermesDesktopUpdateProgress -Plan $plan -Stage rolling-back -Status running -Message 'Restoring the previous known-good launcher and source revision.' -Percent 80 -Failure $null -Result $null | Out-Null
            Invoke-HermesDesktopGit -Arguments @('reset', '--hard', [string]$plan.previousCommit) | Out-Null
            Invoke-HermesDesktopProcess -FilePath 'pwsh.exe' -Arguments @(
                '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
                '-File', (Join-Path $root 'Setup-Hermes-Local.ps1'),
                '-SkipModel', '-SkipLlamaBuild', '-SkipLauncherBuild', '-NonInteractive'
            ) -Description 'Hermes Local rollback source synchronisation'
            Restore-PreviousLauncher -Plan $plan
            $relaunched = Start-HermesKnownGoodLauncher -Plan $plan
            $result = [ordered]@{
                status = 'rolled-back'
                failedStage = 'desktop-self-update'
                previousCommit = [string]$plan.previousCommit
                restoredLauncher = $true
                relaunched = $relaunched
            }
            Write-HermesDesktopUpdateJson -Path ([string]$plan.resultPath) -Value $result
            Write-HermesDesktopUpdateProgress -Plan $plan -Stage rolled-back -Status rolled-back -Message 'The update failed and the previous version was restored.' -Percent 100 -Failure ([ordered]@{
                code = 'desktop-update-rolled-back'
                message = $failure.Exception.Message
            }) -Result $result | Out-Null
        } catch {
            $rollbackFailure = $_
            Write-HermesDesktopUpdateProgress -Plan $plan -Stage failed -Status failed -Message 'Update and automatic rollback failed.' -Percent 100 -Failure ([ordered]@{
                code = 'desktop-update-and-rollback-failed'
                message = $failure.Exception.Message
                rollback = $rollbackFailure.Exception.Message
            }) -Result $null | Out-Null
        }
    } finally {
        Exit-HermesDesktopUpdateLock -LockPath $lockPath
    }
}

try {
    if ($Mode -eq 'Helper') {
        if (-not $PlanPath) { throw 'Helper mode requires -PlanPath.' }
        Invoke-HermesDesktopUpdateHelperMode -Path ([System.IO.Path]::GetFullPath($PlanPath))
        exit 0
    }

    if (-not (Test-Path -LiteralPath (Join-Path $root 'scripts\Hermes-DesktopUpdate.psm1') -PathType Leaf)) {
        throw 'Hermes Desktop update support is not installed.'
    }

    if ($Mode -eq 'Check') {
        try {
            $status = Get-HermesDesktopUpdateStatus -RequestedChannel $Channel -RequestedCommit $TargetCommit
        } catch {
            $status = [ordered]@{
                supported = $true
                branch = $Channel
                error = 'check-failed'
                message = $_.Exception.Message
                fetchedAt = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
            }
        }
        Write-Output (ConvertTo-HermesDesktopUpdateMarker -Name status -Value $status)
        exit 0
    }

    if ($Mode -eq 'Rollback') {
        $latest = Get-ChildItem -LiteralPath (Join-Path $root 'build\updates\desktop-staging') -Directory -ErrorAction SilentlyContinue |
            Sort-Object LastWriteTimeUtc -Descending |
            Where-Object { Test-Path -LiteralPath (Join-Path $_.FullName 'plan.json') -PathType Leaf } |
            Select-Object -First 1
        if (-not $latest) {
            throw 'No staged Hermes Local Desktop update is available to roll back.'
        }
        $prior = Get-Content -Raw -LiteralPath (Join-Path $latest.FullName 'plan.json') | ConvertFrom-Json -Depth 64
        $plan = New-HermesDesktopUpdatePlan -Root $root -CurrentCommit ([string]$prior.targetCommit) -TargetCommit ([string]$prior.previousCommit) -Channel ([string]$prior.channel) -CurrentBranch ([string]$prior.previousBranch) -ParentPid $ParentPid -TaskId $env:HERMES_LOCAL_TASK_ID -RollbackOnly
        $plan['planPath'] = Join-Path ([string]$plan.stagingRoot) 'plan.json'
        $plan['previousDist'] = [string]$prior.previousDist
    } else {
        $status = Get-HermesDesktopUpdateStatus -RequestedChannel $Channel -RequestedCommit $TargetCommit
        if ([bool]$status.dirty) {
            throw 'Tracked or staged working-tree changes are present. Commit or stash them before updating.'
        }
        if (-not [bool]$status.updateAvailable) {
            $result = [ordered]@{ ok = $true; updated = $false; message = 'Hermes Local is already up to date.' }
            Write-Output (ConvertTo-HermesDesktopUpdateMarker -Name result -Value $result)
            exit 0
        }
        Assert-HermesDesktopUpdateDiskSpace -Root $root | Out-Null
        $plan = New-HermesDesktopUpdatePlan -Root $root -CurrentCommit ([string]$status.currentSha) -TargetCommit ([string]$status.targetSha) -Channel $Channel -CurrentBranch ([string]$status.localBranch) -ParentPid $ParentPid -TaskId $env:HERMES_LOCAL_TASK_ID
        $plan['planPath'] = Join-Path ([string]$plan.stagingRoot) 'plan.json'
    }

    [System.IO.Directory]::CreateDirectory([string]$plan.stagingRoot) | Out-Null
    Write-HermesDesktopUpdateJson -Path ([string]$plan.planPath) -Value $plan
    Write-HermesDesktopUpdateProgress -Plan $plan -Stage waiting-for-restart -Status staged -Message 'Update staged. Restarting Hermes Launcher to install.' -Percent 0 -Failure $null -Result $null | Out-Null
    $handoff = Start-HermesDesktopUpdateHelper -Plan $plan
    Write-Output (ConvertTo-HermesDesktopUpdateMarker -Name helper -Value $handoff)
    exit 0
} catch {
    $result = [ordered]@{
        ok = $false
        error = 'desktop-update-failed'
        message = $_.Exception.Message
    }
    Write-Output (ConvertTo-HermesDesktopUpdateMarker -Name result -Value $result)
    exit 1
}
