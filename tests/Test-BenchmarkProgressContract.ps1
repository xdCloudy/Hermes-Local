[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$root = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
Import-Module (Join-Path $root 'scripts\Common-Hermes.psm1') -Force
. (Join-Path $root 'scripts\benchmark\Benchmark-Common.ps1')
. (Join-Path $root 'scripts\benchmark\Benchmark-Progress.ps1')

function Assert-Equal {
    param(
        [AllowNull()][object] $Actual,
        [AllowNull()][object] $Expected,
        [Parameter(Mandatory)][string] $Message
    )
    if ($Actual -ne $Expected) {
        throw "$Message Expected '$Expected', received '$Actual'."
    }
}

$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) "hermes-benchmark-progress-$([guid]::NewGuid().ToString('N'))"
[System.IO.Directory]::CreateDirectory($tempRoot) | Out-Null

$originalTaskId = $env:HERMES_LOCAL_TASK_ID
try {
    $script:benchmarkProgressPath = Join-Path $tempRoot 'benchmark-progress.json'
    $script:benchmarkCancelPath = Join-Path $tempRoot 'benchmark-cancel.json'
    $script:benchmarkTaskId = $null
    $script:benchmarkProgressStartedAt = $null
    $script:benchmarkProgressMode = $null
    $script:benchmarkProgressTerminalStatus = $null
    $script:benchmarkCancellationObserved = $false
    $env:HERMES_LOCAL_TASK_ID = 'benchmark-task-fixture'

    Initialize-BenchmarkProgress -Mode quick
    $initial = Get-Content -Raw -LiteralPath $script:benchmarkProgressPath | ConvertFrom-Json
    Assert-Equal $initial.taskId 'benchmark-task-fixture' 'Progress did not preserve the Desktop task identity.'
    Assert-Equal $initial.stage 'validation' 'Initial stage is not validation.'
    Assert-Equal $initial.mode 'indeterminate' 'Initial validation should be indeterminate.'
    Assert-Equal $initial.status 'running' 'Initial progress should be active.'

    Write-BenchmarkProgress `
        -Stage 'prompt-execution' `
        -Message 'Completed two real cases.' `
        -CompletedUnits 2 `
        -TotalUnits 4 `
        -EstimatedRemainingSeconds 12.5

    $determinate = Get-Content -Raw -LiteralPath $script:benchmarkProgressPath | ConvertFrom-Json
    Assert-Equal $determinate.completedUnits 2 'Completed units were not persisted.'
    Assert-Equal $determinate.totalUnits 4 'Total units were not persisted.'
    Assert-Equal $determinate.percent 50 'Determinate percentage was not derived from real units.'
    Assert-Equal $determinate.estimatedRemainingSeconds 12.5 'Defensible remaining estimate was not persisted.'

    @{
        schemaVersion = 1
        taskId = 'different-task'
        ownerPid = $PID
        requestedAt = (Get-Date).ToUniversalTime().ToString('o')
    } | ConvertTo-Json | Set-Content -LiteralPath $script:benchmarkCancelPath -Encoding utf8
    Assert-Equal (Test-BenchmarkCancellationRequested) $false 'A stale cancellation marker targeted another task.'

    @{
        schemaVersion = 1
        taskId = 'benchmark-task-fixture'
        ownerPid = $PID
        requestedAt = (Get-Date).ToUniversalTime().ToString('o')
    } | ConvertTo-Json | Set-Content -LiteralPath $script:benchmarkCancelPath -Encoding utf8

    $cancelled = $false
    try {
        Assert-BenchmarkNotCancelled
    } catch [System.OperationCanceledException] {
        $cancelled = $true
    }
    Assert-Equal $cancelled $true 'A matching cancellation request was not observed.'
    $cancelling = Get-Content -Raw -LiteralPath $script:benchmarkProgressPath | ConvertFrom-Json
    Assert-Equal $cancelling.status 'cancelling' 'Cancellation did not enter the shared cancelling state.'
    Assert-Equal $cancelling.stage 'restoration' 'Cancellation did not move to the safe restoration stage.'

    Remove-BenchmarkCancellationRequest
    Assert-Equal (Test-Path -LiteralPath $script:benchmarkCancelPath) $false 'Owned cancellation marker was not released.'

    Complete-BenchmarkProgress `
        -Status cancelled `
        -Message 'Cancelled safely.' `
        -ResultPath 'benchmarks\results\latest.json' `
        -ErrorMessage 'Cancelled by test.'

    $terminal = Get-Content -Raw -LiteralPath $script:benchmarkProgressPath | ConvertFrom-Json
    Assert-Equal $terminal.status 'cancelled' 'Terminal cancellation was not durable.'
    Assert-Equal $terminal.result.report 'benchmarks/results/latest.json' 'Result link was not normalized.'
    Assert-Equal $terminal.result.log 'logs/benchmarks/benchmarks.log' 'Benchmark log link was not retained.'
    Assert-Equal ([bool]$terminal.completedAt) $true 'Terminal progress did not record completion time.'

    Write-Host 'Benchmark progress and cancellation contract passed.'
} finally {
    if ($null -eq $originalTaskId) {
        Remove-Item Env:HERMES_LOCAL_TASK_ID -ErrorAction SilentlyContinue
    } else {
        $env:HERMES_LOCAL_TASK_ID = $originalTaskId
    }
    Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
}
