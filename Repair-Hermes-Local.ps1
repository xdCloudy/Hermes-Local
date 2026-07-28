[CmdletBinding()]
param(
    [switch] $SkipModelVerification,
    [switch] $NonInteractive
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'scripts\Common-Hermes.psm1') -Force
Import-Module (Join-Path $PSScriptRoot 'scripts\Hermes-Configuration.psm1') -Force

$wasRunning = $false
$profile = [string](Get-HermesConfiguration).selectedProfile

try {
    Assert-HermesRoot
    Initialize-HermesLayout
    Set-HermesProcessEnvironment
    Write-HermesLog -Component setup -Message 'Starting data-preserving repair.'

    $statusPath = Resolve-HermesPath 'data\runtime\status.json'
    if (Test-Path -LiteralPath $statusPath) {
        $status = Get-Content -Raw -LiteralPath $statusPath | ConvertFrom-Json
        $wasRunning = $status.phase -eq 'running'
        if ($status.profile) {
            $profile = [string]$status.profile
        }
    }
    if ($wasRunning) {
        $null = Invoke-HermesProcess -FilePath 'pwsh.exe' -ArgumentList @(
            '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
            '-File', (Resolve-HermesPath 'Stop-Hermes-Local.ps1'), '-NonInteractive'
        ) -LogComponent setup
    }

    $stamp = (Get-Date).ToUniversalTime().ToString('yyyyMMddTHHmmssZ')
    $null = Invoke-HermesProcess -FilePath 'pwsh.exe' -ArgumentList @(
        '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
        '-File', (Resolve-HermesPath 'Backup-Hermes-Local.ps1'),
        '-Name', "pre-repair-$stamp", '-NonInteractive'
    ) -LogComponent setup

    $setup = Resolve-HermesPath 'Setup-Hermes-Local.ps1'
    $arguments = @(
        '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
        '-File', $setup
    )
    if ($SkipModelVerification) {
        $arguments += '-SkipModel'
    }
    $arguments += '-ReinstallDependencies'
    if ($NonInteractive) {
        $arguments += '-NonInteractive'
    }
    $null = Invoke-HermesProcess -FilePath 'pwsh.exe' -ArgumentList $arguments -LogComponent setup

    Write-HermesLog -Component setup -Message 'Data-preserving repair completed.'
    Write-Host 'Hermes Local repair completed successfully.'
    exit 0
} catch {
    Write-HermesLog -Component setup -Level ERROR -Message $_.Exception.ToString()
    Write-Host 'Hermes Local repair failed. Existing user data was preserved.' -ForegroundColor Red
    exit 1
} finally {
    if ($wasRunning) {
        try {
            $null = Invoke-HermesProcess -FilePath 'pwsh.exe' -ArgumentList @(
                '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
                '-File', (Resolve-HermesPath 'Start-Hermes-Local.ps1'),
                '-Profile', $profile, '-NonInteractive'
            ) -LogComponent setup
        } catch {
            Write-HermesLog -Component setup -Level ERROR -Message (
                "Repair finished but stack restart failed: $($_.Exception.Message)"
            )
        }
    }
}
