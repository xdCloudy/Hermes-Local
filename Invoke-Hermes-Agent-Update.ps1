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

if (-not (Test-Path -LiteralPath $updater -PathType Leaf)) {
    throw "Hermes Agent updater is missing: $updater"
}
if (-not (Test-Path -LiteralPath $activeObjects -PathType Container)) {
    throw "The managed Hermes Agent object database is missing: $activeObjects. Run Repair-Hermes-Local.ps1 first."
}

$pwsh = (Get-Command pwsh.exe -ErrorAction Stop).Source
$previousAlternates = [Environment]::GetEnvironmentVariable(
    'GIT_ALTERNATE_OBJECT_DIRECTORIES',
    [EnvironmentVariableTarget]::Process
)
$pathSeparator = [System.IO.Path]::PathSeparator
$alternateObjects = if ([string]::IsNullOrWhiteSpace($previousAlternates)) {
    $activeObjects
} else {
    "$activeObjects$pathSeparator$previousAlternates"
}

$arguments = @(
    '-NoLogo',
    '-NoProfile',
    '-ExecutionPolicy', 'Bypass',
    '-File', $updater,
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
    # Later Hermes Local patches reference preimage blobs created by earlier
    # integration commits. Those blobs are not part of the upstream repository,
    # so expose the active, verified integration object database while staging.
    [Environment]::SetEnvironmentVariable(
        'GIT_ALTERNATE_OBJECT_DIRECTORIES',
        $alternateObjects,
        [EnvironmentVariableTarget]::Process
    )

    & $pwsh @arguments
    $exitCode = $LASTEXITCODE
} finally {
    [Environment]::SetEnvironmentVariable(
        'GIT_ALTERNATE_OBJECT_DIRECTORIES',
        $previousAlternates,
        [EnvironmentVariableTarget]::Process
    )
}

exit $exitCode
