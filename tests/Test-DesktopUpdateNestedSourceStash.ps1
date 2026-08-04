[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$componentPath = Join-Path `
    $root `
    'scripts\desktop-update\DesktopUpdate-NestedSource.ps1'
$entryPath = Join-Path $root 'Invoke-Hermes-DesktopUpdate.ps1'

if (-not (Test-Path -LiteralPath $componentPath -PathType Leaf)) {
    throw "Nested source updater component is missing: $componentPath"
}
if (-not (Test-Path -LiteralPath $entryPath -PathType Leaf)) {
    throw "Desktop update entry point is missing: $entryPath"
}

$entry = [IO.File]::ReadAllText($entryPath)
if (-not $entry.Contains(
    "'DesktopUpdate-NestedSource.ps1'",
    [StringComparison]::Ordinal
)) {
    throw 'Desktop updater does not load nested source preservation.'
}

function Write-HermesDesktopUpdateJson {
    param(
        [Parameter(Mandatory)][string] $Path,
        [Parameter(Mandatory)][object] $Value
    )

    [IO.Directory]::CreateDirectory(
        [IO.Path]::GetDirectoryName($Path)
    ) | Out-Null
    [IO.File]::WriteAllText(
        $Path,
        ($Value | ConvertTo-Json -Depth 64),
        [Text.UTF8Encoding]::new($false)
    )
}

function Get-HermesDesktopObjectValue {
    param(
        [Parameter(Mandatory)][object] $InputObject,
        [Parameter(Mandatory)][string] $Name,
        $Default = $null
    )

    if (
        $InputObject -is [Collections.IDictionary] -and
        $InputObject.Contains($Name)
    ) {
        return $InputObject[$Name]
    }

    $property = $InputObject.PSObject.Properties[$Name]
    if ($property) {
        return $property.Value
    }

    $Default
}

function Set-HermesDesktopObjectValue {
    param(
        [Parameter(Mandatory)][object] $InputObject,
        [Parameter(Mandatory)][string] $Name,
        $Value
    )

    if ($InputObject -is [Collections.IDictionary]) {
        $InputObject[$Name] = $Value
        return
    }

    $InputObject | Add-Member `
        -NotePropertyName $Name `
        -NotePropertyValue $Value `
        -Force
}

function Write-HermesDesktopUpdateProgress {
    param()
}

function Invoke-HermesDesktopUpdateStage {
    param([Parameter(Mandatory)][object] $Plan)

    [pscustomobject]@{
        status = 'ready-to-restart'
        activationDeferred = $true
    }
}

. $componentPath

$tempRoot = Join-Path `
    ([IO.Path]::GetTempPath()) `
    "hermes-nested-source-test-$([guid]::NewGuid().ToString('N'))"
$repository = Join-Path $tempRoot 'source\hermes-agent'
$stagingRoot = Join-Path $tempRoot 'staging'

try {
    [IO.Directory]::CreateDirectory($repository) | Out-Null
    [IO.Directory]::CreateDirectory($stagingRoot) | Out-Null

    & git -C $repository init --quiet
    if ($LASTEXITCODE -ne 0) {
        throw 'Could not initialise the nested source test repository.'
    }
    & git -C $repository config user.email 'hermes-test@example.invalid'
    & git -C $repository config user.name 'Hermes Update Test'

    $tracked = Join-Path $repository 'tracked.txt'
    $untracked = Join-Path $repository 'untracked.txt'
    [IO.File]::WriteAllText($tracked, "original`n")
    & git -C $repository add tracked.txt
    & git -C $repository commit --quiet -m 'initial'
    if ($LASTEXITCODE -ne 0) {
        throw 'Could not create the nested source test commit.'
    }

    [IO.File]::WriteAllText($tracked, "edited`n")
    [IO.File]::WriteAllText($untracked, "preserved`n")

    $plan = [pscustomobject]@{
        operationId = 'nested-source-contract'
        root = $tempRoot
        stagingRoot = $stagingRoot
    }

    $stash = Save-HermesDesktopNestedSourceWorkingTree `
        -Plan $plan `
        -Repository $repository

    if (-not $stash) {
        throw 'Nested source changes were not preserved.'
    }
    if (
        Get-HermesDesktopNestedSourceChanges -Repository $repository
    ) {
        throw 'Nested source checkout was not clean after preservation.'
    }
    if (-not (Test-Path -LiteralPath (
        Join-Path $stagingRoot 'hermes-agent-working-tree-stash.json'
    ) -PathType Leaf)) {
        throw 'Nested source stash diagnostics were not persisted.'
    }

    $restore = Restore-HermesDesktopNestedSourceWorkingTree `
        -Stash $stash `
        -Plan $plan
    if (-not $restore.Restored) {
        throw "Nested source changes were not restored: $($restore.Message)"
    }
    if ([IO.File]::ReadAllText($tracked) -ne "edited`n") {
        throw 'Tracked nested source change was not restored.'
    }
    if ([IO.File]::ReadAllText($untracked) -ne "preserved`n") {
        throw 'Untracked nested source file was not restored.'
    }

    if (-not (Remove-HermesDesktopNestedSourceStash -Stash $stash)) {
        throw 'Restored nested source stash was not removed.'
    }

    $stashList = @(& git -C $repository stash list)
    if ($stashList.Count -ne 0) {
        throw 'Nested source test left an updater-created stash behind.'
    }

    Write-Host 'Desktop update nested source stash contract passed.'
} finally {
    if (Test-Path -LiteralPath $tempRoot) {
        Remove-Item -LiteralPath $tempRoot -Recurse -Force
    }
}
