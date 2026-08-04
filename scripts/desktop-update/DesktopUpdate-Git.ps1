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

    [pscustomobject]@{
        ExitCode = $exitCode
        Text = $text
    }
}

function Get-HermesDesktopWorkingTreeChanges {
    [CmdletBinding()]
    param()

    (Invoke-HermesDesktopGit -Arguments @(
        'status', '--porcelain=v1', '--untracked-files=all'
    ) -AllowFailure).Text
}

function Write-HermesDesktopWorkingTreeStashState {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][object] $Plan,
        [Parameter(Mandatory)][object] $Value
    )

    $path = Join-Path ([string]$Plan.stagingRoot) 'working-tree-stash.json'
    try {
        Write-HermesDesktopUpdateJson -Path $path -Value $Value
    } catch {
        # The Git stash remains the source of truth if diagnostic persistence fails.
    }
}

function Save-HermesDesktopWorkingTree {
    [CmdletBinding()]
    param([Parameter(Mandatory)][object] $Plan)

    $changes = Get-HermesDesktopWorkingTreeChanges
    if (-not $changes) {
        return $null
    }

    $message = "hermes-desktop-update:$($Plan.operationId)"
    $before = Invoke-HermesDesktopGit -Arguments @(
        'rev-parse', '--verify', 'refs/stash'
    ) -AllowFailure
    $stash = Invoke-HermesDesktopGit -Arguments @(
        'stash', 'push', '--include-untracked', '--message', $message
    ) -AllowFailure
    if ($stash.ExitCode -ne 0) {
        throw "Could not preserve local working-tree changes before updating. $($stash.Text)"
    }

    $after = Invoke-HermesDesktopGit -Arguments @(
        'rev-parse', '--verify', 'refs/stash'
    ) -AllowFailure
    if ($after.ExitCode -ne 0 -or $after.Text -notmatch '^[0-9a-fA-F]{40}$') {
        throw 'Git reported a successful stash, but the updater could not identify the preserved changes.'
    }
    if ($before.ExitCode -eq 0 -and $before.Text -eq $after.Text) {
        throw 'Git did not create a new stash for the local working-tree changes.'
    }

    $remaining = Get-HermesDesktopWorkingTreeChanges
    if ($remaining) {
        throw "Automatic stashing left source changes in the working tree.`n$remaining"
    }

    $record = [ordered]@{
        schemaVersion = 1
        operationId = [string]$Plan.operationId
        commit = $after.Text.ToLowerInvariant()
        message = $message
        createdAt = (Get-Date).ToUniversalTime().ToString('o')
        includesTracked = $true
        includesStaged = $true
        includesUntracked = $true
        ignoredFilesUntouched = $true
        restored = $false
        retained = $true
    }
    Write-HermesDesktopWorkingTreeStashState -Plan $Plan -Value $record
    [pscustomobject]$record
}

function Restore-HermesDesktopWorkingTree {
    [CmdletBinding()]
    param(
        [AllowNull()][object] $Stash,
        [Parameter(Mandatory)][string] $Revision,
        [Parameter(Mandatory)][object] $Plan
    )

    if (-not $Stash) {
        return [pscustomobject]@{
            Restored = $true
            Retained = $false
            Commit = $null
            Message = $null
        }
    }

    $apply = Invoke-HermesDesktopGit -Arguments @(
        'stash', 'apply', '--index', [string]$Stash.commit
    ) -AllowFailure
    if ($apply.ExitCode -eq 0) {
        $record = [ordered]@{
            schemaVersion = 1
            operationId = [string]$Plan.operationId
            commit = [string]$Stash.commit
            message = [string]$Stash.message
            createdAt = [string]$Stash.createdAt
            includesTracked = $true
            includesStaged = $true
            includesUntracked = $true
            ignoredFilesUntouched = $true
            restored = $true
            restoredAt = (Get-Date).ToUniversalTime().ToString('o')
            retained = $true
        }
        Write-HermesDesktopWorkingTreeStashState -Plan $Plan -Value $record
        return [pscustomobject]@{
            Restored = $true
            Retained = $true
            Commit = [string]$Stash.commit
            Message = $null
        }
    }

    Invoke-HermesDesktopGit -Arguments @('reset', '--hard', $Revision) | Out-Null
    $message = "Hermes Local was updated, but the preserved local changes conflicted with the new source. They remain safe in Git stash $($Stash.commit)."
    $record = [ordered]@{
        schemaVersion = 1
        operationId = [string]$Plan.operationId
        commit = [string]$Stash.commit
        message = [string]$Stash.message
        createdAt = [string]$Stash.createdAt
        includesTracked = $true
        includesStaged = $true
        includesUntracked = $true
        ignoredFilesUntouched = $true
        restored = $false
        retained = $true
        restoreAttemptedAt = (Get-Date).ToUniversalTime().ToString('o')
        restoreError = $apply.Text
    }
    Write-HermesDesktopWorkingTreeStashState -Plan $Plan -Value $record

    [pscustomobject]@{
        Restored = $false
        Retained = $true
        Commit = [string]$Stash.commit
        Message = $message
    }
}

function Remove-HermesDesktopWorkingTreeStash {
    [CmdletBinding()]
    param([AllowNull()][object] $Stash)

    if (-not $Stash) {
        return $true
    }

    $lines = (Invoke-HermesDesktopGit -Arguments @(
        'stash', 'list', '--format=%H%x09%gd'
    ) -AllowFailure).Text -split '\r?\n'
    $reference = $null

    foreach ($line in $lines) {
        if (
            $line -match '^([0-9a-fA-F]{40})\t(.+)$' -and
            $Matches[1] -eq [string]$Stash.commit
        ) {
            $reference = $Matches[2]
            break
        }
    }

    if (-not $reference) {
        return $false
    }

    $drop = Invoke-HermesDesktopGit -Arguments @(
        'stash', 'drop', $reference
    ) -AllowFailure
    $drop.ExitCode -eq 0
}

function Get-HermesDesktopSemverTarget {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [ValidateSet('stable', 'beta')]
        [string] $ReleaseChannel
    )

    $lines = (Invoke-HermesDesktopGit -Arguments @(
        'ls-remote', '--tags', '--refs', 'origin', 'refs/tags/v*'
    )).Text -split '\r?\n'

    $records = foreach ($line in $lines) {
        if (
            $line -notmatch '^([0-9a-fA-F]{40})\s+refs/tags/(v(\d+)\.(\d+)\.(\d+)([-+][A-Za-z0-9.-]+)?)$'
        ) {
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
        Sort-Object Major, Minor, Patch, @{
            Expression = { if ($_.Prerelease) { 0 } else { 1 } }
        } -Descending |
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

        return [pscustomobject]@{
            Branch = 'main'
            Commit = $commit
            Release = $null
        }
    }

    $release = Get-HermesDesktopSemverTarget -ReleaseChannel $RequestedChannel
    [pscustomobject]@{
        Branch = $release.Tag
        Commit = $release.Commit
        Release = $release.Tag
    }
}
