[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$root = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))

function Assert-Contains {
    param(
        [Parameter(Mandatory)][string] $Text,
        [Parameter(Mandatory)][string] $Expected,
        [Parameter(Mandatory)][string] $Message
    )
    if (-not $Text.Contains($Expected, [System.StringComparison]::Ordinal)) {
        throw $Message
    }
}

$benchmarkPath = Join-Path $root 'Benchmark-Hermes-Local.ps1'
$benchmarkFiles = @(
    Get-Item -LiteralPath $benchmarkPath
    Get-ChildItem -LiteralPath (Join-Path $root 'scripts\benchmark') -Filter 'Benchmark-*.ps1' -File | Sort-Object Name
)
$benchmark = ($benchmarkFiles | ForEach-Object { Get-Content -Raw -LiteralPath $_.FullName }) -join [Environment]::NewLine
$supervisor = Get-Content -Raw -LiteralPath (Join-Path $root 'scripts\supervisor\Hermes-Supervisor.ps1')
$start = Get-Content -Raw -LiteralPath (Join-Path $root 'Start-Hermes-Local.ps1')
$stop = Get-Content -Raw -LiteralPath (Join-Path $root 'Stop-Hermes-Local.ps1')
$gatewaySnapshot = Get-Content -Raw -LiteralPath (Join-Path $root 'scripts\gateway_snapshot.py')

Assert-Contains -Text $benchmark -Expected 'benchmark.request.json' -Message 'Benchmark lifecycle request contract is missing.'
Assert-Contains -Text $benchmark -Expected 'Enter-HermesBenchmarkMode' -Message 'Benchmark does not request model-only maintenance mode.'
Assert-Contains -Text $benchmark -Expected 'Resolve-BenchmarkGpuLayerArguments' -Message 'Benchmark does not translate abstract GPU-layer settings.'
Assert-Contains -Text $benchmark -Expected 'New-EmptyTelemetry' -Message 'Benchmark failed-case telemetry schema is missing.'
Assert-Contains -Text $benchmark -Expected 'foreach ($context in $contextTargets)' -Message 'Context cases are not flattened into independent cases.'
if ($benchmark.Contains("'-ngl', [string]`$benchmarkProfile.gpu.layers", [System.StringComparison]::Ordinal)) {
    throw 'Benchmark still forwards profile gpu.layers directly to llama-bench.'
}
if ($benchmark.Contains("Resolve-HermesPath 'Stop-Hermes-Local.ps1'", [System.StringComparison]::Ordinal)) {
    throw 'Benchmark must not stop the complete Desktop, gateway and model stack.'
}

Assert-Contains -Text $supervisor -Expected "-Phase 'benchmarking'" -Message 'Supervisor benchmark phase is missing.'
Assert-Contains -Text $supervisor -Expected "Stop-ManagedProcess -Process `$modelProcess -Name 'llama-server for benchmark access'" -Message 'Supervisor does not stop only the model for benchmark access.'
Assert-Contains -Text $supervisor -Expected 'restoring llama-server without restarting Desktop services' -Message 'Supervisor model-only restoration contract is missing.'
Assert-Contains -Text $start -Expected "'benchmark-preparing', 'benchmarking', 'starting-model'" -Message 'Desktop startup does not accept benchmark lifecycle readiness.'
Assert-Contains -Text $gatewaySnapshot -Expected 'runtimeStaleGraceApplied' -Message 'Gateway snapshot lacks bounded transient-staleness handling.'
Assert-Contains -Text $gatewaySnapshot -Expected '_DEFAULT_STALE_GRACE_SECONDS' -Message 'Gateway staleness grace is not explicitly bounded.'
Assert-Contains -Text $stop -Expected 'Get-DescendantProcessIds' -Message 'Stop script does not track the complete supervisor process tree.'
Assert-Contains -Text $stop -Expected '/T /F' -Message 'Stop fallback is not forceful after the graceful timeout.'
Assert-Contains -Text $stop -Expected 'replacement Hermes Local supervisor' -Message 'Stop script does not detect immediate replacement supervisors.'

& $benchmarkPath -SelfTest -NonInteractive
if ($LASTEXITCODE -ne 0) {
    throw "Benchmark portability self-test exited with code $LASTEXITCODE."
}

Write-Host 'Benchmark lifecycle and portability contract passed.'
