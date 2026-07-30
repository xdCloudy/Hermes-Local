[CmdletBinding()]
param(
    [switch] $Quick,
    [switch] $NonInteractive,
    [switch] $ReportOnly,
    [switch] $SelfTest
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'scripts\Common-Hermes.psm1') -Force
Import-Module (Join-Path $PSScriptRoot 'scripts\Hermes-Configuration.psm1') -Force

$script:temporaryFiles = [System.Collections.Generic.List[string]]::new()
$script:benchmarkRequestPath = Resolve-HermesPath 'data\runtime\benchmark.request.json'
$script:wasRunning = $false
$script:stackRestored = $false
$script:restartProfile = ''
$script:restorationRecoveredByReplacement = $false
$script:restorationInitialError = $null

$benchmarkLibrary = Resolve-HermesPath 'scripts\benchmark'
foreach ($part in @(
    'Benchmark-Common.ps1',
    'Benchmark-Lifecycle.ps1',
    'Benchmark-Arguments.ps1',
    'Benchmark-Runner.ps1',
    'Benchmark-Cases.ps1',
    'Benchmark-Report.ps1',
    'Benchmark-Main.ps1'
)) {
    . (Join-Path $benchmarkLibrary $part)
}
