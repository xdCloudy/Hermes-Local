[CmdletBinding()]
param(
    [switch] $NonInteractive
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'scripts\Common-Hermes.psm1') -Force

function New-TemporaryTablerTypeDeclaration {
    param(
        [Parameter(Mandatory)]
        [string] $DesktopSource
    )

    $sourceRoot = Join-Path $DesktopSource 'src'
    $usesDirectTablerImports = Get-ChildItem -LiteralPath $sourceRoot -Recurse -File -Include '*.ts', '*.tsx' |
        Select-String -SimpleMatch '@tabler/icons-react/dist/esm/icons/' -Quiet

    if (-not $usesDirectTablerImports) {
        return $null
    }

    $path = Join-Path $sourceRoot 'hermes-local-tabler-direct-icons.generated.d.ts'
    $content = @'
// Generated temporarily by Hermes Local during launcher compilation.
// Deep Tabler icon modules omit declarations in the pinned npm package, so
// mirror the exact public type of a normal barrel-exported Tabler icon.
declare module '@tabler/icons-react/dist/esm/icons/*.mjs' {
  const Icon: typeof import('@tabler/icons-react').IconActivity
  export default Icon
}
'@

    [System.IO.File]::WriteAllText(
        $path,
        $content + [Environment]::NewLine,
        [System.Text.UTF8Encoding]::new($false)
    )
    Write-HermesLog -Component launcher -Message "Created temporary Tabler direct-import declaration at $path."
    return $path
}

$temporaryTypeDeclaration = $null
$overlayState = $null
$exitCode = 0

try {
    Assert-HermesRoot
    Initialize-HermesLayout
    Set-HermesProcessEnvironment
    $source = Resolve-HermesPath 'source\hermes-agent'
    $desktop = Join-Path $source 'apps\desktop'
    $release = Join-Path $desktop 'release'
    $npm = (Get-Command npm.cmd -ErrorAction Stop).Source

    $overlayState = Resolve-HermesPath "temp\launcher-overlay-$([guid]::NewGuid().ToString('N')).json"
    & (Resolve-HermesPath 'Apply-Hermes-LauncherOverlay.ps1') `
        -Mode Apply `
        -StatePath $overlayState `
        -RepositoryRoot (Get-HermesRoot)
    $temporaryTypeDeclaration = New-TemporaryTablerTypeDeclaration -DesktopSource $desktop

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
} catch {
    $exitCode = 1
    Write-HermesLog -Component launcher -Level ERROR -Message $_.Exception.ToString()
    Write-Host "Hermes Launcher build failed: $($_.Exception.Message)" -ForegroundColor Red
} finally {
    if ($temporaryTypeDeclaration -and (Test-Path -LiteralPath $temporaryTypeDeclaration)) {
        Remove-Item -LiteralPath $temporaryTypeDeclaration -Force
        Write-HermesLog -Component launcher -Message "Removed temporary Tabler direct-import declaration at $temporaryTypeDeclaration."
    }
    if ($overlayState -and (Test-Path -LiteralPath $overlayState -PathType Leaf)) {
        try {
            & (Resolve-HermesPath 'Apply-Hermes-LauncherOverlay.ps1') `
                -Mode Restore `
                -StatePath $overlayState `
                -RepositoryRoot (Get-HermesRoot)
        } catch {
            $exitCode = 1
            Write-HermesLog -Component launcher -Level ERROR -Message $_.Exception.ToString()
            Write-Host "Hermes Launcher source restoration failed: $($_.Exception.Message)" -ForegroundColor Red
        }
    }
}

exit $exitCode
