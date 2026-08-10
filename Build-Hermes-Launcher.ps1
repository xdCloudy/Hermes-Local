[CmdletBinding()]
param(
    [switch] $NonInteractive,
    [string] $DestinationDirectory
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'scripts\Common-Hermes.psm1') -Force

function Resolve-HermesLauncherDestination {
    param(
        [string] $RequestedDestination,
        [Parameter(Mandatory)][string] $Root
    )

    $rootFull = [IO.Path]::GetFullPath($Root).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    $destination = if ([string]::IsNullOrWhiteSpace($RequestedDestination)) {
        Join-Path $Root 'dist'
    } elseif ([IO.Path]::IsPathFullyQualified($RequestedDestination)) {
        $RequestedDestination
    } else {
        Join-Path $Root $RequestedDestination
    }
    $destinationFull = [IO.Path]::GetFullPath($destination)
    $rootWithoutSeparator = $rootFull.TrimEnd('\', '/')

    if ($destinationFull.Equals($rootWithoutSeparator, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Launcher build destination cannot be the Hermes Local root.'
    }
    if (-not $destinationFull.StartsWith($rootFull, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Launcher build destination is outside the Hermes Local root: $destinationFull"
    }

    foreach ($protected in @(
        (Join-Path $Root 'apps'),
        (Join-Path $Root 'packages'),
        (Join-Path $Root 'source'),
        (Join-Path $Root 'models'),
        (Join-Path $Root 'runtimes'),
        (Join-Path $Root 'config'),
        (Join-Path $Root 'data')
    )) {
        $protectedFull = [IO.Path]::GetFullPath($protected).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
        $candidate = $destinationFull.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
        if ($candidate.StartsWith($protectedFull, [StringComparison]::OrdinalIgnoreCase)) {
            throw "Launcher build destination overlaps protected Hermes Local state: $destinationFull"
        }
    }

    $destinationFull
}

function Assert-HermesNativeClient {
    param(
        [Parameter(Mandatory)][string] $Root,
        [Parameter(Mandatory)][string] $Desktop
    )

    $expectedDesktop = [IO.Path]::GetFullPath((Join-Path $Root 'apps\desktop'))
    $actualDesktop = [IO.Path]::GetFullPath($Desktop)
    if (-not $actualDesktop.Equals($expectedDesktop, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Launcher build must use the Hermes Local-owned Desktop at $expectedDesktop; got $actualDesktop."
    }

    foreach ($relativePath in @(
        'package.json',
        'electron\main.ts',
        'electron\hermes-local-control.ts',
        'src\app\chat\sidebar\project-centre-dialog.tsx',
        'src\app\settings\index.tsx'
    )) {
        $path = Join-Path $Desktop $relativePath
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            throw "Hermes Local Desktop source is incomplete: $path"
        }
    }
}

function Assert-HermesHarness {
    param(
        [Parameter(Mandatory)][string] $Source,
        [Parameter(Mandatory)][pscustomobject] $Manifest
    )

    if (-not (Test-Path -LiteralPath (Join-Path $Source '.git') -PathType Container)) {
        throw "Hermes Agent harness checkout is missing: $Source"
    }
    if (-not (Test-Path -LiteralPath (Join-Path $Source 'hermes_cli\main.py') -PathType Leaf)) {
        throw "Hermes Agent harness entry point is missing: $Source"
    }

    $expectedTree = [string]$Manifest.sources.hermesAgent.harnessTree
    if ($expectedTree -notmatch '^[0-9a-fA-F]{40}$') {
        throw 'VERSION.json must declare sources.hermesAgent.harnessTree.'
    }

    $actualTree = (@(& git -C $Source rev-parse 'HEAD^{tree}') -join [Environment]::NewLine).Trim()
    if ($LASTEXITCODE -ne 0 -or $actualTree -ne $expectedTree) {
        throw "Hermes Agent harness tree is $actualTree; expected $expectedTree. Run Setup-Hermes-Local.ps1."
    }
}

$exitCode = 0
try {
    Assert-HermesRoot
    Initialize-HermesLayout
    Set-HermesProcessEnvironment

    $hermesRoot = Get-HermesRoot
    $manifest = Get-HermesVersionManifest
    $agentSource = Resolve-HermesPath 'source\hermes-agent'
    $desktop = Resolve-HermesPath ([string]$manifest.product.client.sourcePath)
    $release = Join-Path $desktop 'release'
    $npm = (Get-Command npm.cmd -ErrorAction Stop).Source
    $python = (Get-Command python.exe -ErrorAction Stop).Source
    $destination = Resolve-HermesLauncherDestination -RequestedDestination $DestinationDirectory -Root $hermesRoot

    Assert-HermesNativeClient -Root $hermesRoot -Desktop $desktop
    Assert-HermesHarness -Source $agentSource -Manifest $manifest

    $null = Invoke-HermesProcess -FilePath $python -ArgumentList @(
        (Resolve-HermesPath 'scripts\ci\check_native_client_architecture.py'),
        '--repository-root', $hermesRoot
    ) -WorkingDirectory $hermesRoot -LogComponent launcher

    $null = Invoke-HermesProcess -FilePath $npm -ArgumentList @('run', 'typecheck') -WorkingDirectory $hermesRoot -LogComponent launcher
    $null = Invoke-HermesProcess -FilePath $npm -ArgumentList @('run', 'build') -WorkingDirectory $hermesRoot -LogComponent launcher
    $null = Invoke-HermesProcess -FilePath $npm -ArgumentList @(
        'run', 'builder', '--workspace', 'apps/desktop', '--', '--dir', '--win'
    ) -WorkingDirectory $hermesRoot -LogComponent launcher

    $unpacked = Join-Path $release 'win-unpacked'
    $executable = Join-Path $unpacked 'Hermes Launcher.exe'
    if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) {
        throw "Packaged launcher executable was not produced: $executable"
    }

    if (Test-Path -LiteralPath $destination) {
        Get-ChildItem -LiteralPath $destination -Force | Remove-Item -Recurse -Force
    } else {
        [IO.Directory]::CreateDirectory($destination) | Out-Null
    }
    Get-ChildItem -LiteralPath $unpacked -Force | Copy-Item -Destination $destination -Recurse -Force

    $target = Join-Path $destination 'Hermes Launcher.exe'
    if (-not (Test-Path -LiteralPath $target -PathType Leaf)) {
        throw "Launcher copy failed: $target"
    }

    Write-HermesLog -Component launcher -Message "Built root-owned Hermes Local client at $target."
    Write-Host "Hermes Launcher built: $target"
} catch {
    $exitCode = 1
    Write-HermesLog -Component launcher -Level ERROR -Message $_.Exception.ToString()
    Write-Host "Hermes Launcher build failed: $($_.Exception.Message)" -ForegroundColor Red
}

exit $exitCode
