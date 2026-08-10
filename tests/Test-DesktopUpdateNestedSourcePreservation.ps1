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
$entry = [IO.File]::ReadAllText($entryPath)
if (-not $entry.Contains("'DesktopUpdate-NestedSource.ps1'", [StringComparison]::Ordinal)) {
    throw 'Desktop updater does not load nested source preservation.'
}

function Get-HermesDesktopObjectValue {
    param(
        [Parameter(Mandatory)][object] $InputObject,
        [Parameter(Mandatory)][string] $Name,
        $Default = $null
    )

    $property = $InputObject.PSObject.Properties[$Name]
    if ($property) { return $property.Value }
    $Default
}

function Set-HermesDesktopObjectValue {
    param(
        [Parameter(Mandatory)][object] $InputObject,
        [Parameter(Mandatory)][string] $Name,
        $Value
    )

    $InputObject | Add-Member `
        -NotePropertyName $Name `
        -NotePropertyValue $Value `
        -Force
}

function Write-HermesDesktopUpdateProgress {
    param(
        $Plan,
        $Stage,
        $Status,
        $Message,
        $Percent,
        $Failure,
        $Result
    )
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

try {
    [IO.Directory]::CreateDirectory($repository) | Out-Null
    & git -C $repository init --quiet
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
    $before = Get-HermesDesktopNestedSourceChanges -Repository $repository
    $plan = [pscustomobject]@{
        operationId = 'nested-source-contract'
        root = $tempRoot
    }

    $result = @(Invoke-HermesDesktopUpdateStage -Plan $plan) | Select-Object -Last 1
    $after = Get-HermesDesktopNestedSourceChanges -Repository $repository
    if ($before -ne $after) {
        throw 'Nested source status changed while the update stage ran.'
    }
    if ([IO.File]::ReadAllText($tracked) -ne "edited`n") {
        throw 'Tracked nested source content was moved or changed.'
    }
    if ([IO.File]::ReadAllText($untracked) -ne "preserved`n") {
        throw 'Untracked nested source content was moved or changed.'
    }
    if (@(& git -C $repository stash list).Count -ne 0) {
        throw 'Nested source preservation created a Git stash.'
    }
    if (-not [bool]$result.nestedSourceChangesPreserved) {
        throw 'The structured result did not report nested-source preservation.'
    }
    if ([string]$result.retainedNestedSourceStashCommit) {
        throw 'The structured result incorrectly reported an updater stash.'
    }

    Write-Host 'Desktop update nested source preservation contract passed.'
} finally {
    if (Test-Path -LiteralPath $tempRoot) {
        Remove-Item -LiteralPath $tempRoot -Recurse -Force
    }
}
