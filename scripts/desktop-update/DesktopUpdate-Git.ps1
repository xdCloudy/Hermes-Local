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

function New-HermesDesktopCandidateWorktree {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][object] $Plan,
        [Parameter(Mandatory)][string] $Revision
    )

    $candidateRoot = Join-Path ([string]$Plan.stagingRoot) 'candidate-source'
    if (Test-Path -LiteralPath $candidateRoot) {
        throw "The isolated candidate path already exists: $candidateRoot"
    }
    Invoke-HermesDesktopGit -Arguments @(
        'worktree', 'add', '--detach', $candidateRoot, $Revision
    ) | Out-Null

    $candidateState = Invoke-HermesDesktopNestedSourceGit `
        -Repository $candidateRoot `
        -Arguments @('rev-parse', 'HEAD') `
        -AllowFailure
    $candidateCommit = ([string]$candidateState.Text).Trim().ToLowerInvariant()
    if ($candidateState.ExitCode -ne 0 -or $candidateCommit -ne $Revision.ToLowerInvariant()) {
        throw 'The isolated Desktop update worktree did not resolve to the requested revision.'
    }

    [IO.Path]::GetFullPath($candidateRoot)
}

function Remove-HermesDesktopCandidateWorktree {
    [CmdletBinding()]
    param([AllowNull()][string] $CandidateRoot)

    if (-not $CandidateRoot) {
        return $true
    }

    $remove = Invoke-HermesDesktopGit -Arguments @(
        'worktree', 'remove', '--force', [IO.Path]::GetFullPath($CandidateRoot)
    ) -AllowFailure
    if ($remove.ExitCode -eq 0) {
        return $true
    }

    # A terminated build may leave a file handle behind. Prune only detached
    # worktree metadata; never delete or reset the installed checkout.
    Invoke-HermesDesktopGit -Arguments @('worktree', 'prune') -AllowFailure | Out-Null
    $false
}

function Set-HermesDesktopSourceRevision {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $Revision,
        [switch] $Rollback
    )

    $beforeCommit = (Invoke-HermesDesktopGit -Arguments @(
        'rev-parse', 'HEAD'
    )).Text.ToLowerInvariant()
    $beforeChanges = Get-HermesDesktopWorkingTreeChanges
    if ($beforeCommit -eq $Revision.ToLowerInvariant()) {
        return [pscustomobject]@{
            PreviousCommit = $beforeCommit
            CurrentCommit = $beforeCommit
            LocalChanges = $beforeChanges
            Changed = $false
        }
    }

    $arguments = if ($Rollback) {
        # --keep moves the source revision only when Git can retain every local
        # tracked and untracked file. It aborts instead of overwriting a conflict.
        @('reset', '--keep', $Revision)
    } else {
        # A fast-forward merge is Git's native non-destructive checkout update:
        # unrelated staged, unstaged and untracked work remains in place, while
        # a path collision aborts before that work can be overwritten.
        @('merge', '--ff-only', '--no-edit', $Revision)
    }
    $promotion = Invoke-HermesDesktopGit -Arguments $arguments -AllowFailure
    if ($promotion.ExitCode -ne 0) {
        $afterFailure = (Invoke-HermesDesktopGit -Arguments @(
            'rev-parse', 'HEAD'
        )).Text.ToLowerInvariant()
        if ($afterFailure -ne $beforeCommit) {
            throw 'Git could not promote the source revision and did not leave HEAD unchanged.'
        }
        throw (
            'The validated update is ready, but Git would overwrite a local path. ' +
            'No local file was changed; move or commit the conflicting work and retry. ' +
            $promotion.Text
        )
    }

    $afterCommit = (Invoke-HermesDesktopGit -Arguments @(
        'rev-parse', 'HEAD'
    )).Text.ToLowerInvariant()
    if ($afterCommit -ne $Revision.ToLowerInvariant()) {
        throw "Git completed source promotion at unexpected revision $afterCommit."
    }

    [pscustomobject]@{
        PreviousCommit = $beforeCommit
        CurrentCommit = $afterCommit
        LocalChanges = Get-HermesDesktopWorkingTreeChanges
        Changed = $true
    }
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
