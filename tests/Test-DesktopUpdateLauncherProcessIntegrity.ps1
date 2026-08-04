[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$promotionPath = Join-Path $root 'scripts\desktop-update\DesktopUpdate-Promotion.ps1'

if (-not (Test-Path -LiteralPath $promotionPath -PathType Leaf)) {
    throw "Desktop promotion component is missing: $promotionPath"
}

$content = [IO.File]::ReadAllText($promotionPath)

foreach ($required in @(
    'Get-HermesDesktopLauncherBrowserProcesses',
    "Get-CimInstance Win32_Process",
    "[string]$_.Name -ne 'Hermes Launcher.exe'",
    "StartsWith(",
    "[StringComparison]::OrdinalIgnoreCase",
    "[string]$_.CommandLine -notmatch '(?i)(?:^|\\s)--type='",
    'Stop-Process',
    'Hermes Launcher browser processes remained active after shutdown'
)) {
    if (-not $content.Contains($required, [StringComparison]::Ordinal)) {
        throw "Launcher process-integrity contract is missing: $required"
    }
}

if (
    $content.Contains(
        "[IO.Path]::GetFullPath((Join-Path $root 'dist\\Hermes Launcher.exe'))",
        [StringComparison]::Ordinal
    )
) {
    throw 'Launcher detection must not be limited to the active dist path.'
}

$parentWait = $content.IndexOf(
    'Test-HermesDesktopProcessIdentity',
    [StringComparison]::Ordinal
)
$rootProcessWait = $content.IndexOf(
    '@(Get-HermesDesktopLauncherBrowserProcesses).Count -gt 0',
    $parentWait,
    [StringComparison]::Ordinal
)
$forcedStop = $content.IndexOf(
    'Stop-Process',
    $rootProcessWait,
    [StringComparison]::Ordinal
)
$promotion = $content.IndexOf(
    'Move-Item -LiteralPath $pendingDist -Destination $dist',
    $forcedStop,
    [StringComparison]::Ordinal
)

if (
    $parentWait -lt 0 -or
    $rootProcessWait -le $parentWait -or
    $forcedStop -le $rootProcessWait -or
    $promotion -le $forcedStop
) {
    throw 'Promotion must wait for the recorded parent, clear stale root-scoped browser processes, then move pending-dist into dist.'
}

Write-Host 'Desktop update launcher process-integrity contract passed.'
