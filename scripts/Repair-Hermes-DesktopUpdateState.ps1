[CmdletBinding()]
param(
    [string] $RepositoryRoot = (Split-Path $PSScriptRoot -Parent)
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

Import-Module (Join-Path $RepositoryRoot 'scripts\Common-Hermes.psm1') -Force

function Get-PendingValue {
    [CmdletBinding()]
    param(
        [AllowNull()][object] $InputObject,
        [Parameter(Mandatory)][string] $Name,
        $Default = $null
    )

    if ($null -eq $InputObject) {
        return $Default
    }

    if (
        $InputObject -is [System.Collections.IDictionary] -and
        $InputObject.Contains($Name)
    ) {
        return $InputObject[$Name]
    }

    $property = $InputObject.PSObject.Properties[$Name]
    if ($property) {
        return $property.Value
    }

    $Default
}

function Test-HermesPendingPromotionProcess {
    [CmdletBinding()]
    param(
        [int] $ProcessId,
        [string] $StartedAt,
        [string] $PlanPath
    )

    if ($ProcessId -le 0) {
        return $false
    }

    try {
        $process = Get-Process -Id $ProcessId -ErrorAction Stop
        if ($StartedAt) {
            $expected = [DateTimeOffset]::Parse($StartedAt).UtcDateTime
            $actual = $process.StartTime.ToUniversalTime()
            if ([math]::Abs(($actual - $expected).TotalSeconds) -ge 2) {
                return $false
            }
        }

        $cim = Get-CimInstance Win32_Process `
            -Filter "ProcessId = $ProcessId" `
            -ErrorAction Stop
        $commandLine = [string]$cim.CommandLine

        if (
            $commandLine -notmatch 'Invoke-Hermes-DesktopUpdate\.ps1' -or
            $commandLine -notmatch '(?i)-Mode\s+Promote'
        ) {
            return $false
        }

        if (
            $PlanPath -and
            $commandLine -notlike "*$PlanPath*"
        ) {
            return $false
        }

        $true
    } catch {
        $false
    }
}

function Wait-HermesPendingActivationResolution {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $PendingPath,
        [ValidateRange(1, 300)][int] $TimeoutSeconds = 150
    )

    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while (
        (Test-Path -LiteralPath $PendingPath -PathType Leaf) -and
        (Get-Date) -lt $deadline
    ) {
        Start-Sleep -Milliseconds 250
    }

    -not (Test-Path -LiteralPath $PendingPath -PathType Leaf)
}

function Invoke-HermesPendingActivationRecovery {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $Root,
        [Parameter(Mandatory)][string] $PendingPath,
        [Parameter(Mandatory)][object] $Pending
    )

    $planPath = [string](
        Get-PendingValue -InputObject $Pending -Name planPath -Default ''
    )
    if (-not $planPath -or -not (Test-Path -LiteralPath $planPath -PathType Leaf)) {
        throw 'Pending Desktop activation has no recoverable plan.'
    }

    $promotionPid = [int](
        Get-PendingValue -InputObject $Pending -Name promotionPid -Default 0
    )
    $promotionStartedAt = [string](
        Get-PendingValue -InputObject $Pending -Name promotionStartedAt -Default ''
    )

    if (
        Test-HermesPendingPromotionProcess `
            -ProcessId $promotionPid `
            -StartedAt $promotionStartedAt `
            -PlanPath $planPath
    ) {
        # The staged helper may have been copied from an older launcher build.
        # Stop it and recover through the currently installed updater code so
        # process-drain and failure-state fixes apply to old pending updates too.
        Stop-Process -Id $promotionPid -Force -ErrorAction Stop
        try {
            Wait-Process -Id $promotionPid -Timeout 10 -ErrorAction SilentlyContinue
        } catch {
        }
    }

    $lockPath = Join-Path $Root 'data\runtime\locks\desktop-activation-recovery.lock'
    [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($lockPath)) | Out-Null
    $lockStream = $null
    try {
        try {
            $lockStream = [IO.File]::Open(
                $lockPath,
                [IO.FileMode]::OpenOrCreate,
                [IO.FileAccess]::ReadWrite,
                [IO.FileShare]::None
            )
        } catch [IO.IOException] {
            if (Wait-HermesPendingActivationResolution -PendingPath $PendingPath) {
                return $true
            }
            throw 'Another Desktop activation recovery remained active for too long.'
        }

        $updater = Join-Path $Root 'Invoke-Hermes-DesktopUpdate.ps1'
        if (-not (Test-Path -LiteralPath $updater -PathType Leaf)) {
            throw "Desktop updater is missing: $updater"
        }

        $pwsh = (Get-Command pwsh.exe -ErrorAction Stop).Source
        $output = @(
            & $pwsh `
                -NoLogo `
                -NoProfile `
                -NonInteractive `
                -ExecutionPolicy Bypass `
                -File $updater `
                -Mode Promote `
                -PlanPath $planPath `
                -NonInteractive 2>&1 |
                ForEach-Object { [string]$_ }
        )
        $exitCode = $LASTEXITCODE

        if (
            $exitCode -ne 0 -or
            (Test-Path -LiteralPath $PendingPath -PathType Leaf)
        ) {
            $detail = ($output | Select-Object -Last 20) -join [Environment]::NewLine
            throw (
                "Automatic Desktop activation recovery failed with exit code $exitCode." +
                $(if ($detail) { "`n$detail" } else { '' })
            )
        }

        Write-HermesLog -Component launcher -Level INFO -Message (
            "Recovered pending Desktop activation through current updater code. Plan: $planPath"
        )
        $true
    } finally {
        if ($lockStream) {
            $lockStream.Dispose()
        }
    }
}

try {
    $root = [IO.Path]::GetFullPath($RepositoryRoot)
    Push-Location $root
    try {
        Assert-HermesRoot
    } finally {
        Pop-Location
    }

    $pendingPath = Join-Path $root 'data\runtime\pending-desktop-update.json'

    if (-not (Test-Path -LiteralPath $pendingPath -PathType Leaf)) {
        exit 0
    }

    $pending = try {
        Get-Content -Raw -LiteralPath $pendingPath |
            ConvertFrom-Json -Depth 64
    } catch {
        $null
    }

    $currentOutput = @(
        & git -C $root rev-parse HEAD 2>&1 |
            ForEach-Object { [string]$_ }
    )
    if ($LASTEXITCODE -ne 0) {
        throw (
            'Could not resolve the installed Hermes Local revision: ' +
            ($currentOutput -join [Environment]::NewLine)
        )
    }

    $currentCommit = ($currentOutput | Select-Object -Last 1).Trim().ToLowerInvariant()
    $targetCommit = [string](
        Get-PendingValue -InputObject $pending -Name targetCommit -Default ''
    )
    $targetCommit = $targetCommit.Trim().ToLowerInvariant()

    $pendingDist = [string](
        Get-PendingValue -InputObject $pending -Name pendingDist -Default ''
    )

    $validPending =
        $pending -and
        $targetCommit -match '^[0-9a-f]{40}$' -and
        $currentCommit -eq $targetCommit -and
        $pendingDist -and
        (Test-Path -LiteralPath $pendingDist -PathType Container) -and
        (Test-Path `
            -LiteralPath (Join-Path $pendingDist 'Hermes Launcher.exe') `
            -PathType Leaf)

    if ($validPending) {
        $null = Invoke-HermesPendingActivationRecovery `
            -Root $root `
            -PendingPath $pendingPath `
            -Pending $pending
        exit 0
    }

    $promotionPid = [int](
        Get-PendingValue -InputObject $pending -Name promotionPid -Default 0
    )
    $promotionStartedAt = [string](
        Get-PendingValue -InputObject $pending -Name promotionStartedAt -Default ''
    )
    $planPath = [string](
        Get-PendingValue -InputObject $pending -Name planPath -Default ''
    )

    if (
        Test-HermesPendingPromotionProcess `
            -ProcessId $promotionPid `
            -StartedAt $promotionStartedAt `
            -PlanPath $planPath
    ) {
        Stop-Process -Id $promotionPid -Force -ErrorAction Stop
    }

    $stamp = (Get-Date).ToUniversalTime().ToString('yyyyMMddTHHmmssfffZ')
    $archivePath = "$pendingPath.stale-$stamp"
    Move-Item `
        -LiteralPath $pendingPath `
        -Destination $archivePath `
        -Force

    Write-HermesLog -Component launcher -Level WARN -Message (
        "Archived stale Desktop update state. Installed revision: $currentCommit; " +
        "pending target: $(if ($targetCommit) { $targetCommit } else { '<invalid>' }); " +
        "state: $archivePath"
    )
    Write-Host 'Removed stale pending Desktop update state.'
    exit 0
} catch {
    Write-HermesLog -Component launcher -Level ERROR -Message $_.Exception.ToString()
    Write-Host "Desktop update-state repair failed: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
}
