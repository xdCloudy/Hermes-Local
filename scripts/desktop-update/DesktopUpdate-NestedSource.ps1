function Invoke-HermesDesktopNestedSourceGit {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $Repository,
        [Parameter(Mandatory)][string[]] $Arguments,
        [switch] $AllowFailure
    )

    $output = @(
        & git -C $Repository @Arguments 2>&1 |
            ForEach-Object { [string]$_ }
    )
    $exitCode = $LASTEXITCODE
    $text = ($output -join [Environment]::NewLine).Trim()

    if ($exitCode -ne 0 -and -not $AllowFailure) {
        throw "git -C $Repository $($Arguments -join ' ') failed with exit code $exitCode.`n$text"
    }

    [pscustomobject]@{
        ExitCode = $exitCode
        Text = $text
    }
}

function Get-HermesDesktopNestedSourceChanges {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string] $Repository)

    if (-not (Test-Path -LiteralPath (Join-Path $Repository '.git'))) {
        return ''
    }

    (Invoke-HermesDesktopNestedSourceGit `
        -Repository $Repository `
        -Arguments @('status', '--porcelain=v1', '--untracked-files=all') `
        -AllowFailure).Text
}

function Write-HermesDesktopNestedSourceStashState {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][object] $Plan,
        [Parameter(Mandatory)][object] $Value
    )

    $path = Join-Path `
        ([string]$Plan.stagingRoot) `
        'hermes-agent-working-tree-stash.json'
    try {
        Write-HermesDesktopUpdateJson -Path $path -Value $Value
    } catch {
        # Git remains the source of truth if diagnostic persistence fails.
    }
}

function Save-HermesDesktopNestedSourceWorkingTree {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][object] $Plan,
        [Parameter(Mandatory)][string] $Repository
    )

    $changes = Get-HermesDesktopNestedSourceChanges -Repository $Repository
    if (-not $changes) {
        return $null
    }

    $revision = (Invoke-HermesDesktopNestedSourceGit `
        -Repository $Repository `
        -Arguments @('rev-parse', 'HEAD')).Text.ToLowerInvariant()
    $message = "hermes-desktop-update-hermes-agent:$($Plan.operationId)"
    $before = Invoke-HermesDesktopNestedSourceGit `
        -Repository $Repository `
        -Arguments @('rev-parse', '--verify', 'refs/stash') `
        -AllowFailure
    $stash = Invoke-HermesDesktopNestedSourceGit `
        -Repository $Repository `
        -Arguments @(
            'stash', 'push', '--include-untracked', '--message', $message
        ) `
        -AllowFailure

    if ($stash.ExitCode -ne 0) {
        throw (
            'Could not preserve local Hermes Agent source changes before ' +
            "updating. $($stash.Text)"
        )
    }

    $after = Invoke-HermesDesktopNestedSourceGit `
        -Repository $Repository `
        -Arguments @('rev-parse', '--verify', 'refs/stash') `
        -AllowFailure
    if (
        $after.ExitCode -ne 0 -or
        $after.Text -notmatch '^[0-9a-fA-F]{40}$'
    ) {
        throw (
            'Git reported a successful Hermes Agent source stash, but the ' +
            'updater could not identify it.'
        )
    }
    if ($before.ExitCode -eq 0 -and $before.Text -eq $after.Text) {
        throw 'Git did not create a new Hermes Agent source stash.'
    }

    $remaining = Get-HermesDesktopNestedSourceChanges -Repository $Repository
    if ($remaining) {
        throw (
            'Automatic Hermes Agent source stashing left changes in the ' +
            "working tree.`n$remaining"
        )
    }

    $record = [ordered]@{
        schemaVersion = 1
        operationId = [string]$Plan.operationId
        repository = [IO.Path]::GetFullPath($Repository)
        revision = $revision
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
    Write-HermesDesktopNestedSourceStashState -Plan $Plan -Value $record
    [pscustomobject]$record
}

function Restore-HermesDesktopNestedSourceWorkingTree {
    [CmdletBinding()]
    param(
        [AllowNull()][object] $Stash,
        [Parameter(Mandatory)][object] $Plan
    )

    if (-not $Stash) {
        return [pscustomobject]@{
            Restored = $true
            Retained = $false
            Commit = $null
            ConflictCommit = $null
            Message = $null
        }
    }

    $repository = [string]$Stash.repository
    $apply = Invoke-HermesDesktopNestedSourceGit `
        -Repository $repository `
        -Arguments @('stash', 'apply', '--index', [string]$Stash.commit) `
        -AllowFailure

    if ($apply.ExitCode -eq 0) {
        $record = [ordered]@{
            schemaVersion = 1
            operationId = [string]$Plan.operationId
            repository = $repository
            revision = [string]$Stash.revision
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
        Write-HermesDesktopNestedSourceStashState -Plan $Plan -Value $record
        return [pscustomobject]@{
            Restored = $true
            Retained = $true
            Commit = [string]$Stash.commit
            ConflictCommit = $null
            Message = $null
        }
    }

    # Remove tracked conflict state. If stash application also materialised
    # untracked files, preserve that partial state in a second stash so the
    # checkout becomes clean without deleting any user-owned source file.
    $null = Invoke-HermesDesktopNestedSourceGit `
        -Repository $repository `
        -Arguments @('reset', '--hard', 'HEAD') `
        -AllowFailure

    $conflictCommit = $null
    $remaining = Get-HermesDesktopNestedSourceChanges -Repository $repository
    if ($remaining) {
        $conflictMessage = (
            "hermes-desktop-update-hermes-agent-restore-conflict:" +
            [string]$Plan.operationId
        )
        $conflict = Invoke-HermesDesktopNestedSourceGit `
            -Repository $repository `
            -Arguments @(
                'stash', 'push', '--include-untracked',
                '--message', $conflictMessage
            ) `
            -AllowFailure
        if ($conflict.ExitCode -eq 0) {
            $resolvedConflict = Invoke-HermesDesktopNestedSourceGit `
                -Repository $repository `
                -Arguments @('rev-parse', '--verify', 'refs/stash') `
                -AllowFailure
            if (
                $resolvedConflict.ExitCode -eq 0 -and
                $resolvedConflict.Text -match '^[0-9a-fA-F]{40}$'
            ) {
                $conflictCommit = $resolvedConflict.Text.ToLowerInvariant()
            }
        }
    }

    $message = (
        'Hermes Local was updated, but preserved Hermes Agent source changes ' +
        "conflicted with the prepared integration. They remain safe in Git " +
        "stash $($Stash.commit)."
    )
    $record = [ordered]@{
        schemaVersion = 1
        operationId = [string]$Plan.operationId
        repository = $repository
        revision = [string]$Stash.revision
        commit = [string]$Stash.commit
        conflictCommit = $conflictCommit
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
    Write-HermesDesktopNestedSourceStashState -Plan $Plan -Value $record

    [pscustomobject]@{
        Restored = $false
        Retained = $true
        Commit = [string]$Stash.commit
        ConflictCommit = $conflictCommit
        Message = $message
    }
}

function Remove-HermesDesktopNestedSourceStash {
    [CmdletBinding()]
    param([AllowNull()][object] $Stash)

    if (-not $Stash) {
        return $true
    }

    $repository = [string]$Stash.repository
    $lines = (Invoke-HermesDesktopNestedSourceGit `
        -Repository $repository `
        -Arguments @('stash', 'list', '--format=%H%x09%gd') `
        -AllowFailure).Text -split '\r?\n'
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

    $drop = Invoke-HermesDesktopNestedSourceGit `
        -Repository $repository `
        -Arguments @('stash', 'drop', $reference) `
        -AllowFailure
    $drop.ExitCode -eq 0
}

$coreVariable = Get-Variable `
    -Name HermesDesktopUpdateStageCore `
    -Scope Script `
    -ErrorAction SilentlyContinue
if (-not $coreVariable) {
    Set-Variable `
        -Name HermesDesktopUpdateStageCore `
        -Scope Script `
        -Value ${function:Invoke-HermesDesktopUpdateStage}
}

function Invoke-HermesDesktopUpdateStage {
    [CmdletBinding()]
    param([Parameter(Mandatory)][object] $Plan)

    $repository = Join-Path ([string]$Plan.root) 'source\hermes-agent'
    $nestedStash = $null
    $items = [System.Collections.Generic.List[object]]::new()
    $coreError = $null

    if (Get-HermesDesktopNestedSourceChanges -Repository $repository) {
        try {
            Write-HermesDesktopUpdateProgress `
                -Plan $Plan `
                -Stage preserving-local-changes `
                -Status running `
                -Message 'Preserving local Hermes Agent source changes before updating.' `
                -Percent 14 `
                -Failure $null `
                -Result $null | Out-Null
        } catch {
        }
        $nestedStash = Save-HermesDesktopNestedSourceWorkingTree `
            -Plan $Plan `
            -Repository $repository
    }

    try {
        & $script:HermesDesktopUpdateStageCore -Plan $Plan |
            ForEach-Object { $items.Add($_) }
    } catch {
        $coreError = $_
    }

    $restore = $null
    $restoreWarning = $null
    try {
        $restore = Restore-HermesDesktopNestedSourceWorkingTree `
            -Stash $nestedStash `
            -Plan $Plan
    } catch {
        $restoreWarning = $_.Exception.Message
        $restore = [pscustomobject]@{
            Restored = $false
            Retained = [bool]$nestedStash
            Commit = if ($nestedStash) {
                [string]$nestedStash.commit
            } else {
                $null
            }
            ConflictCommit = $null
            Message = $restoreWarning
        }
    }

    $structured = @(
        $items |
            Where-Object {
                $null -ne $_ -and
                $null -ne (Get-HermesDesktopObjectValue `
                    -InputObject $_ `
                    -Name status `
                    -Default $null)
            }
    ) | Select-Object -Last 1

    if ($structured) {
        Set-HermesDesktopObjectValue `
            -InputObject $structured `
            -Name nestedSourceChangesPreserved `
            -Value ([bool]$nestedStash)
        Set-HermesDesktopObjectValue `
            -InputObject $structured `
            -Name nestedSourceChangesRestored `
            -Value ([bool]$restore.Restored)
        Set-HermesDesktopObjectValue `
            -InputObject $structured `
            -Name retainedNestedSourceStashCommit `
            -Value $(if (
                $restore.Retained -and
                -not $restore.Restored
            ) {
                [string]$restore.Commit
            } else {
                $null
            })
        Set-HermesDesktopObjectValue `
            -InputObject $structured `
            -Name nestedSourceRestoreWarning `
            -Value $(if ($restoreWarning) {
                $restoreWarning
            } else {
                [string]$restore.Message
            })
    }

    if ($nestedStash -and $restore.Restored) {
        try {
            if (-not (Remove-HermesDesktopNestedSourceStash -Stash $nestedStash)) {
                if ($structured) {
                    Set-HermesDesktopObjectValue `
                        -InputObject $structured `
                        -Name nestedSourceStashCleanupWarning `
                        -Value 'The restored Hermes Agent source stash could not be removed automatically.'
                }
            }
        } catch {
            if ($structured) {
                Set-HermesDesktopObjectValue `
                    -InputObject $structured `
                    -Name nestedSourceStashCleanupWarning `
                    -Value $_.Exception.Message
            }
        }
    }

    foreach ($item in $items) {
        Write-Output $item
    }

    if ($coreError) {
        throw $coreError
    }
}
