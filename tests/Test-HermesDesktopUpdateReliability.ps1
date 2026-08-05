[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repositoryRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$modulePath = Join-Path $repositoryRoot 'scripts\Hermes-DesktopUpdate.psm1'
$partsRoot = Join-Path $repositoryRoot 'scripts\desktop-update'

function Assert-ReliabilityContract {
    param(
        [Parameter(Mandatory)][bool] $Condition,
        [Parameter(Mandatory)][string] $Message
    )

    if (-not $Condition) {
        throw $Message
    }
}

Import-Module $modulePath -Force
foreach ($part in @(
    'DesktopUpdate-Git.ps1',
    'DesktopUpdate-State.ps1',
    'DesktopUpdate-Promotion.ps1',
    'DesktopUpdate-Stage.ps1',
    'DesktopUpdate-NestedSource.ps1',
    'DesktopUpdate-SafeActivation.ps1',
    'DesktopUpdate-Reliability.ps1'
)) {
    . (Join-Path $partsRoot $part)
}

$tempRoot = Join-Path ([IO.Path]::GetTempPath()) (
    'hermes-desktop-reliability-' + [guid]::NewGuid().ToString('N')
)
$script:root = $tempRoot

try {
    [IO.Directory]::CreateDirectory($tempRoot) | Out-Null
    $stagingRoot = Join-Path $tempRoot 'build\updates\desktop-staging\reliability-test'
    [IO.Directory]::CreateDirectory($stagingRoot) | Out-Null
    $logPath = Join-Path $stagingRoot 'desktop-self-update.log'
    $planPath = Join-Path $stagingRoot 'plan.json'
    $plan = [pscustomobject]@{
        schemaVersion = 1
        operationId = 'reliability-test'
        root = $tempRoot
        stagingRoot = $stagingRoot
        logPath = $logPath
        planPath = $planPath
        previousCommit = ('1' * 40)
        targetCommit = ('2' * 40)
        rollbackOnly = $false
    }
    $script:HermesDesktopUpdateActivePlan = $plan

    $hostExecutable = (Get-Process -Id $PID -ErrorAction Stop).Path
    $counterPath = Join-Path $tempRoot 'attempt-count.txt'
    $flakyScript = Join-Path $tempRoot 'flaky.ps1'
    @'
param([Parameter(Mandatory)][string] $CounterPath)
$count = if (Test-Path -LiteralPath $CounterPath) {
    [int](Get-Content -Raw -LiteralPath $CounterPath)
} else {
    0
}
$count += 1
Set-Content -LiteralPath $CounterPath -Value $count -Encoding ascii
Write-Output "flaky-attempt-$count"
if ($count -lt 2) {
    [Console]::Error.WriteLine('transient-updater-failure')
    exit 7
}
exit 0
'@ | Set-Content -LiteralPath $flakyScript -Encoding utf8

    $retryResult = Invoke-HermesDesktopProcess `
        -FilePath $hostExecutable `
        -Arguments @(
            '-NoLogo', '-NoProfile', '-NonInteractive',
            '-File', $flakyScript,
            '-CounterPath', $counterPath
        ) `
        -Description 'Reliability retry probe' `
        -Plan $plan `
        -WorkingDirectory $tempRoot `
        -MaxAttempts 2 `
        -RetryDelaySeconds 0

    Assert-ReliabilityContract `
        ($retryResult.Attempts -eq 2) `
        'The updater process runner did not retry a transient failure.'
    Assert-ReliabilityContract `
        ((Get-Content -Raw -LiteralPath $counterPath).Trim() -eq '2') `
        'The retry probe did not run exactly twice.'

    $logText = Get-Content -Raw -LiteralPath $logPath
    foreach ($required in @(
        'Reliability retry probe',
        'attempt 1/2',
        'transient-updater-failure',
        'exit 7',
        'attempt 2/2',
        'flaky-attempt-2',
        'exit 0'
    )) {
        Assert-ReliabilityContract `
            ($logText.Contains($required)) `
            "The updater log is missing retry evidence: $required"
    }

    $failureScript = Join-Path $tempRoot 'always-fail.ps1'
    @'
[Console]::Error.WriteLine('specific-updater-failure-marker')
exit 9
'@ | Set-Content -LiteralPath $failureScript -Encoding utf8

    $failureMessage = $null
    try {
        $null = Invoke-HermesDesktopProcess `
            -FilePath $hostExecutable `
            -Arguments @(
                '-NoLogo', '-NoProfile', '-NonInteractive',
                '-File', $failureScript
            ) `
            -Description 'Reliability failure probe' `
            -Plan $plan `
            -WorkingDirectory $tempRoot
    } catch {
        $failureMessage = $_.Exception.Message
    }

    Assert-ReliabilityContract `
        (-not [string]::IsNullOrWhiteSpace($failureMessage)) `
        'A failed updater subprocess did not produce an exception.'
    Assert-ReliabilityContract `
        ($failureMessage.Contains('specific-updater-failure-marker')) `
        'The useful subprocess output tail was discarded from the updater failure.'
    Assert-ReliabilityContract `
        ($failureMessage.Contains($logPath)) `
        'The updater failure did not identify its complete log path.'

    $gitRepository = Join-Path $tempRoot 'git-recovery'
    [IO.Directory]::CreateDirectory($gitRepository) | Out-Null
    & git -C $gitRepository init | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'Could not initialise reliability Git fixture.' }
    & git -C $gitRepository config user.name 'Hermes Reliability Test'
    & git -C $gitRepository config user.email 'hermes-reliability@localhost'

    $fixturePath = Join-Path $gitRepository 'fixture.txt'
    Set-Content -LiteralPath $fixturePath -Value 'base' -Encoding utf8
    & git -C $gitRepository add -- fixture.txt
    & git -C $gitRepository commit -m 'base' | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'Could not commit the reliability fixture base.' }
    $baseBranch = (& git -C $gitRepository branch --show-current).Trim()

    $gitDirectory = Get-HermesDesktopGitDirectory -Repository $gitRepository
    $indexLock = Join-Path $gitDirectory 'index.lock'
    Set-Content -LiteralPath $indexLock -Value 'stale lock' -Encoding ascii
    (Get-Item -LiteralPath $indexLock).LastWriteTimeUtc = (Get-Date).ToUniversalTime().AddMinutes(-10)

    $lockRepair = Repair-HermesDesktopGitOperationState `
        -Repository $gitRepository `
        -Description 'Reliability fixture'
    Assert-ReliabilityContract `
        ([bool]$lockRepair.Repaired) `
        'The updater did not report stale Git-lock recovery.'
    Assert-ReliabilityContract `
        (-not (Test-Path -LiteralPath $indexLock)) `
        'The stale Git lock remained in its active path.'
    Assert-ReliabilityContract `
        (@(Get-ChildItem -LiteralPath $gitDirectory -Filter 'index.lock.recovered-*' -File).Count -eq 1) `
        'The stale Git lock was not retained as recovery evidence.'

    & git -C $gitRepository switch -c reliability-feature | Out-Null
    Set-Content -LiteralPath $fixturePath -Value 'feature' -Encoding utf8
    & git -C $gitRepository add -- fixture.txt
    & git -C $gitRepository commit -m 'feature' | Out-Null
    & git -C $gitRepository switch $baseBranch | Out-Null
    Set-Content -LiteralPath $fixturePath -Value 'base-conflict' -Encoding utf8
    & git -C $gitRepository add -- fixture.txt
    & git -C $gitRepository commit -m 'base conflict' | Out-Null

    & git -C $gitRepository merge reliability-feature 2>$null | Out-Null
    Assert-ReliabilityContract `
        ($LASTEXITCODE -ne 0) `
        'The reliability fixture did not produce the expected merge conflict.'
    Assert-ReliabilityContract `
        (Test-Path -LiteralPath (Join-Path $gitDirectory 'MERGE_HEAD')) `
        'The reliability fixture did not enter a merge state.'

    $mergeRepair = Repair-HermesDesktopGitOperationState `
        -Repository $gitRepository `
        -Description 'Reliability merge fixture'
    Assert-ReliabilityContract `
        ([bool]$mergeRepair.Repaired) `
        'The updater did not report interrupted merge recovery.'
    Assert-ReliabilityContract `
        (-not (Test-Path -LiteralPath (Join-Path $gitDirectory 'MERGE_HEAD')) `
        'The updater left the interrupted merge active.'
    Assert-ReliabilityContract `
        ([string]::IsNullOrWhiteSpace((
            Invoke-HermesDesktopNestedSourceGit `
                -Repository $gitRepository `
                -Arguments @('status', '--porcelain=v1', '--untracked-files=all')
        ).Text)) `
        'The updater did not return the recovered Git fixture to a clean state.'

    $entryText = Get-Content -Raw -LiteralPath (
        Join-Path $repositoryRoot 'Invoke-Hermes-DesktopUpdate.ps1'
    )
    Assert-ReliabilityContract `
        ($entryText.IndexOf("'DesktopUpdate-Reliability.ps1'") -gt
            $entryText.IndexOf("'DesktopUpdate-SafeActivation.ps1'")) `
        'The reliability overrides are not loaded after the normal updater implementation.'

    $reliabilityText = Get-Content -Raw -LiteralPath (
        Join-Path $partsRoot 'DesktopUpdate-Reliability.ps1'
    )
    Assert-ReliabilityContract `
        ($reliabilityText -notmatch '(?im)\bgit\s+clean\b') `
        'The updater reliability layer may delete untracked source files.'
    Assert-ReliabilityContract `
        ($reliabilityText.Contains('hermes-agent-working-tree-stash.json')) `
        'Fresh-clone recovery is not gated by nested-source stash preservation.'

    Write-Host 'Hermes Desktop updater reliability tests passed.'
} finally {
    Remove-Variable -Name HermesDesktopUpdateActivePlan -Scope Script -ErrorAction SilentlyContinue
    if (Test-Path -LiteralPath $tempRoot) {
        Remove-Item -LiteralPath $tempRoot -Recurse -Force
    }
}
