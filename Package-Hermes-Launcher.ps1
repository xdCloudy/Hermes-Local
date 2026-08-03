[CmdletBinding()]
param(
    [switch] $NonInteractive
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'scripts\Common-Hermes.psm1') -Force

function Get-HermesReleasePython {
    $managed = Resolve-HermesPath 'runtimes\python\hermes\Scripts\python.exe'
    if (Test-Path -LiteralPath $managed -PathType Leaf) {
        return $managed
    }
    $command = Get-Command python.exe -ErrorAction SilentlyContinue
    if (-not $command) {
        $command = Get-Command python -ErrorAction Stop
    }
    $command.Source
}

function Get-HermesCommandVersion {
    param(
        [Parameter(Mandatory)][string] $FilePath,
        [Parameter(Mandatory)][string[]] $ArgumentList
    )

    try {
        ((& $FilePath @ArgumentList 2>&1 | ForEach-Object { [string]$_ }) -join ' ').Trim()
    } catch {
        'unavailable'
    }
}

try {
    Assert-HermesRoot
    Initialize-HermesLayout
    Set-HermesProcessEnvironment
    $source = Resolve-HermesPath 'source\hermes-agent'
    $desktop = Join-Path $source 'apps\desktop'
    $release = Join-Path $desktop 'release'
    $npm = (Get-Command npm.cmd -ErrorAction Stop).Source
    $versionManifest = Get-HermesVersionManifest
    $version = [string]$versionManifest.product.version

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
    $dist = Resolve-HermesPath 'dist'
    foreach ($artifact in $artifacts) {
        Copy-Item -LiteralPath $artifact.FullName -Destination $dist -Force
    }
    $portable = $artifacts | Where-Object Name -EQ "Hermes-Launcher-$version-windows-x64-portable.exe"
    $launcherAliasPath = Resolve-HermesPath 'dist\Hermes Launcher.exe'
    Copy-Item -LiteralPath $portable.FullName -Destination $launcherAliasPath -Force
    $distributedBinaries = @($artifacts) + @(Get-Item -LiteralPath $launcherAliasPath)

    # Retain the compact package manifest for older tooling. The release manifest
    # below is the authoritative, provenance-aware contract.
    $packageManifest = [ordered]@{
        schemaVersion = 1
        createdAt = (Get-Date).ToUniversalTime().ToString('o')
        artifacts = @(
            $distributedBinaries | ForEach-Object {
                [ordered]@{
                    name = $_.Name
                    sizeBytes = $_.Length
                    sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
                }
            }
        )
    }
    $packageManifestPath = Resolve-HermesPath 'dist\package-manifest.json'
    Write-HermesAtomicText -Path $packageManifestPath -Content (
        ($packageManifest | ConvertTo-Json -Depth 8) + [Environment]::NewLine
    )

    $releaseTool = Resolve-HermesPath 'scripts\ci\release_integrity.py'
    if (-not (Test-Path -LiteralPath $releaseTool -PathType Leaf)) {
        throw "Release integrity tool is missing: $releaseTool"
    }
    $sourceCommit = ((& git -C (Get-HermesRoot) rev-parse HEAD 2>&1) -join '').Trim().ToLowerInvariant()
    if ($LASTEXITCODE -ne 0 -or $sourceCommit -notmatch '^[0-9a-f]{40}$') {
        throw 'Could not resolve the exact Hermes Local source commit for the release manifest.'
    }

    $repository = [Environment]::GetEnvironmentVariable('GITHUB_REPOSITORY')
    if ([string]::IsNullOrWhiteSpace($repository)) {
        $repository = 'xdCloudy/Hermes-Local'
    }
    $workflow = "$repository/.github/workflows/release-integrity.yml"
    $runId = [Environment]::GetEnvironmentVariable('GITHUB_RUN_ID')
    if ([string]::IsNullOrWhiteSpace($runId)) {
        $runId = 'local'
    }
    $channel = [Environment]::GetEnvironmentVariable('HERMES_RELEASE_CHANNEL')
    if ([string]::IsNullOrWhiteSpace($channel)) {
        $channel = 'development'
    }

    $python = Get-HermesReleasePython
    $releaseArguments = @(
        $releaseTool, 'create',
        '--root', $dist,
        '--output', (Join-Path $dist 'release-manifest.json'),
        '--version-manifest', (Resolve-HermesPath 'VERSION.json'),
        '--channel', $channel,
        '--repository', $repository,
        '--source-commit', $sourceCommit,
        '--workflow', $workflow,
        '--run-id', $runId,
        '--artifact', $expectedNames[0],
        '--artifact', $expectedNames[1],
        '--artifact', $expectedNames[2],
        '--artifact', 'Hermes Launcher.exe',
        '--artifact', 'package-manifest.json',
        '--build-command', 'npm run build --workspace apps/desktop',
        '--build-command', 'npm run builder --workspace apps/desktop -- --win nsis portable --x64',
        '--toolchain', "python=$(Get-HermesCommandVersion -FilePath $python -ArgumentList @('--version'))",
        '--toolchain', "node=$(Get-HermesCommandVersion -FilePath 'node' -ArgumentList @('--version'))",
        '--toolchain', "npm=$(Get-HermesCommandVersion -FilePath $npm -ArgumentList @('--version'))"
    )
    foreach ($lock in @(
        @{ Name = 'node'; Path = Join-Path $source 'package-lock.json' },
        @{ Name = 'python'; Path = Join-Path $source 'uv.lock' }
    )) {
        if (Test-Path -LiteralPath $lock.Path -PathType Leaf) {
            $releaseArguments += @('--dependency-lock', "$($lock.Name)=$($lock.Path)")
        }
    }
    $sbomRoot = Join-Path $dist 'sbom'
    if (Test-Path -LiteralPath $sbomRoot -PathType Container) {
        foreach ($sbom in Get-ChildItem -LiteralPath $sbomRoot -Filter '*.cdx.json' -File | Sort-Object Name) {
            $scope = $sbom.Name -replace '\.cdx\.json$', ''
            $releaseArguments += @('--sbom', "$scope=$($sbom.FullName)")
        }
    }
    if ([Environment]::GetEnvironmentVariable('HERMES_REQUIRE_AUTHENTICODE') -eq '1') {
        $releaseArguments += @('--authenticode-required', '*.exe')
    }

    $null = Invoke-HermesProcess `
        -FilePath $python `
        -ArgumentList $releaseArguments `
        -WorkingDirectory (Get-HermesRoot) `
        -LogComponent launcher

    Write-HermesLog -Component launcher -Message "Packaged $($artifacts.Count) launcher artifact(s) with release integrity metadata."
    Write-Host "Hermes Launcher installer and portable build are in: $dist"
    exit 0
} catch {
    Write-HermesLog -Component launcher -Level ERROR -Message $_.Exception.ToString()
    Write-Host "Hermes Launcher packaging failed: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
}
