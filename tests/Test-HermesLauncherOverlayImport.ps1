[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repositoryRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$wrapperPath = Join-Path $repositoryRoot 'Apply-Hermes-LauncherOverlay.ps1'
$wrapper = [System.IO.File]::ReadAllText($wrapperPath)

function Get-EmbeddedHereString {
    param([Parameter(Mandatory)][string] $VariableName)

    $pattern = "(?ms)^\$$([regex]::Escape($VariableName)) = @'\r?\n(?<body>.*?)\r?\n'@\s*$"
    $match = [regex]::Match($wrapper, $pattern)
    if (-not $match.Success) {
        throw "Could not locate embedded here-string '$VariableName'."
    }
    return $match.Groups['body'].Value
}

$payloadMatch = [regex]::Match($wrapper, "(?m)^\$payload = '(?<payload>[^']+)'\s*$")
if (-not $payloadMatch.Success) {
    throw 'Could not locate the compressed launcher overlay payload.'
}

$compressed = [Convert]::FromBase64String($payloadMatch.Groups['payload'].Value)
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

$legacyCount = ([regex]::Matches($embeddedTransformer, [regex]::Escape($legacyTransformer))).Count
if ($legacyCount -ne 1) {
    throw "Embedded overlay must contain exactly one legacy bridge transformer; found $legacyCount."
}

$patchedTransformer = $embeddedTransformer.Replace($legacyTransformer, $sourceAwareTransformer)
if ($patchedTransformer.Contains("Desktop update bridge import expected one match")) {
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

Write-Host 'Hermes Launcher overlay import tests passed.'
