Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Write-HermesDesktopUpdateJson {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $Path,
        [Parameter(Mandatory)][object] $Value
    )

    $fullPath = [IO.Path]::GetFullPath($Path)
    [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($fullPath)) | Out-Null
    $temporary = "$fullPath.$PID.$([guid]::NewGuid().ToString('N')).tmp"
    $json = ($Value | ConvertTo-Json -Depth 64) + [Environment]::NewLine
    [IO.File]::WriteAllText($temporary, $json, [Text.UTF8Encoding]::new($false))
    [IO.File]::Move($temporary, $fullPath, $true)
}

function ConvertTo-HermesDesktopUpdateMarker {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [ValidateSet('status', 'helper', 'result')]
        [string] $Name,
        [Parameter(Mandatory)][object] $Value
    )

    $json = $Value | ConvertTo-Json -Depth 64 -Compress
    $encoded = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($json))
    "::hermes-desktop-update-$Name::$encoded"
}

function ConvertFrom-HermesDesktopUpdateMarker {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [ValidateSet('status', 'helper', 'result')]
        [string] $Name,
        [Parameter(Mandatory)][string] $Text
    )

    $pattern = "::hermes-desktop-update-$([regex]::Escape($Name))::([A-Za-z0-9+/=]+)"
    $match = [regex]::Matches($Text, $pattern) | Select-Object -Last 1
    if (-not $match) { return $null }

    try {
        $json = [Text.Encoding]::UTF8.GetString(
            [Convert]::FromBase64String($match.Groups[1].Value)
        )
        $json | ConvertFrom-Json -Depth 64
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

    $resolvedRoot = [IO.Path]::GetFullPath($Root).TrimEnd('\', '/') +
        [IO.Path]::DirectorySeparatorChar
    $resolvedPath = [IO.Path]::GetFullPath($Path)
    if (-not $resolvedPath.StartsWith($resolvedRoot, [StringComparison]::OrdinalIgnoreCase)) {
        throw "$Description is outside the Hermes Local root: $resolvedPath"
    }
    $resolvedPath
}

function Test-HermesDesktopUpdateOrigin {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string] $Origin)

    $normalized = $Origin.Trim().TrimEnd('/')
    [bool](
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

    $fullRoot = [IO.Path]::GetFullPath($Root)
    $operationId = [guid]::NewGuid().ToString('N')
    $stagingRoot = Assert-HermesDesktopUpdatePath `
        -Root $fullRoot `
        -Path (Join-Path $fullRoot "build\updates\desktop-staging\$operationId") `
        -Description 'Staging root'

    [ordered]@{
        schemaVersion = 1
        operationId = $operationId
        taskId = if ($TaskId) { $TaskId } else { $null }
        requestedAt = (Get-Date).ToUniversalTime().ToString('o')
        root = $fullRoot
        stagingRoot = $stagingRoot
        previousCommit = $CurrentCommit.ToLowerInvariant()
        targetCommit = $TargetCommit.ToLowerInvariant()
        channel = $Channel
        previousBranch = if ($CurrentBranch) { $CurrentBranch } else { $null }
        parentPid = $ParentPid
        rollbackOnly = [bool]$RollbackOnly
        launcherPath = Join-Path $fullRoot 'dist\Hermes Launcher.exe'
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
    $distBytes = 0L
    if (Test-Path -LiteralPath $dist -PathType Container) {
        $measure = Get-ChildItem -LiteralPath $dist -Recurse -File -ErrorAction SilentlyContinue |
            Measure-Object -Property Length -Sum
        if ($null -ne $measure.Sum) { $distBytes = [long]$measure.Sum }
    }
    [long][math]::Max(2GB, ($distBytes * 3) + 512MB)
}

function Assert-HermesDesktopUpdateDiskSpace {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $Root,
        [long] $RequiredBytes = 0
    )

    if ($RequiredBytes -le 0) {
        $RequiredBytes = Get-HermesDesktopUpdateRequiredBytes -Root $Root
    }
    $driveRoot = [IO.Path]::GetPathRoot([IO.Path]::GetFullPath($Root))
    $drive = [IO.DriveInfo]::new($driveRoot)
    if ($drive.AvailableFreeSpace -lt $RequiredBytes) {
        $requiredGiB = [math]::Round($RequiredBytes / 1GB, 2)
        $availableGiB = [math]::Round($drive.AvailableFreeSpace / 1GB, 2)
        throw "Insufficient disk space for the staged update. Required $requiredGiB GiB; available $availableGiB GiB."
    }
    [pscustomobject]@{
        RequiredBytes = $RequiredBytes
        AvailableBytes = $drive.AvailableFreeSpace
    }
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
    param([int] $ParentPid, [int] $TimeoutSeconds = 180)

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
    [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($lockPath)) | Out-Null

    for ($attempt = 0; $attempt -lt 2; $attempt += 1) {
        try {
            $stream = [IO.File]::Open(
                $lockPath,
                [IO.FileMode]::CreateNew,
                [IO.FileAccess]::Write,
                [IO.FileShare]::None
            )
            try {
                $record = [ordered]@{
                    schemaVersion = 1
                    operationId = $OperationId
                    ownerPid = $PID
                    acquiredAt = (Get-Date).ToUniversalTime().ToString('o')
                }
                $recordJson = $record | ConvertTo-Json -Compress
                $payload = [Text.Encoding]::UTF8.GetBytes($recordJson)
                $stream.Write($payload, 0, $payload.Length)
                $stream.Flush($true)
            } finally {
                $stream.Dispose()
            }
            return $lockPath
        } catch [IO.IOException] {
            $existing = try {
                Get-Content -Raw -LiteralPath $lockPath | ConvertFrom-Json
            } catch {
                $null
            }

            $alive = $false
            if ($existing -and $existing.PSObject.Properties['ownerPid']) {
                try {
                    $null = Get-Process -Id ([int]$existing.ownerPid) -ErrorAction Stop
                    $alive = $true
                } catch {
                    $alive = $false
                }
            }
            if ($alive) {
                throw "Desktop update '$($existing.operationId)' is already running under process $($existing.ownerPid)."
            }

            $stamp = (Get-Date).ToUniversalTime().ToString('yyyyMMddTHHmmssfffZ')
            $recovered = "$lockPath.recovered-$stamp"
            if (Test-Path -LiteralPath $lockPath -PathType Leaf) {
                Move-Item -LiteralPath $lockPath -Destination $recovered -Force
            }
        }
    }
    throw 'Could not acquire the Desktop self-update lock.'
}

function Exit-HermesDesktopUpdateLock {
    [CmdletBinding()]
    param([string] $LockPath)

    if (-not $LockPath -or -not (Test-Path -LiteralPath $LockPath -PathType Leaf)) {
        return
    }
    try {
        $record = Get-Content -Raw -LiteralPath $LockPath | ConvertFrom-Json
        if ([int]$record.ownerPid -eq $PID) {
            Remove-Item -LiteralPath $LockPath -Force
        }
    } catch {
        # Retain damaged locks for the next stale-lock recovery pass.
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
