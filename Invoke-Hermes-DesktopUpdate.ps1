[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidateSet('Check', 'Apply', 'Rollback', 'Helper', 'Promote')]
    [string] $Mode,

    [ValidateSet('development', 'stable', 'beta', 'pinned')]
    [string] $Channel = 'development',

    [ValidatePattern('^[0-9a-fA-F]{40}$')]
    [string] $TargetCommit,

    [ValidateRange(0, 2147483647)]
    [int] $ParentPid = 0,

    [string] $PlanPath,

    [switch] $NonInteractive
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$rootHint = if ($PlanPath -and (Test-Path -LiteralPath $PlanPath -PathType Leaf)) {
    [string](Get-Content -Raw -LiteralPath $PlanPath | ConvertFrom-Json -Depth 64).root
} elseif ($env:HERMES_LOCAL_ROOT) {
    [string]$env:HERMES_LOCAL_ROOT
} else {
    $PSScriptRoot
}
$root = [IO.Path]::GetFullPath($rootHint)

$localHelperModule = Join-Path $PSScriptRoot 'Hermes-DesktopUpdate.psm1'
$modulePath = if (
    $Mode -in @('Helper', 'Promote') -and
    (Test-Path -LiteralPath $localHelperModule -PathType Leaf)
) {
    $localHelperModule
} else {
    Join-Path $root 'scripts\Hermes-DesktopUpdate.psm1'
}
Import-Module $modulePath -Force
$script:desktopUpdateEntryScript = $PSCommandPath

$localPartsRoot = Join-Path $PSScriptRoot 'desktop-update'
$script:desktopUpdatePartsRoot = if (Test-Path -LiteralPath $localPartsRoot -PathType Container) {
    $localPartsRoot
} else {
    Join-Path $root 'scripts\desktop-update'
}
foreach ($part in @(
    'DesktopUpdate-Git.ps1',
    'DesktopUpdate-State.ps1',
    'DesktopUpdate-Promotion.ps1',
    'DesktopUpdate-Stage.ps1',
    'DesktopUpdate-SafeActivation.ps1'
)) {
    $partPath = Join-Path $script:desktopUpdatePartsRoot $part
    if (-not (Test-Path -LiteralPath $partPath -PathType Leaf)) {
        throw "Hermes Desktop update component is missing: $partPath"
    }
    . $partPath
}

try {
    if ($Mode -eq 'Promote') {
        if (-not $PlanPath) {
            throw 'Promote mode requires -PlanPath.'
        }

        $plan = Get-Content -Raw -LiteralPath (
            [IO.Path]::GetFullPath($PlanPath)
        ) | ConvertFrom-Json -Depth 64
        $script:root = [IO.Path]::GetFullPath([string]$plan.root)

        try {
            Promote-HermesDesktopPendingLauncher -Plan $plan
            exit 0
        } catch {
            Write-HermesDesktopUpdateProgress `
                -Plan $plan `
                -Stage activation-failed `
                -Status failed `
                -Message 'The update was prepared, but could not be activated after Hermes Launcher closed.' `
                -Percent 100 `
                -Failure ([ordered]@{
                    code = 'desktop-update-activation-failed'
                    message = $_.Exception.Message
                }) `
                -Result $null | Out-Null
            exit 1
        }
    }

    if ($Mode -eq 'Helper') {
        if (-not $PlanPath) {
            throw 'Helper mode requires -PlanPath.'
        }

        $plan = Get-Content -Raw -LiteralPath (
            [IO.Path]::GetFullPath($PlanPath)
        ) | ConvertFrom-Json -Depth 64
        $script:root = [IO.Path]::GetFullPath([string]$plan.root)
        $null = Invoke-HermesDesktopUpdateStage -Plan $plan
        exit 0
    }

    if (-not (Test-Path -LiteralPath $modulePath -PathType Leaf)) {
        throw 'Hermes Desktop update support is not installed.'
    }

    if ($Mode -eq 'Check') {
        try {
            $status = Get-HermesDesktopUpdateStatus `
                -RequestedChannel $Channel `
                -RequestedCommit $TargetCommit `
                -LauncherPid $ParentPid
        } catch {
            $status = [ordered]@{
                supported = $true
                branch = $Channel
                error = 'check-failed'
                message = $_.Exception.Message
                fetchedAt = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
            }
        }

        Write-Output (
            ConvertTo-HermesDesktopUpdateMarker -Name status -Value $status
        )
        exit 0
    }

    $existingPending = Read-HermesDesktopPendingUpdate
    if (
        $existingPending -and
        $existingPending.pendingDist -and
        (Test-Path -LiteralPath ([string]$existingPending.pendingDist) -PathType Container)
    ) {
        try {
            $existingPending = Ensure-HermesDesktopPromotionHelper `
                -Pending $existingPending `
                -ProcessId $ParentPid
        } catch {
        }

        $result = [ordered]@{
            ok = $true
            updated = $true
            status = 'ready-to-restart'
            pendingActivation = $true
            restartRequired = $true
            launcherStayedOpen = $true
            message = 'An update is already ready. Close and reopen Hermes Launcher when convenient to activate it.'
        }
        Write-Output (
            ConvertTo-HermesDesktopUpdateMarker -Name result -Value $result
        )
        exit 0
    }

    if ($Mode -eq 'Rollback') {
        $latest = Get-ChildItem `
            -LiteralPath (Join-Path $root 'build\updates\desktop-staging') `
            -Directory `
            -ErrorAction SilentlyContinue |
            Sort-Object LastWriteTimeUtc -Descending |
            Where-Object {
                Test-Path -LiteralPath (Join-Path $_.FullName 'plan.json') -PathType Leaf
            } |
            Select-Object -First 1

        if (-not $latest) {
            throw 'No staged Hermes Local Desktop update is available to roll back.'
        }

        $prior = Get-Content -Raw -LiteralPath (
            Join-Path $latest.FullName 'plan.json'
        ) | ConvertFrom-Json -Depth 64

        if (
            -not $prior.previousDist -or
            -not (Test-Path -LiteralPath ([string]$prior.previousDist) -PathType Container)
        ) {
            throw 'The previous known-good launcher snapshot is unavailable.'
        }

        $current = (Invoke-HermesDesktopGit -Arguments @(
            'rev-parse', 'HEAD'
        )).Text.ToLowerInvariant()
        $branch = (Invoke-HermesDesktopGit -Arguments @(
            'branch', '--show-current'
        ) -AllowFailure).Text

        $plan = New-HermesDesktopPreparedPlan `
            -CurrentCommit $current `
            -RequestedTargetCommit ([string]$prior.previousCommit) `
            -RequestedChannel ([string]$prior.channel) `
            -CurrentBranch $branch `
            -LauncherPid $ParentPid `
            -TaskId $env:HERMES_LOCAL_TASK_ID `
            -RollbackOnly `
            -PreviousDist ([string]$prior.previousDist)
    } else {
        $status = Get-HermesDesktopUpdateStatus `
            -RequestedChannel $Channel `
            -RequestedCommit $TargetCommit `
            -LauncherPid $ParentPid

        if (-not [bool]$status.updateAvailable) {
            $result = [ordered]@{
                ok = $true
                updated = $false
                status = if ($status.pendingActivation) {
                    'ready-to-restart'
                } else {
                    'up-to-date'
                }
                pendingActivation = [bool]$status.pendingActivation
                restartRequired = [bool]$status.restartRequired
                launcherStayedOpen = $true
                message = [string]$status.message
            }
            Write-Output (
                ConvertTo-HermesDesktopUpdateMarker -Name result -Value $result
            )
            exit 0
        }

        Assert-HermesDesktopUpdateDiskSpace -Root $root | Out-Null
        $plan = New-HermesDesktopPreparedPlan `
            -CurrentCommit ([string]$status.currentSha) `
            -RequestedTargetCommit ([string]$status.targetSha) `
            -RequestedChannel $Channel `
            -CurrentBranch ([string]$status.localBranch) `
            -LauncherPid $ParentPid `
            -TaskId $env:HERMES_LOCAL_TASK_ID
    }

    Write-HermesDesktopUpdateProgress `
        -Plan $plan `
        -Stage preparing `
        -Status staged `
        -Message 'Preparing the update in the background. Hermes Launcher will remain open.' `
        -Percent 0 `
        -Failure $null `
        -Result $null | Out-Null

    $stageOutput = @(Invoke-HermesDesktopUpdateStage -Plan $plan)
    $stageResult = @(
        $stageOutput |
            Where-Object {
                $null -ne $_ -and
                $null -ne (Get-HermesDesktopObjectValue `
                    -InputObject $_ `
                    -Name status `
                    -Default $null)
            }
    ) | Select-Object -Last 1

    if (-not $stageResult) {
        throw 'Desktop update staging did not return a structured result.'
    }

    $activationDeferred = [bool](Get-HermesDesktopObjectValue `
        -InputObject $stageResult `
        -Name activationDeferred `
        -Default $false)

    $result = [ordered]@{
        ok = $true
        updated = $true
        pendingActivation = $activationDeferred
        message = 'Update ready. Close and reopen Hermes Launcher when convenient to activate it.'
    }
    foreach ($property in $stageResult.PSObject.Properties) {
        $result[$property.Name] = $property.Value
    }
    Write-Output (
        ConvertTo-HermesDesktopUpdateMarker -Name result -Value $result
    )
    exit 0
} catch {
    $result = [ordered]@{
        ok = $false
        error = 'desktop-update-failed'
        message = $_.Exception.Message
        launcherStayedOpen = Test-HermesDesktopProcessIdentity `
            -ProcessId $ParentPid `
            -StartedAt ''
    }
    Write-Output (
        ConvertTo-HermesDesktopUpdateMarker -Name result -Value $result
    )
    exit 1
}
