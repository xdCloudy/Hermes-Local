[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repositoryRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$wrapperPath = Join-Path $repositoryRoot 'Apply-Hermes-LauncherOverlay.ps1'
$attributesPath = Join-Path $repositoryRoot '.gitattributes'

if (-not (Test-Path -LiteralPath $attributesPath -PathType Leaf)) {
    throw 'Repository is missing .gitattributes for the launcher overlay checkout contract.'
}

$attributes = [System.IO.File]::ReadAllText($attributesPath).Replace("`r`n", "`n")
if ($attributes -notmatch '(?m)^Apply-Hermes-LauncherOverlay\.ps1\s+text\s+eol=lf\s*$') {
    throw 'Apply-Hermes-LauncherOverlay.ps1 must be pinned to LF in .gitattributes.'
}

$wrapperBytes = [System.IO.File]::ReadAllBytes($wrapperPath)
for ($index = 0; $index -lt ($wrapperBytes.Length - 1); $index += 1) {
    if ($wrapperBytes[$index] -eq 13 -and $wrapperBytes[$index + 1] -eq 10) {
        throw 'Checked-out Apply-Hermes-LauncherOverlay.ps1 contains CRLF despite its LF-only attribute.'
    }
}

$git = (Get-Command git -ErrorAction Stop).Source
$attributeResult = @(& $git -C $repositoryRoot check-attr eol -- 'Apply-Hermes-LauncherOverlay.ps1')
if ($LASTEXITCODE -ne 0) {
    throw "git check-attr failed with exit code $LASTEXITCODE."
}
if (($attributeResult -join "`n") -notmatch ':\s*eol:\s*lf\s*$') {
    throw "Git does not resolve the launcher overlay eol attribute to lf: $($attributeResult -join ' ')"
}

$tempCheckout = Join-Path ([System.IO.Path]::GetTempPath()) (
    'hermes-overlay-eol-' + [guid]::NewGuid().ToString('N')
)
[System.IO.Directory]::CreateDirectory($tempCheckout) | Out-Null
try {
    $checkoutPrefix = $tempCheckout.Replace('\', '/') + '/'
    & $git `
        -C $repositoryRoot `
        -c core.autocrlf=true `
        checkout-index `
        --force `
        "--prefix=$checkoutPrefix" `
        -- `
        'Apply-Hermes-LauncherOverlay.ps1'

    if ($LASTEXITCODE -ne 0) {
        throw "git checkout-index with core.autocrlf=true failed with exit code $LASTEXITCODE."
    }

    $autocrlfWrapper = Join-Path $tempCheckout 'Apply-Hermes-LauncherOverlay.ps1'
    $autocrlfBytes = [System.IO.File]::ReadAllBytes($autocrlfWrapper)
    for ($index = 0; $index -lt ($autocrlfBytes.Length - 1); $index += 1) {
        if ($autocrlfBytes[$index] -eq 13 -and $autocrlfBytes[$index + 1] -eq 10) {
            throw 'core.autocrlf=true converted the launcher overlay wrapper to CRLF.'
        }
    }
} finally {
    Remove-Item -LiteralPath $tempCheckout -Recurse -Force -ErrorAction SilentlyContinue
}

$wrapper = [System.IO.File]::ReadAllText($wrapperPath)
$normalizedWrapper = $wrapper.Replace("`r`n", "`n")

function Get-EmbeddedHereString {
    param([Parameter(Mandatory)][string] $VariableName)

    $startMarker = '$' + $VariableName + " = @'`n"
    $start = $normalizedWrapper.IndexOf($startMarker, [StringComparison]::Ordinal)
    if ($start -lt 0) {
        throw "Could not locate embedded here-string '$VariableName'."
    }

    $bodyStart = $start + $startMarker.Length
    $end = $normalizedWrapper.IndexOf("`n'@", $bodyStart, [StringComparison]::Ordinal)
    if ($end -lt 0) {
        throw "Embedded here-string '$VariableName' has no closing marker."
    }

    return $normalizedWrapper.Substring($bodyStart, $end - $bodyStart)
}

$payloadStartMarker = '$payload = @(' + "`n"
$payloadStart = $normalizedWrapper.IndexOf($payloadStartMarker, [StringComparison]::Ordinal)
if ($payloadStart -lt 0) {
    throw 'Could not locate the compressed launcher overlay payload array.'
}
$payloadBodyStart = $payloadStart + $payloadStartMarker.Length
$payloadEnd = $normalizedWrapper.IndexOf("`n) -join ''", $payloadBodyStart, [StringComparison]::Ordinal)
if ($payloadEnd -lt 0) {
    throw 'The compressed launcher overlay payload array has no closing marker.'
}
$payloadBody = $normalizedWrapper.Substring($payloadBodyStart, $payloadEnd - $payloadBodyStart)

$payloadParts = @(
    [regex]::Matches(
        $payloadBody,
        "(?m)^\s*'(?<part>[^']+)'\s*$"
    )
)
if ($payloadParts.Count -eq 0) {
    throw 'The compressed launcher overlay payload array is empty.'
}

$payloadText = ($payloadParts | ForEach-Object { $_.Groups['part'].Value }) -join ''
$compressed = [Convert]::FromBase64String($payloadText)
$input = [System.IO.MemoryStream]::new($compressed, $false)
$gzip = [System.IO.Compression.GzipStream]::new(
    $input,
    [System.IO.Compression.CompressionMode]::Decompress
)
$output = [System.IO.MemoryStream]::new()
try {
    $gzip.CopyTo($output)
} finally {
    $gzip.Dispose()
    $input.Dispose()
}
$embeddedTransformer = [System.Text.UTF8Encoding]::new($false).GetString($output.ToArray())
$output.Dispose()

$legacyTransformer = Get-EmbeddedHereString -VariableName 'legacyBridgeTransformer'
$sourceAwareTransformer = Get-EmbeddedHereString -VariableName 'sourceAwareBridgeTransformer'
$strictRequiredReplacers = Get-EmbeddedHereString -VariableName 'strictRequiredReplacers'
$idempotentRequiredReplacers = Get-EmbeddedHereString -VariableName 'idempotentRequiredReplacers'

$legacyCount = ([regex]::Matches($embeddedTransformer, [regex]::Escape($legacyTransformer))).Count
if ($legacyCount -ne 1) {
    throw "Embedded overlay must contain exactly one legacy bridge transformer; found $legacyCount."
}

$patchedTransformer = $embeddedTransformer.Replace($legacyTransformer, $sourceAwareTransformer)
$strictReplacerCount = ([regex]::Matches(
    $patchedTransformer,
    [regex]::Escape($strictRequiredReplacers)
)).Count
if ($strictReplacerCount -ne 1) {
    throw "Embedded overlay must contain exactly one strict required-replacement helper block; found $strictReplacerCount."
}
$patchedTransformer = $patchedTransformer.Replace(
    $strictRequiredReplacers,
    $idempotentRequiredReplacers
)
if ($patchedTransformer.Contains('Desktop update bridge import expected one match')) {
    throw 'Patched transformer retained the brittle literal import assertion.'
}
if (-not $patchedTransformer.Contains("Desktop update bridge expected one './hermes-local-control' import")) {
    throw 'Patched transformer is missing the source-aware bridge assertion.'
}

$bridgeTransform = [scriptblock]::Create($sourceAwareTransformer)

function Invoke-BridgeImportTransform {
    param([Parameter(Mandatory)][string] $Text)

    $main = $Text
    . $bridgeTransform
    return $main
}

$currentImport = @'
import {
  execFile,
  spawn
} from 'node:child_process'

import {
  configureHermesLocalDesktopEnvironment,
  ensureHermesLocalWorkstationReady,
  hermesLocalTuiLaunch,
  isHermesLocalModelSwitchActive,
  registerHermesLocalControlIpc,
  retainNewUpstreamControlCapability
} from './hermes-local-control'

const sentinel = true
'@

$updatedImport = Invoke-BridgeImportTransform -Text $currentImport
foreach ($required in @(
    'applyHermesLocalDesktopUpdate',
    'checkHermesLocalDesktopUpdates',
    'retainNewUpstreamControlCapability'
)) {
    $count = ([regex]::Matches($updatedImport, "(?m)^\s*$([regex]::Escape($required)),?\s*$")).Count
    if ($count -ne 1) {
        throw "Bridge transform must retain '$required' exactly once; found $count."
    }
}
if (-not $updatedImport.Contains('const sentinel = true')) {
    throw 'Bridge transform changed source outside the target import.'
}

$childProcessPattern = "(?m)^import\s*\{\s*(?<members>[^{}]*)\}\s*from\s*'node:child_process'\s*$"
$childProcessMatches = [regex]::Matches($updatedImport, $childProcessPattern)
if ($childProcessMatches.Count -ne 1) {
    throw "Bridge transform must preserve exactly one node:child_process import; found $($childProcessMatches.Count)."
}
$childProcessMembers = $childProcessMatches[0].Groups['members'].Value
foreach ($expectedChildProcessMember in @('execFile', 'spawn')) {
    if ($childProcessMembers -notmatch "(?m)^\s*$([regex]::Escape($expectedChildProcessMember)),?\s*$") {
        throw "Bridge transform removed '$expectedChildProcessMember' from node:child_process."
    }
}
foreach ($forbiddenChildProcessMember in @(
    'applyHermesLocalDesktopUpdate',
    'checkHermesLocalDesktopUpdates'
)) {
    if ($childProcessMembers -match [regex]::Escape($forbiddenChildProcessMember)) {
        throw "Bridge transform incorrectly inserted '$forbiddenChildProcessMember' into node:child_process."
    }
}

$idempotentImport = Invoke-BridgeImportTransform -Text $updatedImport
if ($idempotentImport -ne $updatedImport) {
    throw 'Bridge import transform is not idempotent.'
}

$duplicateImport = $currentImport + @'

import { anotherControlCapability } from './hermes-local-control'
'@
$duplicateRejected = $false
try {
    $null = Invoke-BridgeImportTransform -Text $duplicateImport
} catch {
    if ($_.Exception.Message -notmatch "expected one './hermes-local-control' import; found 2") {
        throw
    }
    $duplicateRejected = $true
}
if (-not $duplicateRejected) {
    throw 'Bridge import transform accepted duplicate control-module imports.'
}

# Every overlay edit must accept either the untouched source form or exactly
# one already-applied form. This lets a pinned integration tree contain a
# previously promoted overlay without making the next launcher build fail.
. ([scriptblock]::Create($idempotentRequiredReplacers))
$literalSource = 'before'
$literalApplied = 'after'
if ((Replace-RequiredLiteral -Text $literalSource -Old 'before' -New 'after' -Description 'literal probe') -ne 'after') {
    throw 'Idempotent literal replacement did not apply the source form.'
}
if ((Replace-RequiredLiteral -Text $literalApplied -Old 'before' -New 'after' -Description 'literal probe') -ne 'after') {
    throw 'Idempotent literal replacement rejected the already-applied form.'
}
if ((Replace-RequiredRegex -Text $literalSource -Pattern '^before$' -Replacement 'after' -Description 'regex probe') -ne 'after') {
    throw 'Idempotent regex replacement did not apply the source form.'
}
if ((Replace-RequiredRegex -Text $literalApplied -Pattern '^before$' -Replacement 'after' -Description 'regex probe') -ne 'after') {
    throw 'Idempotent regex replacement rejected the already-applied form.'
}
$newlineSource = "before`none`ntwo"
$newlineApplied = Replace-RequiredRegex `
    -Text $newlineSource `
    -Pattern '^before' `
    -Replacement "`n" `
    -Description 'newline source probe'
if ($newlineApplied -ne "`n`none`ntwo") {
    throw 'Regex replacement rejected a valid source match because its replacement was non-unique.'
}
$pollerApplied = 'export function startUpdatePoller() { return }'
if ((Replace-RequiredRegex `
    -Text $pollerApplied `
    -Pattern 'window\.hermesDesktop\?\.localWorkstation' `
    -Replacement "`n" `
    -Description 'Hermes Local update poller bypass') -ne $pollerApplied) {
    throw 'Poller bypass replacement rejected the semantically already-applied form.'
}
$pollerDivergenceRejected = $false
try {
    $null = Replace-RequiredRegex `
        -Text 'if (window.hermesDesktop?.localWorkstation) { changedUpstreamCode() }' `
        -Pattern 'original pattern that no longer matches' `
        -Replacement "`n" `
        -Description 'Hermes Local update poller bypass'
} catch {
    $pollerDivergenceRejected = $true
}
if (-not $pollerDivergenceRejected) {
    throw 'Poller bypass replacement accepted a divergent local-workstation block.'
}
$divergenceRejected = $false
try {
    $null = Replace-RequiredLiteral -Text 'different' -Old 'before' -New 'after' -Description 'literal probe'
} catch {
    if ($_.Exception.Message -notmatch 'source=0 applied=0') {
        throw
    }
    $divergenceRejected = $true
}
if (-not $divergenceRejected) {
    throw 'Idempotent replacement accepted an unknown divergent source form.'
}

Write-Host 'Hermes Launcher overlay import-boundary and checkout EOL tests passed.'
