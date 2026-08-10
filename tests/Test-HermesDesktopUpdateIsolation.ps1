[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repositoryRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$partsRoot = Join-Path $repositoryRoot 'scripts\desktop-update'
. (Join-Path $partsRoot 'DesktopUpdate-Git.ps1')
. (Join-Path $partsRoot 'DesktopUpdate-NestedSource.ps1')

function Assert-IsolationContract {
    param(
        [Parameter(Mandatory)][bool] $Condition,
        [Parameter(Mandatory)][string] $Message
    )

    if (-not $Condition) {
        throw $Message
    }
}

function Invoke-TestGit {
    param(
        [Parameter(Mandatory)][string] $Repository,
        [Parameter(Mandatory)][string[]] $Arguments
    )

    $output = @(& git -C $Repository @Arguments 2>&1 | ForEach-Object { [string]$_ })
    if ($LASTEXITCODE -ne 0) {
        throw "git $($Arguments -join ' ') failed.`n$($output -join [Environment]::NewLine)"
    }
    ($output -join [Environment]::NewLine).Trim()
}

function New-TestUpdateRepository {
    param([Parameter(Mandatory)][string] $Path)

    [IO.Directory]::CreateDirectory($Path) | Out-Null
    Invoke-TestGit -Repository $Path -Arguments @('init', '--initial-branch=main') | Out-Null
    Invoke-TestGit -Repository $Path -Arguments @('config', 'user.name', 'Hermes Update Test') | Out-Null
    Invoke-TestGit -Repository $Path -Arguments @('config', 'user.email', 'update-test@localhost') | Out-Null

    Set-Content -LiteralPath (Join-Path $Path 'app.txt') -Value 'base app' -Encoding utf8
    Set-Content -LiteralPath (Join-Path $Path 'local.txt') -Value 'base local' -Encoding utf8
    Set-Content -LiteralPath (Join-Path $Path 'notes.txt') -Value 'base notes' -Encoding utf8
    Set-Content -LiteralPath (Join-Path $Path '.gitignore') -Value '/build/' -Encoding utf8
    Invoke-TestGit -Repository $Path -Arguments @('add', '.') | Out-Null
    Invoke-TestGit -Repository $Path -Arguments @('commit', '-m', 'base') | Out-Null
    $base = Invoke-TestGit -Repository $Path -Arguments @('rev-parse', 'HEAD')

    Set-Content -LiteralPath (Join-Path $Path 'app.txt') -Value 'updated app' -Encoding utf8
    Set-Content -LiteralPath (Join-Path $Path 'new-version.txt') -Value 'candidate' -Encoding utf8
    Invoke-TestGit -Repository $Path -Arguments @('add', '.') | Out-Null
    Invoke-TestGit -Repository $Path -Arguments @('commit', '-m', 'candidate') | Out-Null
    $target = Invoke-TestGit -Repository $Path -Arguments @('rev-parse', 'HEAD')
    Invoke-TestGit -Repository $Path -Arguments @('switch', '--detach', $base) | Out-Null

    [pscustomobject]@{ Base = $base; Target = $target }
}

$tempRoot = Join-Path ([IO.Path]::GetTempPath()) (
    'hermes-desktop-isolation-' + [guid]::NewGuid().ToString('N')
)

try {
    $installed = Join-Path $tempRoot 'installed'
    $revisions = New-TestUpdateRepository -Path $installed
    $script:root = $installed

    Set-Content -LiteralPath (Join-Path $installed 'local.txt') -Value 'staged local edit' -Encoding utf8
    Invoke-TestGit -Repository $installed -Arguments @('add', 'local.txt') | Out-Null
    Set-Content -LiteralPath (Join-Path $installed 'notes.txt') -Value 'unstaged local edit' -Encoding utf8
    Set-Content -LiteralPath (Join-Path $installed 'custom-tool.ps1') -Value 'local only' -Encoding utf8

    $beforeStatus = Invoke-TestGit -Repository $installed -Arguments @(
        'status', '--porcelain=v1', '--untracked-files=all'
    )
    $beforeStashes = Invoke-TestGit -Repository $installed -Arguments @('stash', 'list')
    $stagingRoot = Join-Path $installed 'build\updates\desktop-staging\isolation-test'
    [IO.Directory]::CreateDirectory($stagingRoot) | Out-Null
    $plan = [pscustomobject]@{
        operationId = 'isolation-test'
        stagingRoot = $stagingRoot
    }

    $candidate = New-HermesDesktopCandidateWorktree `
        -Plan $plan `
        -Revision $revisions.Target
    Assert-IsolationContract `
        ((Invoke-TestGit -Repository $candidate -Arguments @('rev-parse', 'HEAD')) -eq $revisions.Target) `
        'The isolated candidate did not use the requested revision.'
    Assert-IsolationContract `
        ((Invoke-TestGit -Repository $installed -Arguments @('rev-parse', 'HEAD')) -eq $revisions.Base) `
        'Candidate preparation changed the installed source revision.'
    Assert-IsolationContract `
        ((Invoke-TestGit -Repository $installed -Arguments @('status', '--porcelain=v1', '--untracked-files=all')) -eq $beforeStatus) `
        'Candidate preparation changed a local working-tree file.'
    Assert-IsolationContract `
        (Remove-HermesDesktopCandidateWorktree -CandidateRoot $candidate) `
        'The isolated candidate worktree could not be removed.'

    $promotion = Set-HermesDesktopSourceRevision -Revision $revisions.Target
    Assert-IsolationContract $promotion.Changed 'The validated revision was not promoted.'
    Assert-IsolationContract `
        ((Invoke-TestGit -Repository $installed -Arguments @('rev-parse', 'HEAD')) -eq $revisions.Target) `
        'The installed checkout did not reach the target revision.'
    Assert-IsolationContract `
        ((Invoke-TestGit -Repository $installed -Arguments @('diff', '--cached', '--name-only')) -eq 'local.txt') `
        'The staged local edit was not preserved as staged.'
    Assert-IsolationContract `
        ((Invoke-TestGit -Repository $installed -Arguments @('diff', '--name-only')) -eq 'notes.txt') `
        'The unstaged local edit was not preserved.'
    Assert-IsolationContract `
        (Test-Path -LiteralPath (Join-Path $installed 'custom-tool.ps1') -PathType Leaf) `
        'The untracked local file was removed.'
    Assert-IsolationContract `
        ((Invoke-TestGit -Repository $installed -Arguments @('stash', 'list')) -eq $beforeStashes) `
        'The updater created or changed a Git stash.'

    $conflict = Join-Path $tempRoot 'conflict'
    $conflictRevisions = New-TestUpdateRepository -Path $conflict
    $script:root = $conflict
    Set-Content -LiteralPath (Join-Path $conflict 'app.txt') -Value 'conflicting local edit' -Encoding utf8
    $conflictFailed = $false
    try {
        Set-HermesDesktopSourceRevision -Revision $conflictRevisions.Target | Out-Null
    } catch {
        $conflictFailed = $true
    }
    Assert-IsolationContract $conflictFailed 'A conflicting local edit did not stop promotion.'
    Assert-IsolationContract `
        ((Invoke-TestGit -Repository $conflict -Arguments @('rev-parse', 'HEAD')) -eq $conflictRevisions.Base) `
        'A failed promotion changed the installed revision.'
    Assert-IsolationContract `
        ((Get-Content -Raw -LiteralPath (Join-Path $conflict 'app.txt')).Trim() -eq 'conflicting local edit') `
        'A failed promotion changed the conflicting local file.'

    $untrackedConflict = Join-Path $tempRoot 'untracked-conflict'
    $untrackedRevisions = New-TestUpdateRepository -Path $untrackedConflict
    $script:root = $untrackedConflict
    Set-Content `
        -LiteralPath (Join-Path $untrackedConflict 'new-version.txt') `
        -Value 'untracked collision' `
        -Encoding utf8
    $untrackedFailed = $false
    try {
        Set-HermesDesktopSourceRevision -Revision $untrackedRevisions.Target | Out-Null
    } catch {
        $untrackedFailed = $true
    }
    Assert-IsolationContract $untrackedFailed 'An untracked path collision did not stop promotion.'
    Assert-IsolationContract `
        ((Invoke-TestGit -Repository $untrackedConflict -Arguments @('rev-parse', 'HEAD')) -eq $untrackedRevisions.Base) `
        'An untracked path collision changed the installed revision.'
    Assert-IsolationContract `
        ((Get-Content -Raw -LiteralPath (Join-Path $untrackedConflict 'new-version.txt')).Trim() -eq 'untracked collision') `
        'An untracked path collision changed the local file.'

    Write-Host 'Hermes Desktop isolated updater tests passed.' -ForegroundColor Green
} finally {
    if (Test-Path -LiteralPath $tempRoot) {
        Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}
