[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repositoryRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$manifestPath = Join-Path $repositoryRoot 'VERSION.json'

if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
    throw "Hermes Local version manifest is missing: $manifestPath"
}

$manifest = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json -Depth 32
$agent = $manifest.sources.hermesAgent

foreach ($name in @('commit', 'integrationCommit', 'integrationTree')) {
    $value = [string]$agent.$name
    if ($value -notmatch '^[0-9a-fA-F]{40}$') {
        throw "VERSION.json Hermes Agent field '$name' must be a 40-character Git identity."
    }
}

$patchSeries = [string]$agent.patchSeries
if ([string]::IsNullOrWhiteSpace($patchSeries)) {
    throw 'VERSION.json must declare the Hermes Agent integration patch series.'
}

$patchDirectory = [System.IO.Path]::GetFullPath((Join-Path $repositoryRoot $patchSeries))
$rootPrefix = $repositoryRoot.TrimEnd([System.IO.Path]::DirectorySeparatorChar) + [System.IO.Path]::DirectorySeparatorChar
if (-not $patchDirectory.StartsWith($rootPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Hermes Agent patch series escapes the repository root: $patchSeries"
}
if (-not (Test-Path -LiteralPath $patchDirectory -PathType Container)) {
    throw "Hermes Agent patch series is missing: $patchDirectory"
}

$patches = @(Get-ChildItem -LiteralPath $patchDirectory -Filter '*.patch' -File | Sort-Object Name)
if ($patches.Count -eq 0) {
    throw "Hermes Agent patch series is empty: $patchDirectory"
}

Write-Host (
    'Hermes Agent source manifest tests passed: ' +
    "$($patches.Count) patches pinned to tree $([string]$agent.integrationTree)."
)
