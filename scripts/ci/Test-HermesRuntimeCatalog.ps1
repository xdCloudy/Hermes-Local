[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$root = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$catalogPath = Join-Path $root 'config\runtime\llama-runtime-catalog.json'
$catalog = Get-Content -Raw -LiteralPath $catalogPath | ConvertFrom-Json -Depth 64
$headers = @{ Accept = 'application/vnd.github+json'; 'User-Agent' = 'Hermes-Local-CI' }

foreach ($package in @($catalog.packages)) {
    if ([string]$package.sourceCommit -notmatch '^[0-9a-f]{40}$') {
        throw "Invalid source commit for $($package.id)."
    }
    foreach ($artifact in @($package.artifacts)) {
        $repository = [string]$artifact.repository
        $tag = [string]$artifact.tag
        $assetName = [string]$artifact.asset
        $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$repository/releases/tags/$tag" `
            -Headers $headers -Method Get -TimeoutSec 60
        $assets = @($release.assets | Where-Object name -eq $assetName)
        if ($assets.Count -ne 1) {
            throw "$repository@$tag does not contain exactly one '$assetName'."
        }
        $digest = [string]$assets[0].digest
        if ($digest -notmatch '^sha256:([0-9a-f]{64})$') {
            throw "$assetName does not publish a SHA-256 digest."
        }
        if ($artifact.expectedSha256 -and $Matches[1] -ne [string]$artifact.expectedSha256) {
            throw "$assetName digest differs from the pinned catalog value."
        }
        Write-Host "$($package.id): $assetName $digest"
    }
}
