[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$patchRoot = Join-Path $root 'source\hermes-launcher\patches'
$securityPatch = Join-Path $patchRoot '0036-feat-desktop-instrument-security-scan-task-progress.patch'
$repairPatch = Join-Path $patchRoot '0039-fix-desktop-structurally-repair-private-path-before-typecheck.patch'

function Assert-Contract {
    param(
        [Parameter(Mandatory)][bool] $Condition,
        [Parameter(Mandatory)][string] $Message
    )

    if (-not $Condition) {
        throw $Message
    }
}

Assert-Contract `
    (Test-Path -LiteralPath $securityPatch -PathType Leaf) `
    'The security-progress patch containing private-path redaction is missing.'
Assert-Contract `
    (Test-Path -LiteralPath $repairPatch -PathType Leaf) `
    'The structural Desktop control repair patch is missing.'

$patches = @(
    Get-ChildItem -LiteralPath $patchRoot -Filter '*.patch' -File |
        Sort-Object Name
)
$contents = @(
    $patches |
        ForEach-Object {
            [pscustomobject]@{
                Name = $_.Name
                Text = Get-Content -Raw -LiteralPath $_.FullName
            }
        }
)

$correctStatement = "+    safe = safe.replaceAll(privatePath, '[PRIVATE-PATH]').replaceAll(privatePath.replaceAll('\\', '/'), '[PRIVATE-PATH]')"
$brokenPreimage = "-    safe = safe.replaceAll(privatePath, '[PRIVATE-PATH]').replaceAll(privatePath.replaceAll('\', '/'), '[PRIVATE-PATH]')"

$canonicalInsertions = @(
    $contents |
        Where-Object { $_.Text.Contains($correctStatement, [StringComparison]::Ordinal) }
)
$brokenDependencies = @(
    $contents |
        Where-Object { $_.Text.Contains($brokenPreimage, [StringComparison]::Ordinal) }
)

Assert-Contract `
    ($canonicalInsertions.Count -eq 1) `
    "Expected exactly one patch to insert the escaped private-path literal; found $($canonicalInsertions.Count)."
Assert-Contract `
    ($canonicalInsertions[0].Name -eq [IO.Path]::GetFileName($securityPatch)) `
    'The escaped private-path literal must originate in patch 0036.'
Assert-Contract `
    ($brokenDependencies.Count -eq 0) `
    ('A later patch still depends on the invalid single-backslash preimage: ' +
        (($brokenDependencies | ForEach-Object Name) -join ', '))

$repairText = Get-Content -Raw -LiteralPath $repairPatch
Assert-Contract `
    ($repairText.Contains("const backslashLiteral = JSON.stringify('\\')", [StringComparison]::Ordinal)) `
    'The structural repair script no longer derives the escaped backslash literal safely.'

Write-Host 'Hermes Launcher private-path patch-series contract passed.'
