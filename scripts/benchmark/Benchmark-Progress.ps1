function Initialize-BenchmarkProgress {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [ValidateSet('quick', 'full')]
        [string] $Mode
    )

    $script:benchmarkTaskId = if ($env:HERMES_LOCAL_TASK_ID -and $env:HERMES_LOCAL_TASK_ID -match '^[A-Za-z0-9_-]{1,128}$') {
        [string]$env:HERMES_LOCAL_TASK_ID
    } else {
        [guid]::NewGuid().ToString('N')
    }
    $script:benchmarkProgressStartedAt = (Get-Date).ToUniversalTime()
    $script:benchmarkProgressMode = $Mode
    $script:benchmarkProgressTerminalStatus = $null
    $script:benchmarkCancellationObserved = $false

    Remove-BenchmarkCancellationRequest
    Write-BenchmarkProgress `
        -Stage 'validation' `
        -Message 'Validating the benchmark configuration and model artifacts.' `
        -Indeterminate
}

function Get-BenchmarkProgressElapsedSeconds {
    if (-not $script:benchmarkProgressStartedAt) {
        return 0
    }
    return [math]::Round(((Get-Date).ToUniversalTime() - $script:benchmarkProgressStartedAt).TotalSeconds, 3)
}

function Get-BenchmarkProgressPercent {
    param(
        [AllowNull()][object] $CompletedUnits,
        [AllowNull()][object] $TotalUnits
    )

    if ($null -eq $CompletedUnits -or $null -eq $TotalUnits) {
        return $null
    }
    $completed = [double]$CompletedUnits
    $total = [double]$TotalUnits
    if ($total -le 0 -or $completed -lt 0) {
        return $null
    }
    return [math]::Round(([math]::Min($completed, $total) / $total) * 100, 1)
}

function Write-BenchmarkProgress {
    [CmdletBinding(DefaultParameterSetName = 'Indeterminate')]
    param(
        [Parameter(Mandatory)]
        [ValidateSet(
            'validation',
            'runtime-preparation',
            'model-loading',
            'warm-up',
            'prompt-execution',
            'aggregation',
            'report-generation',
            'restoration',
            'complete'
        )]
        [string] $Stage,

        [Parameter(Mandatory)]
        [string] $Message,

        [Parameter(ParameterSetName = 'Determinate', Mandatory)]
        [ValidateRange(0, 2147483647)]
        [int] $CompletedUnits,

        [Parameter(ParameterSetName = 'Determinate', Mandatory)]
        [ValidateRange(1, 2147483647)]
        [int] $TotalUnits,

        [Parameter(ParameterSetName = 'Indeterminate')]
        [switch] $Indeterminate,

        [AllowNull()]
        [Nullable[double]] $EstimatedRemainingSeconds,

        [ValidateSet('running', 'cancelling', 'succeeded', 'failed', 'cancelled')]
        [string] $Status = 'running',

        [AllowNull()]
        [Nullable[int]] $WorkerPid,

        [AllowNull()]
        [string] $ResultPath,

        [AllowNull()]
        [string] $ErrorMessage
    )

    if (-not $script:benchmarkTaskId) {
        return
    }

    $now = (Get-Date).ToUniversalTime()
    $percent = if ($PSCmdlet.ParameterSetName -eq 'Determinate') {
        Get-BenchmarkProgressPercent -CompletedUnits $CompletedUnits -TotalUnits $TotalUnits
    } else {
        $null
    }
    $completed = if ($PSCmdlet.ParameterSetName -eq 'Determinate') { $CompletedUnits } else { $null }
    $total = if ($PSCmdlet.ParameterSetName -eq 'Determinate') { $TotalUnits } else { $null }
    $estimate = if ($null -ne $EstimatedRemainingSeconds -and $EstimatedRemainingSeconds -ge 0) {
        [math]::Round([double]$EstimatedRemainingSeconds, 1)
    } else {
        $null
    }

    $document = [ordered]@{
        schemaVersion = 1
        taskId = $script:benchmarkTaskId
        ownerPid = $PID
        workerPid = if ($null -ne $WorkerPid) { [int]$WorkerPid } else { $null }
        status = $Status
        stage = $Stage
        mode = if ($PSCmdlet.ParameterSetName -eq 'Determinate') { 'determinate' } else { 'indeterminate' }
        completedUnits = $completed
        totalUnits = $total
        percent = $percent
        elapsedSeconds = Get-BenchmarkProgressElapsedSeconds
        estimatedRemainingSeconds = $estimate
        message = Protect-HermesLogText $Message
        result = if ($ResultPath) {
            [ordered]@{
                report = $ResultPath.Replace('\', '/')
                log = 'logs/benchmarks/benchmarks.log'
            }
        } else {
            [ordered]@{
                report = $null
                log = 'logs/benchmarks/benchmarks.log'
            }
        }
        failure = if ($ErrorMessage) {
            [ordered]@{
                code = if ($Status -eq 'cancelled') { 'benchmark-cancelled' } else { 'benchmark-failed' }
                message = Protect-HermesLogText $ErrorMessage
            }
        } else {
            $null
        }
        startedAt = $script:benchmarkProgressStartedAt.ToString('o')
        updatedAt = $now.ToString('o')
        completedAt = if ($Status -in @('succeeded', 'failed', 'cancelled')) { $now.ToString('o') } else { $null }
    }

    Write-HermesAtomicText `
        -Path $script:benchmarkProgressPath `
        -Content (($document | ConvertTo-Json -Depth 12) + [Environment]::NewLine)

    $units = if ($null -ne $completed -and $null -ne $total) { " $completed/$total" } else { '' }
    $estimateText = if ($null -ne $estimate) { " · approximately $estimate seconds remaining" } else { '' }
    Write-Host "::hermes-benchmark-stage::$Stage::$($document.message)"
    Write-Host "Benchmark progress: $Stage$units · $($document.message)$estimateText"

    if ($Status -in @('succeeded', 'failed', 'cancelled')) {
        $script:benchmarkProgressTerminalStatus = $Status
    }
}

function Test-BenchmarkCancellationRequested {
    [CmdletBinding()]
    param()

    if (-not (Test-Path -LiteralPath $script:benchmarkCancelPath -PathType Leaf)) {
        return $false
    }

    try {
        $request = Get-Content -Raw -LiteralPath $script:benchmarkCancelPath | ConvertFrom-Json
        $requestTaskId = [string](Get-BenchmarkValue -Record $request -Name taskId -Default '')
        $ownerPid = [int](Get-BenchmarkValue -Record $request -Name ownerPid -Default 0)
        return $requestTaskId -eq $script:benchmarkTaskId -and ($ownerPid -eq 0 -or $ownerPid -eq $PID)
    } catch {
        Write-HermesLog -Component benchmarks -Level WARN -Message "Ignoring unreadable benchmark cancellation request: $($_.Exception.Message)"
        return $false
    }
}

function Assert-BenchmarkNotCancelled {
    [CmdletBinding()]
    param(
        [string] $Message = 'Benchmark cancellation was requested.'
    )

    if (-not (Test-BenchmarkCancellationRequested)) {
        return
    }

    $script:benchmarkCancellationObserved = $true
    Write-BenchmarkProgress `
        -Stage 'restoration' `
        -Message 'Cancellation accepted; stopping at the next safe boundary and restoring the model stack.' `
        -Status 'cancelling' `
        -Indeterminate
    throw [System.OperationCanceledException]::new($Message)
}

function Remove-BenchmarkCancellationRequest {
    [CmdletBinding()]
    param()

    if (-not (Test-Path -LiteralPath $script:benchmarkCancelPath -PathType Leaf)) {
        return
    }

    try {
        $request = Get-Content -Raw -LiteralPath $script:benchmarkCancelPath | ConvertFrom-Json
        $requestTaskId = [string](Get-BenchmarkValue -Record $request -Name taskId -Default '')
        if (-not $script:benchmarkTaskId -or $requestTaskId -eq $script:benchmarkTaskId) {
            Remove-Item -LiteralPath $script:benchmarkCancelPath -Force -ErrorAction SilentlyContinue
        }
    } catch {
        Remove-Item -LiteralPath $script:benchmarkCancelPath -Force -ErrorAction SilentlyContinue
    }
}

function Complete-BenchmarkProgress {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [ValidateSet('succeeded', 'failed', 'cancelled')]
        [string] $Status,

        [Parameter(Mandatory)]
        [string] $Message,

        [AllowNull()]
        [string] $ResultPath,

        [AllowNull()]
        [string] $ErrorMessage
    )

    if ($script:benchmarkProgressTerminalStatus) {
        return
    }

    Write-BenchmarkProgress `
        -Stage 'complete' `
        -Message $Message `
        -Status $Status `
        -ResultPath $ResultPath `
        -ErrorMessage $ErrorMessage `
        -Indeterminate
}
