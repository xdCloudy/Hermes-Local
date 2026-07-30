[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$root = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))

function Assert-Contains {
    param(
        [Parameter(Mandatory)]
        [string] $Text,
        [Parameter(Mandatory)]
        [string] $Expected,
        [Parameter(Mandatory)]
        [string] $Message
    )
    if (-not $Text.Contains($Expected, [System.StringComparison]::Ordinal)) {
        throw $Message
    }
}

$benchmark = Get-Content -Raw -LiteralPath (Join-Path $root 'Benchmark-Hermes-Local.ps1')
$supervisor = Get-Content -Raw -LiteralPath (Join-Path $root 'scripts\supervisor\Hermes-Supervisor.ps1')
$start = Get-Content -Raw -LiteralPath (Join-Path $root 'Start-Hermes-Local.ps1')

Assert-Contains -Text $benchmark -Expected 'benchmark.request.json' -Message 'Benchmark lifecycle request contract is missing.'
Assert-Contains -Text $benchmark -Expected 'Enter-HermesBenchmarkMode' -Message 'Benchmark does not request model-only maintenance mode.'
if ($benchmark.Contains("Resolve-HermesPath 'Stop-Hermes-Local.ps1'", [System.StringComparison]::Ordinal)) {
    throw 'Benchmark must not stop the complete Desktop, gateway and model stack.'
}
Assert-Contains -Text $supervisor -Expected "-Phase 'benchmarking'" -Message 'Supervisor benchmark phase is missing.'
Assert-Contains -Text $supervisor -Expected "Stop-ManagedProcess -Process `$modelProcess -Name 'llama-server for benchmark access'" -Message 'Supervisor does not stop only the model for benchmark access.'
Assert-Contains -Text $supervisor -Expected 'restoring llama-server without restarting Desktop services' -Message 'Supervisor model-only restoration contract is missing.'
Assert-Contains -Text $start -Expected "'benchmark-preparing', 'benchmarking', 'starting-model'" -Message 'Desktop startup does not accept benchmark lifecycle readiness.'

Write-Host 'Benchmark lifecycle contract passed.'
