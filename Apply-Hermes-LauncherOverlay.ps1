[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidateSet('Apply', 'Restore')][string] $Mode,
    [Parameter(Mandatory)][string] $StatePath,
    [string] $RepositoryRoot = $PSScriptRoot
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$payload = @(
    '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'
) -join ''
$compressed = [Convert]::FromBase64String($payload)
$input = [System.IO.MemoryStream]::new($compressed, $false)
$gzip = [System.IO.Compression.GzipStream]::new($input, [System.IO.Compression.CompressionMode]::Decompress)
$output = [System.IO.MemoryStream]::new()
try {
    $gzip.CopyTo($output)
} finally {
    $gzip.Dispose()
    $input.Dispose()
}
$source = [System.Text.UTF8Encoding]::new($false).GetString($output.ToArray())
$legacyBridgeTransformer = @'
    $old = @(
        'import {',
        '  configureHermesLocalDesktopEnvironment,',
        '  ensureHermesLocalWorkstationReady,',
        '  hermesLocalTuiLaunch,',
        '  isHermesLocalModelSwitchActive,',
        '  registerHermesLocalControlIpc',
        "} from './hermes-local-control'"
    ) -join "`n"
    $new = @(
        'import {',
        '  applyHermesLocalDesktopUpdate,',
        '  checkHermesLocalDesktopUpdates,',
        '  configureHermesLocalDesktopEnvironment,',
        '  ensureHermesLocalWorkstationReady,',
        '  hermesLocalTuiLaunch,',
        '  isHermesLocalModelSwitchActive,',
        '  registerHermesLocalControlIpc',
        "} from './hermes-local-control'"
    ) -join "`n"
    $main = Replace-RequiredLiteral -Text $main -Description 'Desktop update bridge import' -Old $old -New $new
'@
$sourceAwareBridgeTransformer = @'
    $controlImportPattern = "(?m)^import\s*\{\s*(?<members>[^{}]*)\}\s*from\s*'./hermes-local-control'\s*$"
    $controlImportMatches = [regex]::Matches($main, $controlImportPattern)
    if ($controlImportMatches.Count -ne 1) {
        throw "Desktop update bridge expected one './hermes-local-control' import; found $($controlImportMatches.Count)."
    }

    $controlImportMatch = $controlImportMatches[0]
    $existingMembers = [System.Collections.Generic.List[string]]::new()
    foreach ($member in @($controlImportMatch.Groups['members'].Value -split ',')) {
        $trimmedMember = $member.Trim()
        if (-not [string]::IsNullOrWhiteSpace($trimmedMember) -and -not $existingMembers.Contains($trimmedMember)) {
            $existingMembers.Add($trimmedMember)
        }
    }

    $requiredUpdateMembers = @(
        'applyHermesLocalDesktopUpdate',
        'checkHermesLocalDesktopUpdates'
    )
    $orderedMembers = [System.Collections.Generic.List[string]]::new()
    foreach ($member in $requiredUpdateMembers) {
        $orderedMembers.Add($member)
    }
    foreach ($member in $existingMembers) {
        if ($requiredUpdateMembers -notcontains $member) {
            $orderedMembers.Add($member)
        }
    }

    $renderedMembers = [System.Collections.Generic.List[string]]::new()
    for ($index = 0; $index -lt $orderedMembers.Count; $index += 1) {
        $suffix = if ($index -lt ($orderedMembers.Count - 1)) { ',' } else { '' }
        $renderedMembers.Add("  $($orderedMembers[$index])$suffix")
    }
    $updatedControlImport = (@('import {') + @($renderedMembers) + @("} from './hermes-local-control'")) -join "`n"
    $main = $main.Remove($controlImportMatch.Index, $controlImportMatch.Length).Insert(
        $controlImportMatch.Index,
        $updatedControlImport
    )
'@
if ($source.Contains($legacyBridgeTransformer)) {
    $source = $source.Replace($legacyBridgeTransformer, $sourceAwareBridgeTransformer)
} elseif (-not $source.Contains("Desktop update bridge expected one './hermes-local-control' import")) {
    throw 'The embedded launcher overlay transformer no longer contains the expected Desktop update bridge implementation.'
}
$strictRequiredReplacers = @'
function Replace-RequiredLiteral {
    param(
        [Parameter(Mandatory)][string] $Text,
        [Parameter(Mandatory)][string] $Old,
        [Parameter(Mandatory)][string] $New,
        [Parameter(Mandatory)][string] $Description
    )
    $count = ([regex]::Matches($Text, [regex]::Escape($Old))).Count
    if ($count -ne 1) { throw "$Description expected one match; found $count." }
    $Text.Replace($Old, $New)
}

function Replace-RequiredRegex {
    param(
        [Parameter(Mandatory)][string] $Text,
        [Parameter(Mandatory)][string] $Pattern,
        [Parameter(Mandatory)][string] $Replacement,
        [Parameter(Mandatory)][string] $Description
    )
    $regex = [regex]::new($Pattern, [System.Text.RegularExpressions.RegexOptions]::Singleline)
    $matches = $regex.Matches($Text)
    if ($matches.Count -ne 1) { throw "$Description expected one match; found $($matches.Count)." }
    $regex.Replace($Text, $Replacement, 1)
}
'@
$idempotentRequiredReplacers = @'
function Replace-RequiredLiteral {
    param(
        [Parameter(Mandatory)][string] $Text,
        [Parameter(Mandatory)][string] $Old,
        [Parameter(Mandatory)][string] $New,
        [Parameter(Mandatory)][string] $Description
    )
    $usesCrlf = $Text.Contains("`r`n")
    $normalizedText = $Text.Replace("`r`n", "`n")
    $sourceForm = $Old.Replace("`r`n", "`n")
    $appliedForm = $New.Replace("`r`n", "`n")
    $sourceCount = ([regex]::Matches($normalizedText, [regex]::Escape($sourceForm))).Count
    $appliedCount = ([regex]::Matches($normalizedText, [regex]::Escape($appliedForm))).Count
    if ($sourceCount -eq 1) {
        $updated = $normalizedText.Replace($sourceForm, $appliedForm)
        return $(if ($usesCrlf) { $updated.Replace("`n", "`r`n") } else { $updated })
    }
    if ($sourceCount -eq 0 -and $appliedCount -eq 1) {
        return $Text
    }
    throw "$Description expected one source match or one applied match; found source=$sourceCount applied=$appliedCount."
}

function Replace-RequiredRegex {
    param(
        [Parameter(Mandatory)][string] $Text,
        [Parameter(Mandatory)][string] $Pattern,
        [Parameter(Mandatory)][string] $Replacement,
        [Parameter(Mandatory)][string] $Description
    )
    $usesCrlf = $Text.Contains("`r`n")
    $normalizedText = $Text.Replace("`r`n", "`n")
    $regex = [regex]::new($Pattern, [System.Text.RegularExpressions.RegexOptions]::Singleline)
    $appliedForm = $Replacement.Replace("`r`n", "`n")
    $sourceCount = $regex.Matches($normalizedText).Count
    $appliedCount = ([regex]::Matches($normalizedText, [regex]::Escape($appliedForm))).Count
    if ($sourceCount -eq 1) {
        $updated = $regex.Replace($normalizedText, $appliedForm, 1)
        return $(if ($usesCrlf) { $updated.Replace("`n", "`r`n") } else { $updated })
    }
    if ($sourceCount -eq 0 -and $appliedCount -eq 1) {
        return $Text
    }
    if (
        $sourceCount -eq 0 -and
        $Description -eq 'Hermes Local update poller bypass' -and
        $normalizedText -notmatch 'window\.hermesDesktop\?\.localWorkstation'
    ) {
        return $Text
    }
    throw "$Description expected one source match or one applied match; found source=$sourceCount applied=$appliedCount."
}
'@
if (-not $source.Contains($strictRequiredReplacers)) {
    throw 'The embedded launcher overlay no longer contains the expected required-replacement helpers.'
}
$source = $source.Replace($strictRequiredReplacers, $idempotentRequiredReplacers)
$output.Dispose()
$transformer = [scriptblock]::Create($source)
& $transformer -Mode $Mode -StatePath $StatePath -RepositoryRoot $RepositoryRoot
