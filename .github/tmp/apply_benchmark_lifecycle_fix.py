from __future__ import annotations

from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def replace_exact(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    occurrences = text.count(old)
    if occurrences != 1:
        raise RuntimeError(f"Expected exactly one match in {path}, found {occurrences}")
    path.write_text(text.replace(old, new), encoding="utf-8", newline="\n")


benchmark = ROOT / "Benchmark-Hermes-Local.ps1"
replace_exact(
    benchmark,
    """$stackRestarted = $false
$temporaryFiles = [System.Collections.Generic.List[string]]::new()

function Get-Percentile {
""",
    """$stackRestarted = $false
$temporaryFiles = [System.Collections.Generic.List[string]]::new()
$benchmarkRequestPath = Resolve-HermesPath 'data\\runtime\\benchmark.request.json'

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

    $statusPath = Resolve-HermesPath 'data\\runtime\\status.json'
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
""",
)
replace_exact(
    benchmark,
    """    if ($wasRunning) {
        & (Resolve-HermesPath 'Stop-Hermes-Local.ps1') -NonInteractive
        if ($LASTEXITCODE -ne 0) {
            throw 'Could not stop the stack for exclusive benchmark access.'
        }
    }
""",
    """    if ($wasRunning) {
        Enter-HermesBenchmarkMode -Profile $restartProfile
    }
""",
)
replace_exact(
    benchmark,
    """    if ($wasRunning) {
        & (Resolve-HermesPath 'Start-Hermes-Local.ps1') -Profile $restartProfile -NonInteractive
        if ($LASTEXITCODE -ne 0) {
            throw 'Benchmark completed, but the previous stack profile could not be restarted.'
        }
        $stackRestarted = $true
    }
""",
    """    if ($wasRunning) {
        Exit-HermesBenchmarkMode
        $stackRestarted = $true
    }
""",
)
replace_exact(
    benchmark,
    """} finally {
    if ($wasRunning -and -not $stackRestarted) {
        try {
            & (Resolve-HermesPath 'Start-Hermes-Local.ps1') -Profile $restartProfile -NonInteractive
        } catch {
            Write-HermesLog -Component benchmarks -Level ERROR -Message "Could not restore stack after benchmark failure: $($_.Exception.Message)"
        }
    }
""",
    """} finally {
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
""",
)

supervisor = ROOT / "scripts" / "supervisor" / "Hermes-Supervisor.ps1"
replace_exact(
    supervisor,
    """$controllerPidPath = Join-Path $runtimeDirectory 'supervisor.pid'
$stopRequestPath = Join-Path $runtimeDirectory 'stop.request'
$modelPort = [int]$configuration.network.modelPort
""",
    """$controllerPidPath = Join-Path $runtimeDirectory 'supervisor.pid'
$stopRequestPath = Join-Path $runtimeDirectory 'stop.request'
$benchmarkRequestPath = Join-Path $runtimeDirectory 'benchmark.request.json'
$modelPort = [int]$configuration.network.modelPort
""",
)
replace_exact(
    supervisor,
    """$createdNew = $false
$startedAt = (Get-Date).ToUniversalTime()
$restartTimes = [System.Collections.Generic.List[datetime]]::new()
""",
    """$createdNew = $false
$startedAt = (Get-Date).ToUniversalTime()
$benchmarkMode = $false
$restartTimes = [System.Collections.Generic.List[datetime]]::new()
""",
)
old_start_stack = """
function Start-Stack {
    param(
        [Parameter(Mandatory)]
        [string] $Token
    )

    $selectedProfile = Get-SelectedProfile
    $selectedModel = $configuration.selectedModel
    $modelPath = [string]$selectedModel.resolvedPath
    $verifyHash = [bool]$configuration.runtime.verifyModelOnStart
    if (-not (Test-HermesSelectedModel -Model $selectedModel -Hash:$verifyHash)) {
        throw "Model integrity validation failed: $modelPath"
    }

    $llamaServer = @(Get-ChildItem -LiteralPath (Resolve-HermesPath 'runtimes\\llama.cpp\\build') -Recurse -Filter llama-server.exe -File)
    if ($llamaServer.Count -ne 1) {
        throw "Expected one llama-server.exe; found $($llamaServer.Count)."
    }
    $hermesExecutable = Resolve-HermesPath 'runtimes\\python\\hermes\\Scripts\\hermes.exe'
    $apiKeyFile = Join-Path $runtimeDirectory "llama-api-key-$PID.txt"
    Assert-PortAvailable -Port $modelPort
    Assert-PortAvailable -Port $hermesPort

    Sync-HermesRuntimeConfiguration -Configuration $configuration
    Write-SupervisorState -Phase 'starting-model' -Message "Validating and loading $($selectedModel.displayName)."
    Write-HermesLog -Component supervisor -Message "Starting '$($selectedModel.displayName)' with profile '$Profile'."
    [System.IO.File]::WriteAllText($apiKeyFile, $Token + [Environment]::NewLine, [System.Text.UTF8Encoding]::new($false))
    $acl = Get-Acl -LiteralPath $apiKeyFile
    $currentSid = [System.Security.Principal.WindowsIdentity]::GetCurrent().User
    $acl.SetAccessRuleProtection($true, $false)
    foreach ($rule in @($acl.Access)) {
        [void]$acl.RemoveAccessRuleAll($rule)
    }
    $accessRule = [System.Security.AccessControl.FileSystemAccessRule]::new(
        $currentSid,
        [System.Security.AccessControl.FileSystemRights]::FullControl,
        [System.Security.AccessControl.AccessControlType]::Allow
    )
    $acl.SetOwner($currentSid)
    $acl.SetAccessRule($accessRule)
    Set-Acl -LiteralPath $apiKeyFile -AclObject $acl
    try {
        $script:modelProcess = Start-ManagedProcess `
            -FilePath $llamaServer[0].FullName `
            -ArgumentList (Get-LlamaArguments -SelectedProfile $selectedProfile -ModelPath $modelPath -ApiKeyFile $apiKeyFile) `
            -WorkingDirectory (Resolve-HermesPath 'data\\user') `
            -RedirectInput
        $authHeaders = @{ Authorization = "Bearer $Token" }
        Wait-Endpoint -Name 'llama-server' -Uri "$modelBase/health" -Process $modelProcess -TimeoutSeconds 900
    } finally {
        if (Test-Path -LiteralPath $apiKeyFile) {
            Remove-Item -LiteralPath $apiKeyFile -Force
        }
    }
    if (-not (Test-Endpoint -Uri "$modelBase/v1/models" -Headers $authHeaders -TimeoutSeconds 10)) {
        throw 'llama-server health passed but /v1/models did not.'
    }

"""
new_start_stack = """
function Get-ActiveBenchmarkRequest {
    if (-not (Test-Path -LiteralPath $benchmarkRequestPath)) {
        return $null
    }

    try {
        $request = Get-Content -Raw -LiteralPath $benchmarkRequestPath | ConvertFrom-Json
        $ownerPid = 0
        if (-not $request.ownerPid -or
            -not [int]::TryParse([string]$request.ownerPid, [ref]$ownerPid) -or
            $ownerPid -le 0) {
            throw 'Benchmark request does not contain a valid owner PID.'
        }
        if (-not (Get-Process -Id $ownerPid -ErrorAction SilentlyContinue)) {
            Write-HermesLog -Component supervisor -Level WARN -Message "Removing stale benchmark request owned by exited PID $ownerPid."
            Remove-Item -LiteralPath $benchmarkRequestPath -Force -ErrorAction SilentlyContinue
            return $null
        }
        return $request
    } catch {
        Write-HermesLog -Component supervisor -Level WARN -Message "Removing invalid benchmark request: $($_.Exception.Message)"
        Remove-Item -LiteralPath $benchmarkRequestPath -Force -ErrorAction SilentlyContinue
        return $null
    }
}

function Get-SupervisorGatewayHealth {
    $healthy = -not $gatewayRequired
    $state = if ($gatewayRequired) { 'unknown' } else { 'disabled' }
    $message = ''
    if ($gatewayRequired) {
        try {
            $snapshot = Get-HermesGatewaySnapshot
            if ($snapshot.pid) {
                $script:gatewayRuntimePid = [int]$snapshot.pid
            }
            $healthy = [bool]$snapshot.healthy
            $state = [string]$snapshot.state
            if (-not $healthy) {
                $message = Get-HermesGatewayFailureDetail -Snapshot $snapshot
            }
        } catch {
            $message = $_.Exception.Message
        }
    }
    return [pscustomobject]@{
        healthy = $healthy
        state = $state
        message = $message
    }
}

function Start-Model {
    param(
        [Parameter(Mandatory)]
        [string] $Token,
        [switch] $PreserveDesktopServices
    )

    $selectedProfile = Get-SelectedProfile
    $selectedModel = $configuration.selectedModel
    $modelPath = [string]$selectedModel.resolvedPath
    $verifyHash = [bool]$configuration.runtime.verifyModelOnStart
    if (-not (Test-HermesSelectedModel -Model $selectedModel -Hash:$verifyHash)) {
        throw "Model integrity validation failed: $modelPath"
    }

    $llamaServer = @(Get-ChildItem -LiteralPath (Resolve-HermesPath 'runtimes\\llama.cpp\\build') -Recurse -Filter llama-server.exe -File)
    if ($llamaServer.Count -ne 1) {
        throw "Expected one llama-server.exe; found $($llamaServer.Count)."
    }
    $apiKeyFile = Join-Path $runtimeDirectory "llama-api-key-$PID.txt"
    Assert-PortAvailable -Port $modelPort

    Sync-HermesRuntimeConfiguration -Configuration $configuration
    $gatewayHealth = Get-SupervisorGatewayHealth
    $desktopHealthy = $PreserveDesktopServices -and $hermesProcess -and -not $hermesProcess.HasExited -and
        (Test-Endpoint -Uri "$hermesBase/api/health" -TimeoutSeconds 2)
    Write-SupervisorState -Phase 'starting-model' -Message "Validating and loading $($selectedModel.displayName)." `
        -HermesHealthy $desktopHealthy -GatewayHealthy $gatewayHealth.healthy `
        -GatewayState $gatewayHealth.state -GatewayMessage $gatewayHealth.message
    Write-HermesLog -Component supervisor -Message "Starting '$($selectedModel.displayName)' with profile '$Profile'."
    [System.IO.File]::WriteAllText($apiKeyFile, $Token + [Environment]::NewLine, [System.Text.UTF8Encoding]::new($false))
    $acl = Get-Acl -LiteralPath $apiKeyFile
    $currentSid = [System.Security.Principal.WindowsIdentity]::GetCurrent().User
    $acl.SetAccessRuleProtection($true, $false)
    foreach ($rule in @($acl.Access)) {
        [void]$acl.RemoveAccessRuleAll($rule)
    }
    $accessRule = [System.Security.AccessControl.FileSystemAccessRule]::new(
        $currentSid,
        [System.Security.AccessControl.FileSystemRights]::FullControl,
        [System.Security.AccessControl.AccessControlType]::Allow
    )
    $acl.SetOwner($currentSid)
    $acl.SetAccessRule($accessRule)
    Set-Acl -LiteralPath $apiKeyFile -AclObject $acl
    try {
        $script:modelProcess = Start-ManagedProcess `
            -FilePath $llamaServer[0].FullName `
            -ArgumentList (Get-LlamaArguments -SelectedProfile $selectedProfile -ModelPath $modelPath -ApiKeyFile $apiKeyFile) `
            -WorkingDirectory (Resolve-HermesPath 'data\\user') `
            -RedirectInput
        $authHeaders = @{ Authorization = "Bearer $Token" }
        Wait-Endpoint -Name 'llama-server' -Uri "$modelBase/health" -Process $modelProcess -TimeoutSeconds 900
    } finally {
        if (Test-Path -LiteralPath $apiKeyFile) {
            Remove-Item -LiteralPath $apiKeyFile -Force
        }
    }
    if (-not (Test-Endpoint -Uri "$modelBase/v1/models" -Headers $authHeaders -TimeoutSeconds 10)) {
        throw 'llama-server health passed but /v1/models did not.'
    }
}

function Start-Stack {
    param(
        [Parameter(Mandatory)]
        [string] $Token
    )

    Start-Model -Token $Token
    $hermesExecutable = Resolve-HermesPath 'runtimes\\python\\hermes\\Scripts\\hermes.exe'
    Assert-PortAvailable -Port $hermesPort

"""
replace_exact(supervisor, old_start_stack, new_start_stack)
replace_exact(
    supervisor,
    """    while (-not (Test-Path -LiteralPath $stopRequestPath)) {
        Start-Sleep -Seconds 2
        $modelHealthy = $modelProcess -and -not $modelProcess.HasExited -and
            (Test-Endpoint -Uri "$modelBase/health" -TimeoutSeconds 2)
        $hermesHealthy = $hermesProcess -and -not $hermesProcess.HasExited -and
            (Test-Endpoint -Uri "$hermesBase/api/health" -TimeoutSeconds 2)
        $gatewaySnapshot = $null
        $gatewayHealthy = -not $gatewayRequired
        $gatewayState = if ($gatewayRequired) { 'unknown' } else { 'disabled' }
        $gatewayMessage = ''
        if ($gatewayRequired) {
            try {
                $gatewaySnapshot = Get-HermesGatewaySnapshot
                if ($gatewaySnapshot.pid) {
                    $script:gatewayRuntimePid = [int]$gatewaySnapshot.pid
                }
                $gatewayHealthy = [bool]$gatewaySnapshot.healthy
                $gatewayState = [string]$gatewaySnapshot.state
                if (-not $gatewayHealthy) {
                    $gatewayMessage = Get-HermesGatewayFailureDetail -Snapshot $gatewaySnapshot
                }
            } catch {
                $gatewayMessage = $_.Exception.Message
            }
        }
""",
    """    while (-not (Test-Path -LiteralPath $stopRequestPath)) {
        Start-Sleep -Seconds 2
        $benchmarkRequest = Get-ActiveBenchmarkRequest
        $hermesHealthy = $hermesProcess -and -not $hermesProcess.HasExited -and
            (Test-Endpoint -Uri "$hermesBase/api/health" -TimeoutSeconds 2)
        $gatewayHealth = Get-SupervisorGatewayHealth
        $gatewayHealthy = [bool]$gatewayHealth.healthy
        $gatewayState = [string]$gatewayHealth.state
        $gatewayMessage = [string]$gatewayHealth.message

        if ($benchmarkRequest -and -not $benchmarkMode) {
            if (-not $hermesHealthy -or -not $gatewayHealthy) {
                throw 'Desktop or gateway services were not healthy before entering benchmark mode.'
            }
            Write-SupervisorState -Phase 'benchmark-preparing' `
                -Message 'Preparing exclusive model access; Desktop and gateway services remain online.' `
                -ModelHealthy $true -HermesHealthy $true -GatewayHealthy $gatewayHealthy `
                -GatewayState $gatewayState -GatewayMessage $gatewayMessage
            Write-HermesLog -Component supervisor -Message "Benchmark PID $($benchmarkRequest.ownerPid) requested exclusive model access."
            Stop-ManagedProcess -Process $modelProcess -Name 'llama-server for benchmark access' -GraceSeconds 20 -CloseInput
            $script:modelProcess = $null
            $benchmarkMode = $true
            Write-SupervisorState -Phase 'benchmarking' `
                -Message 'Benchmark owns the model; Desktop and gateway services remain online.' `
                -HermesHealthy $true -GatewayHealthy $gatewayHealthy `
                -GatewayState $gatewayState -GatewayMessage $gatewayMessage
            continue
        }

        if ($benchmarkMode) {
            if ($benchmarkRequest) {
                if (-not $hermesHealthy -or -not $gatewayHealthy) {
                    throw 'Desktop or gateway services became unhealthy during benchmark mode.'
                }
                Write-SupervisorState -Phase 'benchmarking' `
                    -Message 'Benchmark owns the model; Desktop and gateway services remain online.' `
                    -HermesHealthy $true -GatewayHealthy $gatewayHealthy `
                    -GatewayState $gatewayState -GatewayMessage $gatewayMessage
                continue
            }

            Write-HermesLog -Component supervisor -Message 'Benchmark released model access; restoring llama-server without restarting Desktop services.'
            Start-Model -Token $token -PreserveDesktopServices
            $gatewayHealth = Get-SupervisorGatewayHealth
            $hermesHealthy = $hermesProcess -and -not $hermesProcess.HasExited -and
                (Test-Endpoint -Uri "$hermesBase/api/health" -TimeoutSeconds 2)
            if (-not $hermesHealthy -or -not $gatewayHealth.healthy) {
                throw 'Model returned after benchmark, but Desktop or gateway services were unhealthy.'
            }
            $benchmarkMode = $false
            $consecutiveHealthFailures = 0
            Write-SupervisorState -Phase 'running' -Message 'Hermes Local is ready after benchmark completion.' `
                -ModelHealthy $true -HermesHealthy $true -GatewayHealthy $gatewayHealth.healthy `
                -GatewayState $gatewayHealth.state -GatewayMessage $gatewayHealth.message
            continue
        }

        $modelHealthy = $modelProcess -and -not $modelProcess.HasExited -and
            (Test-Endpoint -Uri "$modelBase/health" -TimeoutSeconds 2)
""",
)

start_script = ROOT / "Start-Hermes-Local.ps1"
replace_exact(
    start_script,
    """function Get-RunningHermesSupervisor {
""",
    """function Test-HermesDesktopReadyState {
    param(
        [AllowNull()]
        [psobject] $Status,
        [Parameter(Mandatory)]
        [int] $ControllerPid
    )

    if (-not $Status -or
        $Status.PSObject.Properties.Name -notcontains 'phase' -or
        $Status.PSObject.Properties.Name -notcontains 'controllerPid' -or
        [int]$Status.controllerPid -ne $ControllerPid) {
        return $false
    }
    $gatewayReady = $Status.PSObject.Properties.Name -notcontains 'gateway' -or
        -not $Status.gateway.required -or $Status.gateway.healthy
    $desktopReady = $Status.hermes.healthy -and $gatewayReady
    $inferenceReady = $Status.phase -eq 'running' -and $Status.model.healthy
    $benchmarkReady = $Status.phase -in @('benchmark-preparing', 'benchmarking', 'starting-model') -and $desktopReady
    return $desktopReady -and ($inferenceReady -or $benchmarkReady)
}

function Get-RunningHermesSupervisor {
""",
)
replace_exact(
    start_script,
    """        if ($status -and
            $status.PSObject.Properties.Name -contains 'phase' -and
            $status.PSObject.Properties.Name -contains 'controllerPid' -and
            [int]$status.controllerPid -eq $existingPid -and
            $status.phase -eq 'running' -and
            $status.model.healthy -and
            $status.hermes.healthy -and
            ($status.PSObject.Properties.Name -notcontains 'gateway' -or
                -not $status.gateway.required -or $status.gateway.healthy)) {
            Write-Host "Hermes Local is already running with profile '$($status.profile)' (supervisor PID $existingPid)."
            exit 0
        }
""",
    """        if (Test-HermesDesktopReadyState -Status $status -ControllerPid $existingPid) {
            if ($status.phase -eq 'running') {
                Write-Host "Hermes Local is already running with profile '$($status.profile)' (supervisor PID $existingPid)."
            } else {
                Write-Host "Hermes Local Desktop services are ready while benchmark lifecycle state is '$($status.phase)' (supervisor PID $existingPid)."
            }
            exit 0
        }
""",
)
replace_exact(
    start_script,
    """                if ($status.phase -eq 'running' -and $status.model.healthy -and $status.hermes.healthy -and `
                    ($status.PSObject.Properties.Name -notcontains 'gateway' -or -not $status.gateway.required -or $status.gateway.healthy)) {
                    $gatewayDetail = if ($status.PSObject.Properties.Name -contains 'gateway' -and $status.gateway.required) {
                        " Gateway PID $($status.gateway.pid) ($($status.gateway.ownership))."
                    } else {
                        ' Messaging gateway disabled.'
                    }
                    Write-Host "Hermes Local is ready with profile '$Profile'. Model PID $($status.model.pid); Hermes PID $($status.hermes.pid).$gatewayDetail"
                    exit 0
                }
""",
    """                if (Test-HermesDesktopReadyState -Status $status -ControllerPid $process.Id) {
                    $gatewayDetail = if ($status.PSObject.Properties.Name -contains 'gateway' -and $status.gateway.required) {
                        " Gateway PID $($status.gateway.pid) ($($status.gateway.ownership))."
                    } else {
                        ' Messaging gateway disabled.'
                    }
                    if ($status.phase -eq 'running') {
                        Write-Host "Hermes Local is ready with profile '$Profile'. Model PID $($status.model.pid); Hermes PID $($status.hermes.pid).$gatewayDetail"
                    } else {
                        Write-Host "Hermes Local Desktop services are ready while benchmark lifecycle state is '$($status.phase)'. Hermes PID $($status.hermes.pid).$gatewayDetail"
                    }
                    exit 0
                }
""",
)

test_path = ROOT / "tests" / "Test-BenchmarkLifecycleContract.ps1"
test_path.write_text(
    """[CmdletBinding()]\nparam()\n\nSet-StrictMode -Version Latest\n$ErrorActionPreference = 'Stop'\n$root = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))\n\nfunction Assert-Contains {\n    param(\n        [Parameter(Mandatory)]\n        [string] $Text,\n        [Parameter(Mandatory)]\n        [string] $Expected,\n        [Parameter(Mandatory)]\n        [string] $Message\n    )\n    if (-not $Text.Contains($Expected, [System.StringComparison]::Ordinal)) {\n        throw $Message\n    }\n}\n\n$benchmark = Get-Content -Raw -LiteralPath (Join-Path $root 'Benchmark-Hermes-Local.ps1')\n$supervisor = Get-Content -Raw -LiteralPath (Join-Path $root 'scripts\\supervisor\\Hermes-Supervisor.ps1')\n$start = Get-Content -Raw -LiteralPath (Join-Path $root 'Start-Hermes-Local.ps1')\n\nAssert-Contains -Text $benchmark -Expected 'benchmark.request.json' -Message 'Benchmark lifecycle request contract is missing.'\nAssert-Contains -Text $benchmark -Expected 'Enter-HermesBenchmarkMode' -Message 'Benchmark does not request model-only maintenance mode.'\nif ($benchmark.Contains("Resolve-HermesPath 'Stop-Hermes-Local.ps1'", [System.StringComparison]::Ordinal)) {\n    throw 'Benchmark must not stop the complete Desktop, gateway and model stack.'\n}\nAssert-Contains -Text $supervisor -Expected "-Phase 'benchmarking'" -Message 'Supervisor benchmark phase is missing.'\nAssert-Contains -Text $supervisor -Expected "Stop-ManagedProcess -Process `$modelProcess -Name 'llama-server for benchmark access'" -Message 'Supervisor does not stop only the model for benchmark access.'\nAssert-Contains -Text $supervisor -Expected 'restoring llama-server without restarting Desktop services' -Message 'Supervisor model-only restoration contract is missing.'\nAssert-Contains -Text $start -Expected "'benchmark-preparing', 'benchmarking', 'starting-model'" -Message 'Desktop startup does not accept benchmark lifecycle readiness.'\n\nWrite-Host 'Benchmark lifecycle contract passed.'\n""",
    encoding="utf-8",
    newline="\n",
)

workflow = ROOT / ".github" / "workflows" / "powershell-validation.yml"
replace_exact(
    workflow,
    """          & '.\\tests\\Test-HermesGatewayModule.ps1'

      - name: Verify source override merge
""",
    """          & '.\\tests\\Test-HermesGatewayModule.ps1'
          & '.\\tests\\Test-BenchmarkLifecycleContract.ps1'

      - name: Verify source override merge
""",
)

print("Applied benchmark model-lease lifecycle fix.")
