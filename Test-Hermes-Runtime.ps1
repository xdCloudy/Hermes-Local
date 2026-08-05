[CmdletBinding()]
param([switch] $SmokeTest)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'scripts\Common-Hermes.psm1') -Force
Import-Module (Join-Path $PSScriptRoot 'scripts\Hermes-Configuration.psm1') -Force
Import-Module (Join-Path $PSScriptRoot 'scripts\Hermes-RuntimeManager.psm1') -Force

try {
    Assert-HermesRoot
    $result = Test-HermesManagedLlamaRuntime -SmokeTest:$SmokeTest
    if (-not $result.Managed) {
        Write-Host $result.Reason -ForegroundColor Yellow
        exit 2
    }
    Write-Host "Runtime package $($result.PackageId) passed file-level SHA-256 verification."
    exit 0
} catch {
    Write-HermesLog -Component diagnostics -Level ERROR -Message $_.Exception.ToString()
    Write-Host "Runtime verification failed: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
}
