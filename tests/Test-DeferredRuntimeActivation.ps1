[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$entryPath = Join-Path $root 'Invoke-Hermes-DesktopUpdate.ps1'
$safeActivationPath = Join-Path $root 'scripts\desktop-update\DesktopUpdate-SafeActivation.ps1'
$runtimeSyncPath = Join-Path $root 'scripts\setup\Sync-HermesPythonRuntime.ps1'
$migrationPath = Join-Path $root 'scripts\setup\Python-RuntimeMigration.ps1'

foreach ($path in @($entryPath, $safeActivationPath, $runtimeSyncPath, $migrationPath)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Deferred activation contract file is missing: $path"
    }
}

$entry = [IO.File]::ReadAllText($entryPath)
$safeActivation = [IO.File]::ReadAllText($safeActivationPath)
$runtimeSync = [IO.File]::ReadAllText($runtimeSyncPath)
$migration = [IO.File]::ReadAllText($migrationPath)

$stageIndex = $entry.IndexOf("'DesktopUpdate-Stage.ps1'", [StringComparison]::Ordinal)
$safeIndex = $entry.IndexOf("'DesktopUpdate-SafeActivation.ps1'", [StringComparison]::Ordinal)
if ($stageIndex -lt 0 -or $safeIndex -le $stageIndex) {
    throw 'Safe activation overrides must load after the normal Desktop update stage and promotion components.'
}

foreach ($required in @(
    "'-SkipHermesDependencies'",
    'Sync-HermesPythonRuntime.ps1',
    "'Stop-Hermes-Local.ps1'",
    'Move-HermesDesktopActiveLauncherToActivationBackup',
    'runtimeSynchronizedAfterExit = $true',
    'relaunched = $false'
)) {
    if (-not $safeActivation.Contains($required, [StringComparison]::Ordinal)) {
        throw "Safe activation contract is missing: $required"
    }
}

$reserveCall = $safeActivation.IndexOf(
    '        Move-HermesDesktopActiveLauncherToActivationBackup',
    [StringComparison]::Ordinal
)
$stopCall = $safeActivation.IndexOf(
    "        Invoke-HermesDesktopProcess -FilePath 'pwsh.exe' -Arguments @(",
    $reserveCall,
    [StringComparison]::Ordinal
)
$runtimeCall = $safeActivation.LastIndexOf(
    '        Invoke-HermesDesktopRuntimeSync',
    [StringComparison]::Ordinal
)
$promoteCall = $safeActivation.IndexOf(
    '        Move-Item -LiteralPath $pendingDist -Destination $dist',
    [StringComparison]::Ordinal
)
if (
    $reserveCall -lt 0 -or
    $stopCall -le $reserveCall -or
    $runtimeCall -le $stopCall -or
    $promoteCall -le $runtimeCall
) {
    throw 'Activation must reserve dist, stop services, synchronize Python, then promote the staged Launcher.'
}

foreach ($required in @(
    "'hermes-next-' + [guid]::NewGuid().ToString('N')",
    'VIRTUAL_ENV = $candidateRuntime',
    'Assert-HermesPythonRuntimeInactive -Runtime $runtime',
    '[IO.Directory]::Move($candidateRuntime, $runtime)',
    'from gateway.config import Platform, load_gateway_config'
)) {
    if (-not $runtimeSync.Contains($required, [StringComparison]::Ordinal)) {
        throw "Transactional runtime synchronization contract is missing: $required"
    }
}

if ($runtimeSync.Contains('UV_PROJECT_ENVIRONMENT = $runtime', [StringComparison]::Ordinal)) {
    throw 'Dependency synchronization must not mutate the active runtime before activation.'
}

foreach ($required in @(
    'Assert-HermesPythonRuntimeInactive -Runtime $Runtime',
    '[System.IO.Directory]::Move('
)) {
    if (-not $migration.Contains($required, [StringComparison]::Ordinal)) {
        throw "Python migration contract is missing: $required"
    }
}

if ($migration.Contains('Move-Item -LiteralPath $Runtime', [StringComparison]::Ordinal)) {
    throw 'Python runtime migration must use one same-volume Directory.Move rename, not recursive Move-Item.'
}

Write-Host 'Deferred runtime activation contract passed.'
