function New-EmptyTelemetry {
    return [ordered]@{
        sampleCount = 0
        peakWorkingSetBytes = 0
        peakPrivateBytes = 0
        peakCommittedBytes = 0
        meanCpuPercent = 0
        meanCpuFrequencyMHz = $null
        peakCpuFrequencyMHz = $null
        peakPageReadsPerSecond = 0
        meanPageReadsPerSecond = 0
        minimumPagingFileUsagePercent = $null
        peakPagingFileUsagePercent = $null
        pagingFileUsageDeltaPoints = $null
        peakVramMiB = 0
        meanGpuUtilizationPercent = 0
        peakTemperatureCelsius = 0
        peakPowerWatts = 0
        meanGpuSmClockMHz = $null
        peakGpuSmClockMHz = $null
    }
}

function Get-NvidiaSample {
    $nvidiaSmi = Get-Command nvidia-smi.exe -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $nvidiaSmi) {
        return $null
    }
    try {
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
    } catch {
        return $null
    }
}

function Get-Percentile {
    param([double[]] $Values, [ValidateRange(0, 1)][double] $Percentile)
    if (-not $Values -or $Values.Count -eq 0) {
        return $null
    }
    $ordered = @($Values | Sort-Object)
    $index = [math]::Max(0, [math]::Ceiling($ordered.Count * $Percentile) - 1)
    return [double]$ordered[$index]
}

function Invoke-BenchmarkCase {
    param(
        [Parameter(Mandatory)][string] $Name,
        [Parameter(Mandatory)][string[]] $Arguments,
        [Parameter(Mandatory)][string] $Binary,
        [Parameter(Mandatory)][string] $WorkingDirectory
    )

    $id = [guid]::NewGuid().ToString('N')
    $stdoutPath = Resolve-HermesPath "temp\llama-bench-$id.json"
    $stderrPath = Resolve-HermesPath "temp\llama-bench-$id.stderr.log"
    $script:temporaryFiles.Add($stdoutPath)
    $script:temporaryFiles.Add($stderrPath)
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
            cpuPercent = [math]::Round(($cpuMs / $elapsedMs / [math]::Max(1, [Environment]::ProcessorCount)) * 100, 2)
            cpuFrequencyMHz = if ($processor) {
                [math]::Round(([double]$processor.ProcessorFrequency * [double]$processor.PercentProcessorPerformance) / 100, 1)
            } else { $null }
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

    $rows = @()
    $parseError = $null
    if ($process.ExitCode -eq 0) {
        try {
            $rows = @($stdout | ConvertFrom-Json)
        } catch {
            $parseError = $_.Exception.Message
        }
    }

    $normalisedRows = @(
        foreach ($row in $rows) {
            $nPrompt = [int](Get-BenchmarkValue -Record $row -Name n_prompt -Default 0)
            $nGeneration = [int](Get-BenchmarkValue -Record $row -Name n_gen -Default 0)
            $throughputSamples = @(
                Get-BenchmarkValue -Record $row -Name samples_ts -Default @() |
                    ForEach-Object { [double]$_ }
            )
            $durationSamples = @(
                Get-BenchmarkValue -Record $row -Name samples_ns -Default @() |
                    ForEach-Object { [double]$_ / 1000000 }
            )
            [pscustomobject][ordered]@{
                scenario = $Name
                metric = if ($nPrompt -gt 0) { 'prompt' } else { 'generation' }
                promptTokens = $nPrompt
                generationTokens = $nGeneration
                averageTokensPerSecond = [math]::Round([double](Get-BenchmarkValue -Record $row -Name avg_ts -Default 0), 3)
                minimumTokensPerSecond = if ($throughputSamples.Count) {
                    [math]::Round([double](($throughputSamples | Measure-Object -Minimum).Minimum), 3)
                } else { $null }
                standardDeviationTokensPerSecond = [math]::Round([double](Get-BenchmarkValue -Record $row -Name stddev_ts -Default 0), 3)
                p95LatencyMs = if ($durationSamples.Count) {
                    [math]::Round([double](Get-Percentile -Values $durationSamples -Percentile 0.95), 3)
                } else { $null }
                p95LatencyMsPerToken = if ($durationSamples.Count -and ($nPrompt + $nGeneration) -gt 0) {
                    [math]::Round([double](Get-Percentile -Values $durationSamples -Percentile 0.95) / ($nPrompt + $nGeneration), 3)
                } else { $null }
                threads = [int](Get-BenchmarkValue -Record $row -Name n_threads -Default 0)
                batch = [int](Get-BenchmarkValue -Record $row -Name n_batch -Default 0)
                microBatch = [int](Get-BenchmarkValue -Record $row -Name n_ubatch -Default 0)
                cpuMoeLayers = [int](Get-BenchmarkValue -Record $row -Name n_cpu_moe -Default 0)
                gpuLayers = [int](Get-BenchmarkValue -Record $row -Name n_gpu_layers -Default 0)
                kvKeyType = [string](Get-BenchmarkValue -Record $row -Name type_k -Default '')
                kvValueType = [string](Get-BenchmarkValue -Record $row -Name type_v -Default '')
                flashAttention = [int](Get-BenchmarkValue -Record $row -Name flash_attn -Default 0)
                fitTargetMiB = [int](Get-BenchmarkValue -Record $row -Name fit_target -Default 0)
                samplesTokensPerSecond = $throughputSamples
            }
        }
    )

    $telemetry = New-EmptyTelemetry
    if ($samples.Count) {
        $gpuSamples = @($samples | Where-Object { $null -ne $_.gpu } | ForEach-Object gpu)
        $pageReads = @($samples | Where-Object { $null -ne $_.pageReadsPerSecond } | ForEach-Object pageReadsPerSecond)
        $committed = @($samples | Where-Object { $null -ne $_.committedBytes } | ForEach-Object committedBytes)
        $frequencies = @($samples | Where-Object { $null -ne $_.cpuFrequencyMHz } | ForEach-Object cpuFrequencyMHz)
        $pagingUsage = @($samples | Where-Object { $null -ne $_.pagingFileUsagePercent } | ForEach-Object pagingFileUsagePercent)
        $telemetry.sampleCount = $samples.Count
        $telemetry.peakWorkingSetBytes = [int64](($samples | Measure-Object workingSetBytes -Maximum).Maximum)
        $telemetry.peakPrivateBytes = [int64](($samples | Measure-Object privateBytes -Maximum).Maximum)
        $telemetry.peakCommittedBytes = if ($committed.Count) { [int64](($committed | Measure-Object -Maximum).Maximum) } else { 0 }
        $telemetry.meanCpuPercent = [math]::Round([double](($samples | Measure-Object cpuPercent -Average).Average), 2)
        $telemetry.meanCpuFrequencyMHz = if ($frequencies.Count) { [math]::Round([double](($frequencies | Measure-Object -Average).Average), 1) } else { $null }
        $telemetry.peakCpuFrequencyMHz = if ($frequencies.Count) { [double](($frequencies | Measure-Object -Maximum).Maximum) } else { $null }
        $telemetry.peakPageReadsPerSecond = if ($pageReads.Count) { [double](($pageReads | Measure-Object -Maximum).Maximum) } else { 0 }
        $telemetry.meanPageReadsPerSecond = if ($pageReads.Count) { [math]::Round([double](($pageReads | Measure-Object -Average).Average), 2) } else { 0 }
        $telemetry.minimumPagingFileUsagePercent = if ($pagingUsage.Count) { [double](($pagingUsage | Measure-Object -Minimum).Minimum) } else { $null }
        $telemetry.peakPagingFileUsagePercent = if ($pagingUsage.Count) { [double](($pagingUsage | Measure-Object -Maximum).Maximum) } else { $null }
        $telemetry.pagingFileUsageDeltaPoints = if ($pagingUsage.Count) {
            [double](($pagingUsage | Measure-Object -Maximum).Maximum) - [double](($pagingUsage | Measure-Object -Minimum).Minimum)
        } else { $null }
        $telemetry.peakVramMiB = if ($gpuSamples.Count) { [double](($gpuSamples | Measure-Object memoryUsedMiB -Maximum).Maximum) } else { 0 }
        $telemetry.meanGpuUtilizationPercent = if ($gpuSamples.Count) { [math]::Round([double](($gpuSamples | Measure-Object utilizationPercent -Average).Average), 2) } else { 0 }
        $telemetry.peakTemperatureCelsius = if ($gpuSamples.Count) { [double](($gpuSamples | Measure-Object temperatureCelsius -Maximum).Maximum) } else { 0 }
        $telemetry.peakPowerWatts = if ($gpuSamples.Count) { [double](($gpuSamples | Measure-Object powerWatts -Maximum).Maximum) } else { 0 }
        $telemetry.meanGpuSmClockMHz = if ($gpuSamples.Count) { [math]::Round([double](($gpuSamples | Measure-Object smClockMHz -Average).Average), 1) } else { $null }
        $telemetry.peakGpuSmClockMHz = if ($gpuSamples.Count) { [double](($gpuSamples | Measure-Object smClockMHz -Maximum).Maximum) } else { $null }
    }

    return [ordered]@{
        name = $Name
        startedAt = $startedAt.ToString('o')
        completedAt = $completedAt.ToString('o')
        durationSeconds = [math]::Round(($completedAt - $startedAt).TotalSeconds, 3)
        commandLine = $commandLine
        exitCode = $process.ExitCode
        succeeded = $process.ExitCode -eq 0 -and -not $parseError -and $normalisedRows.Count -gt 0
        cudaOutOfMemory = $stderr -match '(?i)(CUDA.*out of memory|out of memory.*CUDA|cudaMalloc.*failed)'
        parseError = $parseError
        errorTail = Protect-HermesLogText (($stderr -split '\r?\n' | Select-Object -Last 20) -join [Environment]::NewLine)
        telemetry = $telemetry
        rows = $normalisedRows
    }
}

