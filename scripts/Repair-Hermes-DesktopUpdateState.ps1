[CmdletBinding()]
param(
    [string] $RepositoryRoot = (Split-Path $PSScriptRoot -Parent)
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

Import-Module (Join-Path $RepositoryRoot 'scripts\Common-Hermes.psm1') -Force

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

try {
    Assert-HermesRoot

    $root = [IO.Path]::GetFullPath($RepositoryRoot)
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
    $targetCommit = if ($pending) {
        [string]$pending.targetCommit
    } else {
        ''
    }
    $targetCommit = $targetCommit.Trim().ToLowerInvariant()

    $pendingDist = if ($pending) {
        [string]$pending.pendingDist
    } else {
        ''
    }

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
        exit 0
    }

    $promotionPid = if ($pending) {
        [int]($pending.promotionPid ?? 0)
    } else {
        0
    }
    $promotionStartedAt = if ($pending) {
        [string]$pending.promotionStartedAt
    } else {
        ''
    }
    $planPath = if ($pending) {
        [string]$pending.planPath
    } else {
        ''
    }

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
