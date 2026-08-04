function Get-HermesDesktopPendingUpdatePath {
    [CmdletBinding()]
    param()

    Join-Path $root 'data\runtime\pending-desktop-update.json'
}

function Read-HermesDesktopPendingUpdate {
    [CmdletBinding()]
    param()

    $path = Get-HermesDesktopPendingUpdatePath
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        return $null
    }

    try {
        Get-Content -Raw -LiteralPath $path | ConvertFrom-Json -Depth 64
    } catch {
        $null
    }
}

function Get-HermesDesktopProcessStartTime {
    [CmdletBinding()]
    param([int] $ProcessId)

    if ($ProcessId -le 0) {
        return $null
    }

    try {
        (Get-Process -Id $ProcessId -ErrorAction Stop).StartTime.ToUniversalTime().ToString('o')
    } catch {
        $null
    }
}

function Test-HermesDesktopProcessIdentity {
    [CmdletBinding()]
    param(
        [int] $ProcessId,
        [string] $StartedAt
    )

    if ($ProcessId -le 0) {
        return $false
    }

    try {
        $process = Get-Process -Id $ProcessId -ErrorAction Stop
        if (-not $StartedAt) {
            return $true
        }

        $expected = [DateTimeOffset]::Parse($StartedAt).UtcDateTime
        $actual = $process.StartTime.ToUniversalTime()
        [math]::Abs(($actual - $expected).TotalSeconds) -lt 2
    } catch {
        $false
    }
}

function Resolve-HermesDesktopParentPid {
    [CmdletBinding()]
    param([int] $RequestedPid)

    if ($RequestedPid -gt 0) {
        return $RequestedPid
    }

    $launcher = [IO.Path]::GetFullPath((Join-Path $root 'dist\Hermes Launcher.exe'))
    foreach ($process in @(Get-Process -Name 'Hermes Launcher' -ErrorAction SilentlyContinue)) {
        try {
            if (
                $process.Path -and
                [IO.Path]::GetFullPath($process.Path) -eq $launcher
            ) {
                return [int]$process.Id
            }
        } catch {
        }
    }

    0
}

function Copy-HermesDesktopDirectory {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $Source,
        [Parameter(Mandatory)][string] $Destination
    )

    if (-not (Test-Path -LiteralPath $Source -PathType Container)) {
        throw "Required launcher directory is missing: $Source"
    }

    if (Test-Path -LiteralPath $Destination) {
        Remove-Item -LiteralPath $Destination -Recurse -Force
    }
    [IO.Directory]::CreateDirectory($Destination) | Out-Null
    Get-ChildItem -LiteralPath $Source -Force |
        Copy-Item -Destination $Destination -Recurse -Force
}

function Invoke-HermesDesktopProcess {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $FilePath,
        [Parameter(Mandatory)][string[]] $Arguments,
        [Parameter(Mandatory)][string] $Description
    )

    & $FilePath @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$Description failed with exit code $LASTEXITCODE."
    }
}

function Invoke-HermesDesktopSetup {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string] $Description)

    Invoke-HermesDesktopProcess -FilePath 'pwsh.exe' -Arguments @(
        '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
        '-File', (Join-Path $root 'Setup-Hermes-Local.ps1'),
        '-SkipModel', '-SkipLlamaBuild', '-SkipLauncherBuild', '-NonInteractive'
    ) -Description $Description
}

function Copy-HermesDesktopUpdateRuntime {
    [CmdletBinding()]
    param([Parameter(Mandatory)][object] $Plan)

    $runtimeRoot = Join-Path ([string]$Plan.stagingRoot) 'promotion-helper'
    [IO.Directory]::CreateDirectory($runtimeRoot) | Out-Null

    $scriptPath = Join-Path $runtimeRoot 'Invoke-Hermes-DesktopUpdate.ps1'
    $moduleDestination = Join-Path $runtimeRoot 'Hermes-DesktopUpdate.psm1'
    $partsDestination = Join-Path $runtimeRoot 'desktop-update'
    Copy-Item -LiteralPath $script:desktopUpdateEntryScript -Destination $scriptPath -Force
    Copy-Item `
        -LiteralPath (Join-Path $root 'scripts\Hermes-DesktopUpdate.psm1') `
        -Destination $moduleDestination `
        -Force
    [IO.Directory]::CreateDirectory($partsDestination) | Out-Null
    Get-ChildItem -LiteralPath $script:desktopUpdatePartsRoot -Filter '*.ps1' -File |
        Copy-Item -Destination $partsDestination -Force

    [pscustomobject]@{
        Root = $runtimeRoot
        Script = $scriptPath
        Module = $moduleDestination
    }
}

function Set-HermesDesktopPlanParent {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][object] $Plan,
        [int] $ProcessId
    )

    $resolvedPid = Resolve-HermesDesktopParentPid -RequestedPid $ProcessId
    $startedAt = Get-HermesDesktopProcessStartTime -ProcessId $resolvedPid

    $Plan | Add-Member -NotePropertyName parentPid -NotePropertyValue $resolvedPid -Force
    $Plan | Add-Member -NotePropertyName parentStartedAt -NotePropertyValue $startedAt -Force
    Write-HermesDesktopUpdateJson -Path ([string]$Plan.planPath) -Value $Plan
    $Plan
}

function New-HermesDesktopPendingUpdateRecord {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][object] $Plan,
        [int] $PromotionPid = 0
    )

    [ordered]@{
        schemaVersion = 1
        operationId = [string]$Plan.operationId
        status = 'ready-to-restart'
        previousCommit = [string]$Plan.previousCommit
        targetCommit = [string]$Plan.targetCommit
        channel = [string]$Plan.channel
        planPath = [string]$Plan.planPath
        pendingDist = [string]$Plan.pendingDist
        helperScript = [string]$Plan.helperScript
        parentPid = if ($Plan.PSObject.Properties['parentPid']) { [int]$Plan.parentPid } else { 0 }
        parentStartedAt = if (
            $Plan.PSObject.Properties['parentStartedAt'] -and
            $Plan.parentStartedAt
        ) {
            [string]$Plan.parentStartedAt
        } else {
            $null
        }
        promotionPid = if ($PromotionPid -gt 0) { $PromotionPid } else { $null }
        stagedAt = (Get-Date).ToUniversalTime().ToString('o')
        activationDeferred = $true
        relaunchOnActivation = $false
    }
}

function Write-HermesDesktopPendingUpdate {
    [CmdletBinding()]
    param([Parameter(Mandatory)][object] $Value)

    Write-HermesDesktopUpdateJson `
        -Path (Get-HermesDesktopPendingUpdatePath) `
        -Value $Value
}

function Start-HermesDesktopPromotionHelper {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][object] $Plan,
        [int] $ProcessId
    )

    $null = Set-HermesDesktopPlanParent -Plan $Plan -ProcessId $ProcessId
    $helperScript = [string]$Plan.helperScript
    $helperRoot = [IO.Path]::GetDirectoryName($helperScript)

    if (-not (Test-Path -LiteralPath $helperScript -PathType Leaf)) {
        throw "Deferred launcher promotion helper is missing: $helperScript"
    }

    $pending = New-HermesDesktopPendingUpdateRecord -Plan $Plan
    Write-HermesDesktopPendingUpdate -Value $pending

    $pwsh = (Get-Process -Id $PID -ErrorAction Stop).Path
    $arguments = @(
        '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
        '-File', $helperScript,
        '-Mode', 'Promote',
        '-PlanPath', [string]$Plan.planPath,
        '-NonInteractive'
    )
    $process = Start-Process `
        -FilePath $pwsh `
        -ArgumentList $arguments `
        -WorkingDirectory $helperRoot `
        -WindowStyle Hidden `
        -PassThru

    $pending['promotionPid'] = [int]$process.Id
    Write-HermesDesktopPendingUpdate -Value $pending
    [pscustomobject]$pending
}

function Ensure-HermesDesktopPromotionHelper {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][object] $Pending,
        [int] $ProcessId
    )

    $promotionPid = if ($Pending.PSObject.Properties['promotionPid']) {
        [int]$Pending.promotionPid
    } else {
        0
    }

    if (
        (Test-HermesDesktopProcessIdentity -ProcessId $promotionPid -StartedAt '') -and
        (Test-Path -LiteralPath ([string]$Pending.pendingDist) -PathType Container)
    ) {
        return $Pending
    }

    if (
        -not $Pending.planPath -or
        -not (Test-Path -LiteralPath ([string]$Pending.planPath) -PathType Leaf)
    ) {
        return $Pending
    }

    $plan = Get-Content -Raw -LiteralPath ([string]$Pending.planPath) |
        ConvertFrom-Json -Depth 64
    Start-HermesDesktopPromotionHelper -Plan $plan -ProcessId $ProcessId
}

function Get-HermesDesktopUpdateStatus {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $RequestedChannel,
        [string] $RequestedCommit,
        [int] $LauncherPid
    )

    if (-not (Test-Path -LiteralPath (Join-Path $root '.git') -PathType Container)) {
        throw 'Hermes Local self-update requires a Git installation checkout.'
    }

    $origin = (Invoke-HermesDesktopGit -Arguments @(
        'remote', 'get-url', 'origin'
    )).Text
    if (-not (Test-HermesDesktopUpdateOrigin -Origin $origin)) {
        throw "Refusing updates from unexpected origin '$origin'."
    }

    $pending = Read-HermesDesktopPendingUpdate
    if (
        $pending -and
        $pending.pendingDist -and
        (Test-Path -LiteralPath ([string]$pending.pendingDist) -PathType Container)
    ) {
        try {
            $pending = Ensure-HermesDesktopPromotionHelper `
                -Pending $pending `
                -ProcessId $LauncherPid
        } catch {
        }

        $versionPath = Join-Path $root 'VERSION.json'
        $version = if (Test-Path -LiteralPath $versionPath -PathType Leaf) {
            Get-Content -Raw -LiteralPath $versionPath | ConvertFrom-Json -Depth 32
        } else {
            $null
        }

        return [ordered]@{
            supported = $true
            branch = [string]$pending.channel
            localBranch = (
                Invoke-HermesDesktopGit -Arguments @(
                    'branch', '--show-current'
                ) -AllowFailure
            ).Text
            channel = [string]$pending.channel
            currentVersion = if ($version) {
                [string]$version.product.version
            } else {
                ([string]$pending.previousCommit).Substring(0, 12)
            }
            currentSha = [string]$pending.previousCommit
            targetSha = [string]$pending.targetCommit
            behind = 0
            updateAvailable = $false
            dirty = [bool](Get-HermesDesktopWorkingTreeChanges)
            autoStash = $true
            restartRequired = $true
            pendingActivation = $true
            launcherStaysOpen = $true
            commits = @()
            releaseNotes = $null
            message = 'Update ready. Close and reopen Hermes Launcher when convenient to activate it.'
            promotionPid = if ($pending.PSObject.Properties['promotionPid'] -and $pending.promotionPid) { [int]$pending.promotionPid } else { $null }
            fetchedAt = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
        }
    }

    $current = (Invoke-HermesDesktopGit -Arguments @(
        'rev-parse', 'HEAD'
    )).Text.ToLowerInvariant()
    $branch = (Invoke-HermesDesktopGit -Arguments @(
        'branch', '--show-current'
    ) -AllowFailure).Text
    $workingTreeChanges = Get-HermesDesktopWorkingTreeChanges
    $target = Get-HermesDesktopUpdateTarget `
        -RequestedChannel $RequestedChannel `
        -RequestedCommit $RequestedCommit

    $fetch = Invoke-HermesDesktopGit -Arguments @(
        'fetch', '--no-tags', 'origin', $target.Commit
    ) -AllowFailure
    if ($fetch.ExitCode -ne 0) {
        throw "Could not download update metadata for $($target.Commit). $($fetch.Text)"
    }

    $ancestor = Invoke-HermesDesktopGit -Arguments @(
        'merge-base', '--is-ancestor', $current, $target.Commit
    ) -AllowFailure
    $behind = if ($current -eq $target.Commit) {
        0
    } elseif ($ancestor.ExitCode -eq 0) {
        [int](Invoke-HermesDesktopGit -Arguments @(
            'rev-list', '--count', "$current..$($target.Commit)"
        )).Text
    } else {
        1
    }

    $versionPath = Join-Path $root 'VERSION.json'
    $version = if (Test-Path -LiteralPath $versionPath -PathType Leaf) {
        Get-Content -Raw -LiteralPath $versionPath | ConvertFrom-Json -Depth 32
    } else {
        $null
    }

    [ordered]@{
        supported = $true
        branch = $target.Branch
        localBranch = $branch
        channel = $RequestedChannel
        currentVersion = if ($version) {
            [string]$version.product.version
        } else {
            $current.Substring(0, 12)
        }
        currentSha = $current
        targetSha = $target.Commit
        behind = $behind
        updateAvailable = $current -ne $target.Commit
        dirty = [bool]$workingTreeChanges
        autoStash = $true
        restartRequired = $current -ne $target.Commit
        pendingActivation = $false
        launcherStaysOpen = $true
        commits = @()
        releaseNotes = if ($target.Release) {
            "https://github.com/xdCloudy/Hermes-Local/releases/tag/$($target.Release)"
        } else {
            "https://github.com/xdCloudy/Hermes-Local/compare/$current...$($target.Commit)"
        }
        message = if ($current -eq $target.Commit) {
            'Hermes Local is up to date.'
        } elseif ($workingTreeChanges) {
            'An update is available. Local source changes will be stashed automatically and restored afterwards.'
        } else {
            'An update is available and can be prepared without closing Hermes Launcher.'
        }
        fetchedAt = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
    }
}
