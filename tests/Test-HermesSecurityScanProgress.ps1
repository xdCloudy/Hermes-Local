[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Assert-True {
    param(
        [Parameter(Mandatory)][bool] $Condition,
        [Parameter(Mandatory)][string] $Message
    )
    if (-not $Condition) {
        throw $Message
    }
}

function Write-HermesAtomicText {
    param(
        [Parameter(Mandatory)][string] $Path,
        [Parameter(Mandatory)][string] $Content
    )
    [System.IO.Directory]::CreateDirectory([System.IO.Path]::GetDirectoryName($Path)) | Out-Null
    $temporary = "$Path.$PID.$([guid]::NewGuid().ToString('N')).tmp"
    [System.IO.File]::WriteAllText($temporary, $Content, [System.Text.UTF8Encoding]::new($false))
    Move-Item -LiteralPath $temporary -Destination $Path -Force
}

function Protect-HermesLogText {
    param([AllowEmptyString()][string] $Text)
    return ([string]$Text) `
        -replace '(?i)(authorization\s*[:=]\s*bearer\s+)[^\s"'']+', '$1[REDACTED]' `
        -replace '(?i)((?:api[_-]?key|password|secret|token|credential)\s*[:=]\s*)[^\s,"'']+', '$1[REDACTED]' `
        -replace '[A-Za-z0-9_-]{48,}', '[REDACTED-LONG-VALUE]'
}

function Write-HermesLog {
    param(
        [string] $Component,
        [string] $Level = 'INFO',
        [string] $Message
    )
}

$temporaryRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("hermes-security-progress-" + [guid]::NewGuid().ToString('N'))
$previousTaskId = $env:HERMES_LOCAL_TASK_ID
$previousProfile = $env:USERPROFILE

try {
    [System.IO.Directory]::CreateDirectory($temporaryRoot) | Out-Null
    $env:USERPROFILE = Join-Path $temporaryRoot 'private-user'

    $script:securityRoot = $temporaryRoot
    $script:securityProgressPath = Join-Path $temporaryRoot 'data\runtime\security-scan-progress.json'
    $script:securityCancelPath = Join-Path $temporaryRoot 'data\runtime\security-scan-cancel.json'
    $script:securityTaskLogPath = Join-Path $temporaryRoot 'security\scans\task.log'
    $script:securityTaskId = $null
    $script:securityProgressStartedAt = $null
    $script:securityProgressTerminalStatus = $null
    $script:securityCancellationObserved = $false
    $script:securityCompletedChecks = 0
    $script:securityTotalChecks = 0
    $script:securityTargetCount = 0
    $script:securityFindingCount = 0
    $script:securityCurrentStage = 'scope-validation'
    $script:securityCurrentTool = $null

    . (Join-Path $PSScriptRoot '..\scripts\security\Security-Progress.ps1')

    $env:HERMES_LOCAL_TASK_ID = 'security-task-25'
    Initialize-SecurityScanProgress -TotalChecks 8 -TargetCount 1 -Quick -SkipDefender
    $startup = Get-Content -Raw -LiteralPath $script:securityProgressPath | ConvertFrom-Json -Depth 32
    Assert-True ($startup.taskId -eq 'security-task-25') 'Startup did not preserve the Desktop task ID.'
    Assert-True ($startup.status -eq 'running') 'Startup did not create an active progress marker.'
    Assert-True ($startup.stage -eq 'scope-validation') 'Startup stage was not scope validation.'
    Assert-True ($startup.mode -eq 'indeterminate') 'Scope validation should use indeterminate progress.'

    Start-SecurityCheck -Stage discovery -Tool 'npm-audit-production' -Message 'Auditing dependencies.' -WorkerPid 4242
    Complete-SecurityCheck -Stage discovery -Tool 'npm-audit-production' -Message 'Dependency audit complete.' -FindingsAdded 2
    Start-SecurityCheck -Stage crawling -Tool 'gitleaks-production-source' -Message 'Crawling source.' -WorkerPid 4343
    Complete-SecurityCheck -Stage crawling -Tool 'gitleaks-production-source' -Message 'Source crawl complete.' -FindingsAdded 1
    $phase = Get-Content -Raw -LiteralPath $script:securityProgressPath | ConvertFrom-Json -Depth 32
    Assert-True ($phase.stage -eq 'crawling') 'Multiple scan phases were not recorded.'
    Assert-True ($phase.completedChecks -eq 2) 'Completed check counter is incorrect.'
    Assert-True ($phase.totalChecks -eq 8) 'Total check counter is incorrect.'
    Assert-True ($phase.percent -eq 25) 'Determinate percentage is incorrect.'
    Assert-True ($phase.counters.findings -eq 3) 'Finding counter is incorrect.'
    Assert-True ($phase.counters.targets -eq 1) 'Target counter is incorrect.'

    $wrongRequest = [ordered]@{ schemaVersion = 1; taskId = 'other-task'; ownerPid = $PID }
    Write-HermesAtomicText -Path $script:securityCancelPath -Content (($wrongRequest | ConvertTo-Json) + [Environment]::NewLine)
    Assert-True (-not (Test-SecurityCancellationRequested)) 'Cancellation accepted a different task ID.'

    $request = [ordered]@{ schemaVersion = 1; taskId = 'security-task-25'; ownerPid = $PID }
    Write-HermesAtomicText -Path $script:securityCancelPath -Content (($request | ConvertTo-Json) + [Environment]::NewLine)
    Assert-True (Test-SecurityCancellationRequested) 'Matching cancellation request was not detected.'
    $cancelThrown = $false
    try {
        Assert-SecurityNotCancelled
    } catch [System.OperationCanceledException] {
        $cancelThrown = $true
    }
    Assert-True $cancelThrown 'Cancellation did not stop at a safe boundary.'
    Complete-SecurityScanProgress `
        -Status cancelled `
        -Message 'Security scan cancelled safely.' `
        -ResultDirectory (Join-Path $temporaryRoot 'security\scans\run-1') `
        -ReportPath (Join-Path $temporaryRoot 'security\scans\run-1\summary.json') `
        -FindingsPath (Join-Path $temporaryRoot 'security\scans\run-1\findings.json') `
        -LogPath $script:securityTaskLogPath `
        -FailureCode 'security-scan-cancelled' `
        -ErrorMessage 'Cancellation requested.'
    $cancelled = Get-Content -Raw -LiteralPath $script:securityProgressPath | ConvertFrom-Json -Depth 32
    Assert-True ($cancelled.status -eq 'cancelled') 'Cancelled scan remained marked as running.'
    Assert-True (-not [string]::IsNullOrWhiteSpace([string]$cancelled.completedAt)) 'Cancelled scan omitted its completion timestamp.'
    Assert-True (-not (Test-Path -LiteralPath $script:securityCancelPath)) 'Cancellation request was not consumed.'

    $env:HERMES_LOCAL_TASK_ID = 'security-success'
    Initialize-SecurityScanProgress -TotalChecks 2 -TargetCount 1 -Quick
    Complete-SecurityCheck -Stage passive-checks -Tool 'ruff' -Message 'Ruff complete.'
    Complete-SecurityCheck -Stage passive-checks -Tool 'typescript' -Message 'TypeScript complete.'
    $runRoot = Join-Path $temporaryRoot 'security\scans\run-success'
    Complete-SecurityScanProgress `
        -Status succeeded `
        -Message 'Security scan completed.' `
        -ResultDirectory $runRoot `
        -ReportPath (Join-Path $runRoot 'summary.json') `
        -FindingsPath (Join-Path $runRoot 'findings.json') `
        -LogPath $script:securityTaskLogPath
    $success = Get-Content -Raw -LiteralPath $script:securityProgressPath | ConvertFrom-Json -Depth 32
    Assert-True ($success.status -eq 'succeeded') 'Successful scan did not terminate.'
    Assert-True ($success.percent -eq 100) 'Successful scan did not report 100 percent.'
    Assert-True ($success.result.directory -eq 'security/scans/run-success') 'Result directory was not made safely relative.'
    Assert-True ($success.result.report -eq 'security/scans/run-success/summary.json') 'Report was not linked.'
    Assert-True ($success.result.findings -eq 'security/scans/run-success/findings.json') 'Findings were not linked.'

    foreach ($terminal in @(
        [ordered]@{ task = 'security-failure'; status = 'failed'; code = 'security-tool-exit' },
        [ordered]@{ task = 'security-stale'; status = 'stale'; code = 'security-scan-stale' }
    )) {
        $env:HERMES_LOCAL_TASK_ID = $terminal.task
        Initialize-SecurityScanProgress -TotalChecks 4 -TargetCount 1 -Quick
        Complete-SecurityScanProgress `
            -Status $terminal.status `
            -Message "Security scan $($terminal.status)." `
            -ResultDirectory $runRoot `
            -ReportPath (Join-Path $runRoot 'summary.json') `
            -FindingsPath (Join-Path $runRoot 'findings.json') `
            -LogPath $script:securityTaskLogPath `
            -FailureCode $terminal.code `
            -ErrorMessage 'Scanner token=top-secret failed against 192.168.1.8.'
        $document = Get-Content -Raw -LiteralPath $script:securityProgressPath | ConvertFrom-Json -Depth 32
        Assert-True ($document.status -eq $terminal.status) "Terminal status '$($terminal.status)' was not persisted."
        Assert-True ($document.failure.code -eq $terminal.code) "Failure code '$($terminal.code)' was not persisted."
        Assert-True ([string]$document.failure.message -notmatch 'top-secret|192\.168\.1\.8') 'Sensitive failure details were not redacted.'
    }

    $secret = 'A' * 64
    $privatePath = Join-Path $env:USERPROFILE 'targets\internal.txt'
    $redacted = Protect-SecurityTaskText "Authorization: Bearer $secret api_key=$secret target=10.0.0.5 path=$privatePath"
    Assert-True ($redacted -notmatch [regex]::Escape($secret)) 'Long credential was exposed.'
    Assert-True ($redacted -notmatch '10\.0\.0\.5') 'Private target was exposed.'
    Assert-True ($redacted -notmatch [regex]::Escape($privatePath)) 'Private path was exposed.'
    Assert-True ($redacted -match '\[PRIVATE-TARGET\]') 'Private target redaction marker is missing.'
    Assert-True ($redacted -match '\[PRIVATE-PATH\]') 'Private path redaction marker is missing.'

    Write-Host 'Hermes security scan progress contract passed.'
} finally {
    if ($null -eq $previousTaskId) {
        Remove-Item Env:HERMES_LOCAL_TASK_ID -ErrorAction SilentlyContinue
    } else {
        $env:HERMES_LOCAL_TASK_ID = $previousTaskId
    }
    if ($null -eq $previousProfile) {
        Remove-Item Env:USERPROFILE -ErrorAction SilentlyContinue
    } else {
        $env:USERPROFILE = $previousProfile
    }
    Remove-Item -LiteralPath $temporaryRoot -Recurse -Force -ErrorAction SilentlyContinue
}
