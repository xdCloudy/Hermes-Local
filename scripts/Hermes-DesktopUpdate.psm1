Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Write-HermesDesktopUpdateJson {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $Path,
        [Parameter(Mandatory)][object] $Value
    )

    $directory = [System.IO.Path]::GetDirectoryName([System.IO.Path]::GetFullPath($Path))
    [System.IO.Directory]::CreateDirectory($directory) | Out-Null
    $temporary = "$Path.$PID.$([guid]::NewGuid().ToString('N')).tmp"
    [System.IO.File]::WriteAllText(
        $temporary,
        (($Value | ConvertTo-Json -Depth 64) + [Environment]::NewLine),
        [System.Text.UTF8Encoding]::new($false)
    )
    [System.IO.File]::Move($temporary, $Path, $true)
}

function ConvertTo-HermesDesktopUpdateMarker {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][ValidateSet('status', 'helper', 'result')][string] $Name,
        [Parameter(Mandatory)][object] $Value
    )

    $json = $Value | ConvertTo-Json -Depth 64 -Compress
    $encoded = [Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes($json))
    "::hermes-desktop-update-$Name::$encoded"
}

function ConvertFrom-HermesDesktopUpdateMarker {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][ValidateSet('status', 'helper', 'result')][string] $Name,
        [Parameter(Mandatory)][string] $Text
    )

    $pattern = "::hermes-desktop-update-$([regex]::Escape($Name))::([A-Za-z0-9+/=]+)"
    $match = [regex]::Matches($Text, $pattern) | Select-Object -Last 1
    if (-not $match) { return $null }

    try {
        $json = [System.Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($match.Groups[1].Value))
        return $json | ConvertFrom-Json -Depth 64
    } catch {
        throw "Hermes Desktop update marker '$Name' is malformed. $($_.Exception.Message)"
    }
}

function Assert-HermesDesktopUpdatePath {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $Root,
        [Parameter(Mandatory)][string] $Path,
        [string] $Description = 'Update path'
    )

    $resolvedRoot = [System.IO.Path]::GetFullPath($Root).TrimEnd('\', '/') + [System.IO.Path]::DirectorySeparatorChar
    $resolvedPath = [System.IO.Path]::GetFullPath($Path)
    if (-not $resolvedPath.StartsWith($resolvedRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "$Description is outside the Hermes Local root: $resolvedPath"
    }
    $resolvedPath
}

function Test-HermesDesktopUpdateOrigin {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string] $Origin)

    $normalized = $Origin.Trim().TrimEnd('/')
    return [bool](
        $normalized -match '^https://github\.com/xdCloudy/Hermes-Local(?:\.git)?$' -or
        $normalized -match '^git@github\.com:xdCloudy/Hermes-Local(?:\.git)?$'
    )
}

function New-HermesDesktopUpdatePlan {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $Root,
        [Parameter(Mandatory)][string] $CurrentCommit,
        [Parameter(Mandatory)][string] $TargetCommit,
        [Parameter(Mandatory)][string] $Channel,
        [string] $CurrentBranch,
        [Parameter(Mandatory)][int] $ParentPid,
        [string] $TaskId,
        [switch] $RollbackOnly
    )

    foreach ($commit in @($CurrentCommit, $TargetCommit)) {
        if ($commit -notmatch '^[0-9a-fA-F]{40}$') {
            throw 'Desktop update plans require full 40-character Git commit identities.'
        }
    }
    if ($ParentPid -lt 0) { throw 'ParentPid cannot be negative.' }

    $operationId = [guid]::NewGuid().ToString('N')
    $stagingRoot = Assert-HermesDesktopUpdatePath -Root $Root -Path (Join-Path $Root "build\updates\desktop-staging\$operationId") -Description 'Staging root'
    [ordered]@{
        schemaVersion = 1
        operationId = $operationId
        taskId = if ($TaskId) { $TaskId } else { $null }
        requestedAt = (Get-Date).ToUniversalTime().ToString('o')
        root = [System.IO.Path]::GetFullPath($Root)
        stagingRoot = $stagingRoot
        previousCommit = $CurrentCommit.ToLowerInvariant()
        targetCommit = $TargetCommit.ToLowerInvariant()
        channel = $Channel
        previousBranch = if ($CurrentBranch) { $CurrentBranch } else { $null }
        parentPid = $ParentPid
        rollbackOnly = [bool]$RollbackOnly
        launcherPath = Join-Path ([System.IO.Path]::GetFullPath($Root)) 'dist\Hermes Launcher.exe'
        previousDist = Join-Path $stagingRoot 'previous-dist'
        progressPath = Join-Path $stagingRoot 'progress.json'
        resultPath = Join-Path $stagingRoot 'result.json'
        logPath = Join-Path $stagingRoot 'desktop-self-update.log'
    }
}

function Get-HermesDesktopUpdateRequiredBytes {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string] $Root)

    $dist = Join-Path $Root 'dist'
    $distBytes = if (Test-Path -LiteralPath $dist -PathType Container) {
        [long](Get-ChildItem -LiteralPath $dist -Recurse -File -ErrorAction SilentlyContinue |
            Measure-Object -Property Length -Sum).Sum
    } else { 0L }
    [long][math]::Max(2GB, ($distBytes * 3) + 512MB)
}

function Assert-HermesDesktopUpdateDiskSpace {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $Root,
        [long] $RequiredBytes = 0
    )

    if ($RequiredBytes -le 0) { $RequiredBytes = Get-HermesDesktopUpdateRequiredBytes -Root $Root }
    $driveRoot = [System.IO.Path]::GetPathRoot([System.IO.Path]::GetFullPath($Root))
    $drive = [System.IO.DriveInfo]::new($driveRoot)
    if ($drive.AvailableFreeSpace -lt $RequiredBytes) {
        throw "Insufficient disk space for the staged update. Required $([math]::Round($RequiredBytes / 1GB, 2)) GiB; available $([math]::Round($drive.AvailableFreeSpace / 1GB, 2)) GiB."
    }
    [pscustomobject]@{ RequiredBytes = $RequiredBytes; AvailableBytes = $drive.AvailableFreeSpace }
}

function Write-HermesDesktopUpdateProgress {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][object] $Plan,
        [Parameter(Mandatory)][string] $Stage,
        [Parameter(Mandatory)][string] $Status,
        [Parameter(Mandatory)][string] $Message,
        [AllowNull()][double] $Percent,
        [AllowNull()][object] $Failure,
        [AllowNull()][object] $Result
    )

    $record = [ordered]@{
        schemaVersion = 1
        operationId = [string]$Plan.operationId
        taskId = if ($Plan.taskId) { [string]$Plan.taskId } else { $null }
        status = $Status
        stage = $Stage
        percent = $Percent
        message = $Message
        previousCommit = [string]$Plan.previousCommit
        targetCommit = [string]$Plan.targetCommit
        rollbackOnly = [bool]$Plan.rollbackOnly
        failure = $Failure
        result = $Result
        updatedAt = (Get-Date).ToUniversalTime().ToString('o')
    }
    Write-HermesDesktopUpdateJson -Path ([string]$Plan.progressPath) -Value $record
    $record
}

function Wait-HermesDesktopUpdateParent {
    [CmdletBinding()]
    param(
        [int] $ParentPid,
        [int] $TimeoutSeconds = 180
    )

    if ($ParentPid -le 0) { return $true }
    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
        try {
            $null = Get-Process -Id $ParentPid -ErrorAction Stop
            Start-Sleep -Milliseconds 250
        } catch {
            return $true
        }
    }
    throw "The running Hermes Launcher process $ParentPid did not exit within $TimeoutSeconds seconds."
}

function Enter-HermesDesktopUpdateLock {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $Root,
        [Parameter(Mandatory)][string] $OperationId
    )

    $lockPath = Join-Path $Root 'data\runtime\locks\desktop-self-update.json'
    [System.IO.Directory]::CreateDirectory([System.IO.Path]::GetDirectoryName($lockPath)) | Out-Null
    for ($attempt = 0; $attempt -lt 2; $attempt += 1) {
        try {
            $stream = [System.IO.File]::Open($lockPath, [System.IO.FileMode]::CreateNew, [System.IO.FileAccess]::Write, [System.IO.FileShare]::None)
            try {
                $payload = [System.Text.Encoding]::UTF8.GetBytes((@{
                    schemaVersion = 1; operationId = $OperationId; ownerPid = $PID; acquiredAt = (Get-Date).ToUniversalTime().ToString('o')
                } | ConvertTo-Json -Compress))
                $stream.Write($payload, 0, $payload.Length)
                $stream.Flush($true)
            } finally { $stream.Dispose() }
            return $lockPath
        } catch [System.IO.IOException] {
            $existing = try { Get-Content -Raw -LiteralPath $lockPath | ConvertFrom-Json } catch { $null }
            $alive = $false
            if ($existing?.ownerPid) {
                try { $null = Get-Process -Id ([int]$existing.ownerPid) -ErrorAction Stop; $alive = $true } catch { }
            }
            if ($alive) { throw "Desktop update '$($existing.operationId)' is already running under process $($existing.ownerPid)." }
            $recovered = "$lockPath.recovered-$((Get-Date).ToUniversalTime().ToString('yyyyMMddTHHmmssfffZ'))"
            Move-Item -LiteralPath $lockPath -Destination $recovered -Force
        }
    }
    throw 'Could not acquire the Desktop self-update lock.'
}

function Exit-HermesDesktopUpdateLock {
    [CmdletBinding()]
    param([string] $LockPath)
    if ($LockPath -and (Test-Path -LiteralPath $LockPath -PathType Leaf)) {
        try {
            $record = Get-Content -Raw -LiteralPath $LockPath | ConvertFrom-Json
            if ([int]$record.ownerPid -eq $PID) { Remove-Item -LiteralPath $LockPath -Force }
        } catch { }
    }
}

Export-ModuleMember -Function @(
    'Assert-HermesDesktopUpdateDiskSpace',
    'Assert-HermesDesktopUpdatePath',
    'ConvertFrom-HermesDesktopUpdateMarker',
    'ConvertTo-HermesDesktopUpdateMarker',
    'Enter-HermesDesktopUpdateLock',
    'Exit-HermesDesktopUpdateLock',
    'Get-HermesDesktopUpdateRequiredBytes',
    'New-HermesDesktopUpdatePlan',
    'Test-HermesDesktopUpdateOrigin',
    'Wait-HermesDesktopUpdateParent',
    'Write-HermesDesktopUpdateJson',
    'Write-HermesDesktopUpdateProgress'
)
