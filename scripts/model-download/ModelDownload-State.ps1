function Add-HermesModelDownloadLog {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] $Context,
        [ValidateSet('DEBUG', 'INFO', 'WARN', 'ERROR')][string] $Level = 'INFO',
        [Parameter(Mandatory)][string] $Message
    )

    $safe = Protect-HermesModelDownloadText -Text $Message -Root $Context.Root
    $line = '{0} [{1}] {2}' -f (Get-Date).ToUniversalTime().ToString('o'), $Level, $safe
    [System.IO.File]::AppendAllText($Context.LogPath, $line + [Environment]::NewLine, [System.Text.UTF8Encoding]::new($false))
}

function Write-HermesModelDownloadProgress {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] $Context,
        [Parameter(Mandatory)][string] $Stage,
        [Parameter(Mandatory)][string] $Message,
        [ValidateSet('queued', 'running', 'paused', 'cancelling', 'cancelled', 'failed', 'succeeded')]
        [string] $Status = 'running',
        [Nullable[long]] $BytesCompleted,
        [Nullable[long]] $BytesTotal,
        [Nullable[double]] $RateBytesPerSecond,
        [Nullable[double]] $EtaSeconds,
        [bool] $Cancellable = $true,
        [bool] $PauseSupported = $true,
        [bool] $ResumeSupported = $false,
        [hashtable] $Counters = @{},
        $Result = $null,
        $Failure = $null,
        [string] $CompletedAt
    )

    $now = (Get-Date).ToUniversalTime().ToString('o')
    $completed = $(if ($null -ne $BytesCompleted) { [long]$BytesCompleted } else { $null })
    $total = $(if ($null -ne $BytesTotal -and $BytesTotal -gt 0) { [long]$BytesTotal } else { $null })
    $percent = if ($null -ne $completed -and $null -ne $total) {
        [math]::Min(100, [math]::Round(($completed / $total) * 100, 1))
    } else {
        $null
    }
    $safeMessage = Protect-HermesModelDownloadText -Text $Message -Root $Context.Root

    $document = [ordered]@{
        schemaVersion = 1
        taskId = $Context.TaskId
        operation = 'model-download'
        status = $Status
        stage = $Stage
        message = $safeMessage
        startedAt = $Context.StartedAt
        updatedAt = $now
        completedAt = $(if ($CompletedAt) { $CompletedAt } else { $null })
        source = [ordered]@{
            repository = $Context.Repository
            revision = $Context.Revision
            identity = $Context.Source
        }
        target = [ordered]@{
            modelId = $Context.ModelId
            alias = $Context.Alias
            displayName = $Context.DisplayName
            filename = $Context.Filename
            relativePath = $Context.Primary.targetRelativePath
            identity = $Context.TargetIdentity
        }
        files = @($Context.Files | ForEach-Object {
            [ordered]@{
                kind = $_.kind
                filename = $_.filename
                source = $_.source
                targetRelativePath = $_.targetRelativePath
                expectedSizeBytes = $_.expectedSizeBytes
                expectedSha256 = $_.expectedSha256
                partialBytes = $(if (Test-Path -LiteralPath $_.partialPath -PathType Leaf) {
                    (Get-Item -LiteralPath $_.partialPath).Length
                } else { 0 })
            }
        })
        progress = [ordered]@{
            mode = $(if ($null -ne $total) { 'determinate' } else { 'indeterminate' })
            bytesCompleted = $completed
            bytesTotal = $total
            percent = $percent
            rateBytesPerSecond = $(if ($null -ne $RateBytesPerSecond) { [math]::Max(0, [double]$RateBytesPerSecond) } else { $null })
            etaSeconds = $(if ($null -ne $EtaSeconds) { [math]::Max(0, [double]$EtaSeconds) } else { $null })
            counters = $Counters
            cancellable = $Cancellable
            pauseSupported = $PauseSupported
            resumeSupported = $ResumeSupported
        }
        retention = [ordered]@{
            keepPartialOnCancel = [bool]$Context.KeepPartialOnCancel
            partialSuffix = '.partial'
        }
        owner = [ordered]@{
            kind = 'powershell-process'
            pid = $PID
        }
        result = $Result
        failure = $Failure
    }

    $Context.CurrentStage = $Stage
    $Context.Progress = $document
    Write-HermesModelDownloadJson -Path $Context.ProgressPath -Value $document
    Add-HermesModelDownloadLog -Context $Context -Message "${Stage}: $safeMessage"
    Write-Host "::hermes-model-download-stage::$Stage::$safeMessage"
    return $document
}

function Get-HermesModelDownloadControl {
    [CmdletBinding()]
    param([Parameter(Mandatory)] $Context)

    if (-not (Test-Path -LiteralPath $Context.ControlPath -PathType Leaf)) {
        return $null
    }
    try {
        $control = Get-Content -Raw -LiteralPath $Context.ControlPath | ConvertFrom-Json -Depth 16
        if ([string]$control.taskId -ne $Context.TaskId) {
            return $null
        }
        $action = [string]$control.action
        return $(if ($action -in @('cancel', 'pause')) { $action } else { $null })
    } catch {
        Add-HermesModelDownloadLog -Context $Context -Level WARN -Message "Ignored invalid control request: $($_.Exception.Message)"
        return $null
    }
}

function Remove-HermesModelDownloadControl {
    [CmdletBinding()]
    param([Parameter(Mandatory)] $Context)

    Remove-Item -LiteralPath $Context.ControlPath -Force -ErrorAction SilentlyContinue
}

function Assert-HermesModelDownloadNotControlled {
    [CmdletBinding()]
    param([Parameter(Mandatory)] $Context)

    $action = Get-HermesModelDownloadControl -Context $Context
    if (-not $action) {
        return
    }

    $exception = [OperationCanceledException]::new("Model download $action requested.")
    $exception.Data['HermesModelDownloadAction'] = $action
    throw $exception
}

function Test-HermesModelDownloadProcessAlive {
    [CmdletBinding()]
    param([int] $ProcessId)

    return $ProcessId -gt 0 -and $null -ne (Get-Process -Id $ProcessId -ErrorAction SilentlyContinue)
}

function Enter-HermesModelDownloadLock {
    [CmdletBinding()]
    param([Parameter(Mandatory)] $Context)

    [System.IO.Directory]::CreateDirectory([System.IO.Path]::GetDirectoryName($Context.LockPath)) | Out-Null
    if (Test-Path -LiteralPath $Context.LockPath -PathType Leaf) {
        $existing = $null
        try {
            $existing = Get-Content -Raw -LiteralPath $Context.LockPath | ConvertFrom-Json -Depth 16
        } catch {
            $existing = $null
        }
        $sameTask = [string](Get-HermesModelDownloadValue -Record $existing -Name taskId -Default '') -eq $Context.TaskId
        $ownerPid = [int](Get-HermesModelDownloadValue -Record $existing -Name ownerPid -Default 0)
        $state = [string](Get-HermesModelDownloadValue -Record $existing -Name state -Default 'running')
        if (-not $sameTask -and ($state -eq 'paused' -or (Test-HermesModelDownloadProcessAlive -ProcessId $ownerPid))) {
            throw "Target model is already owned by task '$([string]$existing.taskId)'."
        }
        if ($sameTask -or -not (Test-HermesModelDownloadProcessAlive -ProcessId $ownerPid)) {
            Remove-Item -LiteralPath $Context.LockPath -Force -ErrorAction SilentlyContinue
        }
    }

    $stream = [System.IO.File]::Open(
        $Context.LockPath,
        [System.IO.FileMode]::CreateNew,
        [System.IO.FileAccess]::ReadWrite,
        [System.IO.FileShare]::Read
    )
    $record = [ordered]@{
        schemaVersion = 1
        taskId = $Context.TaskId
        targetIdentity = $Context.TargetIdentity
        targetRelativePath = $Context.Primary.targetRelativePath
        ownerPid = $PID
        state = 'running'
        acquiredAt = (Get-Date).ToUniversalTime().ToString('o')
    }
    $bytes = [System.Text.Encoding]::UTF8.GetBytes(($record | ConvertTo-Json -Depth 16) + [Environment]::NewLine)
    $stream.Write($bytes, 0, $bytes.Length)
    $stream.Flush($true)
    $Context.LockStream = $stream
    $Context.LockOwned = $true
}

function Exit-HermesModelDownloadLock {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] $Context,
        [switch] $Paused
    )

    if ($Context.LockStream) {
        $Context.LockStream.Dispose()
        $Context.LockStream = $null
    }
    if (-not $Context.LockOwned) {
        return
    }
    if ($Paused) {
        Write-HermesModelDownloadJson -Path $Context.LockPath -Value ([ordered]@{
            schemaVersion = 1
            taskId = $Context.TaskId
            targetIdentity = $Context.TargetIdentity
            targetRelativePath = $Context.Primary.targetRelativePath
            ownerPid = $null
            state = 'paused'
            updatedAt = (Get-Date).ToUniversalTime().ToString('o')
        })
    } else {
        Remove-Item -LiteralPath $Context.LockPath -Force -ErrorAction SilentlyContinue
    }
    $Context.LockOwned = $false
}

function Get-HermesModelDownloadExpectedTotal {
    [CmdletBinding()]
    param([Parameter(Mandatory)] $Context)

    $sizes = @($Context.Files | ForEach-Object { $_.expectedSizeBytes })
    if ($sizes.Count -eq 0 -or @($sizes | Where-Object { $null -eq $_ }).Count -gt 0) {
        return $null
    }
    return [long](($sizes | Measure-Object -Sum).Sum)
}

function Get-HermesModelDownloadCompletedBytes {
    [CmdletBinding()]
    param([Parameter(Mandatory)] $Context)

    $total = [long]0
    foreach ($file in $Context.Files) {
        if (Test-Path -LiteralPath $file.partialPath -PathType Leaf) {
            $total += (Get-Item -LiteralPath $file.partialPath).Length
        } elseif (Test-Path -LiteralPath $file.targetPath -PathType Leaf) {
            $actualSha256 = [string](Get-HermesModelDownloadValue -Record $file -Name 'actualSha256')
            $expectedSha256 = [string](Get-HermesModelDownloadValue -Record $file -Name 'expectedSha256')
            if ($actualSha256 -or ($expectedSha256 -and
                (Get-FileHash -LiteralPath $file.targetPath -Algorithm SHA256).Hash.ToLowerInvariant() -eq $expectedSha256)) {
                $total += (Get-Item -LiteralPath $file.targetPath).Length
            }
        }
    }
    return $total
}

function Assert-HermesModelDownloadDiskSpace {
    [CmdletBinding()]
    param([Parameter(Mandatory)] $Context)

    $expectedTotal = Get-HermesModelDownloadExpectedTotal -Context $Context
    if ($null -eq $expectedTotal) {
        return
    }
    $completed = Get-HermesModelDownloadCompletedBytes -Context $Context
    $remaining = [math]::Max(0, [long]$expectedTotal - [long]$completed)
    $root = [System.IO.Path]::GetPathRoot($Context.Primary.targetPath)
    $drive = [System.IO.DriveInfo]::new($root)
    $reserve = [long](512MB)
    if ($drive.AvailableFreeSpace -lt ($remaining + $reserve)) {
        throw "Insufficient disk space. Need $remaining bytes plus a $reserve-byte safety reserve, but only $($drive.AvailableFreeSpace) bytes are available."
    }
}
