[CmdletBinding()]
param(
    [ValidateSet('Check', 'Apply', 'Rollback')]
    [string] $Mode = 'Apply',

    [ValidatePattern('^[0-9a-fA-F]{40}$')]
    [string] $TargetCommit,

    [ValidatePattern('^[A-Za-z0-9._/-]+$')]
    [string] $TargetBranch,

    [switch] $NonInteractive
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$updater = Join-Path $PSScriptRoot 'Update-Hermes-Agent.ps1'
$activeObjects = Join-Path $PSScriptRoot 'source\hermes-agent\.git\objects'
$generatedUpdater = Join-Path $PSScriptRoot '.Update-Hermes-Agent.generated.ps1'

if (-not (Test-Path -LiteralPath $updater -PathType Leaf)) {
    throw "Hermes Agent updater is missing: $updater"
}
if (-not (Test-Path -LiteralPath $activeObjects -PathType Container)) {
    throw "The managed Hermes Agent object database is missing: $activeObjects. Run Repair-Hermes-Local.ps1 first."
}

$pwsh = (Get-Command pwsh.exe -ErrorAction Stop).Source
$previousObjectHint = [Environment]::GetEnvironmentVariable(
    'HERMES_LOCAL_INTEGRATION_OBJECTS',
    [EnvironmentVariableTarget]::Process
)

$arguments = @(
    '-NoLogo',
    '-NoProfile',
    '-ExecutionPolicy', 'Bypass',
    '-File', $generatedUpdater,
    '-Mode', $Mode
)
if ($TargetCommit) {
    $arguments += @('-TargetCommit', $TargetCommit)
}
if ($TargetBranch) {
    $arguments += @('-TargetBranch', $TargetBranch)
}
if ($NonInteractive) {
    $arguments += '-NonInteractive'
}

$exitCode = 1
try {
    if (Test-Path -LiteralPath $generatedUpdater) {
        Remove-Item -LiteralPath $generatedUpdater -Force
    }

    $lines = [System.Collections.Generic.List[string]]::new()
    foreach ($line in [System.IO.File]::ReadAllLines($updater)) {
        [void] $lines.Add($line)
    }

    $matches = @()
    for ($index = 0; $index -lt $lines.Count; $index++) {
        if ($lines[$index].Trim() -eq "GIT_COMMITTER_EMAIL = 'hermes-local@localhost'") {
            $matches += $index
        }
    }
    if ($matches.Count -ne 1) {
        throw "Could not safely locate the updater's git am environment block. Found $($matches.Count) matches."
    }

    # Only git am needs the original integration object database. Applying the
    # alternate globally corrupts clone/index-pack negotiation with unresolved
    # deltas, so inject it into the patch subprocess environment alone.
    $lines.Insert(
        ([int] $matches[0]) + 1,
        "                GIT_ALTERNATE_OBJECT_DIRECTORIES = `$env:HERMES_LOCAL_INTEGRATION_OBJECTS"
    )

    [System.IO.File]::WriteAllLines(
        $generatedUpdater,
        $lines,
        [System.Text.UTF8Encoding]::new($false)
    )

    [Environment]::SetEnvironmentVariable(
        'HERMES_LOCAL_INTEGRATION_OBJECTS',
        $activeObjects,
        [EnvironmentVariableTarget]::Process
    )

    & $pwsh @arguments
    $exitCode = $LASTEXITCODE
} finally {
    if (Test-Path -LiteralPath $generatedUpdater) {
        Remove-Item -LiteralPath $generatedUpdater -Force
    }
    [Environment]::SetEnvironmentVariable(
        'HERMES_LOCAL_INTEGRATION_OBJECTS',
        $previousObjectHint,
        [EnvironmentVariableTarget]::Process
    )
}

exit $exitCode
