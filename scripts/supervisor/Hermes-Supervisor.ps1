[CmdletBinding()]
param(
    [ValidatePattern('^[A-Za-z][A-Za-z0-9 ]{0,31}$')]
    [string] $Profile
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$root = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
Import-Module (Join-Path $root 'scripts\Common-Hermes.psm1') -Force
Import-Module (Join-Path $root 'scripts\Hermes-Configuration.psm1') -Force
. (Join-Path $PSScriptRoot 'Hermes-Job.ps1')

$configuration = Get-HermesConfiguration
if (-not $Profile) {
    $Profile = [string]$configuration.selectedProfile
}
$runtimeDirectory = Resolve-HermesPath 'data\runtime'
$statusPath = Join-Path $runtimeDirectory 'status.json'
$controllerPidPath = Join-Path $runtimeDirectory 'supervisor.pid'
$stopRequestPath = Join-Path $runtimeDirectory 'stop.request'
$modelPort = [int]$configuration.network.modelPort
$hermesPort = [int]$configuration.network.hermesPort
$listenHost = [string]$configuration.network.host
$modelBase = "http://$listenHost`:$modelPort"
$hermesBase = "http://$listenHost`:$hermesPort"
$modelProcess = $null
$hermesProcess = $null
$job = $null
$mutex = $null
$createdNew = $false
$startedAt = (Get-Date).ToUniversalTime()
$restartTimes = [System.Collections.Generic.List[datetime]]::new()

function Write-SupervisorState {
    param(
        [Parameter(Mandatory)]
        [string] $Phase,
        [string] $Message = '',
        [bool] $ModelHealthy = $false,
        [bool] $HermesHealthy = $false
    )

    $state = [ordered]@{
        schemaVersion = 1
        phase = $Phase
        message = $Message
        profile = $Profile
        selectedModelId = [string]$configuration.selectedModelId
        controllerPid = $PID
        model = [ordered]@{
            pid = if ($modelProcess -and -not $modelProcess.HasExited) { $modelProcess.Id } else { $null }
            healthy = $ModelHealthy
            url = $modelBase
            name = [string]$configuration.selectedModel.displayName
            alias = [string]$configuration.selectedModel.alias
        }
        hermes = [ordered]@{
            pid = if ($hermesProcess -and -not $hermesProcess.HasExited) { $hermesProcess.Id } else { $null }
            healthy = $HermesHealthy
            url = $hermesBase
        }
        dashboard = [ordered]@{
            pid = if ($hermesProcess -and -not $hermesProcess.HasExited) { $hermesProcess.Id } else { $null }
            healthy = $HermesHealthy
            url = $hermesBase
            sharedWithHermesBackend = $true
        }
        startedAt = $startedAt.ToString('o')
        updatedAt = (Get-Date).ToUniversalTime().ToString('o')
        restartCount = $restartTimes.Count
    }
    Write-HermesAtomicText -Path $statusPath -Content (($state | ConvertTo-Json -Depth 8) + [Environment]::NewLine)
}

function Test-Endpoint {
    param(
        [Parameter(Mandatory)]
        [string] $Uri,
        [hashtable] $Headers = @{},
        [int] $TimeoutSeconds = 3
    )

    try {
        $response = Invoke-WebRequest -Uri $Uri -Headers $Headers -TimeoutSec $TimeoutSeconds -UseBasicParsing
        return $response.StatusCode -ge 200 -and $response.StatusCode -lt 300
    } catch {
        return $false
    }
}

function Wait-Endpoint {
    param(
        [Parameter(Mandatory)]
        [string] $Name,
        [Parameter(Mandatory)]
        [string] $Uri,
        [Parameter(Mandatory)]
        [System.Diagnostics.Process] $Process,
        [hashtable] $Headers = @{},
        [int] $TimeoutSeconds = 600
    )

    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
        if ($Process.HasExited) {
            throw "$Name exited with code $($Process.ExitCode) before becoming healthy."
        }
        if (Test-Endpoint -Uri $Uri -Headers $Headers -TimeoutSeconds 3) {
            return
        }
        Start-Sleep -Milliseconds 750
    }
    throw "$Name did not become healthy within $TimeoutSeconds seconds."
}

function Assert-PortAvailable {
    param(
        [Parameter(Mandatory)]
        [int] $Port
    )

    $listener = Get-NetTCPConnection -State Listen -LocalPort $Port -ErrorAction SilentlyContinue |
        Select-Object -First 1
    if ($listener) {
        throw "TCP port $Port is already in use by PID $($listener.OwningProcess)."
    }
}

function Start-ManagedProcess {
    param(
        [Parameter(Mandatory)]
        [string] $FilePath,
        [Parameter(Mandatory)]
        [string[]] $ArgumentList,
        [Parameter(Mandatory)]
        [string] $WorkingDirectory,
        [hashtable] $Environment = @{},
        [switch] $RedirectInput
    )

    if (-not (Test-Path -LiteralPath $FilePath -PathType Leaf)) {
        throw "Executable not found: $FilePath"
    }
    if (-not (Test-Path -LiteralPath $WorkingDirectory -PathType Container)) {
        throw "Working directory not found: $WorkingDirectory"
    }

    $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = [System.IO.Path]::GetFullPath($FilePath)
    $startInfo.WorkingDirectory = [System.IO.Path]::GetFullPath($WorkingDirectory)
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardInput = [bool]$RedirectInput
    foreach ($argument in $ArgumentList) {
        $startInfo.ArgumentList.Add([string]$argument)
    }
    foreach ($entry in $Environment.GetEnumerator()) {
        $startInfo.Environment[[string]$entry.Key] = [string]$entry.Value
    }

    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $startInfo
    if (-not $process.Start()) {
        throw "Failed to start $FilePath."
    }
    try {
        $job.Assign($process)
    } catch {
        if (-not $process.HasExited) {
            $process.Kill($true)
        }
        throw
    }
    return $process
}

function Stop-ManagedProcess {
    param(
        [System.Diagnostics.Process] $Process,
        [Parameter(Mandatory)]
        [string] $Name,
        [int] $GraceSeconds = 8,
        [switch] $CloseInput
    )

    if (-not $Process -or $Process.HasExited) {
        return
    }
    Write-HermesLog -Component supervisor -Message "Stopping $Name PID $($Process.Id)."
    if ($CloseInput) {
        try {
            $Process.StandardInput.Close()
        } catch {
            Write-HermesLog -Component supervisor -Level WARN -Message "$Name stdin close failed: $($_.Exception.Message)"
        }
    } else {
        try {
            [void]$Process.CloseMainWindow()
        } catch {
            Write-HermesLog -Component supervisor -Level WARN -Message "$Name graceful close request failed: $($_.Exception.Message)"
        }
    }
    if (-not $Process.WaitForExit($GraceSeconds * 1000)) {
        Write-HermesLog -Component supervisor -Level WARN -Message "$Name did not stop within $GraceSeconds seconds; terminating its process tree."
        $Process.Kill($true)
        [void]$Process.WaitForExit(5000)
    }
}

function Get-SelectedProfile {
    $matches = @($configuration.profiles | Where-Object name -eq $Profile)
    if ($matches.Count -ne 1) {
        throw "Profile '$Profile' was not found exactly once."
    }
    return $matches[0]
}

function Get-LlamaArguments {
    param(
        [Parameter(Mandatory)]
        [pscustomobject] $SelectedProfile,
        [Parameter(Mandatory)]
        [string] $ModelPath,
        [Parameter(Mandatory)]
        [string] $ApiKeyFile
    )

    $acceleration = Get-HermesEffectiveAcceleration -Configuration $configuration
    $layers = $(if ($acceleration -eq 'cpu') { '0' } else { [string]$SelectedProfile.gpu.layers })
    $arguments = [System.Collections.Generic.List[string]]::new()
    foreach ($value in @(
        '-m', $ModelPath,
        '--alias', [string]$configuration.selectedModel.alias,
        '--host', $listenHost,
        '--port', [string]$modelPort,
        '-c', [string]$SelectedProfile.contextTokens,
        '-ctk', [string]$SelectedProfile.kvCache.keyType,
        '-ctv', [string]$SelectedProfile.kvCache.valueType,
        '-t', [string]$SelectedProfile.threads.generation,
        '-tb', [string]$SelectedProfile.threads.batch,
        '-b', [string]$SelectedProfile.batch.logical,
        '-ub', [string]$SelectedProfile.batch.physical,
        '-ngl', $layers,
        '-fa', $(if ($SelectedProfile.flashAttention) { 'on' } else { 'off' }),
        $(if ($SelectedProfile.promptCache) { '--cache-prompt' } else { '--no-cache-prompt' }),
        '--metrics',
        '--no-ui',
        '--no-cors-credentials',
        '--api-key-file', $ApiKeyFile,
        '--log-file', (Resolve-HermesPath 'logs\model-server\llama-server.log'),
        '--log-prefix'
    )) {
        $arguments.Add([string]$value)
    }
    if ($acceleration -ne 'cpu') {
        $arguments.Add('-fit')
        $arguments.Add('on')
        $arguments.Add('-fitt')
        $arguments.Add([string]$SelectedProfile.gpu.vramReserveMiB)
    }
    if (-not $configuration.selectedModel.server -or $configuration.selectedModel.server.jinja -ne $false) {
        $arguments.Add('--jinja')
    } else {
        $arguments.Add('--no-jinja')
    }
    if ($configuration.selectedModel.server -and $configuration.selectedModel.server.chatTemplate) {
        $arguments.Add('--chat-template')
        $arguments.Add([string]$configuration.selectedModel.server.chatTemplate)
    }
    if ($configuration.selectedModel.server -and $configuration.selectedModel.server.extraArguments) {
        foreach ($argument in @($configuration.selectedModel.server.extraArguments)) {
            $arguments.Add([string]$argument)
        }
    }
    if ($SelectedProfile.PSObject.Properties.Name -contains 'seed') {
        $arguments.Add('--seed')
        $arguments.Add([string]$SelectedProfile.seed)
    }
    return $arguments.ToArray()
}

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

    $llamaServer = @(Get-ChildItem -LiteralPath (Resolve-HermesPath 'runtimes\llama.cpp\build') -Recurse -Filter llama-server.exe -File)
    if ($llamaServer.Count -ne 1) {
        throw "Expected one llama-server.exe; found $($llamaServer.Count)."
    }
    $hermesExecutable = Resolve-HermesPath 'runtimes\python\hermes\Scripts\hermes.exe'
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
            -WorkingDirectory (Resolve-HermesPath 'data\user') `
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

    Write-SupervisorState -Phase 'starting-hermes' -Message 'Model ready; starting Hermes backend.' -ModelHealthy $true
    Write-HermesLog -Component supervisor -Message 'Model server is healthy; starting the unified Hermes backend and dashboard.'
    $hermesEnvironment = @{
        HERMES_HOME = (Resolve-HermesPath 'data\hermes')
        HERMES_DASHBOARD_SESSION_TOKEN = $Token
        HERMES_LOCAL_API_TOKEN = $Token
        LLAMA_API_KEY = $Token
        UV_CACHE_DIR = (Resolve-HermesPath 'cache\uv')
        HF_HOME = (Resolve-HermesPath 'cache\huggingface')
        TRANSFORMERS_CACHE = (Resolve-HermesPath 'cache\huggingface\transformers')
        PLAYWRIGHT_BROWSERS_PATH = (Resolve-HermesPath 'cache\playwright')
    }
    $script:hermesProcess = Start-ManagedProcess `
        -FilePath $hermesExecutable `
        -ArgumentList @('dashboard', '--host', $listenHost, '--port', [string]$hermesPort, '--skip-build', '--no-open') `
        -WorkingDirectory (Resolve-HermesPath 'source\hermes-agent') `
        -Environment $hermesEnvironment
    Wait-Endpoint -Name 'Hermes backend/dashboard' -Uri "$hermesBase/api/health" -Process $hermesProcess -TimeoutSeconds 120
    Write-SupervisorState -Phase 'running' -Message 'Hermes Local is ready.' -ModelHealthy $true -HermesHealthy $true
    Write-HermesLog -Component supervisor -Message "Stack ready. Model PID $($modelProcess.Id); Hermes PID $($hermesProcess.Id)."
}

function Stop-Stack {
    Write-SupervisorState -Phase 'stopping' -Message 'Stopping services in reverse order.'
    Stop-ManagedProcess -Process $hermesProcess -Name 'Hermes backend/dashboard' -GraceSeconds 8
    Stop-ManagedProcess -Process $modelProcess -Name 'llama-server' -GraceSeconds 20 -CloseInput
    $script:hermesProcess = $null
    $script:modelProcess = $null
}

try {
    Assert-HermesRoot
    Initialize-HermesLayout
    Set-HermesProcessEnvironment
    [System.IO.Directory]::CreateDirectory($runtimeDirectory) | Out-Null
    $rootHash = [Convert]::ToHexString(
        [System.Security.Cryptography.SHA256]::HashData(
            [System.Text.Encoding]::UTF8.GetBytes((Get-HermesRoot).ToLowerInvariant())
        )
    ).Substring(0, 12)
    $mutex = [System.Threading.Mutex]::new($true, "Local\HermesLocalSupervisor-$rootHash", [ref]$createdNew)
    if (-not $createdNew) {
        Write-HermesLog -Component supervisor -Level WARN -Message 'A Hermes Local supervisor is already running.'
        exit 16
    }
    if (Test-Path -LiteralPath $stopRequestPath) {
        Remove-Item -LiteralPath $stopRequestPath -Force
    }
    Write-HermesAtomicText -Path $controllerPidPath -Content ("$PID" + [Environment]::NewLine)
    $job = [Hermes.Local.WindowsJob]::new("HermesLocal-$rootHash-$PID")
    $token = Get-OrCreateHermesApiToken
    Start-Stack -Token $token

    $consecutiveHealthFailures = 0
    while (-not (Test-Path -LiteralPath $stopRequestPath)) {
        Start-Sleep -Seconds 2
        $modelHealthy = $modelProcess -and -not $modelProcess.HasExited -and
            (Test-Endpoint -Uri "$modelBase/health" -TimeoutSeconds 2)
        $hermesHealthy = $hermesProcess -and -not $hermesProcess.HasExited -and
            (Test-Endpoint -Uri "$hermesBase/api/health" -TimeoutSeconds 2)
        if ($modelHealthy -and $hermesHealthy) {
            $consecutiveHealthFailures = 0
            Write-SupervisorState -Phase 'running' -Message 'Hermes Local is ready.' -ModelHealthy $true -HermesHealthy $true
            continue
        }

        $consecutiveHealthFailures++
        Write-SupervisorState -Phase 'degraded' -Message "Health failure $consecutiveHealthFailures of 3." -ModelHealthy $modelHealthy -HermesHealthy $hermesHealthy
        if ($consecutiveHealthFailures -lt 3) {
            continue
        }

        $now = (Get-Date).ToUniversalTime()
        for ($index = $restartTimes.Count - 1; $index -ge 0; $index--) {
            if (($now - $restartTimes[$index]).TotalMinutes -gt 5) {
                $restartTimes.RemoveAt($index)
            }
        }
        if ($restartTimes.Count -ge 5) {
            throw 'Restart-loop protection opened after five failures in five minutes.'
        }
        $restartTimes.Add($now)
        $delaySeconds = [math]::Min(16, [math]::Pow(2, $restartTimes.Count - 1))
        Write-HermesLog -Component supervisor -Level WARN -Message "Stack health failed; restarting after $delaySeconds second(s)."
        Stop-Stack
        Start-Sleep -Seconds $delaySeconds
        Start-Stack -Token $token
        $consecutiveHealthFailures = 0
    }

    Stop-Stack
    Write-SupervisorState -Phase 'stopped' -Message 'Hermes Local stopped.'
    Write-HermesLog -Component supervisor -Message 'Supervisor stopped normally.'
    exit 0
} catch {
    Write-HermesLog -Component supervisor -Level ERROR -Message $_.Exception.ToString()
    try {
        Stop-Stack
        Write-SupervisorState -Phase 'failed' -Message $_.Exception.Message
    } catch {
        Write-HermesLog -Component supervisor -Level ERROR -Message "Cleanup failed: $($_.Exception.Message)"
    }
    exit 1
} finally {
    if ($job) {
        $job.Dispose()
    }
    if (Test-Path -LiteralPath $controllerPidPath) {
        Remove-Item -LiteralPath $controllerPidPath -Force -ErrorAction SilentlyContinue
    }
    if (Test-Path -LiteralPath $stopRequestPath) {
        Remove-Item -LiteralPath $stopRequestPath -Force -ErrorAction SilentlyContinue
    }
    if ($mutex) {
        try {
            $mutex.ReleaseMutex()
        } catch {
        }
        $mutex.Dispose()
    }
}
