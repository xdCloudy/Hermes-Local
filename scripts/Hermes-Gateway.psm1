Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

Import-Module (Join-Path $PSScriptRoot 'Common-Hermes.psm1') -Force

$script:GatewaySnapshotCache = $null
$script:GatewaySnapshotCachedAt = [datetime]::MinValue
$script:GatewayEnabledPlatforms = $null

function Get-HermesGatewayEnvironment {
    [CmdletBinding()]
    param([string] $Token)

    $environment = [ordered]@{
        HERMES_HOME = (Resolve-HermesPath 'data\hermes')
        UV_CACHE_DIR = (Resolve-HermesPath 'cache\uv')
        HF_HOME = (Resolve-HermesPath 'cache\huggingface')
        TRANSFORMERS_CACHE = (Resolve-HermesPath 'cache\huggingface\transformers')
        PLAYWRIGHT_BROWSERS_PATH = (Resolve-HermesPath 'cache\playwright')
    }
    if (-not [string]::IsNullOrWhiteSpace($Token)) {
        $environment.HERMES_DASHBOARD_SESSION_TOKEN = $Token
        $environment.HERMES_LOCAL_API_TOKEN = $Token
        $environment.LLAMA_API_KEY = $Token
    }
    return $environment
}

function Get-HermesGatewaySnapshot {
    [CmdletBinding()]
    param(
        [switch] $Discover,
        [ValidateRange(0, 60)]
        [int] $CacheSeconds = 8
    )

    $now = (Get-Date).ToUniversalTime()
    if (-not $Discover -and $CacheSeconds -gt 0 -and $script:GatewaySnapshotCache) {
        $ageSeconds = ($now - $script:GatewaySnapshotCachedAt).TotalSeconds
        $cacheIsStable = -not [bool]$script:GatewaySnapshotCache.required -or
            [bool]$script:GatewaySnapshotCache.healthy
        if ($cacheIsStable -and $ageSeconds -lt $CacheSeconds) {
            return $script:GatewaySnapshotCache
        }
    }

    $python = Resolve-HermesPath 'runtimes\python\hermes\Scripts\python.exe'
    if (-not (Test-Path -LiteralPath $python -PathType Leaf)) {
        throw "Hermes Python runtime not found: $python"
    }
    $arguments = [System.Collections.Generic.List[string]]::new()
    $arguments.Add((Resolve-HermesPath 'scripts\gateway_snapshot.py'))
    if ($Discover) {
        $arguments.Add('--discover')
    } elseif ($null -ne $script:GatewayEnabledPlatforms) {
        $arguments.Add('--enabled-platforms-json')
        $arguments.Add((ConvertTo-Json -InputObject @($script:GatewayEnabledPlatforms) -Compress))
    }
    $output = @(
        Invoke-HermesProcess `
            -FilePath $python `
            -ArgumentList $arguments.ToArray() `
            -WorkingDirectory (Resolve-HermesPath 'source\hermes-agent') `
            -Environment (Get-HermesGatewayEnvironment) `
            -LogComponent supervisor `
            -PassThruOutput
    )
    $jsonLine = @($output | Where-Object { $_ -and $_.TrimStart().StartsWith('{') }) | Select-Object -Last 1
    if (-not $jsonLine) {
        throw 'Hermes gateway inspection returned no JSON snapshot.'
    }
    try {
        $snapshot = $jsonLine | ConvertFrom-Json -Depth 16
        $script:GatewaySnapshotCache = $snapshot
        $script:GatewaySnapshotCachedAt = $now
        $script:GatewayEnabledPlatforms = @($snapshot.enabledPlatforms)
        return $snapshot
    } catch {
        throw "Hermes gateway inspection returned invalid JSON: $($_.Exception.Message)"
    }
}

function Get-HermesGatewayFailureDetail {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [pscustomobject] $Snapshot
    )

    if ($Snapshot.duplicateLogicalRoots) {
        return "Multiple independent gateway roots were detected: $(@($Snapshot.logicalPids) -join ', ')."
    }
    $failedPlatforms = @($Snapshot.platforms | Where-Object { $_.failed })
    if ($failedPlatforms.Count -gt 0) {
        return ($failedPlatforms | ForEach-Object {
            $suffix = if ($_.errorCode) { " ($($_.errorCode))" } else { '' }
            "$($_.name): $($_.state)$suffix"
        }) -join '; '
    }
    if (-not $Snapshot.running) {
        return 'No authoritative gateway process is running.'
    }
    if ($Snapshot.runtimeStale) {
        return 'Gateway runtime status is stale.'
    }
    if (-not $Snapshot.runtimeLive) {
        return 'Gateway runtime PID is not live or no longer matches its recorded process identity.'
    }
    $pendingPlatforms = @($Snapshot.platforms | Where-Object { -not $_.healthy })
    if ($pendingPlatforms.Count -gt 0) {
        return ($pendingPlatforms | ForEach-Object { "$($_.name): $($_.state)" }) -join '; '
    }
    return "Gateway state is '$($Snapshot.state)'."
}

Export-ModuleMember -Function @(
    'Get-HermesGatewayEnvironment',
    'Get-HermesGatewaySnapshot',
    'Get-HermesGatewayFailureDetail'
)
