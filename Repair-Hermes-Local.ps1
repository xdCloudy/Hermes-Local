[CmdletBinding()]
param(
    [switch] $SkipModelVerification,
    [switch] $NonInteractive
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'scripts\Common-Hermes.psm1') -Force

try {
    Assert-HermesRoot
    Initialize-HermesLayout
    Set-HermesProcessEnvironment
    Write-HermesLog -Component setup -Message 'Starting data-preserving repair.'

    $setup = Resolve-HermesPath 'Setup-Hermes-Local.ps1'
    $arguments = @{
        SkipModel = $SkipModelVerification
        SkipLauncherBuild = $false
        NonInteractive = $NonInteractive
    }
    & $setup @arguments
    if ($LASTEXITCODE -ne 0) {
        throw "Repair setup pass failed with exit code $LASTEXITCODE."
    }

    Write-HermesLog -Component setup -Message 'Data-preserving repair completed.'
    Write-Host 'Hermes Local repair completed successfully.'
    exit 0
} catch {
    Write-HermesLog -Component setup -Level ERROR -Message $_.Exception.ToString()
    Write-Host 'Hermes Local repair failed. Existing user data was preserved.' -ForegroundColor Red
    exit 1
}

