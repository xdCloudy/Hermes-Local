[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$root = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
Import-Module (Join-Path $root 'scripts\Hermes-Gateway.psm1') -Force

function Assert-Equal {
    param(
        [Parameter(Mandatory)] $Actual,
        [Parameter(Mandatory)] $Expected,
        [Parameter(Mandatory)] [string] $Message
    )
    if ($Actual -ne $Expected) {
        throw "$Message Expected '$Expected'; got '$Actual'."
    }
}

$duplicate = [pscustomobject]@{
    duplicateLogicalRoots = $true
    logicalPids = @(4100, 4200)
    platforms = @()
    running = $true
    runtimeStale = $false
    runtimeLive = $true
    state = 'running'
}
Assert-Equal `
    -Actual (Get-HermesGatewayFailureDetail -Snapshot $duplicate) `
    -Expected 'Multiple independent gateway roots were detected: 4100, 4200.' `
    -Message 'Duplicate roots must be actionable.'

$failed = [pscustomobject]@{
    duplicateLogicalRoots = $false
    logicalPids = @(4100)
    platforms = @([pscustomobject]@{
        name = 'discord'
        state = 'fatal'
        errorCode = 'auth_failed'
        failed = $true
        healthy = $false
    })
    running = $true
    runtimeStale = $false
    runtimeLive = $true
    state = 'running'
}
Assert-Equal `
    -Actual (Get-HermesGatewayFailureDetail -Snapshot $failed) `
    -Expected 'discord: fatal (auth_failed)' `
    -Message 'Platform failures must preserve a safe error code.'

$pending = [pscustomobject]@{
    duplicateLogicalRoots = $false
    logicalPids = @(4100)
    platforms = @([pscustomobject]@{
        name = 'discord'
        state = 'starting'
        errorCode = $null
        failed = $false
        healthy = $false
    })
    running = $true
    runtimeStale = $false
    runtimeLive = $true
    state = 'starting'
}
Assert-Equal `
    -Actual (Get-HermesGatewayFailureDetail -Snapshot $pending) `
    -Expected 'discord: starting' `
    -Message 'Pending platform state must be visible.'

$diagnosticsText = Get-Content -Raw -LiteralPath (Join-Path $root 'Test-Hermes-Local.ps1')
$gatewayImport = "Import-Module (Join-Path `$PSScriptRoot 'scripts\Hermes-Gateway.psm1') -Force"
Assert-Equal `
    -Actual ([regex]::Matches($diagnosticsText, [regex]::Escape($gatewayImport)).Count) `
    -Expected 1 `
    -Message 'Diagnostics must import the gateway module exactly once.'

$supervisorText = Get-Content -Raw -LiteralPath (Join-Path $root 'scripts\supervisor\Hermes-Supervisor.ps1')
Assert-Equal `
    -Actual ([regex]::Matches($supervisorText, 'function Wait-HermesGatewayHealthy \{').Count) `
    -Expected 1 `
    -Message 'Supervisor must define the gateway wait helper exactly once.'
if ($supervisorText.Contains("-GatewayState 'stopped' -GatewayState 'stopped'")) {
    throw 'Supervisor contains a duplicate stopped-state argument.'
}

Write-Host 'Hermes gateway module tests passed.'
