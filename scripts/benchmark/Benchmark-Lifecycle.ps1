function Test-HermesProcessAlive {
    param([Parameter(Mandatory)][int] $ProcessId)
    return $null -ne (Get-Process -Id $ProcessId -ErrorAction SilentlyContinue)
}

function Get-CurrentSupervisorStatus {
    $statusPath = Resolve-HermesPath 'data\runtime\status.json'
    if (-not (Test-Path -LiteralPath $statusPath)) {
        return $null
    }
    try {
        return Get-Content -Raw -LiteralPath $statusPath | ConvertFrom-Json
    } catch {
        return $null
    }
}

function Wait-HermesBenchmarkPhase {
    param(
        [Parameter(Mandatory)]
        [ValidateSet('benchmarking', 'running')]
        [string] $Phase,
        [int] $TimeoutSeconds = 960
    )

    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
        $status = Get-CurrentSupervisorStatus
        if ($status) {
            $controllerPid = [int](Get-BenchmarkValue -Record $status -Name controllerPid -Default 0)
            if ($controllerPid -gt 0 -and -not (Test-HermesProcessAlive -ProcessId $controllerPid)) {
                throw "Hermes Local supervisor PID $controllerPid exited during benchmark lifecycle coordination."
            }
            $gateway = Get-BenchmarkValue -Record $status -Name gateway
            $gatewayRequired = [bool](Get-BenchmarkValue -Record $gateway -Name required -Default $false)
            $gatewayHealthy = [bool](Get-BenchmarkValue -Record $gateway -Name healthy -Default (-not $gatewayRequired))
            $gatewayReady = -not $gatewayRequired -or $gatewayHealthy
            $hermes = Get-BenchmarkValue -Record $status -Name hermes
            $model = Get-BenchmarkValue -Record $status -Name model
            if ($Phase -eq 'benchmarking' -and
                [string]$status.phase -eq 'benchmarking' -and
                -not (Get-BenchmarkValue -Record $model -Name pid) -and
                [bool](Get-BenchmarkValue -Record $hermes -Name healthy -Default $false) -and
                $gatewayReady) {
                return
            }
            if ($Phase -eq 'running' -and
                [string]$status.phase -eq 'running' -and
                [bool](Get-BenchmarkValue -Record $model -Name healthy -Default $false) -and
                [bool](Get-BenchmarkValue -Record $hermes -Name healthy -Default $false) -and
                $gatewayReady) {
                return
            }
            if ([string]$status.phase -eq 'failed') {
                throw "Hermes Local supervisor failed during benchmark lifecycle coordination: $($status.message)"
            }
        }
        Start-Sleep -Milliseconds 500
    }
    throw "Hermes Local did not enter '$Phase' benchmark lifecycle state within $TimeoutSeconds seconds."
}

function Enter-HermesBenchmarkMode {
    param([Parameter(Mandatory)][string] $Profile)

    if (Test-Path -LiteralPath $script:benchmarkRequestPath) {
        try {
            $existing = Get-Content -Raw -LiteralPath $script:benchmarkRequestPath | ConvertFrom-Json
            $existingOwnerPid = [int](Get-BenchmarkValue -Record $existing -Name ownerPid -Default 0)
            if ($existingOwnerPid -gt 0 -and (Test-HermesProcessAlive -ProcessId $existingOwnerPid)) {
                throw "Benchmark lifecycle is already owned by PID $existingOwnerPid."
            }
        } catch [System.Management.Automation.RuntimeException] {
            throw
        } catch {
            Write-HermesLog -Component benchmarks -Level WARN -Message "Removing unreadable stale benchmark request: $($_.Exception.Message)"
        }
        Remove-Item -LiteralPath $script:benchmarkRequestPath -Force -ErrorAction SilentlyContinue
    }

    $request = [ordered]@{
        schemaVersion = 1
        ownerPid = $PID
        profile = $Profile
        requestedAt = (Get-Date).ToUniversalTime().ToString('o')
    }
    Write-HermesAtomicText -Path $script:benchmarkRequestPath -Content (($request | ConvertTo-Json -Depth 4) + [Environment]::NewLine)
    Write-HermesLog -Component benchmarks -Message 'Requested exclusive model access while preserving Desktop and gateway services.'
    try {
        Wait-HermesBenchmarkPhase -Phase benchmarking -TimeoutSeconds 120
    } catch {
        Remove-Item -LiteralPath $script:benchmarkRequestPath -Force -ErrorAction SilentlyContinue
        throw
    }
}

function Exit-HermesBenchmarkMode {
    Remove-Item -LiteralPath $script:benchmarkRequestPath -Force -ErrorAction SilentlyContinue
    Write-HermesLog -Component benchmarks -Message 'Released exclusive model access; waiting for the model server to return.'
    Wait-HermesBenchmarkPhase -Phase running -TimeoutSeconds 960
    $script:stackRestored = $true
}

