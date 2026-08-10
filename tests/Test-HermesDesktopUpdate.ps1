[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$modulePath = Join-Path $root 'scripts\Hermes-DesktopUpdate.psm1'
$scriptPath = Join-Path $root 'Invoke-Hermes-DesktopUpdate.ps1'
$buildPath = Join-Path $root 'Build-Hermes-Launcher.ps1'
$partsRoot = Join-Path $root 'scripts\desktop-update'

function Assert-Contract {
    param([Parameter(Mandatory)][bool] $Condition, [Parameter(Mandatory)][string] $Message)
    if (-not $Condition) { throw $Message }
}

foreach ($path in @($modulePath, $scriptPath, $buildPath)) {
    Assert-Contract (Test-Path -LiteralPath $path -PathType Leaf) "Missing Desktop update file: $path"
}
Assert-Contract (Test-Path -LiteralPath $partsRoot -PathType Container) "Missing Desktop update directory: $partsRoot"

Import-Module $modulePath -Force
Get-ChildItem -LiteralPath $partsRoot -Filter '*.ps1' -File |
    Sort-Object Name |
    ForEach-Object { . $_.FullName }

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

$scriptText = @(
    Get-Content -Raw -LiteralPath $scriptPath
    Get-ChildItem -LiteralPath $partsRoot -Filter '*.ps1' -File |
        Sort-Object Name |
        ForEach-Object { Get-Content -Raw -LiteralPath $_.FullName }
) -join [Environment]::NewLine
foreach ($required in @(
    "'Promote'",
    'pending-desktop-update\.json',
    'ready-to-restart',
    'Start-HermesDesktopPromotionHelper',
    'Wait-HermesDesktopLauncherExit',
    'Test-HermesDesktopLauncherRunning',
    "'-DestinationDirectory'",
    'launcherStayedOpen',
    'relaunchOnActivation = \$false',
    'New-HermesDesktopCandidateWorktree',
    'Remove-HermesDesktopCandidateWorktree',
    'Set-HermesDesktopSourceRevision',
    "'worktree', 'add', '--detach'",
    "'merge', '--ff-only', '--no-edit'",
    "'reset', '--keep'",
    'preservesLocalChanges = \$true',
    'SkipModel'
)) {
    Assert-Contract ($scriptText -match $required) "Updater is missing contract: $required"
}
Assert-Contract (
    $scriptText -notmatch 'ConvertTo-HermesDesktopUpdateMarker -Name helper'
) 'Updater still tells Electron to close for a detached helper handoff.'
Assert-Contract (
    $scriptText -notmatch 'Start-HermesKnownGoodLauncher|Wait-HermesDesktopUpdateParent'
) 'Updater still closes or automatically relaunches Hermes Launcher.'
Assert-Contract (
    $scriptText -notmatch 'Restarting Hermes Launcher to install'
) 'Updater still presents the old forced-restart message.'
Assert-Contract (
    $scriptText -notmatch 'Commit or stash them before updating'
) 'Desktop updater still blocks updates when local source changes exist.'
Assert-Contract (
    $scriptText -notmatch "'stash',\s*'(?:push|apply)'|'reset',\s*'--hard'"
) 'Desktop updater still moves local changes or hard-resets the installed checkout.'
Assert-Contract (
    $scriptText -notmatch "(?im)\bgit\s+clean\b|'clean'\s*,\s*'-"
) 'Desktop updater may delete untracked user files.'
Assert-Contract (
    $scriptText -notmatch '(?im)Remove-Item[^\r\n]+(?:models|data\\hermes|config\\launcher)'
) 'Desktop updater may delete user-owned state.'
Assert-Contract (
    (Get-Content -Raw -LiteralPath $modulePath) -match 'desktop-self-update\.json'
) 'Desktop updater has no stale-recoverable lock.'

$buildText = Get-Content -Raw -LiteralPath $buildPath
foreach ($required in @(
    '\[string\] \$DestinationDirectory',
    'Resolve-HermesLauncherDestination',
    'Launcher build destination cannot be the Hermes Local root',
    'Launcher build destination is outside the Hermes Local root',
    'Launcher build destination overlaps protected Hermes Local state'
)) {
    Assert-Contract ($buildText -match $required) "Launcher builder is missing contract: $required"
}

foreach ($buildScript in @('Build-Hermes-Launcher.ps1', 'Package-Hermes-Launcher.ps1')) {
    $content = Get-Content -Raw -LiteralPath (Join-Path $root $buildScript)
    Assert-Contract (
        $content -match 'apps[\\/]desktop|product\.client\.sourcePath'
    ) "$buildScript does not build the tracked client."
    Assert-Contract ($content -match 'check_native_client_architecture\.py') "$buildScript omits the ownership guard."
    Assert-Contract ($content -notmatch 'Apply-Hermes-LauncherOverlay\.ps1') "$buildScript still applies the removed overlay."
}

Write-Host 'Hermes native Desktop update contract tests passed.'
