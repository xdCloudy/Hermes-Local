[CmdletBinding()]
param(
    [switch] $NonInteractive
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'scripts\Common-Hermes.psm1') -Force
Import-Module (Join-Path $PSScriptRoot 'scripts\Hermes-Configuration.psm1') -Force

$staging = $null

function Read-SafeLogTail {
    param(
        [Parameter(Mandatory)]
        [string] $RelativePath,
        [int] $Lines = 200
    )

    $path = Resolve-HermesPath $RelativePath
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        return @()
    }

    return @(
        Get-Content -LiteralPath $path -Tail $Lines |
            ForEach-Object { Protect-HermesLogText ([string]$_) }
    )
}

try {
    Assert-HermesRoot
    Initialize-HermesLayout
    Set-HermesProcessEnvironment

    $stamp = (Get-Date).ToUniversalTime().ToString('yyyyMMddTHHmmssZ')
    $diagnosticRoot = Resolve-HermesPath 'logs\diagnostics'
    $staging = Resolve-HermesPath "temp\diagnostics-$([guid]::NewGuid().ToString('N'))"
    [System.IO.Directory]::CreateDirectory($diagnosticRoot) | Out-Null
    [System.IO.Directory]::CreateDirectory($staging) | Out-Null

    $statusPath = Resolve-HermesPath 'data\runtime\status.json'
    $configuration = Get-HermesConfiguration
    $status = if (Test-Path -LiteralPath $statusPath) {
        Get-Content -Raw -LiteralPath $statusPath | ConvertFrom-Json
    } else {
        $null
    }
    $listeners = @(
        Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue |
            Where-Object LocalPort -In @(
                [int]$configuration.network.modelPort,
                [int]$configuration.network.hermesPort
            ) |
            Select-Object LocalAddress, LocalPort, OwningProcess
    )
    $firewall = @(
        Get-NetFirewallRule -ErrorAction SilentlyContinue |
            Where-Object DisplayName -Like 'Hermes Local*' |
            Select-Object DisplayName, Enabled, Direction, Action
    )
    $sourceRoot = Resolve-HermesPath 'source\hermes-agent'
    $llamaRoot = Resolve-HermesPath 'runtimes\llama.cpp\source'
    $allowListedEnvironment = [ordered]@{}
    foreach ($name in @(
        'HERMES_HOME', 'HERMES_LOCAL_ROOT', 'HF_HOME', 'UV_CACHE_DIR',
        'PLAYWRIGHT_BROWSERS_PATH', 'CUDA_PATH'
    )) {
        $allowListedEnvironment[$name] = [ordered]@{
            configured = [bool][Environment]::GetEnvironmentVariable($name)
            valueOmitted = $true
        }
    }

    $report = [ordered]@{
        schemaVersion = 1
        generatedAt = (Get-Date).ToUniversalTime().ToString('o')
        privacy = [ordered]@{
            tokens = 'omitted'
            passwords = 'omitted'
            cookies = 'omitted'
            environmentValues = 'omitted'
            conversations = 'omitted'
            privateFiles = 'omitted'
        }
        version = Get-HermesVersionManifest
        hardware = Get-HermesHardwareSnapshot
        tools = Get-HermesToolSnapshot
        runtime = $status
        profiles = [ordered]@{
            schemaVersion = $configuration.schemaVersion
            selected = $configuration.selectedProfile
            names = @($configuration.profiles | ForEach-Object name)
        }
        selection = [ordered]@{
            modelId = $configuration.selectedModelId
            modelName = $configuration.selectedModel.displayName
            acceleration = $configuration.runtime.acceleration
            configuredHost = $configuration.network.host
            modelPort = $configuration.network.modelPort
            hermesPort = $configuration.network.hermesPort
        }
        network = [ordered]@{
            listeners = $listeners
            firewallRules = $firewall
        }
        repositories = [ordered]@{
            hermes = [ordered]@{
                commit = (& git -C $sourceRoot rev-parse HEAD 2>$null).Trim()
                branch = (& git -C $sourceRoot branch --show-current 2>$null).Trim()
                dirtyEntries = @(& git -C $sourceRoot status --short 2>$null).Count
            }
            llamaCpp = [ordered]@{
                commit = (& git -C $llamaRoot rev-parse HEAD 2>$null).Trim()
                branch = (& git -C $llamaRoot branch --show-current 2>$null).Trim()
                dirtyEntries = @(& git -C $llamaRoot status --short 2>$null).Count
            }
        }
        environment = $allowListedEnvironment
        artefacts = [ordered]@{
            packageManifest = Test-Path -LiteralPath (Resolve-HermesPath 'dist\package-manifest.json')
            benchmarkReport = Test-Path -LiteralPath (Resolve-HermesPath 'benchmarks\reports\LATEST.md')
            securityReport = Test-Path -LiteralPath (Resolve-HermesPath 'security\reports\LATEST.md')
            sbom = Test-Path -LiteralPath (Resolve-HermesPath 'security\sbom')
        }
        safeLogs = [ordered]@{
            supervisor = Read-SafeLogTail 'logs\supervisor\supervisor.log'
            setup = Read-SafeLogTail 'logs\setup\setup.log'
            launcher = Read-SafeLogTail 'logs\launcher\launcher.log'
            security = Read-SafeLogTail 'logs\security\security.log'
        }
    }

    $jsonPath = Join-Path $staging 'diagnostics.json'
    [System.IO.File]::WriteAllText(
        $jsonPath,
        (($report | ConvertTo-Json -Depth 32) + [Environment]::NewLine),
        [System.Text.UTF8Encoding]::new($false)
    )
    foreach ($relative in @('VERSION.json', 'logs\diagnostics\latest-test.json', 'dist\package-manifest.json')) {
        $source = Resolve-HermesPath $relative
        if (Test-Path -LiteralPath $source -PathType Leaf) {
            Copy-Item -LiteralPath $source -Destination (Join-Path $staging ([System.IO.Path]::GetFileName($source)))
        }
    }

    $token = Get-OrCreateHermesApiToken
    foreach ($file in Get-ChildItem -LiteralPath $staging -File) {
        $text = Get-Content -Raw -LiteralPath $file.FullName -ErrorAction SilentlyContinue
        if ($text -and $text.Contains($token, [System.StringComparison]::Ordinal)) {
            throw "Diagnostic privacy validation found the live token in $($file.Name)."
        }
    }

    $archive = Join-Path $diagnosticRoot "Hermes-Local-Diagnostics-$stamp.zip"
    [System.IO.Compression.ZipFile]::CreateFromDirectory(
        $staging,
        $archive,
        [System.IO.Compression.CompressionLevel]::Optimal,
        $false
    )
    $hash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
    Write-HermesAtomicText -Path "$archive.sha256" -Content (
        "$hash  $([System.IO.Path]::GetFileName($archive))" + [Environment]::NewLine
    )
    Write-HermesLog -Component diagnostics -Message "Created redacted diagnostic export $archive."
    Write-Host "Redacted diagnostics: $archive"
    Write-Host "SHA-256: $hash"
    exit 0
} catch {
    Write-HermesLog -Component diagnostics -Level ERROR -Message $_.Exception.ToString()
    Write-Host "Diagnostic export failed: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
} finally {
    if ($staging -and (Test-Path -LiteralPath $staging)) {
        $resolved = [System.IO.Path]::GetFullPath($staging)
        $diagnosticPrefix = Resolve-HermesPath 'temp\diagnostics-'
        if ($resolved.StartsWith($diagnosticPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
            Remove-Item -LiteralPath $resolved -Recurse -Force
        }
    }
}
