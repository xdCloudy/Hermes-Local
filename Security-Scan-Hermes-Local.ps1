[CmdletBinding()]
param(
    [switch] $Quick,
    [switch] $SkipDefender,
    [switch] $NonInteractive
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'scripts\Common-Hermes.psm1') -Force

try {
    Assert-HermesRoot
    Initialize-HermesLayout
    Set-HermesProcessEnvironment

    $gitleaks = Resolve-HermesPath 'runtimes\tools\security\gitleaks-8.30.1\gitleaks.exe'
    $osv = Resolve-HermesPath 'runtimes\tools\security\osv-scanner-2.4.0\osv-scanner.exe'

    if (-not (Test-Path -LiteralPath $gitleaks -PathType Leaf) -or
        -not (Test-Path -LiteralPath $osv -PathType Leaf)) {
        $installer = Resolve-HermesPath 'Install-Hermes-SecurityTools.ps1'
        if (-not (Test-Path -LiteralPath $installer -PathType Leaf)) {
            throw "Security tool installer is missing: $installer"
        }

        Write-HermesLog -Component security -Message 'Security scanners are missing; provisioning verified pinned binaries.'
        & $installer
    }

    $implementation = Join-Path $PSScriptRoot 'Security-Scan-Hermes-Local.Impl.ps1'
    if (-not (Test-Path -LiteralPath $implementation -PathType Leaf)) {
        throw "Security scan implementation is missing: $implementation"
    }

    $arguments = @{}
    if ($Quick) { $arguments.Quick = $true }
    if ($SkipDefender) { $arguments.SkipDefender = $true }
    if ($NonInteractive) { $arguments.NonInteractive = $true }

    & $implementation @arguments
} catch {
    $failure = $_.Exception
    try {
        Write-HermesLog -Component security -Level WARN -Message $failure.ToString()
    } catch {
    }
    Write-Host "Hermes Local security scan failed: $($failure.Message)" -ForegroundColor Red
    exit 1
}
