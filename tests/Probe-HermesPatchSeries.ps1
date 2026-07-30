[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$root = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$manifest = Get-Content -Raw -LiteralPath (Join-Path $root 'VERSION.json') | ConvertFrom-Json
$checkout = Join-Path $env:RUNNER_TEMP ("hermes-agent-patch-probe-" + [guid]::NewGuid().ToString('N'))
$patchDirectory = Join-Path $root 'source\hermes-launcher\patches'
$newPatchName = '0014-fix-desktop-allow-start-recovery-during-benchmark.patch'

function Invoke-Native {
    param(
        [Parameter(Mandatory)]
        [string] $FilePath,
        [Parameter(Mandatory)]
        [string[]] $ArgumentList
    )

    & $FilePath @ArgumentList
    if ($LASTEXITCODE -ne 0) {
        throw "$FilePath failed with exit code $LASTEXITCODE."
    }
}

try {
    [System.IO.Directory]::CreateDirectory($checkout) | Out-Null
    Invoke-Native git @('-C', $checkout, 'init')
    Invoke-Native git @('-C', $checkout, 'remote', 'add', 'origin', [string]$manifest.sources.hermesAgent.repository)
    Invoke-Native git @('-C', $checkout, 'fetch', '--depth', '1', 'origin', [string]$manifest.sources.hermesAgent.commit)
    Invoke-Native git @('-C', $checkout, 'checkout', '--detach', 'FETCH_HEAD')
    Invoke-Native git @('-C', $checkout, 'config', 'user.name', 'github-actions[bot]')
    Invoke-Native git @('-C', $checkout, 'config', 'user.email', '41898282+github-actions[bot]@users.noreply.github.com')

    $patches = @(
        Get-ChildItem -LiteralPath $patchDirectory -Filter '*.patch' -File |
            Sort-Object Name
    )
    if ($patches.Name -notcontains $newPatchName) {
        throw "Required integration patch is missing: $newPatchName"
    }
    if ($patches[-1].Name -ne $newPatchName) {
        throw "$newPatchName must be the final integration patch; found $($patches[-1].Name)."
    }

    $amArguments = @('-C', $checkout, 'am', '--committer-date-is-author-date') + @($patches.FullName)
    Invoke-Native git $amArguments

    Push-Location $checkout
    try {
        Invoke-Native npm @('ci', '--ignore-scripts')
        Invoke-Native npm @(
            'exec', '--workspace', 'apps/desktop', '--',
            'vitest', 'run', 'electron/hermes-local-control.test.ts'
        )
    } finally {
        Pop-Location
    }

    $integrationCommit = (& git -C $checkout rev-parse HEAD).Trim()
    $integrationTree = (& git -C $checkout rev-parse 'HEAD^{tree}').Trim()
    $marker = "PATCH_SERIES_RESULT commit=$integrationCommit tree=$integrationTree"
    Write-Host $marker
    throw $marker
} finally {
    Remove-Item -LiteralPath $checkout -Recurse -Force -ErrorAction SilentlyContinue
}
