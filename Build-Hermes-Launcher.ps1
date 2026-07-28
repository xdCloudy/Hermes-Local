[CmdletBinding()]
param(
    [switch] $NonInteractive
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'scripts\Common-Hermes.psm1') -Force

try {
    Assert-HermesRoot
    Initialize-HermesLayout
    Set-HermesProcessEnvironment
    $source = Resolve-HermesPath 'source\hermes-agent'
    $desktop = Join-Path $source 'apps\desktop'
    $release = Join-Path $desktop 'release'
    $npm = (Get-Command npm.cmd -ErrorAction Stop).Source

    $null = Invoke-HermesProcess -FilePath $npm -ArgumentList @('run', 'build', '--workspace', 'web') -WorkingDirectory $source -LogComponent launcher
    $null = Invoke-HermesProcess -FilePath $npm -ArgumentList @('run', 'typecheck', '--workspace', 'apps/desktop') -WorkingDirectory $source -LogComponent launcher
    $null = Invoke-HermesProcess -FilePath $npm -ArgumentList @('run', 'build', '--workspace', 'apps/desktop') -WorkingDirectory $source -LogComponent launcher
    $null = Invoke-HermesProcess -FilePath $npm -ArgumentList @('run', 'builder', '--workspace', 'apps/desktop', '--', '--dir', '--win') -WorkingDirectory $source -LogComponent launcher

    $unpacked = Join-Path $release 'win-unpacked'
    $executable = Join-Path $unpacked 'Hermes Launcher.exe'
    if (-not (Test-Path -LiteralPath $executable)) {
        throw "Packaged launcher executable was not produced: $executable"
    }
    $destination = Resolve-HermesPath 'dist'
    $expectedDestination = [System.IO.Path]::GetFullPath((Join-Path (Get-HermesRoot) 'dist'))
    if ([System.IO.Path]::GetFullPath($destination) -ne $expectedDestination) {
        throw "Refusing to replace unexpected launcher destination: $destination"
    }
    Get-ChildItem -LiteralPath $destination -Force |
        Remove-Item -Recurse -Force
    Get-ChildItem -LiteralPath $unpacked -Force |
        Copy-Item -Destination $destination -Recurse -Force
    $target = Resolve-HermesPath 'dist\Hermes Launcher.exe'
    if (-not (Test-Path -LiteralPath $target)) {
        throw "Launcher copy failed: $target"
    }
    Write-HermesLog -Component launcher -Message "Built production launcher at $target."
    Write-Host "Hermes Launcher built: $target"
    exit 0
} catch {
    Write-HermesLog -Component launcher -Level ERROR -Message $_.Exception.ToString()
    Write-Host "Hermes Launcher build failed: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
}
