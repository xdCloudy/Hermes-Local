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
    $version = [string](Get-HermesVersionManifest).product.version

    $null = Invoke-HermesProcess -FilePath $npm -ArgumentList @('run', 'build', '--workspace', 'apps/desktop') -WorkingDirectory $source -LogComponent launcher
    $null = Invoke-HermesProcess -FilePath $npm -ArgumentList @(
        'run', 'builder', '--workspace', 'apps/desktop', '--', '--win', 'nsis', 'portable', '--x64'
    ) -WorkingDirectory $source -LogComponent launcher

    $expectedNames = @(
        "Hermes-Launcher-$version-windows-x64-setup.exe",
        "Hermes-Launcher-$version-windows-x64-setup.exe.blockmap",
        "Hermes-Launcher-$version-windows-x64-portable.exe"
    )
    $artifacts = @(
        foreach ($name in $expectedNames) {
            $artifact = Join-Path $release $name
            if (-not (Test-Path -LiteralPath $artifact -PathType Leaf)) {
                throw "Expected release artifact was not produced: $artifact"
            }
            Get-Item -LiteralPath $artifact
        }
    )
    foreach ($artifact in $artifacts) {
        Copy-Item -LiteralPath $artifact.FullName -Destination (Resolve-HermesPath 'dist') -Force
    }
    $portable = $artifacts | Where-Object Name -EQ "Hermes-Launcher-$version-windows-x64-portable.exe"
    Copy-Item -LiteralPath $portable.FullName -Destination (Resolve-HermesPath 'dist\Hermes Launcher.exe') -Force
    $manifest = [ordered]@{
        schemaVersion = 1
        createdAt = (Get-Date).ToUniversalTime().ToString('o')
        artifacts = @(
            $artifacts | ForEach-Object {
                [ordered]@{
                    name = $_.Name
                    sizeBytes = $_.Length
                    sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
                }
            }
        )
    }
    Write-HermesAtomicText -Path (Resolve-HermesPath 'dist\package-manifest.json') -Content (
        ($manifest | ConvertTo-Json -Depth 8) + [Environment]::NewLine
    )
    Write-HermesLog -Component launcher -Message "Packaged $($artifacts.Count) launcher artifact(s)."
    Write-Host "Hermes Launcher installer and portable build are in: $(Resolve-HermesPath 'dist')"
    exit 0
} catch {
    Write-HermesLog -Component launcher -Level ERROR -Message $_.Exception.ToString()
    Write-Host "Hermes Launcher packaging failed: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
}
