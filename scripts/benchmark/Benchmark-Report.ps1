function Write-BenchmarkReport {
    param([Parameter(Mandatory)][System.Collections.IDictionary] $Document)

    $cases = @(Get-BenchmarkValue -Record $Document -Name cases -Default @())
    $failed = @($cases | Where-Object { -not [bool](Get-BenchmarkValue -Record $_ -Name succeeded -Default $false) })
    $allRows = @($cases | ForEach-Object { @(Get-BenchmarkValue -Record $_ -Name rows -Default @()) })
    $contextRows = @($allRows | Where-Object { $_.scenario -match '^context-' -and $_.metric -eq 'prompt' } | Sort-Object promptTokens)
    $decode = $allRows | Where-Object { $_.scenario -eq 'sustained-decode-1000' -and $_.metric -eq 'generation' } | Select-Object -First 1
    $short = $allRows | Where-Object { $_.scenario -eq 'short-chat-2k' -and $_.metric -eq 'generation' } | Select-Object -First 1
    $profile = Get-BenchmarkValue -Record $Document -Name selectedProfile
    $machine = Get-BenchmarkValue -Record $Document -Name machine
    $acceleration = [string](Get-BenchmarkValue -Record $Document -Name acceleration -Default 'unknown')

    $lines = [System.Collections.Generic.List[string]]::new()
    foreach ($line in @(
        '# Hermes Local benchmark report',
        '',
        '## Summary',
        '',
        "- Generated: $(Get-BenchmarkValue -Record $Document -Name generatedAt -Default 'unknown').",
        "- Mode: $(Get-BenchmarkValue -Record $Document -Name mode -Default 'unknown').",
        "- Acceleration: $acceleration.",
        "- Native cases: $($cases.Count) total, $($failed.Count) failed.",
        "- Short generation: $(if ($short) { "$($short.averageTokensPerSecond) tok/s" } else { 'not measured' }).",
        "- Sustained generation: $(if ($decode) { "$($decode.averageTokensPerSecond) tok/s" } else { 'not measured' }).",
        '',
        '## Environment',
        '',
        "- CPU: $(Get-BenchmarkValue -Record $machine -Name Cpu -Default 'unknown').",
        "- Logical processors: $(Get-BenchmarkValue -Record $machine -Name LogicalProcessors -Default 'unknown').",
        "- RAM: $([math]::Round([double](Get-BenchmarkValue -Record $machine -Name MemoryBytes -Default 0) / 1GB, 1)) GiB.",
        "- Selected profile: $(Get-BenchmarkValue -Record $profile -Name name -Default 'unknown').",
        "- Configured context: $(Get-BenchmarkValue -Record $profile -Name contextTokens -Default 'unknown') tokens.",
        '',
        '## Case results',
        '',
        '| Case | Status | Exit | Duration s | Peak RAM GiB | Peak VRAM MiB | Error |',
        '|---|---|---:|---:|---:|---:|---|'
    )) {
        $lines.Add($line)
    }

    foreach ($case in $cases) {
        $telemetry = Get-BenchmarkValue -Record $case -Name telemetry -Default (New-EmptyTelemetry)
        $error = [string](Get-BenchmarkValue -Record $case -Name errorTail -Default '')
        $error = (($error -split '\r?\n' | Select-Object -First 1) -replace '\|', '\|')
        $lines.Add((
            '| {0} | {1} | {2} | {3} | {4} | {5} | {6} |' -f
                (Get-BenchmarkValue -Record $case -Name name -Default 'unknown'),
                $(if ([bool](Get-BenchmarkValue -Record $case -Name succeeded -Default $false)) { 'passed' } else { 'failed' }),
                (Get-BenchmarkValue -Record $case -Name exitCode -Default 'n/a'),
                (Get-BenchmarkValue -Record $case -Name durationSeconds -Default 0),
                [math]::Round([double](Get-BenchmarkValue -Record $telemetry -Name peakWorkingSetBytes -Default 0) / 1GB, 2),
                (Get-BenchmarkValue -Record $telemetry -Name peakVramMiB -Default 0),
                $error
        ))
    }

    $lines.Add('')
    $lines.Add('## Context throughput')
    $lines.Add('')
    if ($contextRows.Count) {
        $lines.Add('| Context | Prompt tok/s | Peak RAM GiB | Peak VRAM MiB | Page reads/s | Paging file peak % |')
        $lines.Add('|---:|---:|---:|---:|---:|---:|')
        foreach ($row in $contextRows) {
            $case = $cases | Where-Object { (Get-BenchmarkValue -Record $_ -Name name) -eq $row.scenario } | Select-Object -First 1
            $telemetry = Get-BenchmarkValue -Record $case -Name telemetry -Default (New-EmptyTelemetry)
            $lines.Add((
                '| {0}K | {1} | {2} | {3} | {4} | {5} |' -f
                    [math]::Round([double]$row.promptTokens / 1024),
                    $row.averageTokensPerSecond,
                    [math]::Round([double](Get-BenchmarkValue -Record $telemetry -Name peakWorkingSetBytes -Default 0) / 1GB, 2),
                    (Get-BenchmarkValue -Record $telemetry -Name peakVramMiB -Default 0),
                    (Get-BenchmarkValue -Record $telemetry -Name peakPageReadsPerSecond -Default 0),
                    (Get-BenchmarkValue -Record $telemetry -Name peakPagingFileUsagePercent -Default 'n/a')
            ))
        }
    } else {
        $lines.Add('_No context cases completed._')
    }

    if ($failed.Count) {
        $lines.Add('')
        $lines.Add('## Failures')
        $lines.Add('')
        foreach ($case in $failed) {
            $lines.Add("### $(Get-BenchmarkValue -Record $case -Name name -Default 'unknown')")
            $lines.Add('')
            $lines.Add('```text')
            $lines.Add([string](Get-BenchmarkValue -Record $case -Name errorTail -Default 'No error text was captured.'))
            $lines.Add('```')
            $lines.Add('')
        }
    }

    $lines.Add('## Portability behaviour')
    $lines.Add('')
    $lines.Add('- Abstract GPU-layer settings such as `auto` are translated only for llama-bench; the saved workstation profile is not modified.')
    $lines.Add('- Unsupported binary options are omitted after inspecting the installed llama-bench help text.')
    $lines.Add('- Context, thread, batch and reserve cases are derived from the selected model, profile and detected hardware rather than a fixed machine configuration.')
    $lines.Add('- Failed cases retain a complete telemetry schema so report generation cannot hide the original native error.')

    $reportPath = Resolve-HermesPath 'benchmarks\reports\LATEST.md'
    Write-HermesAtomicText -Path $reportPath -Content (($lines -join [Environment]::NewLine) + [Environment]::NewLine)
}

function Invoke-BenchmarkSelfTest {
    $help = @'
-ngl, --n-gpu-layers N
-fit [on|off]
-fitt N
-t N
-b N
-ub N
-ctk TYPE
-ctv TYPE
-fa <0|1>
-m MODEL
'@
    $auto = @(Resolve-BenchmarkGpuLayerArguments -Acceleration cuda -ConfiguredLayers auto -HelpText $help)
    if ($auto.Count -ne 0) {
        throw "Auto GPU-layer translation must defer to the installed binary: $($auto -join ' ')"
    }
    $explicit = @(Resolve-BenchmarkGpuLayerArguments -Acceleration cuda -ConfiguredLayers '0,12-24' -HelpText $help)
    if (($explicit -join ' ') -ne '-ngl 0,12-24') {
        throw "Explicit GPU-layer preservation failed: $($explicit -join ' ')"
    }
    $cpu = @(Resolve-BenchmarkGpuLayerArguments -Acceleration cpu -ConfiguredLayers auto -HelpText $help)
    if (($cpu -join ' ') -ne '-ngl 0') {
        throw "CPU GPU-layer translation failed: $($cpu -join ' ')"
    }
    $flashEnabled = Get-BenchmarkBooleanValue -HelpText $help -Option '-fa' -Enabled $true
    if ($flashEnabled -ne '1') {
        throw "Numeric boolean translation failed: $flashEnabled"
    }
    $onOffHelp = '-fit <on|off>'
    $fitEnabled = Get-BenchmarkBooleanValue -HelpText $onOffHelp -Option '-fit' -Enabled $true
    if ($fitEnabled -ne 'on') {
        throw "On/off boolean translation failed: $fitEnabled"
    }

    $bindingProfile = [ordered]@{
        name = 'SelfTest'
        contextTokens = 32768
        flashAttention = $true
        gpu = [ordered]@{ layers = 'auto'; vramReserveMiB = 1024 }
        kvCache = [ordered]@{ keyType = 'q8_0'; valueType = 'q8_0' }
        threads = [ordered]@{ generation = 4 }
        batch = [ordered]@{ logical = 512; physical = 128 }
    }
    $baseArguments = @(New-BaseBenchmarkArguments `
        -Profile $bindingProfile `
        -ModelPath 'self-test.gguf' `
        -Acceleration cuda `
        -HelpText $help)
    if (-not $baseArguments.Count -or ($baseArguments -join ' ') -notmatch '(?:^| )-m self-test\.gguf(?: |$)') {
        throw "Fresh argument-accumulator binding failed: $($baseArguments -join ' ')"
    }
    if (($baseArguments -join ' ') -match '(?:^| )-ngl auto(?: |$)') {
        throw 'Fresh argument-accumulator binding reintroduced -ngl auto.'
    }

    $quickCases = @(New-BenchmarkCases `
        -Profile $bindingProfile `
        -ModelPath 'self-test.gguf' `
        -Acceleration cuda `
        -HelpText $help `
        -MaximumContext 32768 `
        -QuickMode)
    if ($quickCases.Count -ne 2 -or @($quickCases | Where-Object { -not $_.args.Count }).Count) {
        throw 'Fresh case-accumulator binding failed.'
    }

    $telemetry = New-EmptyTelemetry
    foreach ($name in @('peakPageReadsPerSecond', 'peakWorkingSetBytes', 'peakVramMiB')) {
        if (-not $telemetry.Contains($name)) {
            throw "Telemetry schema is missing $name."
        }
    }
    Write-Host 'Benchmark portability self-test passed.'
}
