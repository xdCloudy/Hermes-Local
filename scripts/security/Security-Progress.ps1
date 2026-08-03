function Protect-SecurityTaskText {
    [CmdletBinding()]
    param([AllowEmptyString()][string] $Text)

    $safe = Protect-HermesLogText ([string]$Text)
    foreach ($privatePath in @($script:securityRoot, $env:USERPROFILE, $env:HOMEDRIVE + $env:HOMEPATH)) {
        if ($privatePath -and [System.IO.Path]::IsPathRooted([string]$privatePath)) {
            $safe = $safe -replace [regex]::Escape(([string]$privatePath).TrimEnd([char[]]@([System.IO.Path]::DirectorySeparatorChar, [System.IO.Path]::AltDirectorySeparatorChar))), '[PRIVATE-PATH]'
        }
    }
    $safe = $safe -replace '(?i)\bAuthorization\s*:\s*Bearer\s+[^\s,;]+', 'Authorization: Bearer [REDACTED]'
    $safe = $safe -replace '(?i)\b(?:token|api[_-]?key|secret|password|credential)\s*[:=]\s*[^\s,;]+', '[REDACTED-CREDENTIAL]'
    $safe = $safe -replace '(?i)\b(?:https?|wss?)://[^\s/@:]+:[^\s/@]+@', 'https://[REDACTED]@'
    $safe = $safe -replace '(?<![\d.])(?:10(?:\.\d{1,3}){3}|192\.168(?:\.\d{1,3}){2}|172\.(?:1[6-9]|2\d|3[01])(?:\.\d{1,3}){2})(?!\d|\.\d)', '[PRIVATE-TARGET]'
    return $safe
}

function Get-SecurityProgressPercent {
    [CmdletBinding()]
    param(
        [AllowNull()][object] $CompletedChecks,
        [AllowNull()][object] $TotalChecks
    )

    if ($null -eq $CompletedChecks -or $null -eq $TotalChecks) {
        return $null
    }
    $completed = [double]$CompletedChecks
    $total = [double]$TotalChecks
    if ($completed -lt 0 -or $total -le 0) {
        return $null
    }
    return [math]::Round(([math]::Min($completed, $total) / $total) * 100, 1)
}

function ConvertTo-SecurityRelativePath {
    [CmdletBinding()]
    param([AllowNull()][string] $Path)

    if (-not $Path) {
        return $null
    }
    try {
        $full = [System.IO.Path]::GetFullPath($Path)
        $relative = [System.IO.Path]::GetRelativePath($script:securityRoot, $full)
        if ($relative -eq '..' -or $relative.StartsWith('..' + [System.IO.Path]::DirectorySeparatorChar)) {
            return '[OUTSIDE-HERMES-ROOT]'
        }
        return $relative.Replace([System.IO.Path]::DirectorySeparatorChar, [System.IO.Path]::AltDirectorySeparatorChar)
    } catch {
        return '[INVALID-PATH]'
    }
}

function Initialize-SecurityScanProgress {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][ValidateRange(1, 4096)][int] $TotalChecks,
        [Parameter(Mandatory)][ValidateRange(1, 4096)][int] $TargetCount,
        [switch] $Quick,
        [switch] $SkipDefender
    )

    $script:securityTaskId = if ($env:HERMES_LOCAL_TASK_ID -and $env:HERMES_LOCAL_TASK_ID -match '^[A-Za-z0-9_-]{1,128}$') {
        [string]$env:HERMES_LOCAL_TASK_ID
    } else {
        [guid]::NewGuid().ToString('N')
    }
    $script:securityProgressStartedAt = (Get-Date).ToUniversalTime()
    $script:securityProgressTerminalStatus = $null
    $script:securityCancellationObserved = $false
    $script:securityCompletedChecks = 0
    $script:securityTotalChecks = $TotalChecks
    $script:securityTargetCount = $TargetCount
    $script:securityFindingCount = 0
    $script:securityQuick = [bool]$Quick
    $script:securitySkipDefender = [bool]$SkipDefender
    $script:securityCurrentStage = 'scope-validation'
    $script:securityCurrentTool = $null

    Remove-SecurityCancellationRequest
    Write-SecurityScanProgress `
        -Stage 'scope-validation' `
        -Message 'Validating the local scan scope and trust boundary.' `
        -Indeterminate
}

function Get-SecurityProgressElapsedSeconds {
    if (-not $script:securityProgressStartedAt) {
        return 0
    }
    return [math]::Round(((Get-Date).ToUniversalTime() - $script:securityProgressStartedAt).TotalSeconds, 3)
}

function Write-SecurityScanProgress {
    [CmdletBinding(DefaultParameterSetName = 'Indeterminate')]
    param(
        [Parameter(Mandatory)]
        [ValidateSet(
            'scope-validation',
            'tool-preparation',
            'discovery',
            'crawling',
            'passive-checks',
            'active-checks',
            'validation',
            'report-generation',
            'complete'
        )]
        [string] $Stage,

        [Parameter(Mandatory)][string] $Message,

        [Parameter(ParameterSetName = 'Determinate', Mandatory)]
        [ValidateRange(0, 4096)][int] $CompletedChecks,

        [Parameter(ParameterSetName = 'Determinate', Mandatory)]
        [ValidateRange(1, 4096)][int] $TotalChecks,

        [Parameter(ParameterSetName = 'Indeterminate')][switch] $Indeterminate,

        [ValidateSet('running', 'cancelling', 'succeeded', 'failed', 'cancelled', 'stale')]
        [string] $Status = 'running',

        [AllowNull()][Nullable[int]] $WorkerPid,
        [AllowNull()][string] $CurrentTool,
        [AllowNull()][string] $ResultDirectory,
        [AllowNull()][string] $ReportPath,
        [AllowNull()][string] $FindingsPath,
        [AllowNull()][string] $LogPath,
        [AllowNull()][string] $FailureCode,
        [AllowNull()][string] $ErrorMessage
    )

    if (-not $script:securityTaskId) {
        return
    }

    $script:securityCurrentStage = $Stage
    if ($CurrentTool) {
        $script:securityCurrentTool = $CurrentTool
    }
    $now = (Get-Date).ToUniversalTime()
    $determinate = $PSCmdlet.ParameterSetName -eq 'Determinate'
    $completed = if ($determinate) { $CompletedChecks } else { $null }
    $total = if ($determinate) { $TotalChecks } else { $null }
    $percent = if ($determinate) { Get-SecurityProgressPercent -CompletedChecks $completed -TotalChecks $total } else { $null }
    $safeMessage = Protect-SecurityTaskText $Message
    $safeTool = if ($CurrentTool) { Protect-SecurityTaskText $CurrentTool } else { $null }

    $document = [ordered]@{
        schemaVersion = 1
        taskId = $script:securityTaskId
        ownerPid = $PID
        workerPid = if ($null -ne $WorkerPid) { [int]$WorkerPid } else { $null }
        status = $Status
        stage = $Stage
        mode = if ($determinate) { 'determinate' } else { 'indeterminate' }
        completedChecks = $completed
        totalChecks = $total
        percent = $percent
        counters = [ordered]@{
            targets = [int]$script:securityTargetCount
            checks = [int]$script:securityCompletedChecks
            findings = [int]$script:securityFindingCount
        }
        quick = [bool]$script:securityQuick
        skipDefender = [bool]$script:securitySkipDefender
        elapsedSeconds = Get-SecurityProgressElapsedSeconds
        message = $safeMessage
        currentTool = $safeTool
        result = [ordered]@{
            directory = ConvertTo-SecurityRelativePath $ResultDirectory
            report = ConvertTo-SecurityRelativePath $ReportPath
            findings = ConvertTo-SecurityRelativePath $FindingsPath
            log = ConvertTo-SecurityRelativePath $LogPath
        }
        failure = if ($ErrorMessage) {
            [ordered]@{
                code = if ($FailureCode) { $FailureCode } elseif ($Status -eq 'cancelled') { 'security-scan-cancelled' } elseif ($Status -eq 'stale') { 'security-scan-stale' } else { 'security-scan-failed' }
                message = Protect-SecurityTaskText $ErrorMessage
                stage = $Stage
                tool = $safeTool
            }
        } else {
            $null
        }
        startedAt = $script:securityProgressStartedAt.ToString('o')
        updatedAt = $now.ToString('o')
        completedAt = if ($Status -in @('succeeded', 'failed', 'cancelled', 'stale')) { $now.ToString('o') } else { $null }
    }

    Write-HermesAtomicText `
        -Path $script:securityProgressPath `
        -Content (($document | ConvertTo-Json -Depth 16) + [Environment]::NewLine)

    if ($script:securityTaskLogPath) {
        $logDirectory = [System.IO.Path]::GetDirectoryName($script:securityTaskLogPath)
        if ($logDirectory) {
            [System.IO.Directory]::CreateDirectory($logDirectory) | Out-Null
        }
        $logLine = '{0} [{1}] {2}: {3}' -f $now.ToString('o'), $Status, $Stage, $safeMessage
        Add-Content -LiteralPath $script:securityTaskLogPath -Value $logLine -Encoding utf8
    }

    $units = if ($determinate) { " $completed/$total" } else { '' }
    Write-Host "::hermes-security-stage::$Stage::$safeMessage"
    Write-Host "Security scan progress: $Stage$units · $safeMessage"

    if ($Status -in @('succeeded', 'failed', 'cancelled', 'stale')) {
        $script:securityProgressTerminalStatus = $Status
    }
}

function Start-SecurityCheck {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $Stage,
        [Parameter(Mandatory)][string] $Tool,
        [Parameter(Mandatory)][string] $Message,
        [AllowNull()][Nullable[int]] $WorkerPid
    )

    Write-SecurityScanProgress `
        -Stage $Stage `
        -Message $Message `
        -CurrentTool $Tool `
        -WorkerPid $WorkerPid `
        -Indeterminate
}

function Complete-SecurityCheck {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $Stage,
        [Parameter(Mandatory)][string] $Tool,
        [Parameter(Mandatory)][string] $Message,
        [ValidateRange(0, 2147483647)][int] $FindingsAdded = 0
    )

    $script:securityCompletedChecks += 1
    $script:securityFindingCount += $FindingsAdded
    Write-SecurityScanProgress `
        -Stage $Stage `
        -Message $Message `
        -CurrentTool $Tool `
        -CompletedChecks $script:securityCompletedChecks `
        -TotalChecks $script:securityTotalChecks
}

function Test-SecurityCancellationRequested {
    [CmdletBinding()]
    param()

    if (-not (Test-Path -LiteralPath $script:securityCancelPath -PathType Leaf)) {
        return $false
    }
    try {
        $request = Get-Content -Raw -LiteralPath $script:securityCancelPath | ConvertFrom-Json
        $requestTaskId = [string]$request.taskId
        $ownerPid = if ($null -ne $request.ownerPid) { [int]$request.ownerPid } else { 0 }
        return $requestTaskId -eq $script:securityTaskId -and ($ownerPid -eq 0 -or $ownerPid -eq $PID)
    } catch {
        try {
            Write-HermesLog -Component security -Level WARN -Message "Ignoring unreadable security scan cancellation request: $($_.Exception.Message)"
        } catch { }
        return $false
    }
}

function Assert-SecurityNotCancelled {
    [CmdletBinding()]
    param([string] $Message = 'Security scan cancellation was requested.')

    if (-not (Test-SecurityCancellationRequested)) {
        return
    }
    $script:securityCancellationObserved = $true
    Write-SecurityScanProgress `
        -Stage $script:securityCurrentStage `
        -Message 'Cancellation accepted; stopping the owned scanner at a safe boundary.' `
        -CurrentTool $script:securityCurrentTool `
        -Status 'cancelling' `
        -Indeterminate
    throw [System.OperationCanceledException]::new($Message)
}

function Remove-SecurityCancellationRequest {
    [CmdletBinding()]
    param()

    if (-not (Test-Path -LiteralPath $script:securityCancelPath -PathType Leaf)) {
        return
    }
    try {
        $request = Get-Content -Raw -LiteralPath $script:securityCancelPath | ConvertFrom-Json
        if (-not $script:securityTaskId -or [string]$request.taskId -eq $script:securityTaskId) {
            Remove-Item -LiteralPath $script:securityCancelPath -Force -ErrorAction SilentlyContinue
        }
    } catch {
        Remove-Item -LiteralPath $script:securityCancelPath -Force -ErrorAction SilentlyContinue
    }
}

function Complete-SecurityScanProgress {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][ValidateSet('succeeded', 'failed', 'cancelled', 'stale')][string] $Status,
        [Parameter(Mandatory)][string] $Message,
        [AllowNull()][string] $ResultDirectory,
        [AllowNull()][string] $ReportPath,
        [AllowNull()][string] $FindingsPath,
        [AllowNull()][string] $LogPath,
        [AllowNull()][string] $FailureCode,
        [AllowNull()][string] $ErrorMessage
    )

    if ($script:securityProgressTerminalStatus) {
        return
    }
    Write-SecurityScanProgress `
        -Stage 'complete' `
        -Message $Message `
        -Status $Status `
        -CurrentTool $script:securityCurrentTool `
        -ResultDirectory $ResultDirectory `
        -ReportPath $ReportPath `
        -FindingsPath $FindingsPath `
        -LogPath $LogPath `
        -FailureCode $FailureCode `
        -ErrorMessage $ErrorMessage `
        -CompletedChecks $script:securityCompletedChecks `
        -TotalChecks $script:securityTotalChecks
    Remove-SecurityCancellationRequest
}
