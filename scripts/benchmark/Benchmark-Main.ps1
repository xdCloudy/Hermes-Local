try {
    Assert-HermesRoot
    Initialize-HermesLayout

    if ($SelfTest) {
        Invoke-BenchmarkSelfTest
        exit 0
    }

    $resultsPath = Resolve-HermesPath 'benchmarks\results\latest.json'
    if ($ReportOnly) {
        if (-not (Test-Path -LiteralPath $resultsPath)) {
            throw "No benchmark result exists: $resultsPath"
        }
        $existing = Get-Content -Raw -LiteralPath $resultsPath | ConvertFrom-Json -AsHashtable
        Write-BenchmarkReport -Document $existing
        Write-Host "Benchmark report regenerated: $(Resolve-HermesPath 'benchmarks\reports\LATEST.md')"
        exit 0
    }

    $startedAt = (Get-Date).ToUniversalTime()
    $configuration = Get-HermesConfiguration
    $modelManifest = $configuration.selectedModel
    $profile = $configuration.selectedProfileConfiguration
    $script:restartProfile = [string]$configuration.selectedProfile

    $benchMatches = @(Get-ChildItem -LiteralPath (Resolve-HermesPath 'runtimes\llama.cpp\build') -Recurse -Filter llama-bench.exe -File)
    if ($benchMatches.Count -ne 1) {
        throw "Expected one llama-bench.exe; found $($benchMatches.Count)."
    }
    if (-not (Test-HermesSelectedModel -Model $modelManifest -Hash:([bool]$modelManifest.sha256))) {
        throw 'Model integrity failed before benchmark.'
    }

    $binary = $benchMatches[0].FullName
    $modelPath = [string]$modelManifest.resolvedPath
    $acceleration = Get-HermesEffectiveAcceleration -Configuration $configuration
    $helpText = Get-LlamaBenchHelp -Binary $binary
    $maximumContext = [int](Get-BenchmarkPathValue -Record $modelManifest -Path @('metadata', 'modelMaximumContextTokens') -Default (
        Get-BenchmarkValue -Record $profile -Name contextTokens -Default 32768
    ))
    $cases = @(New-BenchmarkCases `
        -Profile $profile `
        -ModelPath $modelPath `
        -Acceleration $acceleration `
        -HelpText $helpText `
        -MaximumContext $maximumContext `
        -QuickMode:$Quick)

    $status = Get-CurrentSupervisorStatus
    if ($status -and [string]$status.phase -eq 'running') {
        $script:wasRunning = $true
        if (Get-BenchmarkValue -Record $status -Name profile) {
            $script:restartProfile = [string]$status.profile
        }
        Enter-HermesBenchmarkMode -Profile $script:restartProfile
    }

    $runCases = [System.Collections.Generic.List[object]]::new()
    foreach ($case in $cases) {
        try {
            $runCases.Add((Invoke-BenchmarkCase `
                -Name ([string]$case.name) `
                -Arguments @($case.args | ForEach-Object { [string]$_ }) `
                -Binary $binary `
                -WorkingDirectory (Resolve-HermesPath 'data\user')))
        } catch {
            $runCases.Add([ordered]@{
                name = [string](Get-BenchmarkValue -Record $case -Name name -Default 'unknown')
                startedAt = (Get-Date).ToUniversalTime().ToString('o')
                completedAt = (Get-Date).ToUniversalTime().ToString('o')
                durationSeconds = 0
                commandLine = $null
                exitCode = $null
                succeeded = $false
                cudaOutOfMemory = $_.Exception.Message -match '(?i)(CUDA|out of memory)'
                parseError = $null
                errorTail = Protect-HermesLogText $_.Exception.Message
                telemetry = New-EmptyTelemetry
                rows = @()
            })
        }
    }

    if ($script:wasRunning) {
        Exit-HermesBenchmarkMode
    }

    $validation = [ordered]@{
        succeeded = $false
        report = $null
        error = 'Stack was not running before the benchmark.'
    }
    if ($script:stackRestored) {
        try {
            & (Resolve-HermesPath 'Test-Hermes-Local.ps1') -Quick -SkipAgentTool -NonInteractive
            $validation = [ordered]@{
                succeeded = $LASTEXITCODE -eq 0
                report = 'logs\diagnostics\latest-test.json'
                error = $null
            }
        } catch {
            $validation = [ordered]@{
                succeeded = $false
                report = 'logs\diagnostics\latest-test.json'
                error = Protect-HermesLogText $_.Exception.Message
            }
        }
    }

    $completedAt = (Get-Date).ToUniversalTime()
    $manifest = Get-HermesVersionManifest
    $document = [ordered]@{
        schemaVersion = 1
        harnessVersion = 3
        generatedAt = $completedAt.ToString('o')
        runStartedAt = $startedAt.ToString('o')
        runCompletedAt = $completedAt.ToString('o')
        totalDurationSeconds = [math]::Round(($completedAt - $startedAt).TotalSeconds, 3)
        mode = if ($Quick) { 'quick' } else { 'full' }
        acceleration = $acceleration
        machine = Get-HermesHardwareSnapshot
        model = [ordered]@{
            id = $modelManifest.id
            displayName = $modelManifest.displayName
            alias = $modelManifest.alias
            filename = $modelManifest.filename
            sizeBytes = $modelManifest.sizeBytes
            sha256 = Get-BenchmarkValue -Record $modelManifest -Name sha256
            maximumContextTokens = $maximumContext
        }
        llamaCpp = [ordered]@{
            commit = $manifest.sources.llamaCpp.commit
            binary = $binary
        }
        selectedProfile = [ordered]@{
            name = Get-BenchmarkValue -Record $profile -Name name
            contextTokens = Get-BenchmarkValue -Record $profile -Name contextTokens
            kvCache = Get-BenchmarkValue -Record $profile -Name kvCache
            threads = Get-BenchmarkValue -Record $profile -Name threads
            batch = Get-BenchmarkValue -Record $profile -Name batch
            gpu = Get-BenchmarkValue -Record $profile -Name gpu
            flashAttention = Get-BenchmarkValue -Record $profile -Name flashAttention
        }
        validation = $validation
        cases = $runCases
    }

    Write-HermesAtomicText -Path $resultsPath -Content (($document | ConvertTo-Json -Depth 32) + [Environment]::NewLine)
    Write-BenchmarkReport -Document $document

    $failed = @($runCases | Where-Object { -not $_.succeeded })
    Write-HermesLog -Component benchmarks -Message "Benchmark completed with $($runCases.Count) case(s), $($failed.Count) failed."
    if ($failed.Count) {
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
    if (Test-Path -LiteralPath $script:benchmarkRequestPath) {
        try {
            $request = Get-Content -Raw -LiteralPath $script:benchmarkRequestPath | ConvertFrom-Json
            if ([int](Get-BenchmarkValue -Record $request -Name ownerPid -Default 0) -eq $PID) {
                Remove-Item -LiteralPath $script:benchmarkRequestPath -Force -ErrorAction SilentlyContinue
            }
        } catch {
            Remove-Item -LiteralPath $script:benchmarkRequestPath -Force -ErrorAction SilentlyContinue
        }
    }

    if ($script:wasRunning -and -not $script:stackRestored) {
        try {
            $status = Get-CurrentSupervisorStatus
            $controllerPid = if ($status) { [int](Get-BenchmarkValue -Record $status -Name controllerPid -Default 0) } else { 0 }
            if ($controllerPid -gt 0 -and (Test-HermesProcessAlive -ProcessId $controllerPid)) {
                Wait-HermesBenchmarkPhase -Phase running -TimeoutSeconds 960
            } else {
                & (Resolve-HermesPath 'Start-Hermes-Local.ps1') -Profile $script:restartProfile -NonInteractive
                if ($LASTEXITCODE -ne 0) {
                    throw "Start-Hermes-Local.ps1 exited with code $LASTEXITCODE."
                }
            }
        } catch {
            Write-HermesLog -Component benchmarks -Level ERROR -Message "Could not restore stack after benchmark failure: $($_.Exception.Message)"
        }
    }

    foreach ($file in $script:temporaryFiles) {
        if (Test-Path -LiteralPath $file) {
            Remove-Item -LiteralPath $file -Force -ErrorAction SilentlyContinue
        }
    }
}
