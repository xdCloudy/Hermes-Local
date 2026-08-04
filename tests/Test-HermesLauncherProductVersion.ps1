[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$manifestPath = Join-Path $root 'VERSION.json'
$patchPath = Join-Path $root 'source\hermes-launcher\patches\0040-fix-desktop-stamp-Hermes-Local-product-version.patch'

foreach ($path in @($manifestPath, $patchPath)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Hermes Launcher product-version contract file is missing: $path"
    }
}

$manifest = Get-Content -Raw -LiteralPath $manifestPath |
    ConvertFrom-Json -Depth 32
$productVersion = [string]$manifest.product.version

if ($productVersion -notmatch '^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$') {
    throw "VERSION.json contains an invalid Hermes Launcher product version: $productVersion"
}

$patch = [IO.File]::ReadAllText($patchPath)
foreach ($required in @(
    'function hermesLocalProductVersion()',
    'process.env.HERMES_LOCAL_ROOT?.trim()',
    'path.resolve(import.meta.dirname, "../../../../..")',
    'path.join(root, "VERSION.json")',
    'manifest?.product?.version',
    '`-c.extraMetadata.version=${productVersion}`',
    'Hermes Local product version ${productVersion}'
)) {
    if (-not $patch.Contains($required, [StringComparison]::Ordinal)) {
        throw "Hermes Launcher package-version patch is missing: $required"
    }
}

if ($patch -match '-c\.extraMetadata\.version=0\.\d+\.\d+') {
    throw 'Hermes Launcher package metadata must be derived from VERSION.json, not hardcoded in the patch.'
}

Write-Host "Hermes Launcher product-version packaging contract passed for $productVersion."
