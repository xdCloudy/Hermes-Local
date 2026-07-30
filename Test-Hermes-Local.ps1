[CmdletBinding()]
param(
    [switch] $BootstrapOnly,
    [switch] $Quick,
    [switch] $SkipAgentTool,
    [switch] $NonInteractive
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'scripts\Common-Hermes.psm1') -Force
Import-Module (Join-Path $PSScriptRoot 'scripts\Hermes-Configuration.psm1') -Force
Import-Module (Join-Path $PSScriptRoot 'scripts\Hermes-Gateway.psm1') -Force

$results = [System.Collections.Generic.List[object]]::new()

function Add-TestResult {
    param(
        [Parameter(Mandatory)]
        [string] $Name,
        [Parameter(Mandatory)]
        [bool] $Passed,
        [string] $Detail = ''
    )

    $results.Add([pscustomobject]@{
        name = $Name
        passed = $Passed
        detail = Protect-HermesLogText $Detail
    })
    if (-not $Passed) {
        throw "$Name failed: $Detail"
    }
}

try {
    Assert-HermesRoot
    Initialize-HermesLayout
    Set-HermesProcessEnvironment
    $configuration = Get-HermesConfiguration
    $selectedModel = $configuration.selectedModel
    $modelPath = [string]$selectedModel.resolvedPath
    $modelBase = "http://$($configuration.network.host):$($configuration.network.modelPort)"
    $hermesBase = "http://$($configuration.network.host):$($configuration.network.hermesPort)"
    $modelAlias = [string]$selectedModel.alias

    if ($BootstrapOnly) {
        $modelItem = Get-Item -LiteralPath $modelPath -ErrorAction Stop
        $sizeMatches = -not $selectedModel.sizeBytes -or $modelItem.Length -eq [int64]$selectedModel.sizeBytes
        Add-TestResult -Name 'Model file' -Passed $sizeMatches -Detail "$($modelItem.Length) bytes"
        $llamaServers = @(Get-ChildItem -LiteralPath (Resolve-HermesPath 'runtimes\llama.cpp\build') -Recurse -Filter llama-server.exe -File)
        Add-TestResult -Name 'llama-server' -Passed ($llamaServers.Count -eq 1) -Detail 'Native model server is installed.'
        Add-TestResult -Name 'Hermes runtime' -Passed (
            Test-Path -LiteralPath (Resolve-HermesPath 'runtimes\python\hermes\Scripts\hermes.exe') -PathType Leaf
        ) -Detail 'Project-managed Hermes entry point is installed.'
        Add-TestResult -Name 'Hermes source' -Passed (
            Test-Path -LiteralPath (Resolve-HermesPath 'source\hermes-agent\.git') -PathType Container
        ) -Detail 'Official source checkout is present.'
        Add-TestResult -Name 'Runtime configuration' -Passed (
            Test-Path -LiteralPath (Resolve-HermesPath 'data\hermes\config.yaml') -PathType Leaf
        ) -Detail 'Local Hermes configuration is present.'
        $configurationValidation = @(
            Invoke-HermesProcess `
                -FilePath (Resolve-HermesPath 'runtimes\python\hermes\Scripts\python.exe') `
                -ArgumentList @(
                    (Resolve-HermesPath 'scripts\validate_configuration.py'),
                    (Get-HermesRoot)
                ) `
                -LogComponent diagnostics `
                -PassThruOutput
        ) -join [Environment]::NewLine
        Add-TestResult -Name 'Portable configuration schemas' -Passed (
            $configurationValidation -match 'configuration is valid'
        ) -Detail 'Defaults, profiles, model manifests, and current-user settings satisfy their schemas.'

        $bootstrapReport = [ordered]@{
            schemaVersion = 1
            generatedAt = (Get-Date).ToUniversalTime().ToString('o')
            passed = $true
            bootstrapOnly = $true
            results = $results
        }
        $bootstrapReportPath = Resolve-HermesPath 'logs\diagnostics\latest-bootstrap.json'
        Write-HermesAtomicText -Path $bootstrapReportPath -Content (
            ($bootstrapReport | ConvertTo-Json -Depth 12) + [Environment]::NewLine
        )
        Write-HermesLog -Component diagnostics -Message (
            "Hermes Local bootstrap diagnostics passed $($results.Count) checks."
        )
        Write-Host "Hermes Local bootstrap diagnostics passed: $($results.Count) checks. Report: $bootstrapReportPath"
        exit 0
    }

    if ($Quick) {
        $item = Get-Item -LiteralPath $modelPath -ErrorAction Stop
        $sizeMatches = -not $selectedModel.sizeBytes -or $item.Length -eq [int64]$selectedModel.sizeBytes
        Add-TestResult -Name 'Model size' -Passed $sizeMatches -Detail "$($item.Length) bytes"
    } else {
        Add-TestResult -Name 'Model SHA-256' -Passed (
            Test-HermesSelectedModel -Model $selectedModel -Hash:([bool]$selectedModel.sha256)
        ) -Detail 'The selected model matches all integrity metadata supplied by its registration.'
    }

    $statusPath = Resolve-HermesPath 'data\runtime\status.json'
    $running = if (Test-Path -LiteralPath $statusPath) {
    $runtimeStatus = Get-Content -Raw -LiteralPath $statusPath | ConvertFrom-Json
    $gatewayReady = $runtimeStatus.PSObject.Properties.Name -notcontains 'gateway' -or
        -not $runtimeStatus.gateway.required -or $runtimeStatus.gateway.healthy
    $runtimeStatus.phase -eq 'running' -and $runtimeStatus.model.healthy -and
        $runtimeStatus.hermes.healthy -and $gatewayReady
} else {
    $false
}
    if (-not $running) {
        $null = Invoke-HermesProcess -FilePath 'pwsh.exe' -ArgumentList @(
            '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
            '-File', (Resolve-HermesPath 'Start-Hermes-Local.ps1'), '-NonInteractive'
        ) -LogComponent diagnostics
    }

    $token = Get-OrCreateHermesApiToken
    $headers = @{ Authorization = "Bearer $token" }
    $modelHealth = Invoke-RestMethod -Uri "$modelBase/health" -TimeoutSec 5
    Add-TestResult -Name 'Model health' -Passed ($modelHealth.status -eq 'ok') -Detail 'Structured model health returned ok.'
    $models = Invoke-RestMethod -Uri "$modelBase/v1/models" -Headers $headers -TimeoutSec 10
    Add-TestResult -Name 'Authenticated model inventory' -Passed (
        @($models.data | Where-Object id -eq $modelAlias).Count -eq 1
    ) -Detail "Selected model alias '$modelAlias' is available."
    $unauthenticatedDenied = $false
    try {
        $unauthenticatedRequest = @{
            model = $modelAlias
            messages = @(@{ role = 'user'; content = 'authentication probe' })
            max_tokens = 1
        } | ConvertTo-Json -Depth 8 -Compress
        Invoke-RestMethod -Method Post -Uri "$modelBase/v1/chat/completions" `
            -ContentType 'application/json' `
            -Body $unauthenticatedRequest `
            -TimeoutSec 5 | Out-Null
    } catch {
        $unauthenticatedDenied = $_.Exception.Response.StatusCode.value__ -in @(401, 403)
    }
    Add-TestResult -Name 'Model authentication' -Passed $unauthenticatedDenied -Detail 'Unauthenticated inference request was denied.'

    $hermesHealth = Invoke-RestMethod -Uri "$hermesBase/api/health" -TimeoutSec 5
    Add-TestResult -Name 'Hermes health' -Passed ([bool]$hermesHealth.ok) -Detail "Hermes $($hermesHealth.version)"
    $dashboard = Invoke-WebRequest -UseBasicParsing -Uri "$hermesBase/" -TimeoutSec 10
    Add-TestResult -Name 'Web dashboard' -Passed (
    $dashboard.StatusCode -eq 200 -and $dashboard.Headers.'Content-Type' -match 'text/html'
) -Detail 'Official dashboard HTML is served from loopback.'

$gatewaySnapshot = Get-HermesGatewaySnapshot -Discover
if ($gatewaySnapshot.required) {
    $gatewayDetail = if ($gatewaySnapshot.healthy -and -not $gatewaySnapshot.duplicateLogicalRoots) {
        "Healthy gateway PID $($gatewaySnapshot.pid) for $(@($gatewaySnapshot.enabledPlatforms) -join ', ')."
    } else {
        Get-HermesGatewayFailureDetail -Snapshot $gatewaySnapshot
    }
    Add-TestResult -Name 'Messaging gateway' -Passed (
        $gatewaySnapshot.healthy -and -not $gatewaySnapshot.duplicateLogicalRoots
    ) -Detail $gatewayDetail
} else {
    Add-TestResult -Name 'Messaging gateway' -Passed $true `
        -Detail 'No gateway-backed messaging platform is enabled.'
}

    $configuredPorts = @([int]$configuration.network.modelPort, [int]$configuration.network.hermesPort)
    $listeners = @(Get-NetTCPConnection -State Listen -LocalPort $configuredPorts -ErrorAction Stop)
    $nonLoopback = @($listeners | Where-Object {
        $_.LocalAddress -notin @('127.0.0.1', '::1')
    })
    Add-TestResult -Name 'Loopback binding' -Passed ($nonLoopback.Count -eq 0) -Detail "Configured ports $($configuredPorts -join ', ') listen only on loopback."

    $completionRequest = [ordered]@{
        model = $modelAlias
        messages = @([ordered]@{ role = 'user'; content = 'Reply with exactly HERMES_MODEL_OK' })
        temperature = 0
        max_tokens = 128
    }
    $completionResponse = Invoke-RestMethod -Method Post -Uri "$modelBase/v1/chat/completions" `
        -Headers $headers -ContentType 'application/json' -Body ($completionRequest | ConvertTo-Json -Depth 8 -Compress) `
        -TimeoutSec 180
    $completionMessage = $completionResponse.choices[0].message
    $completionText = @(
        [string]$completionMessage.content,
        $(if ($completionMessage.PSObject.Properties.Name -contains 'reasoning_content') {
            [string]$completionMessage.reasoning_content
        })
    ) -join ''
    Add-TestResult -Name 'Model completion' -Passed (
        @($completionResponse.choices).Count -ge 1 -and -not [string]::IsNullOrWhiteSpace($completionText)
    ) -Detail "The selected model '$($selectedModel.displayName)' returned generated content."

    $supportsNativeTools = $selectedModel.metadata -and
        $selectedModel.metadata.PSObject.Properties.Name -contains 'nativeToolCalling' -and
        [bool]$selectedModel.metadata.nativeToolCalling
    if ($supportsNativeTools) {
      $request = [ordered]@{
        model = $modelAlias
        messages = @(
            [ordered]@{ role = 'user'; content = 'Call get_timezone exactly once for London. Do not answer in prose.' }
        )
        tools = @(
            [ordered]@{
                type = 'function'
                function = [ordered]@{
                    name = 'get_timezone'
                    description = 'Return an IANA timezone for a city.'
                    parameters = [ordered]@{
                        type = 'object'
                        additionalProperties = $false
                        required = @('timezone')
                        properties = [ordered]@{
                            timezone = [ordered]@{
                                type = 'string'
                                enum = @('Europe/London')
                            }
                        }
                    }
                }
            }
        )
        tool_choice = [ordered]@{ type = 'function'; function = [ordered]@{ name = 'get_timezone' } }
        temperature = 0
        seed = 3407
        max_tokens = 96
    }
      $toolResponse = Invoke-RestMethod -Method Post -Uri "$modelBase/v1/chat/completions" `
        -Headers $headers -ContentType 'application/json' -Body ($request | ConvertTo-Json -Depth 16 -Compress) `
        -TimeoutSec 180
    $call = @($toolResponse.choices[0].message.tool_calls)[0]
    $arguments = $call.function.arguments | ConvertFrom-Json
      Add-TestResult -Name 'Native tool-call schema' -Passed (
        $call.function.name -eq 'get_timezone' -and $arguments.timezone -eq 'Europe/London'
      ) -Detail "$($selectedModel.displayName) returned one schema-valid native function call."
    }

    if (-not $SkipAgentTool) {
        $probe = Resolve-HermesPath "temp\agent-tool-$([guid]::NewGuid().ToString('N')).txt"
        $usage = Resolve-HermesPath "temp\agent-tool-usage-$([guid]::NewGuid().ToString('N')).json"
        $prompt = "Use the terminal tool to run one PowerShell command that writes the exact text HERMES_AGENT_TOOL_OK to '$probe'. Then read the file with the terminal tool and reply exactly HERMES_AGENT_TOOL_OK."
        $hermes = Resolve-HermesPath 'runtimes\python\hermes\Scripts\hermes.exe'
        try {
            $output = @(
                Invoke-HermesProcess -FilePath $hermes -ArgumentList @(
                    '--oneshot', $prompt,
                    '--usage-file', $usage,
                    '--provider', 'local-llama',
                    '--model', $modelAlias,
                    '--toolsets', 'terminal'
                ) -WorkingDirectory (Resolve-HermesPath 'data\user') -Environment @{
                    HERMES_HOME = (Resolve-HermesPath 'data\hermes')
                    HERMES_LOCAL_API_TOKEN = $token
                    LLAMA_API_KEY = $token
                } -LogComponent diagnostics -PassThruOutput
            ) -join [Environment]::NewLine
            $probeText = if (Test-Path -LiteralPath $probe) { (Get-Content -Raw -LiteralPath $probe).Trim() } else { '' }
            $usageRecord = if (Test-Path -LiteralPath $usage) {
                Get-Content -Raw -LiteralPath $usage | ConvertFrom-Json
            } else {
                $null
            }
            Add-TestResult -Name 'Hermes terminal tool' -Passed (
                $probeText -eq 'HERMES_AGENT_TOOL_OK' -and
                $output -match 'HERMES_AGENT_TOOL_OK' -and
                $usageRecord.completed
            ) -Detail "Hermes completed $($usageRecord.api_calls) local model call(s) and a real terminal tool."
        } finally {
            foreach ($temporary in @($probe, $usage)) {
                if (Test-Path -LiteralPath $temporary) {
                    Remove-Item -LiteralPath $temporary -Force
                }
            }
        }
    }

    $report = [ordered]@{
        schemaVersion = 1
        generatedAt = (Get-Date).ToUniversalTime().ToString('o')
        passed = $true
        quick = [bool]$Quick
        results = $results
    }
    $reportPath = Resolve-HermesPath 'logs\diagnostics\latest-test.json'
    Write-HermesAtomicText -Path $reportPath -Content (($report | ConvertTo-Json -Depth 12) + [Environment]::NewLine)
    Write-HermesLog -Component diagnostics -Message "Hermes Local test suite passed $($results.Count) checks."
    Write-Host "Hermes Local tests passed: $($results.Count) checks. Report: $reportPath"
    exit 0
} catch {
    $report = [ordered]@{
        schemaVersion = 1
        generatedAt = (Get-Date).ToUniversalTime().ToString('o')
        passed = $false
        error = Protect-HermesLogText $_.Exception.Message
        results = $results
    }
    try {
        Write-HermesAtomicText -Path (Resolve-HermesPath 'logs\diagnostics\latest-test.json') -Content (
            ($report | ConvertTo-Json -Depth 12) + [Environment]::NewLine
        )
    } catch {
    }
    Write-HermesLog -Component diagnostics -Level ERROR -Message $_.Exception.ToString()
    Write-Host "Hermes Local tests failed: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
}
