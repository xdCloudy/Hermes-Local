[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$modulePath = Join-Path $root 'scripts\Hermes-DesktopUpdate.psm1'
$scriptPath = Join-Path $root 'Invoke-Hermes-DesktopUpdate.ps1'
$overlayPath = Join-Path $root 'Apply-Hermes-LauncherOverlay.ps1'

function Assert-Contract {
    param([Parameter(Mandatory)][bool] $Condition, [Parameter(Mandatory)][string] $Message)
    if (-not $Condition) { throw $Message }
}

function Get-EmbeddedPowerShellSource {
    param([Parameter(Mandatory)][string] $Path)

    $wrapper = Get-Content -Raw -LiteralPath $Path
    $match = [regex]::Match($wrapper, '(?s)\$payload\s*=\s*@\((.*?)\)\s*-join')
    if (-not $match.Success) { throw "Embedded PowerShell payload is missing: $Path" }
    $payload = (([regex]::Matches(
        $match.Groups[1].Value,
        "'([A-Za-z0-9+/=]+)'"
    ) | ForEach-Object { $_.Groups[1].Value }) -join '')
    $bytes = [Convert]::FromBase64String($payload)
    $input = [IO.MemoryStream]::new($bytes, $false)
    $gzip = [IO.Compression.GzipStream]::new(
        $input,
        [IO.Compression.CompressionMode]::Decompress
    )
    $output = [IO.MemoryStream]::new()
    try { $gzip.CopyTo($output) } finally { $gzip.Dispose(); $input.Dispose() }
    try { [Text.UTF8Encoding]::new($false).GetString($output.ToArray()) } finally { $output.Dispose() }
}

foreach ($path in @($modulePath, $scriptPath, $overlayPath)) {
    Assert-Contract (Test-Path -LiteralPath $path -PathType Leaf) "Missing Desktop update file: $path"
}

Import-Module $modulePath -Force

$markerValue = [ordered]@{ supported = $true; behind = 3; targetSha = ('a' * 40) }
$decoded = ConvertFrom-HermesDesktopUpdateMarker `
    -Name status `
    -Text (ConvertTo-HermesDesktopUpdateMarker -Name status -Value $markerValue)
Assert-Contract ([bool]$decoded.supported) 'Status marker lost supported.'
Assert-Contract ([int]$decoded.behind -eq 3) 'Status marker lost behind count.'
Assert-Contract ([string]$decoded.targetSha -eq ('a' * 40)) 'Status marker lost target SHA.'

Assert-Contract (
    Test-HermesDesktopUpdateOrigin 'https://github.com/xdCloudy/Hermes-Local.git'
) 'Trusted HTTPS origin was rejected.'
Assert-Contract (
    Test-HermesDesktopUpdateOrigin 'git@github.com:xdCloudy/Hermes-Local.git'
) 'Trusted SSH origin was rejected.'
Assert-Contract (-not (
    Test-HermesDesktopUpdateOrigin 'https://github.com/example/Hermes-Local.git'
)) 'Unexpected origin was accepted.'

$tempRoot = Join-Path ([IO.Path]::GetTempPath()) "hermes-desktop-update-test-$([guid]::NewGuid().ToString('N'))"
try {
    [IO.Directory]::CreateDirectory($tempRoot) | Out-Null
    $plan = New-HermesDesktopUpdatePlan `
        -Root $tempRoot `
        -CurrentCommit ('1' * 40) `
        -TargetCommit ('2' * 40) `
        -Channel development `
        -ParentPid 0 `
        -TaskId 'desktop-task-35'
    Assert-Contract ([string]$plan.taskId -eq 'desktop-task-35') 'Task identity was not retained.'
    Assert-Contract ([string]$plan.previousCommit -eq ('1' * 40)) 'Previous revision was not retained.'
    Assert-Contract ([string]$plan.targetCommit -eq ('2' * 40)) 'Target revision was not retained.'
    Assert-Contract (
        [IO.Path]::GetFullPath([string]$plan.stagingRoot).StartsWith(
            [IO.Path]::GetFullPath($tempRoot),
            [StringComparison]::OrdinalIgnoreCase
        )
    ) 'Staging escaped the installation root.'

    $outsideRejected = $false
    try {
        $null = Assert-HermesDesktopUpdatePath `
            -Root $tempRoot `
            -Path (Join-Path ([IO.Path]::GetTempPath()) 'outside-update')
    } catch { $outsideRejected = $true }
    Assert-Contract $outsideRejected 'An outside update path was accepted.'

    $lockPath = Enter-HermesDesktopUpdateLock -Root $tempRoot -OperationId 'first-operation'
    $concurrentRejected = $false
    try {
        $null = Enter-HermesDesktopUpdateLock -Root $tempRoot -OperationId 'second-operation'
    } catch { $concurrentRejected = $true }
    Assert-Contract $concurrentRejected 'A concurrent update acquired the same lock.'
    Exit-HermesDesktopUpdateLock -LockPath $lockPath

    $staleLock = Join-Path $tempRoot 'data\runtime\locks\desktop-self-update.json'
    [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($staleLock)) | Out-Null
    @{
        schemaVersion = 1
        operationId = 'stale-operation'
        ownerPid = 2147483647
        acquiredAt = '2000-01-01T00:00:00Z'
    } | ConvertTo-Json | Set-Content -LiteralPath $staleLock -Encoding utf8

    $recoveredLock = Enter-HermesDesktopUpdateLock -Root $tempRoot -OperationId 'recovered-operation'
    Assert-Contract (
        @(Get-ChildItem -Path "$staleLock.recovered-*" -File).Count -eq 1
    ) 'Stale-lock recovery evidence was not retained.'
    Exit-HermesDesktopUpdateLock -LockPath $recoveredLock
} finally {
    if (Test-Path -LiteralPath $tempRoot) {
        Remove-Item -LiteralPath $tempRoot -Recurse -Force
    }
}

$scriptText = Get-Content -Raw -LiteralPath $scriptPath
foreach ($required in @(
    'Wait-HermesDesktopUpdateParent',
    'Update-Hermes-Local\.ps1',
    'Setup-Hermes-Local\.ps1',
    "'reset', '--hard'",
    'SkipModel',
    'Restore-PreviousLauncher'
)) {
    Assert-Contract ($scriptText -match $required) "Detached updater is missing contract: $required"
}
Assert-Contract (
    $scriptText -notmatch "(?im)\bgit\s+clean\b|'clean'\s*,\s*'-"
) 'Detached updater may delete untracked user files.'
Assert-Contract (
    $scriptText -notmatch '(?im)Remove-Item[^\r\n]+(?:models|data\\hermes|config\\launcher)'
) 'Detached updater may delete user-owned state.'
Assert-Contract (
    (Get-Content -Raw -LiteralPath $modulePath) -match 'desktop-self-update\.json'
) 'Desktop updater has no stale-recoverable lock.'

$overlayWrapper = Get-Content -Raw -LiteralPath $overlayPath
Assert-Contract ($overlayWrapper -match 'GzipStream') 'Overlay wrapper is not validated/compressed.'
$overlayText = Get-EmbeddedPowerShellSource -Path $overlayPath
foreach ($required in @(
    'checkHermesLocalDesktopUpdates',
    'applyHermesLocalDesktopUpdate',
    'waitForDesktopUpdateTask',
    'Restore-State',
    'Hermes Local update check handler'
)) {
    Assert-Contract ($overlayText -match $required) "Overlay is missing contract: $required"
}

foreach ($buildScript in @('Build-Hermes-Launcher.ps1', 'Package-Hermes-Launcher.ps1')) {
    $content = Get-Content -Raw -LiteralPath (Join-Path $root $buildScript)
    Assert-Contract ($content -match 'Apply-Hermes-LauncherOverlay\.ps1') "$buildScript omits the overlay."
    Assert-Contract ($content -match '-Mode Restore') "$buildScript does not restore source."
}

Write-Host 'Hermes native Desktop update contract tests passed.'
