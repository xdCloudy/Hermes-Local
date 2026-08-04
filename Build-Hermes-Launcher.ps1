[CmdletBinding()]
param(
    [switch] $NonInteractive,
    [string] $DestinationDirectory
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

function Get-PendingHermesLauncherPatches {
    param(
        [Parameter(Mandatory)][string] $Git,
        [Parameter(Mandatory)][string] $Source
    )

    $manifest = Get-HermesVersionManifest
    $baseCommit = [string]$manifest.sources.hermesAgent.commit
    $patchSeries = [string]$manifest.sources.hermesAgent.patchSeries

    if ($baseCommit -notmatch '^[0-9a-fA-F]{40}$') {
        throw "Hermes Agent source commit is invalid in VERSION.json: $baseCommit"
    }
    if ([string]::IsNullOrWhiteSpace($patchSeries)) {
        throw 'Hermes Agent patch series is not configured in VERSION.json.'
    }

    $patchDirectory = Resolve-HermesPath $patchSeries
    $patches = @(
        Get-ChildItem -LiteralPath $patchDirectory -File -Filter '*.patch' |
            Sort-Object Name
    )
    if ($patches.Count -eq 0) {
        throw "Hermes Agent patch series is empty: $patchDirectory"
    }

    $status = @(
        Invoke-HermesProcess `
            -FilePath $Git `
            -ArgumentList @('-C', $Source, 'status', '--porcelain', '--untracked-files=no') `
            -WorkingDirectory $Source `
            -LogComponent launcher `
            -PassThruOutput
    ) -join [Environment]::NewLine
    if (-not [string]::IsNullOrWhiteSpace($status)) {
        throw "Hermes Agent source contains tracked local changes. Refusing to apply temporary launcher patches:`n$status"
    }

    $null = Invoke-HermesProcess `
        -FilePath $Git `
        -ArgumentList @('-C', $Source, 'merge-base', '--is-ancestor', $baseCommit, 'HEAD') `
        -WorkingDirectory $Source `
        -LogComponent launcher

    $countText = @(
        Invoke-HermesProcess `
            -FilePath $Git `
            -ArgumentList @('-C', $Source, 'rev-list', '--count', "$baseCommit..HEAD") `
            -WorkingDirectory $Source `
            -LogComponent launcher `
            -PassThruOutput
    ) -join ''
    $appliedCount = 0
    if (-not [int]::TryParse($countText.Trim(), [ref]$appliedCount)) {
        throw "Could not determine the integrated Hermes Agent patch count: $countText"
    }
    if ($appliedCount -gt $patches.Count) {
        throw "Hermes Agent source has $appliedCount integration commits, but only $($patches.Count) launcher patches are installed."
    }

    if ($appliedCount -eq $patches.Count) {
        return @()
    }

    $pending = @($patches | Select-Object -Skip $appliedCount)
    Write-HermesLog -Component launcher -Message (
        "Reconciling {0} launcher patch(es) missing from the prepared source: {1}" -f
        $pending.Count,
        (($pending | ForEach-Object Name) -join ', ')
    )
    return $pending
}

function Test-HermesGitApply {
    param(
        [Parameter(Mandatory)][string] $Git,
        [Parameter(Mandatory)][string] $Source,
        [Parameter(Mandatory)][string] $PatchPath,
        [switch] $Reverse
    )

    $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $Git
    $startInfo.WorkingDirectory = $Source
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    foreach ($argument in @('-C', $Source, 'apply')) {
        $startInfo.ArgumentList.Add($argument)
    }
    if ($Reverse) {
        $startInfo.ArgumentList.Add('--reverse')
    }
    foreach ($argument in @('--check', '--whitespace=nowarn', '--', $PatchPath)) {
        $startInfo.ArgumentList.Add($argument)
    }

    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $startInfo
    try {
        if (-not $process.Start()) {
            throw "Could not start git apply check for $PatchPath"
        }
        $stdout = $process.StandardOutput.ReadToEnd()
        $stderr = $process.StandardError.ReadToEnd()
        $process.WaitForExit()
        return [pscustomobject]@{
            Success = $process.ExitCode -eq 0
            ExitCode = $process.ExitCode
            Output = (@($stdout.Trim(), $stderr.Trim()) | Where-Object { $_ }) -join [Environment]::NewLine
        }
    } finally {
        $process.Dispose()
    }
}

function Repair-HermesLauncherBuildSource {
    param([Parameter(Mandatory)][string] $Source)

    $encoding = [System.Text.UTF8Encoding]::new($false)
    $controlPath = Join-Path $Source 'apps\desktop\electron\hermes-local-control.ts'
    $taskModelPath = Join-Path $Source 'apps\desktop\electron\hermes-local-task-model.ts'
    $statusbarPath = Join-Path $Source 'apps\desktop\src\app\shell\hooks\use-statusbar-items.tsx'
    $englishPath = Join-Path $Source 'apps\desktop\src\i18n\en.ts'

    $control = [System.IO.File]::ReadAllText($controlPath)
    $badBackslashLiteral = "privatePath.replaceAll('\', '/')"
    $goodBackslashLiteral = "privatePath.replaceAll('\\', '/')"
    if ($control.Contains($badBackslashLiteral)) {
        $control = $control.Replace($badBackslashLiteral, $goodBackslashLiteral)
        [System.IO.File]::WriteAllText($controlPath, $control, $encoding)
        Write-HermesLog -Component launcher -Message 'Repaired all malformed private-path backslash literals in the temporary launcher build source.'
    }

    $control = [System.IO.File]::ReadAllText($controlPath)
    if ($control.Contains($badBackslashLiteral) -or -not $control.Contains($goodBackslashLiteral)) {
        throw 'The temporary launcher build source still contains an invalid private-path backslash literal.'
    }

    $taskModel = [System.IO.File]::ReadAllText($taskModelPath)
    $updatePolicyPattern = "update:\s*\{\s*cancellable:\s*false,\s*conflictPolicy:\s*'reject',\s*resources:\s*\[[^\]]*\]\s*\}"
    $updatePolicyFixed = [regex]::IsMatch(
        $taskModel,
        "update:\s*\{\s*cancellable:\s*false,\s*conflictPolicy:\s*'reject',\s*resources:\s*\[\s*\]\s*\}"
    )
    if (-not $updatePolicyFixed) {
        $updatedTaskModel = [regex]::Replace(
            $taskModel,
            $updatePolicyPattern,
            "update: { cancellable: false, conflictPolicy: 'reject', resources: [] }"
        )
        if ($updatedTaskModel -eq $taskModel) {
            throw 'Could not locate the Desktop update task policy in the temporary launcher build source.'
        }
        [System.IO.File]::WriteAllText($taskModelPath, $updatedTaskModel, $encoding)
        Write-HermesLog -Component launcher -Message 'Removed the workstation lock from observational Desktop update checks in the temporary build source.'
    }

    $statusbar = [System.IO.File]::ReadAllText($statusbarPath)
    $managedDeclaration = @'
  const managedLocalConnection =
    connection?.mode === 'remote' &&
    /^(?:https?:\/\/)?(?:localhost|127(?:\.\d{1,3}){3}|\[?::1\]?)(?::\d+)?(?:\/.*)?$/i.test(
      connection.remoteHost?.trim() ?? ''
    )

'@
    if (-not $statusbar.Contains('const managedLocalConnection =')) {
        $anchor = '  const clientVersionItem = useMemo<StatusbarItem>(() => {'
        if (-not $statusbar.Contains($anchor)) {
            throw 'Could not locate the Desktop client version statusbar item.'
        }
        $statusbar = $statusbar.Replace($anchor, $managedDeclaration + $anchor)
    }

    $statusbarRepairs = [ordered]@{
        "remote: connection?.mode === 'remote'," = "remote: connection?.mode === 'remote' && !managedLocalConnection,"
        'version: desktopVersion?.appVersion' = 'version: updateStatus?.currentVersion ?? desktopVersion?.appVersion'
        "if (connection?.mode !== 'remote') {" = "if (connection?.mode !== 'remote' || managedLocalConnection) {"
    }
    foreach ($entry in $statusbarRepairs.GetEnumerator()) {
        if ($statusbar.Contains([string]$entry.Key)) {
            $statusbar = $statusbar.Replace([string]$entry.Key, [string]$entry.Value)
        } elseif (-not $statusbar.Contains([string]$entry.Value)) {
            throw "Could not apply Desktop footer identity repair: $($entry.Key)"
        }
    }

    if (
        -not $statusbar.Contains('const managedLocalConnection =') -or
        -not $statusbar.Contains("remote: connection?.mode === 'remote' && !managedLocalConnection,") -or
        -not $statusbar.Contains('version: updateStatus?.currentVersion ?? desktopVersion?.appVersion') -or
        -not $statusbar.Contains("if (connection?.mode !== 'remote' || managedLocalConnection) {")
    ) {
        throw 'The temporary Desktop footer identity repair did not verify.'
    }
    [System.IO.File]::WriteAllText($statusbarPath, $statusbar, $encoding)

    $english = [System.IO.File]::ReadAllText($englishPath)
    $desktopLabel = 'desktopVersion: version => `Hermes Desktop v${version}`'
    $launcherLabel = 'desktopVersion: version => `Hermes Launcher v${version}`'
    if ($english.Contains($desktopLabel)) {
        $english = $english.Replace($desktopLabel, $launcherLabel)
    } elseif (-not $english.Contains($launcherLabel)) {
        throw 'Could not locate the Desktop version tooltip label.'
    }
    [System.IO.File]::WriteAllText($englishPath, $english, $encoding)
    Write-HermesLog -Component launcher -Message 'Applied the managed Hermes Local footer identity to the temporary launcher build source.'
}

function Test-HermesPatchSemanticallyPresent {
    param(
        [Parameter(Mandatory)][System.IO.FileInfo] $Patch,
        [Parameter(Mandatory)][string] $Source
    )

    if ($Patch.Name.StartsWith('0037-', [StringComparison]::OrdinalIgnoreCase)) {
        $taskModel = [System.IO.File]::ReadAllText(
            (Join-Path $Source 'apps\desktop\electron\hermes-local-task-model.ts')
        )
        return [regex]::IsMatch(
            $taskModel,
            "update:\s*\{\s*cancellable:\s*false,\s*conflictPolicy:\s*'reject',\s*resources:\s*\[\s*\]\s*\}"
        )
    }

    if ($Patch.Name.StartsWith('0038-', [StringComparison]::OrdinalIgnoreCase)) {
        $control = [System.IO.File]::ReadAllText(
            (Join-Path $Source 'apps\desktop\electron\hermes-local-control.ts')
        )
        $badBackslashLiteral = "privatePath.replaceAll('\', '/')"
        $goodBackslashLiteral = "privatePath.replaceAll('\\', '/')"
        return $control.Contains($goodBackslashLiteral) -and -not $control.Contains($badBackslashLiteral)
    }

    return $false
}

$temporaryTypeDeclaration = $null
$temporaryPatches = [System.Collections.Generic.List[string]]::new()
$repairSnapshots = [ordered]@{}
$overlayState = $null
$exitCode = 0
$git = $null
$source = $null

try {
    Assert-HermesRoot
    Initialize-HermesLayout
    Set-HermesProcessEnvironment

    $hermesRoot = Get-HermesRoot
    $source = Resolve-HermesPath 'source\hermes-agent'
    $desktop = Join-Path $source 'apps\desktop'
    $release = Join-Path $desktop 'release'
    $npm = (Get-Command npm.cmd -ErrorAction Stop).Source
    $node = (Get-Command node.exe -ErrorAction Stop).Source
    $git = (Get-Command git -ErrorAction Stop).Source
    $destination = Resolve-HermesLauncherDestination `
        -RequestedDestination $DestinationDirectory `
        -Root $hermesRoot

    $pendingPatches = @(Get-PendingHermesLauncherPatches -Git $git -Source $source)

    $overlayState = Resolve-HermesPath "temp\launcher-overlay-$([guid]::NewGuid().ToString('N')).json"
    & (Resolve-HermesPath 'Apply-Hermes-LauncherOverlay.ps1') `
        -Mode Apply `
        -StatePath $overlayState `
        -RepositoryRoot $hermesRoot

    foreach ($relativePath in @(
        'apps\desktop\electron\hermes-local-control.ts',
        'apps\desktop\electron\hermes-local-task-model.ts',
        'apps\desktop\src\app\shell\hooks\use-statusbar-items.tsx',
        'apps\desktop\src\i18n\en.ts'
    )) {
        $snapshotPath = Join-Path $source $relativePath
        $repairSnapshots[$snapshotPath] = [System.IO.File]::ReadAllBytes($snapshotPath)
    }

    Repair-HermesLauncherBuildSource -Source $source

    foreach ($patch in $pendingPatches) {
        if (Test-HermesPatchSemanticallyPresent -Patch $patch -Source $source) {
            Write-HermesLog -Component launcher -Message "Launcher patch is already represented in the build source: $($patch.Name)"
            continue
        }

        $forward = Test-HermesGitApply -Git $git -Source $source -PatchPath $patch.FullName
        if ($forward.Success) {
            $null = Invoke-HermesProcess `
                -FilePath $git `
                -ArgumentList @('-C', $source, 'apply', '--whitespace=nowarn', '--', $patch.FullName) `
                -WorkingDirectory $source `
                -LogComponent launcher
            $temporaryPatches.Add($patch.FullName)
            continue
        }

        $reverse = Test-HermesGitApply -Git $git -Source $source -PatchPath $patch.FullName -Reverse
        if ($reverse.Success) {
            Write-HermesLog -Component launcher -Message "Launcher patch is already applied after overlay composition: $($patch.Name)"
            continue
        }

        throw (
            "Launcher patch '$($patch.Name)' neither applies nor is already represented after overlay composition." +
            "`nForward check:`n$($forward.Output)" +
            "`nReverse check:`n$($reverse.Output)"
        )
    }
    $temporaryTypeDeclaration = New-TemporaryTablerTypeDeclaration -DesktopSource $desktop

    $null = Invoke-HermesProcess -FilePath $npm -ArgumentList @(
        'run', 'build', '--workspace', 'web'
    ) -WorkingDirectory $source -LogComponent launcher

    $privatePathRepair = Join-Path $desktop 'scripts\repair-hermes-local-control.mjs'
    if (-not (Test-Path -LiteralPath $privatePathRepair -PathType Leaf)) {
        throw "Desktop private-path repair script was not installed: $privatePathRepair"
    }
    $null = Invoke-HermesProcess `
        -FilePath $node `
        -ArgumentList @($privatePathRepair) `
        -WorkingDirectory $desktop `
        -LogComponent launcher

    $null = Invoke-HermesProcess -FilePath $npm -ArgumentList @(
        'run', 'typecheck', '--workspace', 'apps/desktop'
    ) -WorkingDirectory $source -LogComponent launcher
    $null = Invoke-HermesProcess -FilePath $npm -ArgumentList @(
        'run', 'build', '--workspace', 'apps/desktop'
    ) -WorkingDirectory $source -LogComponent launcher
    $null = Invoke-HermesProcess -FilePath $npm -ArgumentList @(
        'run', 'builder', '--workspace', 'apps/desktop', '--', '--dir', '--win'
    ) -WorkingDirectory $source -LogComponent launcher

    $unpacked = Join-Path $release 'win-unpacked'
    $executable = Join-Path $unpacked 'Hermes Launcher.exe'
    if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) {
        throw "Packaged launcher executable was not produced: $executable"
    }

    if (Test-Path -LiteralPath $destination) {
        Get-ChildItem -LiteralPath $destination -Force |
            Remove-Item -Recurse -Force
    } else {
        [IO.Directory]::CreateDirectory($destination) | Out-Null
    }

    Get-ChildItem -LiteralPath $unpacked -Force |
        Copy-Item -Destination $destination -Recurse -Force

    $target = Join-Path $destination 'Hermes Launcher.exe'
    if (-not (Test-Path -LiteralPath $target -PathType Leaf)) {
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
    if ($git -and $source -and $temporaryPatches.Count -gt 0) {
        for ($index = $temporaryPatches.Count - 1; $index -ge 0; $index -= 1) {
            try {
                $null = Invoke-HermesProcess `
                    -FilePath $git `
                    -ArgumentList @(
                        '-C', $source, 'apply', '--reverse', '--whitespace=nowarn', '--', $temporaryPatches[$index]
                    ) `
                    -WorkingDirectory $source `
                    -LogComponent launcher
            } catch {
                $exitCode = 1
                Write-HermesLog -Component launcher -Level ERROR -Message $_.Exception.ToString()
                Write-Host "Hermes Launcher patch restoration failed: $($_.Exception.Message)" -ForegroundColor Red
            }
        }
    }
    if ($repairSnapshots.Count -gt 0) {
        foreach ($entry in $repairSnapshots.GetEnumerator()) {
            try {
                [System.IO.File]::WriteAllBytes([string]$entry.Key, [byte[]]$entry.Value)
            } catch {
                $exitCode = 1
                Write-HermesLog -Component launcher -Level ERROR -Message $_.Exception.ToString()
                Write-Host "Hermes Launcher semantic repair restoration failed: $($_.Exception.Message)" -ForegroundColor Red
            }
        }
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
