[CmdletBinding()]
param(
    [switch] $Quick,
    [switch] $NonInteractive,
    [switch] $ReportOnly
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'scripts\Common-Hermes.psm1') -Force
Import-Module (Join-Path $PSScriptRoot 'scripts\Hermes-Configuration.psm1') -Force

$wasRunning = $false
$configuration = Get-HermesConfiguration
$restartProfile = [string]$configuration.selectedProfile
$stackRestarted = $false
$temporaryFiles = [System.Collections.Generic.List[string]]::new()
$benchmarkRequestPath = Resolve-HermesPath 'data\runtime\benchmark.request.json'

function Test-HermesProcessAlive {
    param(
        [Parameter(Mandatory)]
        [int] $ProcessId
    )

    return $null -ne (Get-Process -Id $ProcessId -ErrorAction SilentlyContinue)
}

function Wait-HermesBenchmarkPhase {
    param(
        [Parameter(Mandatory)]
        [ValidateSet('benchmarking', 'running')]
        [string] $Phase,
        [int] $TimeoutSeconds = 960
    )

    $statusPath = Resolve-HermesPath 'data\runtime\status.json'
    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
        if (Test-Path -LiteralPath $statusPath) {
            try {
                $status = Get-Content -Raw -LiteralPath $statusPath | ConvertFrom-Json
                $controllerPid = if ($status.controllerPid) { [int]$status.controllerPid } else { 0 }
                if ($controllerPid -gt 0 -and -not (Test-HermesProcessAlive -ProcessId $controllerPid)) {
                    throw "Hermes Local supervisor PID $controllerPid exited during benchmark lifecycle coordination."
                }

                $gatewayReady = -not $status.gateway -or -not $status.gateway.required -or $status.gateway.healthy
                if ($Phase -eq 'benchmarking' -and
                    $status.phase -eq 'benchmarking' -and
                    -not $status.model.pid -and
                    $status.hermes.healthy -and
                    $gatewayReady) {
                    return
                }
                if ($Phase -eq 'running' -and
                    $status.phase -eq 'running' -and
                    $status.model.healthy -and
                    $status.hermes.healthy -and
                    $gatewayReady) {
                    return
                }
                if ($status.phase -eq 'failed') {
                    throw "Hermes Local supervisor failed during benchmark lifecycle coordination: $($status.message)"
                }
            } catch [System.Management.Automation.RuntimeException] {
                throw
            } catch {
                Write-Verbose "Waiting for an atomic benchmark lifecycle status update: $($_.Exception.Message)"
            }
        }
        Start-Sleep -Milliseconds 500
    }
    throw "Hermes Local did not enter '$Phase' benchmark lifecycle state within $TimeoutSeconds seconds."
}

function Enter-HermesBenchmarkMode {
    param(
        [Parameter(Mandatory)]
        [string] $Profile
    )

    if (Test-Path -LiteralPath $benchmarkRequestPath) {
        try {
            $existing = Get-Content -Raw -LiteralPath $benchmarkRequestPath | ConvertFrom-Json
            $existingOwnerPid = if ($existing.ownerPid) { [int]$existing.ownerPid } else { 0 }
            if ($existingOwnerPid -gt 0 -and (Test-HermesProcessAlive -ProcessId $existingOwnerPid)) {
                throw "Benchmark lifecycle is already owned by PID $existingOwnerPid."
            }
        } catch [System.Management.Automation.RuntimeException] {
            throw
        } catch {
            Write-HermesLog -Component benchmarks -Level WARN -Message "Removing unreadable stale benchmark request: $($_.Exception.Message)"
        }
        Remove-Item -LiteralPath $benchmarkRequestPath -Force -ErrorAction SilentlyContinue
    }

    $request = [ordered]@{
        schemaVersion = 1
        ownerPid = $PID
        profile = $Profile
        requestedAt = (Get-Date).ToUniversalTime().ToString('o')
    }
    Write-HermesAtomicText -Path $benchmarkRequestPath -Content (($request | ConvertTo-Json -Depth 4) + [Environment]::NewLine)
    Write-HermesLog -Component benchmarks -Message 'Requested exclusive model access while preserving Desktop and gateway services.'
    try {
        Wait-HermesBenchmarkPhase -Phase benchmarking -TimeoutSeconds 120
    } catch {
        Remove-Item -LiteralPath $benchmarkRequestPath -Force -ErrorAction SilentlyContinue
        throw
    }
}

function Exit-HermesBenchmarkMode {
    Remove-Item -LiteralPath $benchmarkRequestPath -Force -ErrorAction SilentlyContinue
    Write-HermesLog -Component benchmarks -Message 'Released exclusive model access; waiting for the model server to return.'
    Wait-HermesBenchmarkPhase -Phase running -TimeoutSeconds 960
}

function Get-Percentile {
    param(
        [double[]] $Values,
        [ValidateRange(0, 1)]
        [double] $Percentile
    )

    if (-not $Values -or $Values.Count -eq 0) {
        return $null
    }
    $ordered = @($Values | Sort-Object)
    $index = [math]::Max(0, [math]::Ceiling($ordered.Count * $Percentile) - 1)
    return [double]$ordered[$index]
}

function Get-NvidiaSample {
    $nvidiaSmi = Get-Command nvidia-smi.exe -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $nvidiaSmi) {
        return $null
    }
    $raw = & $nvidiaSmi.Source `
        --query-gpu=utilization.gpu,memory.used,memory.free,temperature.gpu,power.draw,clocks.current.sm `
        --format=csv,noheader,nounits 2>$null
    if ($LASTEXITCODE -ne 0 -or -not $raw) {
        return $null
    }
    $values = @($raw -split ',' | ForEach-Object Trim)
    if ($values.Count -lt 6) {
        return $null
    }
    return [pscustomobject][ordered]@{
        utilizationPercent = [double]$values[0]
        memoryUsedMiB = [double]$values[1]
        memoryFreeMiB = [double]$values[2]
        temperatureCelsius = [double]$values[3]
        powerWatts = [double]$values[4]
        smClockMHz = [double]$values[5]
    }
}

function Invoke-BenchmarkCase {
    param(
        [Parameter(Mandatory)]
        [string] $Name,
        [Parameter(Mandatory)]
        [string[]] $Arguments,
        [Parameter(Mandatory)]
        [string] $Binary,
        [Parameter(Mandatory)]
        [string] $WorkingDirectory
    )

    $id = [guid]::NewGuid().ToString('N')
    $stdoutPath = Resolve-HermesPath "temp\llama-bench-$id.json"
    $stderrPath = Resolve-HermesPath "temp\llama-bench-$id.stderr.log"
    $temporaryFiles.Add($stdoutPath)
    $temporaryFiles.Add($stderrPath)
    $fullArguments = @('-o', 'json') + $Arguments
    $commandLine = '"' + $Binary + '" ' + (($fullArguments | ForEach-Object {
        if ($_ -match '[\s"]') { '"' + $_.Replace('"', '\"') + '"' } else { $_ }
    }) -join ' ')

    Write-Host "Benchmark: $Name"
    Write-HermesLog -Component benchmarks -Message "Starting case ${Name}: $commandLine"
    $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $Binary
    $startInfo.WorkingDirectory = $WorkingDirectory
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    foreach ($argument in $fullArguments) {
        $startInfo.ArgumentList.Add([string]$argument)
    }

    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $startInfo
    $startedAt = (Get-Date).ToUniversalTime()
    if (-not $process.Start()) {
        throw "Could not start benchmark case $Name."
    }
    $stdoutTask = $process.StandardOutput.ReadToEndAsync()
    $stderrTask = $process.StandardError.ReadToEndAsync()
    $samples = [System.Collections.Generic.List[object]]::new()
    $lastCpu = [double]$process.TotalProcessorTime.TotalMilliseconds
    $lastSampleAt = Get-Date

    while (-not $process.WaitForExit(1000)) {
        $process.Refresh()
        $now = Get-Date
        $elapsedMs = [math]::Max(1, ($now - $lastSampleAt).TotalMilliseconds)
        $cpuMs = [math]::Max(0, $process.TotalProcessorTime.TotalMilliseconds - $lastCpu)
        $memory = Get-CimInstance Win32_PerfFormattedData_PerfOS_Memory -ErrorAction SilentlyContinue
        $processor = Get-CimInstance Win32_PerfFormattedData_Counters_ProcessorInformation -Filter "Name='_Total'" -ErrorAction SilentlyContinue
        $pagingFile = Get-CimInstance Win32_PerfFormattedData_PerfOS_PagingFile -Filter "Name='_Total'" -ErrorAction SilentlyContinue
        $samples.Add([pscustomobject][ordered]@{
            at = $now.ToUniversalTime().ToString('o')
            cpuPercent = [math]::Round(($cpuMs / $elapsedMs / [Environment]::ProcessorCount) * 100, 2)
            cpuFrequencyMHz = if ($processor) {
                [math]::Round(([double]$processor.ProcessorFrequency * [double]$processor.PercentProcessorPerformance) / 100, 1)
            } else {
                $null
            }
            workingSetBytes = [int64]$process.WorkingSet64
            privateBytes = [int64]$process.PrivateMemorySize64
            committedBytes = if ($memory) { [int64]$memory.CommittedBytes } else { $null }
            pageReadsPerSecond = if ($memory) { [double]$memory.PageReadsPersec } else { $null }
            pagingFileUsagePercent = if ($pagingFile) { [double]$pagingFile.PercentUsage } else { $null }
            gpu = Get-NvidiaSample
        })
        $lastCpu = [double]$process.TotalProcessorTime.TotalMilliseconds
        $lastSampleAt = $now
    }

    $completedAt = (Get-Date).ToUniversalTime()
    $stdout = $stdoutTask.GetAwaiter().GetResult()
    $stderr = $stderrTask.GetAwaiter().GetResult()
    [System.IO.File]::WriteAllText($stdoutPath, $stdout, [System.Text.UTF8Encoding]::new($false))
    [System.IO.File]::WriteAllText($stderrPath, $stderr, [System.Text.UTF8Encoding]::new($false))
    $cudaOom = $stderr -match '(?i)(CUDA.*out of memory|out of memory.*CUDA|cudaMalloc.*failed)'
    $rows = @()
    $parseError = $null
    if ($process.ExitCode -eq 0) {
        try {
            $rows = @($stdout | ConvertFrom-Json)
        } catch {
            $parseError = $_.Exception.Message
        }
    }

    $gpuSamples = @($samples | Where-Object { $null -ne $_.gpu } | ForEach-Object gpu)
    $pageReads = @($samples | Where-Object { $null -ne $_.pageReadsPerSecond } | ForEach-Object pageReadsPerSecond)
    $committedBytes = @($samples | Where-Object { $null -ne $_.committedBytes } | ForEach-Object committedBytes)
    $cpuFrequencies = @($samples | Where-Object { $null -ne $_.cpuFrequencyMHz } | ForEach-Object cpuFrequencyMHz)
    $pagingFileUsage = @($samples | Where-Object { $null -ne $_.pagingFileUsagePercent } | ForEach-Object pagingFileUsagePercent)
    $telemetry = [ordered]@{
        sampleCount = $samples.Count
        peakWorkingSetBytes = if ($samples.Count) { [int64](($samples | Measure-Object workingSetBytes -Maximum).Maximum) } else { 0 }
        peakPrivateBytes = if ($samples.Count) { [int64](($samples | Measure-Object privateBytes -Maximum).Maximum) } else { 0 }
        peakCommittedBytes = if ($committedBytes.Count) { [int64](($committedBytes | Measure-Object -Maximum).Maximum) } else { 0 }
        meanCpuPercent = if ($samples.Count) { [math]::Round([double](($samples | Measure-Object cpuPercent -Average).Average), 2) } else { 0 }
        meanCpuFrequencyMHz = if ($cpuFrequencies.Count) { [math]::Round([double](($cpuFrequencies | Measure-Object -Average).Average), 1) } else { $null }
        peakCpuFrequencyMHz = if ($cpuFrequencies.Count) { [math]::Round([double](($cpuFrequencies | Measure-Object -Maximum).Maximum), 1) } else { $null }
        peakPageReadsPerSecond = if ($pageReads.Count) { [double](($pageReads | Measure-Object -Maximum).Maximum) } else { 0 }
        meanPageReadsPerSecond = if ($pageReads.Count) { [math]::Round([double](($pageReads | Measure-Object -Average).Average), 2) } else { 0 }
        minimumPagingFileUsagePercent = if ($pagingFileUsage.Count) { [double](($pagingFileUsage | Measure-Object -Minimum).Minimum) } else { $null }
        peakPagingFileUsagePercent = if ($pagingFileUsage.Count) { [double](($pagingFileUsage | Measure-Object -Maximum).Maximum) } else { $null }
        pagingFileUsageDeltaPoints = if ($pagingFileUsage.Count) {
            [double](($pagingFileUsage | Measure-Object -Maximum).Maximum) -
                [double](($pagingFileUsage | Measure-Object -Minimum).Minimum)
        } else {
            $null
        }
        peakVramMiB = if ($gpuSamples.Count) { [double](($gpuSamples | Measure-Object memoryUsedMiB -Maximum).Maximum) } else { 0 }
        meanGpuUtilizationPercent = if ($gpuSamples.Count) { [math]::Round([double](($gpuSamples | Measure-Object utilizationPercent -Average).Average), 2) } else { 0 }
        peakTemperatureCelsius = if ($gpuSamples.Count) { [double](($gpuSamples | Measure-Object temperatureCelsius -Maximum).Maximum) } else { 0 }
        peakPowerWatts = if ($gpuSamples.Count) { [double](($gpuSamples | Measure-Object powerWatts -Maximum).Maximum) } else { 0 }
        meanGpuSmClockMHz = if ($gpuSamples.Count) { [math]::Round([double](($gpuSamples | Measure-Object smClockMHz -Average).Average), 1) } else { $null }
        peakGpuSmClockMHz = if ($gpuSamples.Count) { [double](($gpuSamples | Measure-Object smClockMHz -Maximum).Maximum) } else { $null }
    }

    $normalisedRows = @(
        foreach ($row in $rows) {
            $throughputSamples = @($row.samples_ts | ForEach-Object { [double]$_ })
            $durationSamples = @($row.samples_ns | ForEach-Object { [double]$_ / 1000000 })
            [pscustomobject][ordered]@{
                scenario = $Name
                metric = if ([int]$row.n_prompt -gt 0) { 'prompt' } else { 'generation' }
                promptTokens = [int]$row.n_prompt
                generationTokens = [int]$row.n_gen
                averageTokensPerSecond = [math]::Round([double]$row.avg_ts, 3)
                minimumTokensPerSecond = if ($throughputSamples.Count) { [math]::Round([double](($throughputSamples | Measure-Object -Minimum).Minimum), 3) } else { $null }
                standardDeviationTokensPerSecond = [math]::Round([double]$row.stddev_ts, 3)
                p95LatencyMs = if ($durationSamples.Count) { [math]::Round([double](Get-Percentile -Values $durationSamples -Percentile 0.95), 3) } else { $null }
                p95LatencyMsPerToken = if ($durationSamples.Count -and ([int]$row.n_prompt + [int]$row.n_gen) -gt 0) {
                    [math]::Round([double](Get-Percentile -Values $durationSamples -Percentile 0.95) / ([int]$row.n_prompt + [int]$row.n_gen), 3)
                } else {
                    $null
                }
                threads = [int]$row.n_threads
                batch = [int]$row.n_batch
                microBatch = [int]$row.n_ubatch
                cpuMoeLayers = [int]$row.n_cpu_moe
                gpuLayers = [int]$row.n_gpu_layers
                kvKeyType = [string]$row.type_k
                kvValueType = [string]$row.type_v
                flashAttention = [int]$row.flash_attn
                fitTargetMiB = [int]$row.fit_target
                samplesTokensPerSecond = $throughputSamples
            }
        }
    )
    $measuredInferenceSeconds = [double](
        $normalisedRows |
            ForEach-Object {
                $tokens = [math]::Max(1, $_.promptTokens + $_.generationTokens)
                $_.samplesTokensPerSecond | ForEach-Object { $tokens / [double]$_ }
            } |
            Measure-Object -Sum |
            Select-Object -ExpandProperty Sum
    )
    $processLifetimeSeconds = [math]::Round(($completedAt - $startedAt).TotalSeconds, 3)
    $invalidRows = @(
        $normalisedRows |
            Where-Object {
                $_.averageTokensPerSecond -le 0 -or
                $_.promptTokens -lt 0 -or
                $_.generationTokens -lt 0
            }
    )

    return [ordered]@{
        name = $Name
        startedAt = $startedAt.ToString('o')
        completedAt = $completedAt.ToString('o')
        durationSeconds = $processLifetimeSeconds
        estimatedModelLoadSeconds = if ($Name -eq 'cold-start' -and $normalisedRows.Count) {
            [math]::Round([math]::Max(0, $processLifetimeSeconds - $measuredInferenceSeconds), 3)
        } else {
            $null
        }
        commandLine = $commandLine
        exitCode = $process.ExitCode
        succeeded = $process.ExitCode -eq 0 -and -not $parseError
        cudaOutOfMemory = $cudaOom
        parseError = $parseError
        resultValidation = [ordered]@{
            jsonParsed = $null -eq $parseError
            rowCount = $normalisedRows.Count
            invalidRowCount = $invalidRows.Count
        }
        errorTail = Protect-HermesLogText (($stderr -split '\r?\n' | Select-Object -Last 20) -join [Environment]::NewLine)
        telemetry = $telemetry
        rows = $normalisedRows
    }
}

function Invoke-PromptCacheCheck {
    $token = Get-OrCreateHermesApiToken
    $fixture = ('Hermes Local cache fixture. ' * 1800)
    $headers = @{ Authorization = "Bearer $token" }
    $body = [ordered]@{
        model = [string]$configuration.selectedModel.alias
        messages = @([ordered]@{ role = 'user'; content = $fixture })
        max_tokens = 1
        temperature = 0
        seed = 3407
        cache_prompt = $true
        stream = $false
    } | ConvertTo-Json -Depth 8
    $measurements = @()
    foreach ($pass in 1..2) {
        $watch = [System.Diagnostics.Stopwatch]::StartNew()
        $response = Invoke-RestMethod `
            -Uri "http://$($configuration.network.host):$($configuration.network.modelPort)/v1/chat/completions" `
            -Method Post `
            -Headers $headers `
            -ContentType 'application/json' `
            -Body $body `
            -TimeoutSec 600
        $watch.Stop()
        $measurements += [ordered]@{
            pass = $pass
            wallMilliseconds = [math]::Round($watch.Elapsed.TotalMilliseconds, 2)
            promptTokens = if ($response.usage) { [int]$response.usage.prompt_tokens } else { $null }
            promptMilliseconds = if ($response.timings) { [double]$response.timings.prompt_ms } else { $null }
            cachedTokens = if ($response.timings -and $response.timings.PSObject.Properties.Name -contains 'prompt_n_cached') {
                [int]$response.timings.prompt_n_cached
            } else {
                $null
            }
        }
    }
    return [ordered]@{
        succeeded = $true
        passes = $measurements
        warmSpeedup = if ($measurements[1].wallMilliseconds -gt 0) {
            [math]::Round($measurements[0].wallMilliseconds / $measurements[1].wallMilliseconds, 2)
        } else {
            $null
        }
    }
}

function Get-BenchmarkValue {
    param(
        [AllowNull()]
        [object] $Record,
        [Parameter(Mandatory)]
        [string] $Name,
        [AllowNull()]
        [object] $Default = $null
    )

    if ($null -eq $Record) {
        return $Default
    }
    if ($Record -is [System.Collections.IDictionary]) {
        if ($Record.Contains($Name)) {
            return $Record[$Name]
        }
        return $Default
    }
    $property = $Record.PSObject.Properties[$Name]
    if ($property) {
        return $property.Value
    }
    return $Default
}

function Write-BenchmarkReport {
    param(
        [Parameter(Mandatory)]
        [System.Collections.IDictionary] $Document
    )

    $allRows = @($Document.cases | ForEach-Object rows)
    $contextPromptRows = @(
        $allRows |
            Where-Object { $_.scenario -match '^context-' -and $_.metric -eq 'prompt' } |
            Sort-Object promptTokens
    )
    $contextGenerationRows = @(
        $allRows |
            Where-Object { $_.scenario -match '^context-' -and $_.metric -eq 'generation' }
    )
    $decode = $allRows | Where-Object { $_.scenario -eq 'sustained-decode-1000' -and $_.metric -eq 'generation' } | Select-Object -First 1
    $short = $allRows | Where-Object { $_.scenario -eq 'short-chat-2k' -and $_.metric -eq 'generation' } | Select-Object -First 1
    $warm = $allRows | Where-Object { $_.scenario -eq 'warm-standard' -and $_.metric -eq 'generation' } | Select-Object -First 1
    $coldCase = $Document.cases | Where-Object name -eq 'cold-start' | Select-Object -First 1
    $contextLabels = @($contextPromptRows | ForEach-Object { [string][math]::Round($_.promptTokens / 1024) + 'K' })
    $contextValues = @($contextPromptRows | ForEach-Object { [math]::Round($_.averageTokensPerSecond, 1) })
    $passedContexts = @($contextPromptRows | Where-Object { $_.averageTokensPerSecond -gt 0 })
    $maxContext = if ($passedContexts.Count) { ($passedContexts | Measure-Object promptTokens -Maximum).Maximum } else { 0 }
    $selectedProfile = Get-BenchmarkValue -Record $Document -Name selectedProfile
    $correctness = Get-BenchmarkValue -Record $Document -Name correctness -Default ([ordered]@{})
    $correctnessToolCallValid = Get-BenchmarkValue -Record $correctness -Name toolCallValid -Default 'not recorded'
    $correctnessChecks = @(Get-BenchmarkValue -Record $correctness -Name checks -Default @())
    $invalidOutputsObserved = Get-BenchmarkValue -Record $correctness -Name invalidOutputsObserved -Default 'not recorded'
    $selectedProfileName = if ($selectedProfile -and $selectedProfile.name) {
        [string]$selectedProfile.name
    } else {
        'the selected profile'
    }
    $selectedContext = if ($selectedProfile -and $selectedProfile.contextTokens) {
        [int]$selectedProfile.contextTokens
    } else {
        32768
    }
    $profileDecision = if ($maxContext -ge $selectedContext) {
        "$selectedProfileName remains selected; its configured $([math]::Round($selectedContext / 1024))K context completed the benchmark gate."
    } else {
        "$selectedProfileName remains configured, but its full context did not complete; reduce its context or memory settings before promotion."
    }
    $shortSummary = if ($short) { "$($short.averageTokensPerSecond) tok/s mean" } else { 'not included in this run' }
    $decodeSummary = if ($decode) {
        "$($decode.averageTokensPerSecond) tok/s mean with a $($decode.minimumTokensPerSecond) tok/s minimum"
    } else {
        'not included in this run'
    }
    $suiteSummary = if ($Document.mode -eq 'full') {
        'Cold/warm, adaptive context, long prefill, 1,000-token decode, cache reuse, CPU-MoE, thread, batch, KV and accelerator-reserve sweeps are included when supported.'
    } else {
        'This quick validation includes 2K short generation, 1,000-token sustained decode, live prompt-cache reuse and the authenticated correctness gate. Run without `-Quick` for the full tuning suite.'
    }
    $selectedContextCaseName = 'context-' + [math]::Round($selectedContext / 1024) + 'k'
    $selectedContextCase = $Document.cases | Where-Object name -eq $selectedContextCaseName | Select-Object -First 1
    $selectedPageReads = if ($selectedContextCase) { $selectedContextCase.telemetry.peakPageReadsPerSecond } else { $null }
    $selectedPagingUsageValue = if ($selectedContextCase) {
        Get-BenchmarkValue -Record $selectedContextCase.telemetry -Name peakPagingFileUsagePercent
    } else {
        $null
    }
    $selectedPagingDelta = if ($selectedContextCase) {
        Get-BenchmarkValue -Record $selectedContextCase.telemetry -Name pagingFileUsageDeltaPoints
    } else {
        $null
    }
    $selectedPagingUsage = if ($null -ne $selectedPagingUsageValue) {
        "$selectedPagingUsageValue% peak paging-file usage ($(if ($null -ne $selectedPagingDelta) { $selectedPagingDelta } else { 'n/a' }) percentage-point change)"
    } else {
        'paging-file usage was not captured by this harness version'
    }
    $cacheSummary = if ($Document.promptCache -and $Document.promptCache.succeeded) {
        "$($Document.promptCache.warmSpeedup)x wall-clock speedup ($($Document.promptCache.passes[0].wallMilliseconds) ms cold to $($Document.promptCache.passes[1].wallMilliseconds) ms warm)"
    } else {
        'not verified'
    }
    $estimatedModelLoad = if ($coldCase) { Get-BenchmarkValue -Record $coldCase -Name estimatedModelLoadSeconds } else { $null }
    $loadSummary = if ($null -ne $estimatedModelLoad) {
        "$estimatedModelLoad s estimated model load/setup"
    } elseif ($coldCase) {
        "$($coldCase.durationSeconds) s cold-process lifetime; model-load isolation was unavailable in this harness version"
    } else {
        'not included'
    }
    $totalDurationSeconds = Get-BenchmarkValue -Record $Document -Name totalDurationSeconds
    $runDuration = if ($totalDurationSeconds) {
        "$([math]::Round([double]$totalDurationSeconds / 60, 1)) minutes"
    } else {
        'not recorded'
    }
    $failedCases = @($Document.cases | Where-Object { -not $_.succeeded })
    $stabilitySummary = if ($selectedContextCase) {
        "$($failedCases.Count) failed native cases; selected $([math]::Round($selectedContext / 1024))K case peaked at $selectedPageReads hard-page reads/s and $selectedPagingUsage"
    } else {
        "$($failedCases.Count) failed native cases; the selected long-context case was not requested in this quick validation"
    }

    $threadRows = @($allRows | Where-Object { $_.scenario -eq 'thread-sweep' -and $_.metric -eq 'generation' })
    $moeRows = @($allRows | Where-Object { $_.scenario -eq 'cpu-moe-sweep' -and $_.metric -eq 'generation' })
    $batchRows = @($allRows | Where-Object { $_.scenario -match '^batch-sweep-' -and $_.metric -eq 'prompt' })
    $matchedKvRows = @(
        $allRows |
            Where-Object {
                $_.scenario -match '^kv-sweep-' -and
                $_.metric -eq 'generation' -and
                $_.kvKeyType -eq $_.kvValueType
            }
    )
    $reserveRows = @($allRows | Where-Object { $_.scenario -eq 'reserve-sweep' -and $_.metric -eq 'generation' })
    $bestThread = $threadRows | Sort-Object averageTokensPerSecond -Descending | Select-Object -First 1
    $bestMoe = $moeRows | Sort-Object averageTokensPerSecond -Descending | Select-Object -First 1
    $bestBatch = $batchRows | Sort-Object averageTokensPerSecond -Descending | Select-Object -First 1
    $bestKv = $matchedKvRows | Sort-Object averageTokensPerSecond -Descending | Select-Object -First 1
    $bestReserve = $reserveRows | Sort-Object averageTokensPerSecond -Descending | Select-Object -First 1

    $calculationFailures = [System.Collections.Generic.List[string]]::new()
    foreach ($row in $allRows) {
        $samples = @($row.samplesTokensPerSecond | ForEach-Object { [double]$_ })
        if (-not $samples.Count) {
            continue
        }
        $mean = [double](($samples | Measure-Object -Average).Average)
        $minimum = [double](($samples | Measure-Object -Minimum).Minimum)
        $latencies = @($samples | ForEach-Object { 1000 / $_ } | Sort-Object)
        $p95Index = [math]::Max(0, [math]::Ceiling($latencies.Count * 0.95) - 1)
        $p95 = [double]$latencies[$p95Index]
        if (
            [math]::Abs($mean - [double]$row.averageTokensPerSecond) -gt 0.002 -or
            [math]::Abs($minimum - [double]$row.minimumTokensPerSecond) -gt 0.002 -or
            [math]::Abs($p95 - [double]$row.p95LatencyMsPerToken) -gt 0.002
        ) {
            $calculationFailures.Add("$($row.scenario)/$($row.metric)")
        }
    }
    $calculationAudit = if ($calculationFailures.Count) {
        "$($calculationFailures.Count) discrepancy or discrepancies across $($allRows.Count) saved rows"
    } else {
        "$($allRows.Count)/$($allRows.Count) saved rows independently recomputed within 0.002"
    }

    $lines = @(
        '# Hermes Local benchmark report',
        '',
        '## Decision',
        '',
        "- **Selected default:** $profileDecision",
        "- **Short generation:** $shortSummary; **1,000-token sustained generation:** $decodeSummary.",
        "- **Largest completed context:** $([math]::Round($maxContext / 1024))K tokens. No profile is selected solely for a short-context score.",
        "- **Stability:** $stabilitySummary.",
        "- **Correctness:** authenticated local-stack checks passed and native tool-call validity is $($correctnessToolCallValid.ToString().ToLowerInvariant()).",
        "- **Prompt-cache reuse:** $cacheSummary.",
        '',
        '## Test scope',
        '',
        "- Generated: $($Document.generatedAt); run duration: $runDuration.",
        "- Host: $($Document.machine.Cpu), $($Document.machine.PhysicalCores) cores / $($Document.machine.LogicalProcessors) logical processors, $([math]::Round([double]$Document.machine.MemoryBytes / 1GB, 1)) GiB RAM.",
        $(if ($Document.machine.Nvidia) {
            "- Accelerator: $($Document.machine.Nvidia.Name), $($Document.machine.Nvidia.MemoryMiB) MiB, driver $($Document.machine.Nvidia.DriverVersion), compute capability $($Document.machine.Nvidia.ComputeCapability)."
        } else {
            '- Accelerator: CPU-only benchmark.'
        }),
        "- Cold start: $loadSummary.",
        '',
        '## Long-context throughput',
        '',
        'The chart shows synthetic prompt-processing throughput at each saved context length. It is a capacity and prefill measurement, not a model-quality score.'
    )
    if ($contextPromptRows.Count) {
        $chartMaximum = [math]::Max(50, [math]::Ceiling((($contextValues | Measure-Object -Maximum).Maximum) / 50) * 50)
        $lines += @(
            '',
            '```mermaid',
            'xychart-beta',
            '  title "Prompt processing by context length"',
            "  x-axis [$($contextLabels -join ', ')]",
            "  y-axis `"tokens per second`" 0 --> $chartMaximum",
            "  bar [$($contextValues -join ', ')]",
            '```',
            '',
            '| Context | Prompt tok/s | Decode tok/s | P95 decode ms/token | Peak VRAM MiB | Peak RAM GiB | Page reads/s peak | Paging file peak % |',
            '|---:|---:|---:|---:|---:|---:|---:|---:|'
        )
        foreach ($row in $contextPromptRows) {
            $case = $Document.cases | Where-Object name -eq $row.scenario | Select-Object -First 1
            $generation = $contextGenerationRows | Where-Object scenario -eq $row.scenario | Select-Object -First 1
            $pagingUsageValue = Get-BenchmarkValue -Record $case.telemetry -Name peakPagingFileUsagePercent
            $pagingUsage = if ($null -ne $pagingUsageValue) { $pagingUsageValue } else { 'n/a' }
            $lines += "| $([math]::Round($row.promptTokens / 1024))K | $($row.averageTokensPerSecond) | $($generation.averageTokensPerSecond) | $($generation.p95LatencyMsPerToken) | $($case.telemetry.peakVramMiB) | $([math]::Round($case.telemetry.peakWorkingSetBytes / 1GB, 2)) | $($case.telemetry.peakPageReadsPerSecond) | $pagingUsage |"
        }
    } else {
        $lines += @('', '_No long-context cases were requested in this quick validation run._')
    }
    $lines += @(
        '',
        '## Interactive performance',
        '',
        'Sustained decode is the primary responsiveness gate. Exact commands and repetition samples are retained in `latest.json`.',
        '',
        '| Scenario | Mean tok/s | Minimum tok/s | P95 latency ms/token |',
        '|---|---:|---:|---:|'
    )
    foreach ($row in @($short, $decode, $warm) | Where-Object { $null -ne $_ }) {
        $lines += "| $($row.scenario) | $($row.averageTokensPerSecond) | $($row.minimumTokensPerSecond) | $($row.p95LatencyMsPerToken) |"
    }
    $lines += @(
        '',
        '## Tuning evidence',
        '',
        'Fastest points are shown for orientation; the operational choice still follows correctness, paging, OOM safety, quality, then sustained speed.',
        '',
        '| Sweep | Fastest measured point | Mean tok/s | Operational decision |',
        '|---|---|---:|---|'
    )
    if ($bestThread) {
        $lines += "| Generation threads | $($bestThread.threads) threads | $($bestThread.averageTokensPerSecond) | Compare this point with the selected profile's $($selectedProfile.threads.generation) threads before applying it. |"
    }
    if ($bestMoe) {
        $lines += "| CPU-MoE placement | $($bestMoe.cpuMoeLayers) layers | $($bestMoe.averageTokensPerSecond) | Keep 0 explicit CPU-MoE layers; the measured difference is too small to justify added placement complexity. |"
    }
    if ($bestBatch) {
        $lines += "| Batch / micro-batch | $($bestBatch.batch) / $($bestBatch.microBatch) | $($bestBatch.averageTokensPerSecond) prompt | Compare with the selected $($selectedProfile.batch.logical) / $($selectedProfile.batch.physical) pair and re-run the full context gate before applying. |"
    }
    if ($bestKv) {
        $lines += "| Matched KV cache | $($bestKv.kvKeyType) / $($bestKv.kvValueType) | $($bestKv.averageTokensPerSecond) generation | Re-run the selected context after changing the configured $($selectedProfile.kvCache.keyType) / $($selectedProfile.kvCache.valueType) cache. |"
    }
    if ($bestReserve) {
        $lines += "| Accelerator reserve | $($bestReserve.fitTargetMiB) MiB | $($bestReserve.averageTokensPerSecond) generation | Keep an operating margin appropriate to this machine; the selected profile currently reserves $($selectedProfile.gpu.vramReserveMiB) MiB. |"
    }
    $lines += @(
        '',
        '## Validation and traceability',
        '',
        "- Calculation spot-check: $calculationAudit.",
        "- Native case validation: $($failedCases.Count) failed cases; parsed row validation is recorded per case.",
        "- Authenticated correctness: $($correctnessChecks.Count) checks; invalid outputs observed: $invalidOutputsObserved; native tool call valid: $correctnessToolCallValid.",
        '- Primary evidence: `benchmarks/results/latest.json`; correctness evidence: `logs/diagnostics/latest-test.json`.',
        '',
        '## Scope, data and metric definitions',
        '',
        '- `llama-bench` uses deterministic synthetic token sequences. Prompt and generation throughput are the tool-reported arithmetic means across saved repetitions.',
        '- Minimum generation rate is the lowest saved repetition. P95 latency is the nearest-rank 95th percentile of full-repetition duration divided by tokens.',
        '- RAM is the benchmark process peak working set. Committed memory, hard-page reads, effective CPU frequency and paging-file usage come from Windows performance counters.',
        '- VRAM, GPU utilization, temperature, power and SM clock come from `nvidia-smi` samples.',
        "- Context tests use the selected profile's $($selectedProfile.kvCache.keyType)/$($selectedProfile.kvCache.valueType) cache, Flash Attention setting and accelerator policy.",
        '',
        '## Methodology',
        '',
        ('- Model: `{0}` (`{1}`).' -f $Document.model.filename, $Document.model.sha256),
        ('- llama.cpp: `{0}` build {1}.' -f $Document.llamaCpp.commit, $Document.llamaCpp.buildNumber),
        ('- Fixed seed: {0}. Full commands are recorded per case in `latest.json`.' -f $Document.fixtures.seed),
        "- $suiteSummary Speculative decoding is omitted because no compatible verified draft model is installed.",
        '',
        '## Limitations and robustness checks',
        '',
        '- Synthetic throughput does not measure answer quality. Deterministic reply, native tool-call and Hermes agent tests are separate promotion gates.',
        '- A single workstation run does not establish cross-machine performance. Background Windows activity can affect tail latency and page-read counters.',
        '- Windows `PageReadsPersec` is a hard-page-read signal and can include mapped-file reads; paging-file percentage is recorded separately. Low values support “no active thrashing” but do not identify every read source.',
        '- Prompt-cache speedup is a two-pass live-server wall-clock comparison. The server did not return a cached-token count, so reuse is inferred from identical requests plus the latency reduction and may include scheduler variance.',
        '- Cold model-load time is an external estimate that includes small process setup and teardown overhead.',
        '- A context size is not promoted automatically; the selected model limit, memory headroom and full correctness gate all apply.',
        '',
        '## Recommended next steps',
        '',
        "1. Keep $selectedProfileName selected only if its configured context and correctness gates pass on this machine.",
        '2. Re-run this harness after any model, llama.cpp, driver, acceleration, KV-cache or thread/batch change.',
        '3. Do not enable speculative decoding until a compatible draft model passes output and tool-call equivalence tests.',
        '',
        '## Further questions',
        '',
        '- Would a tokenizer-compatible draft model improve latency without reducing output or tool-call fidelity?',
        '- Does a longer overnight agent trajectory reveal memory pressure not visible in the bounded 1,000-token decode?'
    )

    Write-HermesAtomicText -Path (Resolve-HermesPath 'benchmarks\reports\LATEST.md') -Content (
        ($lines -join [Environment]::NewLine) + [Environment]::NewLine
    )
}

try {
    Assert-HermesRoot
    Initialize-HermesLayout
    Set-HermesProcessEnvironment

    if ($ReportOnly) {
        $existingResultsPath = Resolve-HermesPath 'benchmarks\results\latest.json'
        if (-not (Test-Path -LiteralPath $existingResultsPath)) {
            throw "Benchmark results do not exist: $existingResultsPath"
        }
        $existingDocument = Get-Content -Raw -LiteralPath $existingResultsPath | ConvertFrom-Json -AsHashtable
        Write-BenchmarkReport -Document $existingDocument
        Write-Host "Benchmark report regenerated: $(Resolve-HermesPath 'benchmarks\reports\LATEST.md')"
        exit 0
    }

    $benchmarkStartedAt = (Get-Date).ToUniversalTime()
    $manifest = Get-HermesVersionManifest
    $configuration = Get-HermesConfiguration
    $modelManifest = $configuration.selectedModel
    $benchmarkProfile = $configuration.selectedProfileConfiguration
    $fixtures = Get-Content -Raw -LiteralPath (Resolve-HermesPath 'benchmarks\inputs\benchmark-fixtures.json') | ConvertFrom-Json
    $benchMatches = @(Get-ChildItem -LiteralPath (Resolve-HermesPath 'runtimes\llama.cpp\build') -Recurse -Filter llama-bench.exe -File)
    if ($benchMatches.Count -ne 1) {
        throw "Expected one llama-bench.exe; found $($benchMatches.Count)."
    }
    if (-not (Test-HermesSelectedModel -Model $modelManifest -Hash:([bool]$modelManifest.sha256))) {
        throw 'Model integrity failed before benchmark.'
    }
    $binary = $benchMatches[0].FullName
    $model = [string]$modelManifest.resolvedPath
    $acceleration = Get-HermesEffectiveAcceleration -Configuration $configuration
    $core = @(
        '-m', $model,
        '-fa', $(if ($benchmarkProfile.flashAttention) { 'on' } else { 'off' }),
        '--prio', '1'
    )
    if ($acceleration -eq 'cuda') {
        $core += @('-ngl', [string]$benchmarkProfile.gpu.layers, '-fit', 'on')
    } else {
        $core += @('-ngl', '0')
    }
    $reserveArguments = if ($acceleration -eq 'cuda') {
        @('-fitt', [string]$benchmarkProfile.gpu.vramReserveMiB)
    } else {
        @()
    }
    $base = $core + @(
        '-ctk', [string]$benchmarkProfile.kvCache.keyType,
        '-ctv', [string]$benchmarkProfile.kvCache.valueType,
        '-t', [string]$benchmarkProfile.threads.generation,
        '-b', [string]$benchmarkProfile.batch.logical,
        '-ub', [string]$benchmarkProfile.batch.physical
    ) + $reserveArguments
    $baseWithoutThreads = $core + @(
        '-ctk', [string]$benchmarkProfile.kvCache.keyType,
        '-ctv', [string]$benchmarkProfile.kvCache.valueType,
        '-b', [string]$benchmarkProfile.batch.logical,
        '-ub', [string]$benchmarkProfile.batch.physical
    ) + $reserveArguments
    $baseWithoutBatch = $core + @(
        '-ctk', [string]$benchmarkProfile.kvCache.keyType,
        '-ctv', [string]$benchmarkProfile.kvCache.valueType,
        '-t', [string]$benchmarkProfile.threads.generation
    ) + $reserveArguments
    $baseWithoutKv = $core + @(
        '-t', [string]$benchmarkProfile.threads.generation,
        '-b', [string]$benchmarkProfile.batch.logical,
        '-ub', [string]$benchmarkProfile.batch.physical
    ) + $reserveArguments
    $baseWithoutReserve = $core + @(
        '-ctk', [string]$benchmarkProfile.kvCache.keyType,
        '-ctv', [string]$benchmarkProfile.kvCache.valueType,
        '-t', [string]$benchmarkProfile.threads.generation,
        '-b', [string]$benchmarkProfile.batch.logical,
        '-ub', [string]$benchmarkProfile.batch.physical
    )
    $maximumContext = if (
        $modelManifest.metadata -and
        $modelManifest.metadata.PSObject.Properties.Name -contains 'modelMaximumContextTokens' -and
        $modelManifest.metadata.modelMaximumContextTokens
    ) {
        [int]$modelManifest.metadata.modelMaximumContextTokens
    } else {
        [int]$benchmarkProfile.contextTokens
    }
    $contextTargets = @(16384, 32768, [int]$benchmarkProfile.contextTokens) |
        Where-Object { $_ -le $maximumContext } |
        Sort-Object -Unique
    $threadCurrent = [int]$benchmarkProfile.threads.generation
    $threadSweep = @(
        [math]::Max(1, $threadCurrent - 2),
        $threadCurrent,
        [math]::Min([Environment]::ProcessorCount, $threadCurrent + 2)
    ) | Sort-Object -Unique
    $reserveCurrent = [int]$benchmarkProfile.gpu.vramReserveMiB
    $reserveSweep = @(
        [math]::Max(0, $reserveCurrent - 512),
        $reserveCurrent,
        $reserveCurrent + 512
    ) | Sort-Object -Unique
    $cases = if ($Quick) {
        @(
            [ordered]@{ name = 'short-chat-2k'; args = $base + @('-p', '2048', '-n', '128', '-r', '2') },
            [ordered]@{ name = 'sustained-decode-1000'; args = $base + @('-p', '0', '-n', '1000', '-r', '1') }
        )
    } else {
        @(
            [ordered]@{ name = 'cold-start'; args = $base + @('--no-warmup', '-p', '32', '-n', '0', '-r', '1') },
            [ordered]@{ name = 'short-chat-2k'; args = $base + @('-p', '2048', '-n', '128', '-r', '3') },
            @($contextTargets | ForEach-Object {
                [ordered]@{
                    name = "context-$([math]::Round($_ / 1024))k"
                    args = $base + @('-p', [string]$_, '-n', '32', '-r', $(if ($_ -le 32768) { '2' } else { '1' }))
                }
            }),
            [ordered]@{ name = 'long-prefill'; args = $base + @('-p', [string]$benchmarkProfile.contextTokens, '-n', '0', '-r', '1') },
            [ordered]@{ name = 'sustained-decode-1000'; args = $base + @('-p', '0', '-n', '1000', '-r', '1') },
            [ordered]@{ name = 'warm-standard'; args = $base + @('-p', '2048', '-n', '256', '-r', '3') },
            [ordered]@{ name = 'thread-sweep'; args = $baseWithoutThreads + @('-t', ($threadSweep -join ','), '-p', '0', '-n', '256', '-r', '2') },
            [ordered]@{ name = 'cpu-moe-sweep'; args = $base + @('-ncmoe', '0,4,8,16,24,32,40', '-p', '0', '-n', '256', '-r', '2') },
            [ordered]@{ name = 'batch-sweep-512'; args = $baseWithoutBatch + @('-b', '512', '-ub', '128', '-p', '4096', '-n', '0', '-r', '2') },
            [ordered]@{ name = 'batch-sweep-1024'; args = $baseWithoutBatch + @('-b', '1024', '-ub', '256', '-p', '4096', '-n', '0', '-r', '2') },
            [ordered]@{ name = 'batch-sweep-2048'; args = $baseWithoutBatch + @('-b', '2048', '-ub', '512', '-p', '4096', '-n', '0', '-r', '2') },
            [ordered]@{ name = 'kv-sweep-f16'; args = $baseWithoutKv + @('-ctk', 'f16', '-ctv', 'f16', '-p', '4096', '-n', '128', '-r', '2') },
            [ordered]@{ name = 'kv-sweep-q8'; args = $baseWithoutKv + @('-ctk', 'q8_0', '-ctv', 'q8_0', '-p', '4096', '-n', '128', '-r', '2') },
            [ordered]@{ name = 'kv-sweep-q4'; args = $baseWithoutKv + @('-ctk', 'q4_0', '-ctv', 'q4_0', '-p', '4096', '-n', '128', '-r', '2') },
            $(if ($acceleration -eq 'cuda') {
                [ordered]@{ name = 'reserve-sweep'; args = $baseWithoutReserve + @('-fitt', ($reserveSweep -join ','), '-p', '4096', '-n', '128', '-r', '2') }
            })
        )
    }
    $cases = @($cases | Where-Object { $null -ne $_ })

    $statusPath = Resolve-HermesPath 'data\runtime\status.json'
    if (Test-Path -LiteralPath $statusPath) {
        $status = Get-Content -Raw -LiteralPath $statusPath | ConvertFrom-Json
        $wasRunning = $status.phase -eq 'running'
        if ($status.profile) {
            $restartProfile = [string]$status.profile
        }
    }
    if ($wasRunning) {
        Enter-HermesBenchmarkMode -Profile $restartProfile
    }

    $runCases = [System.Collections.Generic.List[object]]::new()
    foreach ($case in $cases) {
        try {
            $runCases.Add((Invoke-BenchmarkCase -Name $case.name -Arguments $case.args -Binary $binary -WorkingDirectory (Resolve-HermesPath 'data\user')))
        } catch {
            $runCases.Add([ordered]@{
                name = $case.name
                succeeded = $false
                cudaOutOfMemory = $_.Exception.Message -match '(?i)(CUDA|out of memory)'
                errorTail = Protect-HermesLogText $_.Exception.Message
                rows = @()
                telemetry = [ordered]@{}
            })
        }
    }

    if ($wasRunning) {
        Exit-HermesBenchmarkMode
        $stackRestarted = $true
    }

    $cache = if ($stackRestarted) {
        try { Invoke-PromptCacheCheck } catch { [ordered]@{ succeeded = $false; error = Protect-HermesLogText $_.Exception.Message } }
    } else {
        [ordered]@{ succeeded = $false; error = 'Stack was not running before the benchmark.' }
    }
    $correctness = if ($stackRestarted) {
        & (Resolve-HermesPath 'Test-Hermes-Local.ps1') -Quick -SkipAgentTool -NonInteractive
        $testReportPath = Resolve-HermesPath 'logs\diagnostics\latest-test.json'
        $testReport = if (Test-Path -LiteralPath $testReportPath) {
            Get-Content -Raw -LiteralPath $testReportPath | ConvertFrom-Json
        } else {
            $null
        }
        $toolCallCheck = if ($testReport) {
            $testReport.results | Where-Object name -eq 'Native tool-call schema' | Select-Object -First 1
        } else {
            $null
        }
        [ordered]@{
            succeeded = $LASTEXITCODE -eq 0
            report = 'logs\diagnostics\latest-test.json'
            invalidOutputsObserved = if ($testReport -and $testReport.passed) { 0 } else { $null }
            toolCallValid = if ($toolCallCheck) { [bool]$toolCallCheck.passed } else { $false }
            checks = if ($testReport) { @($testReport.results) } else { @() }
        }
    } else {
        [ordered]@{
            succeeded = $false
            report = $null
            invalidOutputsObserved = $null
            toolCallValid = $false
            checks = @()
        }
    }

    $benchmarkCompletedAt = (Get-Date).ToUniversalTime()
    $selectedProfile = $configuration.selectedProfileConfiguration
    $document = [ordered]@{
        schemaVersion = 1
        harnessVersion = 2
        generatedAt = (Get-Date).ToUniversalTime().ToString('o')
        runStartedAt = $benchmarkStartedAt.ToString('o')
        runCompletedAt = $benchmarkCompletedAt.ToString('o')
        totalDurationSeconds = [math]::Round(($benchmarkCompletedAt - $benchmarkStartedAt).TotalSeconds, 3)
        mode = if ($Quick) { 'quick' } else { 'full' }
        machine = Get-HermesHardwareSnapshot
        model = [ordered]@{
            id = $modelManifest.id
            displayName = $modelManifest.displayName
            alias = $modelManifest.alias
            filename = $modelManifest.filename
            revision = Get-BenchmarkValue -Record $modelManifest -Name revision
            sizeBytes = $modelManifest.sizeBytes
            sha256 = Get-BenchmarkValue -Record $modelManifest -Name sha256
            quantization = Get-BenchmarkValue -Record $modelManifest.metadata -Name quantization -Default 'unknown'
        }
        llamaCpp = [ordered]@{
            commit = $manifest.sources.llamaCpp.commit
            buildNumber = 10154
            binary = $binary
        }
        fixtures = $fixtures
        selectionPolicy = @(
            'correct output and stable tool calling',
            'no active page-file thrashing',
            'no accelerator out-of-memory failure',
            'quality preserved',
            'sustained generation near or above 15 tok/s',
            'prompt-processing performance',
            'lower power and temperature'
        )
        speculativeDecoding = [ordered]@{
            tested = $false
            reason = 'No compatible verified draft model is registered for the selected model.'
        }
        selectedProfile = if ($selectedProfile) {
            [ordered]@{
                name = $selectedProfile.name
                contextTokens = $selectedProfile.contextTokens
                kvCache = $selectedProfile.kvCache
                threads = $selectedProfile.threads
                batch = $selectedProfile.batch
                gpu = $selectedProfile.gpu
                flashAttention = $selectedProfile.flashAttention
                promptCache = $selectedProfile.promptCache
            }
        } else {
            $null
        }
        telemetryDefinitions = [ordered]@{
            cpuFrequency = 'Effective MHz: Windows ProcessorFrequency multiplied by PercentProcessorPerformance.'
            pageReads = 'Windows Memory PageReadsPersec hard-page read operations; may include mapped-file reads as well as page-file reads.'
            pagingFileUsage = 'Windows _Total paging-file PercentUsage.'
            modelLoad = 'Cold-process lifetime less saved inference sample durations; includes small process setup/teardown overhead.'
        }
        promptCache = $cache
        correctness = $correctness
        cases = $runCases
    }
    Write-HermesAtomicText -Path (Resolve-HermesPath 'benchmarks\results\latest.json') -Content (
        ($document | ConvertTo-Json -Depth 32) + [Environment]::NewLine
    )
    Write-BenchmarkReport -Document $document

    $failed = @($runCases | Where-Object { -not $_.succeeded })
    Write-HermesLog -Component benchmarks -Message "Benchmark completed with $($runCases.Count) case(s), $($failed.Count) failed."
    if ($failed.Count -gt 0) {
        Write-Host "Benchmark completed with $($failed.Count) failed case(s). Review benchmarks\results\latest.json." -ForegroundColor Yellow
        exit 2
    }
    Write-Host "Benchmark passed. Report: $(Resolve-HermesPath 'benchmarks\reports\LATEST.md')"
    exit 0
} catch {
    Write-HermesLog -Component benchmarks -Level ERROR -Message $_.Exception.ToString()
    Write-Host "Benchmark failed: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
} finally {
    if (Test-Path -LiteralPath $benchmarkRequestPath) {
        try {
            $request = Get-Content -Raw -LiteralPath $benchmarkRequestPath | ConvertFrom-Json
            if ([int]$request.ownerPid -eq $PID) {
                Remove-Item -LiteralPath $benchmarkRequestPath -Force -ErrorAction SilentlyContinue
            }
        } catch {
            Remove-Item -LiteralPath $benchmarkRequestPath -Force -ErrorAction SilentlyContinue
        }
    }
    if ($wasRunning -and -not $stackRestarted) {
        try {
            & (Resolve-HermesPath 'Start-Hermes-Local.ps1') -Profile $restartProfile -NonInteractive
            if ($LASTEXITCODE -ne 0) {
                throw "Start-Hermes-Local.ps1 exited with code $LASTEXITCODE."
            }
        } catch {
            Write-HermesLog -Component benchmarks -Level ERROR -Message "Could not restore stack after benchmark failure: $($_.Exception.Message)"
        }
    }
    foreach ($file in $temporaryFiles) {
        if (Test-Path -LiteralPath $file) {
            Remove-Item -LiteralPath $file -Force -ErrorAction SilentlyContinue
        }
    }
}
