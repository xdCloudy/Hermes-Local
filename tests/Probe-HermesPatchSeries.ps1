[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$root = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$manifest = Get-Content -Raw -LiteralPath (Join-Path $root 'VERSION.json') | ConvertFrom-Json
$checkout = Join-Path $env:RUNNER_TEMP ("hermes-agent-patch-probe-" + [guid]::NewGuid().ToString('N'))
$patchOutput = Join-Path $env:RUNNER_TEMP ("hermes-agent-generated-patch-" + [guid]::NewGuid().ToString('N'))
$patchDirectory = Join-Path $root 'source\hermes-launcher\patches'
$newPatchName = '0019-fix-desktop-allow-start-recovery-during-benchmark.patch'
$newPatchPath = Join-Path $patchDirectory $newPatchName

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
    [System.IO.Directory]::CreateDirectory($patchOutput) | Out-Null
    Invoke-Native git @('-C', $checkout, 'init')
    Invoke-Native git @('-C', $checkout, 'remote', 'add', 'origin', [string]$manifest.sources.hermesAgent.repository)
    Invoke-Native git @('-C', $checkout, 'fetch', '--depth', '1', 'origin', [string]$manifest.sources.hermesAgent.commit)
    Invoke-Native git @('-C', $checkout, 'checkout', '--detach', 'FETCH_HEAD')
    Invoke-Native git @('-C', $checkout, 'config', 'user.name', 'xdCloudy')
    Invoke-Native git @('-C', $checkout, 'config', 'user.email', '52116030+xdCloudy@users.noreply.github.com')

    $patches = @(
        Get-ChildItem -LiteralPath $patchDirectory -Filter '*.patch' -File |
            Where-Object Name -ne $newPatchName |
            Sort-Object Name
    )
    $amArguments = @('-C', $checkout, 'am', '--committer-date-is-author-date') + @($patches.FullName)
    Invoke-Native git $amArguments

    # The temporary hand-authored patch contains the already-tested source
    # change but intentionally needs recounting before it is replaced with the
    # exact format-patch output generated below.
    Invoke-Native git @('-C', $checkout, 'apply', '--recount', '--whitespace=nowarn', $newPatchPath)

    $env:GIT_AUTHOR_DATE = '2026-07-30T19:05:00Z'
    $env:GIT_COMMITTER_DATE = '2026-07-30T19:05:00Z'
    Invoke-Native git @('-C', $checkout, 'add', '--',
        'apps/desktop/electron/hermes-local-control.ts',
        'apps/desktop/electron/hermes-local-control.test.ts')
    Invoke-Native git @('-C', $checkout, 'commit', '-m', 'fix(desktop): allow start recovery during benchmarks')

    Invoke-Native git @('-C', $checkout, 'format-patch', '-1', '--no-signature', '--output-directory', $patchOutput)
    $generatedPatches = @(Get-ChildItem -LiteralPath $patchOutput -Filter '*.patch' -File)
    if ($generatedPatches.Count -ne 1) {
        throw "Expected one generated patch; found $($generatedPatches.Count)."
    }
    $patchBase64 = [Convert]::ToBase64String([System.IO.File]::ReadAllBytes($generatedPatches[0].FullName))
    Write-Host 'PATCH_BASE64_BEGIN'
    for ($offset = 0; $offset -lt $patchBase64.Length; $offset += 2000) {
        $length = [Math]::Min(2000, $patchBase64.Length - $offset)
        Write-Host $patchBase64.Substring($offset, $length)
    }
    Write-Host 'PATCH_BASE64_END'

    $integrationCommit = (& git -C $checkout rev-parse HEAD).Trim()
    $integrationTree = (& git -C $checkout rev-parse 'HEAD^{tree}').Trim()
    $marker = "PATCH_SERIES_RESULT commit=$integrationCommit tree=$integrationTree"
    Write-Host $marker
    throw $marker
} finally {
    Remove-Item -LiteralPath $checkout -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $patchOutput -Recurse -Force -ErrorAction SilentlyContinue
}
