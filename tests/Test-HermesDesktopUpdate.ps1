[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$root = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$modulePath = Join-Path $root 'scripts\Hermes-DesktopUpdate.psm1'
$scriptPath = Join-Path $root 'Invoke-Hermes-DesktopUpdate.ps1'
$overlayPath = Join-Path $root 'Apply-Hermes-LauncherOverlay.ps1'

\nfunction Get-EmbeddedPowerShellSource {\n    param([Parameter(Mandatory)][string] $Path)\n    $wrapper = Get-Content -Raw -LiteralPath $Path\n    $match = [regex]::Match($wrapper, '(?s)\$payload\s*=\s*@\((.*?)\)\s*-join')\n    if (-not $match.Success) { throw "Embedded PowerShell payload is missing: $Path" }\n    $payload = (([regex]::Matches($match.Groups[1].Value, "'([A-Za-z0-9+/=]+)'") | ForEach-Object { $_.Groups[1].Value }) -join '')\n    $bytes = [Convert]::FromBase64String($payload)\n    $input = [IO.MemoryStream]::new($bytes, $false)\n    $gzip = [IO.Compression.GzipStream]::new($input, [IO.Compression.CompressionMode]::Decompress)\n    $output = [IO.MemoryStream]::new()\n    try { $gzip.CopyTo($output) } finally { $gzip.Dispose(); $input.Dispose() }\n    try { [Text.UTF8Encoding]::new($false).GetString($output.ToArray()) } finally { $output.Dispose() }\n}\n
function Assert-Contract {
    param([Parameter(Mandatory)][bool] $Condition, [Parameter(Mandatory)][string] $Message)
    if (-not $Condition) { throw $Message }
}

foreach ($path in @($modulePath, $scriptPath, $overlayPath)) {
    Assert-Contract (Test-Path -LiteralPath $path -PathType Leaf) "Required Desktop update file is missing: $path"
}

Import-Module $modulePath -Force

$markerValue = [ordered]@{ supported = $true; behind = 3; targetSha = ('a' * 40) }
$marker = ConvertTo-HermesDesktopUpdateMarker -Name status -Value $markerValue
$decoded = ConvertFrom-HermesDesktopUpdateMarker -Text $marker -Name status
Assert-Contract ([bool]$decoded.supported) 'Desktop update marker lost its supported flag.'
Assert-Contract ([int]$decoded.behind -eq 3) 'Desktop update marker lost its behind count.'
Assert-Contract ([string]$decoded.targetSha -eq ('a' * 40)) 'Desktop update marker lost its target identity.'

Assert-Contract (Test-HermesDesktopUpdateOrigin 'https://github.com/xdCloudy/Hermes-Local.git') 'Trusted HTTPS origin was rejected.'
Assert-Contract (Test-HermesDesktopUpdateOrigin 'git@github.com:xdCloudy/Hermes-Local.git') 'Trusted SSH origin was rejected.'
Assert-Contract (-not (Test-HermesDesktopUpdateOrigin 'https://github.com/example/Hermes-Local.git')) 'Unexpected update origin was accepted.'

$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) "hermes-desktop-update-test-$([guid]::NewGuid().ToString('N'))"
try {
    [System.IO.Directory]::CreateDirectory($tempRoot) | Out-Null
    $plan = New-HermesDesktopUpdatePlan `
        -Root $tempRoot `
        -CurrentCommit ('1' * 40) `
        -TargetCommit ('2' * 40) `
        -Channel development `
        -ParentPid 0 `
        -TaskId 'desktop-task-35'

    Assert-Contract ([string]$plan.taskId -eq 'desktop-task-35') 'Task Centre identity was not retained in the staged plan.'
    Assert-Contract ([string]$plan.previousCommit -eq ('1' * 40)) 'Previous revision was not retained for rollback.'
    Assert-Contract ([string]$plan.targetCommit -eq ('2' * 40)) 'Target revision was not retained for promotion.'
    Assert-Contract ([System.IO.Path]::GetFullPath([string]$plan.stagingRoot).StartsWith(
        [System.IO.Path]::GetFullPath($tempRoot),
        [System.StringComparison]::OrdinalIgnoreCase
    )) 'Staging escaped the configured Hermes Local root.'

    $outsideRejected = $false
    try {
        $null = Assert-HermesDesktopUpdatePath -Root $tempRoot -Path (Join-Path ([System.IO.Path]::GetTempPath()) 'outside-update')
    } catch {
        $outsideRejected = $true
    }
    Assert-Contract $outsideRejected 'An update path outside the installation root was accepted.'

    $lockPath = Enter-HermesDesktopUpdateLock -Root $tempRoot -OperationId 'first-operation'
    $concurrentRejected = $false
    try {
        $null = Enter-HermesDesktopUpdateLock -Root $tempRoot -OperationId 'second-operation'
    } catch {
        $concurrentRejected = $true
    }
    Assert-Contract $concurrentRejected 'A concurrent Desktop update acquired the same installation lock.'
    Exit-HermesDesktopUpdateLock -LockPath $lockPath
} finally {
    if (Test-Path -LiteralPath $tempRoot) { Remove-Item -LiteralPath $tempRoot -Recurse -Force }
}

$scriptText = Get-EmbeddedPowerShellSource -Path $scriptPath
Assert-Contract ($scriptText -match "Wait-HermesDesktopUpdateParent") 'Detached replacement does not wait for the running launcher to exit.'
Assert-Contract ($scriptText -match "Update-Hermes-Local\.ps1") 'Desktop replacement bypasses the authoritative update orchestrator.'
Assert-Contract ($scriptText -match "Setup-Hermes-Local\.ps1") 'Desktop replacement does not synchronise the pinned integration before rebuilding.'
Assert-Contract ($scriptText -match "reset', '--hard'") 'Desktop replacement does not pin source promotion and rollback to exact commits.'
Assert-Contract ($scriptText -notmatch '(?im)git\s+clean|''clean''\s*,\s*''-') 'Desktop replacement may delete untracked user files.'
Assert-Contract ($scriptText -notmatch '(?im)Remove-Item[^\r\n]+(?:models|data\\hermes|config\\launcher)') 'Desktop replacement may delete user-owned state.'
Assert-Contract ($scriptText -match "SkipModel") 'Desktop replacement does not explicitly preserve model files.'
Assert-Contract ($scriptText -match "desktop-self-update\.json") 'Desktop replacement has no dedicated stale-recoverable lock.'

$overlayWrapper = Get-Content -Raw -LiteralPath $overlayPath
Assert-Contract ($overlayWrapper -match 'GzipStream') 'Overlay wrapper does not load its embedded validated transformer.'
$overlayText = Get-EmbeddedPowerShellSource -Path $overlayPath
Assert-Contract ($overlayText -match 'checkHermesLocalDesktopUpdates') 'Overlay does not activate the functional update check.'
Assert-Contract ($overlayText -match 'applyHermesLocalDesktopUpdate') 'Overlay does not activate native update apply.'
Assert-Contract ($overlayText -match 'waitForDesktopUpdateTask') 'Overlay does not connect updates to durable Task Centre state.'
Assert-Contract ($overlayText -match 'Restore-State') 'Overlay cannot restore the pristine pinned Hermes Agent source.'
Assert-Contract ($overlayText -match 'Hermes Local update check handler') 'Overlay does not replace the dead-end update handler.'

foreach ($buildScript in @('Build-Hermes-Launcher.ps1', 'Package-Hermes-Launcher.ps1')) {
    $content = Get-Content -Raw -LiteralPath (Join-Path $root $buildScript)
    Assert-Contract ($content -match 'Apply-Hermes-LauncherOverlay\.ps1') "$buildScript does not include the native updater overlay."
    Assert-Contract ($content -match '-Mode Restore') "$buildScript does not restore the pinned source after packaging."
}

Write-Host 'Hermes native Desktop update contract tests passed.'
