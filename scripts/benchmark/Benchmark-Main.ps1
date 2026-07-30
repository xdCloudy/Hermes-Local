function New-BenchmarkDocument {
    param(
        [Parameter(Mandatory)][datetime] $StartedAt,
        [Parameter(Mandatory)][datetime] $CompletedAt,
        [Parameter(Mandatory)][bool] $QuickMode,
        [Parameter(Mandatory)][string] $Acceleration,
        [Parameter(Mandatory)][object] $ModelManifest,
        [Parameter(Mandatory)][int] $MaximumContext,
        [Parameter(Mandatory)][object] $Manifest,
        [Parameter(Mandatory)][string] $Binary,
        [Parameter(Mandatory)][object] $Profile,
        [Parameter(Mandatory)][System.Collections.IDictionary] $Validation,
        [Parameter(Mandatory)][AllowEmptyCollection()][object[]] $Cases,
        [Parameter(Mandatory)]
        [ValidateSet('restoration-pending', 'complete')]
        [string] $LifecycleState
    )

    return [ordered]@{
        schemaVersion = 1
        harnessVersion = 4
        generatedAt = $CompletedAt.ToString('o')
        runStartedAt = $StartedAt.ToString('o')
        runCompletedAt = $CompletedAt.ToString('o')
        totalDurationSeconds = [math]::Round(($CompletedAt - $StartedAt).TotalSeconds, 3)
        mode = if ($QuickMode) { 'quick' } else { 'full' }
        acceleration = $Acceleration
        lifecycle = [ordered]@{
            state = $LifecycleState
            stackWasRunning = $script:wasRunning
            stackRestored = $script:stackRestored
            recoveredByReplacement = $script:restorationRecoveredByReplacement
            initialRestorationError = $script:restorationInitialError
        }
        machine = Get-HermesHardwareSnapshot
        model = [ordered]@{
            id = $ModelManifest.id
            displayName = $ModelManifest.displayName
            alias = $ModelManifest.alias
            filename = $ModelManifest.filename
            sizeBytes = $ModelManifest.sizeBytes
            sha256 = Get-BenchmarkValue -Record $ModelManifest -Name sha256
            maximumContextTokens = $MaximumContext
        }
        llamaCpp = [ordered]@{
            commit = $Manifest.sources.llamaCpp.commit
            binary = $Binary
        }
        selectedProfile = [ordered]@{
            name = Get-BenchmarkValue -Record $Profile -Name name
            contextTokens = Get-BenchmarkValue -Record $Profile -Name contextTokens
            kvCache = Get-BenchmarkValue -Record $Profile -Name kvCache
            threads = Get-BenchmarkValue -Record $Profile -Name threads
            batch = Get-BenchmarkValue -Record $Profile -Name batch
            gpu = Get-BenchmarkValue -Record $Profile -Name gpu
            flashAttention = Get-BenchmarkValue -Record $Profile -Name flashAttention
        }
        validation = $Validation
        cases = $Cases
    }
}

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

    $manifest = Get-HermesVersionManifest
    $checkpointAt = (Get-Date).ToUniversalTime()
    $checkpointValidation = [ordered]@{
        succeeded = $false
        report = $null
        error = if ($script:wasRunning) {
            'Native benchmark cases completed; stack restoration and validation are pending.'
        } else {
            'Stack was not running before the benchmark.'
        }
    }
    $checkpointDocument = New-BenchmarkDocument `
        -StartedAt $startedAt `
        -CompletedAt $checkpointAt `
        -QuickMode ([bool]$Quick) `
        -Acceleration $acceleration `
        -ModelManifest $modelManifest `
        -MaximumContext $maximumContext `
        -Manifest $manifest `
        -Binary $binary `
        -Profile $profile `
        -Validation $checkpointValidation `
        -Cases $runCases.ToArray() `
        -LifecycleState 'restoration-pending'
    Write-HermesAtomicText -Path $resultsPath -Content (($checkpointDocument | ConvertTo-Json -Depth 32) + [Environment]::NewLine)
    Write-BenchmarkReport -Document $checkpointDocument
    Write-HermesLog -Component benchmarks -Message "Checkpointed $($runCases.Count) native benchmark case(s) before stack restoration."

    if ($script:wasRunning) {
        Exit-HermesBenchmarkMode -Profile $script:restartProfile
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
    $document = New-BenchmarkDocument `
        -StartedAt $startedAt `
        -CompletedAt $completedAt `
        -QuickMode ([bool]$Quick) `
        -Acceleration $acceleration `
        -ModelManifest $modelManifest `
        -MaximumContext $maximumContext `
        -Manifest $manifest `
        -Binary $binary `
        -Profile $profile `
        -Validation $validation `
        -Cases $runCases.ToArray() `
        -LifecycleState 'complete'

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
            Restore-HermesBenchmarkStack -Profile $script:restartProfile
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
