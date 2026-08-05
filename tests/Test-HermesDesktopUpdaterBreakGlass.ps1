[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repositoryRoot = [IO.Path]::GetFullPath((Split-Path $PSScriptRoot -Parent))
$partsRoot = Join-Path $repositoryRoot 'scripts\desktop-update'
$stackDrainPath = Join-Path $partsRoot 'DesktopUpdate-StackDrain.ps1'
$activationLoaderPath = Join-Path $partsRoot 'DesktopUpdate-Activation.ps1'
$activationCorePath = Join-Path $partsRoot 'DesktopUpdate-Activation-Core.ps1'
$breakGlassPath = Join-Path $repositoryRoot 'Repair-Hermes-DesktopUpdater.ps1'

function Assert-Contract {
    param(
        [Parameter(Mandatory)][bool] $Condition,
        [Parameter(Mandatory)][string] $Message
    )
    if (-not $Condition) {
        throw $Message
    }
}

foreach ($path in @(
    $stackDrainPath,
    $activationLoaderPath,
    $activationCorePath,
    $breakGlassPath
)) {
    Assert-Contract (Test-Path -LiteralPath $path -PathType Leaf) "Missing updater recovery file: $path"
}

$loaderText = Get-Content -Raw -LiteralPath $activationLoaderPath
Assert-Contract ($loaderText -match 'DesktopUpdate-Activation-Core\.ps1') 'Activation loader omits the core implementation.'
Assert-Contract ($loaderText -match 'DesktopUpdate-StackDrain\.ps1') 'Activation loader omits the full stack drain.'

$stackText = Get-Content -Raw -LiteralPath $stackDrainPath
foreach ($required in @(
    'Get-HermesDesktopProtectedProcessIds',
    'Get-HermesDesktopOwnedProcesses',
    'Stop-HermesDesktopOwnedProcesses',
    'source and dependency synchronisation',
    'stopping-services',
    "Desktop update failed during stage",
    'Write-Output -NoEnumerate',
    'CommandLine',
    'ExecutablePath'
)) {
    Assert-Contract ($stackText -match [regex]::Escape($required)) "Stack-drain contract is missing: $required"
}

$breakGlassText = Get-Content -Raw -LiteralPath $breakGlassPath
foreach ($required in @(
    'recovery/desktop-updater-',
    "'stash', 'push', '--include-untracked'",
    'desktop-break-glass',
    'Stop-HermesRecoveryProcesses',
    'Repair-RecoveryGitState',
    'Promote-RecoveryLauncher',
    'Restoring previous launcher',
    'Recovery evidence:',
    'HERMES_DESKTOP_RECOVERY_RELOCATED',
    "'-SkipModel', '-SkipLlamaBuild', '-SkipLauncherBuild'"
)) {
    Assert-Contract ($breakGlassText -match [regex]::Escape($required)) "Break-glass contract is missing: $required"
}

# Load the process-drain layer against lightweight stubs. This exercises the
# same CIM matching and termination code used by the real updater without
# touching the repository checkout or a real Hermes process.
function Invoke-HermesDesktopUpdateStage { param([object] $Plan) }
function Request-HermesDesktopLauncherClose { param([object] $Plan) $false }
function Add-HermesDesktopUpdateLog { param([object] $Plan, [string] $Message) $null }
function Write-HermesDesktopUpdateProgress {
    param(
        [object] $Plan,
        [string] $Stage,
        [string] $Status,
        [string] $Message,
        $Percent,
        $Failure,
        $Result
    )
}
function Get-HermesDesktopObjectValue {
    param([object] $InputObject, [string] $Name, $Default = $null)
    $property = $InputObject.PSObject.Properties[$Name]
    if ($property) { return $property.Value }
    $Default
}

$tempRoot = Join-Path ([IO.Path]::GetTempPath()) (
    'hermes-updater-stack-drain-' + [guid]::NewGuid().ToString('N')
)
[IO.Directory]::CreateDirectory($tempRoot) | Out-Null
$script:root = $tempRoot
. $stackDrainPath

$childScript = Join-Path $tempRoot 'linger.ps1'
@'
param([Parameter(Mandatory)][string] $HermesRoot)
Start-Sleep -Seconds 120
'@ | Set-Content -LiteralPath $childScript -Encoding utf8

$pwsh = (Get-Command pwsh.exe -ErrorAction Stop).Source
$child = Start-Process `
    -FilePath $pwsh `
    -ArgumentList @(
        '-NoLogo', '-NoProfile', '-NonInteractive',
        '-File', $childScript,
        '-HermesRoot', $tempRoot
    ) `
    -WindowStyle Hidden `
    -PassThru

try {
    $deadline = (Get-Date).AddSeconds(10)
    $detected = $false
    while ((Get-Date) -lt $deadline) {
        $detected = @(
            Get-HermesDesktopOwnedProcesses |
                Where-Object ProcessId -eq $child.Id
        ).Count -eq 1
        if ($detected) { break }
        Start-Sleep -Milliseconds 100
    }
    Assert-Contract $detected 'The full-stack matcher did not detect a root-owned PowerShell child.'

    Stop-HermesDesktopOwnedProcesses `
        -Plan $null `
        -Reason 'contract test' `
        -GraceSeconds 0

    try {
        Wait-Process -Id $child.Id -Timeout 10 -ErrorAction SilentlyContinue
    } catch {
    }
    $child.Refresh()
    Assert-Contract $child.HasExited 'The full-stack drain did not terminate the root-owned PowerShell child.'
} finally {
    $child.Refresh()
    if (-not $child.HasExited) {
        Stop-Process -Id $child.Id -Force -ErrorAction SilentlyContinue
    }
    if (Test-Path -LiteralPath $tempRoot) {
        Remove-Item -LiteralPath $tempRoot -Recurse -Force
    }
}

Write-Host 'Hermes Desktop updater break-glass and full-stack drain tests passed.'
