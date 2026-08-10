[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repositoryRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$componentPath = Join-Path `
    $repositoryRoot `
    'scripts\desktop-update\DesktopUpdate-SafeActivation.ps1'

function Assert-ActivationContract {
    param([bool] $Condition, [string] $Message)
    if (-not $Condition) { throw $Message }
}

function Get-HermesDesktopObjectValue {
    param([object] $InputObject, [string] $Name, $Default = $null)
    $property = $InputObject.PSObject.Properties[$Name]
    if ($property) { return $property.Value }
    $Default
}

function Assert-HermesDesktopUpdatePath {
    param([string] $Root, [string] $Path, [string] $Description)
    $resolvedRoot = [IO.Path]::GetFullPath($Root).TrimEnd('\', '/') + '\'
    $resolvedPath = [IO.Path]::GetFullPath($Path)
    if (-not $resolvedPath.StartsWith($resolvedRoot, [StringComparison]::OrdinalIgnoreCase)) {
        throw "$Description is outside the test root."
    }
    $resolvedPath
}

function Write-HermesDesktopUpdateJson {
    param([string] $Path, [object] $Value)
    [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($Path)) | Out-Null
    [IO.File]::WriteAllText($Path, ($Value | ConvertTo-Json -Depth 32))
}

function Write-HermesDesktopUpdateProgress {
    param($Plan, $Stage, $Status, $Message, $Percent, $Failure, $Result)
}

function Wait-HermesDesktopLauncherExit { param($Plan) }
function Invoke-HermesDesktopProcess { param($FilePath, $Arguments, $Description) }
function Test-HermesDesktopLauncherRunning { $false }
function Get-HermesDesktopPendingUpdatePath { Join-Path $script:root 'data\runtime\pending.json' }

function Restore-HermesDesktopActivationBackup {
    param([string] $Dist, [string] $PendingDist, [string] $ActivationBackup)
    if (Test-Path -LiteralPath $Dist) {
        Remove-Item -LiteralPath $Dist -Recurse -Force
    }
    if (Test-Path -LiteralPath $ActivationBackup) {
        Move-Item -LiteralPath $ActivationBackup -Destination $Dist
    }
}

. $componentPath

$tempRoot = Join-Path ([IO.Path]::GetTempPath()) (
    'hermes-source-activation-' + [guid]::NewGuid().ToString('N')
)

function New-ActivationFixture {
    param([string] $Root, [string] $OperationId)

    $script:root = $Root
    $staging = Join-Path $Root "build\updates\desktop-staging\$OperationId"
    $dist = Join-Path $Root 'dist'
    $pendingDist = Join-Path $staging 'pending-dist'
    $activeSource = Join-Path $Root 'source\hermes-agent'
    $pendingSource = Join-Path $staging 'pending-source'
    foreach ($path in @($dist, $pendingDist, $activeSource, $pendingSource)) {
        [IO.Directory]::CreateDirectory($path) | Out-Null
    }
    [IO.Directory]::CreateDirectory((Join-Path $activeSource '.git')) | Out-Null
    [IO.Directory]::CreateDirectory((Join-Path $pendingSource '.git')) | Out-Null
    Copy-Item -LiteralPath $env:ComSpec -Destination (Join-Path $dist 'Hermes Launcher.exe')
    Copy-Item -LiteralPath $env:ComSpec -Destination (Join-Path $pendingDist 'Hermes Launcher.exe')
    Set-Content -LiteralPath (Join-Path $dist 'identity.txt') -Value 'old launcher'
    Set-Content -LiteralPath (Join-Path $pendingDist 'identity.txt') -Value 'new launcher'
    Set-Content -LiteralPath (Join-Path $activeSource 'identity.txt') -Value 'old source'
    Set-Content -LiteralPath (Join-Path $pendingSource 'identity.txt') -Value 'new source'

    [pscustomobject]@{
        schemaVersion = 1
        operationId = $OperationId
        root = $Root
        stagingRoot = $staging
        pendingDist = $pendingDist
        pendingSource = $pendingSource
        preserveNestedSource = $false
        resultPath = Join-Path $staging 'result.json'
        previousCommit = ('1' * 40)
        targetCommit = ('2' * 40)
    }
}

try {
    $successRoot = Join-Path $tempRoot 'success'
    $successPlan = New-ActivationFixture -Root $successRoot -OperationId 'success'
    function Invoke-HermesDesktopRuntimeSync {}
    $success = Promote-HermesDesktopPendingLauncher -Plan $successPlan
    Assert-ActivationContract `
        ((Get-Content -Raw -LiteralPath (Join-Path $successRoot 'source\hermes-agent\identity.txt')).Trim() -eq 'new source') `
        'Successful activation did not promote the prepared source.'
    Assert-ActivationContract ([bool]$success.nestedSourcePromoted) 'Source promotion was not reported.'
    Assert-ActivationContract `
        (-not (Test-Path -LiteralPath (Join-Path $successPlan.stagingRoot 'active-source-at-activation'))) `
        'Successful activation retained the obsolete source backup.'

    $failureRoot = Join-Path $tempRoot 'failure'
    $failurePlan = New-ActivationFixture -Root $failureRoot -OperationId 'failure'
    function Invoke-HermesDesktopRuntimeSync { throw 'controlled runtime failure' }
    $failed = $false
    try {
        Promote-HermesDesktopPendingLauncher -Plan $failurePlan | Out-Null
    } catch {
        $failed = $true
    }
    Assert-ActivationContract $failed 'The controlled activation failure did not fail.'
    Assert-ActivationContract `
        ((Get-Content -Raw -LiteralPath (Join-Path $failureRoot 'source\hermes-agent\identity.txt')).Trim() -eq 'old source') `
        'Failed activation did not restore the previous source.'
    Assert-ActivationContract `
        ((Get-Content -Raw -LiteralPath (Join-Path $failureRoot 'dist\identity.txt')).Trim() -eq 'old launcher') `
        'Failed activation did not restore the previous launcher.'

    Write-Host 'Desktop update source activation tests passed.'
} finally {
    if (Test-Path -LiteralPath $tempRoot) {
        Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}
