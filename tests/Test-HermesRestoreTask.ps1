[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repositoryRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
. (Join-Path $repositoryRoot 'scripts\restore\Restore-Common.ps1')
. (Join-Path $repositoryRoot 'scripts\restore\Restore-Reliability.ps1')

function Assert-RestoreContract {
    param(
        [Parameter(Mandatory)][bool] $Condition,
        [Parameter(Mandatory)][string] $Message
    )

    if (-not $Condition) {
        throw $Message
    }
}

function New-RestoreTestArchive {
    param(
        [Parameter(Mandatory)][string] $Root,
        [Parameter(Mandatory)][string] $Name,
        [hashtable] $Entries,
        [switch] $SkipSidecar
    )

    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $backups = Join-Path $Root 'backups'
    [IO.Directory]::CreateDirectory($backups) | Out-Null
    $path = Join-Path $backups $Name
    $archive = [IO.Compression.ZipFile]::Open($path, [IO.Compression.ZipArchiveMode]::Create)
    try {
        foreach ($item in $Entries.GetEnumerator()) {
            $entry = $archive.CreateEntry([string]$item.Key)
            $stream = $entry.Open()
            $writer = [IO.StreamWriter]::new($stream, [Text.UTF8Encoding]::new($false))
            try {
                $writer.Write([string]$item.Value)
            } finally {
                $writer.Dispose()
                $stream.Dispose()
            }
        }
    } finally {
        $archive.Dispose()
    }

    if (-not $SkipSidecar) {
        $hash = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
        [IO.File]::WriteAllText(
            "$path.sha256",
            "$hash  $Name$([Environment]::NewLine)",
            [Text.UTF8Encoding]::new($false)
        )
    }
    $path
}

function New-ValidRestoreEntries {
    @{
        'backup-manifest.json' = (@{
            schemaVersion = 1
            product = 'Hermes Local'
            createdAt = '2026-08-05T03:30:00.000Z'
            profile = 'Balanced'
            version = @{ product = @{ version = '0.18.50' } }
        } | ConvertTo-Json -Depth 8)
        'VERSION.json' = '{"schemaVersion":1}'
        'config/workstation.json' = '{"selectedProfile":"Balanced"}'
        'data/user/preferences.json' = '{"theme":"dark"}'
    }
}

$tempRoot = Join-Path ([IO.Path]::GetTempPath()) ('hermes-restore-contract-' + [guid]::NewGuid().ToString('N'))
$originalProfile = $env:USERPROFILE

try {
    [IO.Directory]::CreateDirectory($tempRoot) | Out-Null
    $env:USERPROFILE = Join-Path $tempRoot 'private-user'

    Assert-RestoreContract `
        (($script:HermesRestoreStages -join ',') -eq (
            'validation,archive-inspection,safety-snapshot,service-shutdown,extraction,' +
            'data-restoration,configuration-migration,validation-after-restore,service-restart,rollback,complete'
        )) `
        'The durable restore stage contract changed unexpectedly.'

    $validArchive = New-RestoreTestArchive `
        -Root $tempRoot `
        -Name 'Hermes-Local-valid.zip' `
        -Entries (New-ValidRestoreEntries)
    $plan = Get-HermesRestoreArchivePlan -Root $tempRoot -BackupPath $validArchive
    Assert-RestoreContract ($plan.Id -match '^[0-9a-f]{16}$') 'Valid backup identity was not derived from SHA-256.'
    Assert-RestoreContract ($plan.Profile -eq 'Balanced') 'Valid backup profile was not retained.'
    Assert-RestoreContract ($plan.FileCount -eq 4) 'Valid backup entry count is incorrect.'

    $unsafeEntries = New-ValidRestoreEntries
    $unsafeEntries['../escape.txt'] = 'unsafe'
    $unsafeArchive = New-RestoreTestArchive `
        -Root $tempRoot `
        -Name 'Hermes-Local-unsafe.zip' `
        -Entries $unsafeEntries
    $unsafeMessage = $null
    try {
        $null = Get-HermesRestoreArchivePlan -Root $tempRoot -BackupPath $unsafeArchive
    } catch {
        $unsafeMessage = $_.Exception.Message
    }
    Assert-RestoreContract `
        ($unsafeMessage -match 'Unsafe or unexpected archive entry') `
        'Path traversal was not rejected before extraction.'

    $unsignedArchive = New-RestoreTestArchive `
        -Root $tempRoot `
        -Name 'Hermes-Local-unsigned.zip' `
        -Entries (New-ValidRestoreEntries) `
        -SkipSidecar
    $unsignedMessage = $null
    try {
        $null = Get-HermesRestoreArchivePlan -Root $tempRoot -BackupPath $unsignedArchive
    } catch {
        $unsignedMessage = $_.Exception.Message
    }
    Assert-RestoreContract `
        ($unsignedMessage -match 'integrity sidecar is missing') `
        'A backup without integrity evidence was accepted.'

    $context = New-HermesRestoreContext `
        -Root $tempRoot `
        -TaskId 'restore-contract-task' `
        -BackupPath $validArchive
    $request = @{
        schemaVersion = 1
        taskId = $context.TaskId
        ownerPid = $PID
        requestedAt = (Get-Date).ToUniversalTime().ToString('o')
    }
    Write-HermesRestoreAtomicJson -Path $context.CancelPath -Document $request
    Assert-RestoreContract `
        (Test-HermesRestoreCancellationRequested -Context $context) `
        'Matching cooperative cancellation request was not recognised.'
    $context.Cancellable = $false
    Assert-RestoreContract `
        (-not (Test-HermesRestoreCancellationRequested -Context $context)) `
        'Cancellation remained available after the destructive boundary.'
    Remove-HermesRestoreCancellationRequest -Context $context

    $transactionRoot = Join-Path $tempRoot 'transaction'
    $transactionStaging = Join-Path $tempRoot 'transaction-staging'
    [IO.Directory]::CreateDirectory((Join-Path $transactionRoot 'config')) | Out-Null
    [IO.Directory]::CreateDirectory((Join-Path $transactionRoot 'data\user')) | Out-Null
    [IO.Directory]::CreateDirectory((Join-Path $transactionStaging 'config')) | Out-Null
    [IO.Directory]::CreateDirectory((Join-Path $transactionStaging 'data\user')) | Out-Null
    Set-Content -LiteralPath (Join-Path $transactionRoot 'config\state.txt') -Value 'original' -Encoding utf8
    Set-Content -LiteralPath (Join-Path $transactionRoot 'data\user\state.txt') -Value 'original-user' -Encoding utf8
    Set-Content -LiteralPath (Join-Path $transactionStaging 'config\state.txt') -Value 'restored' -Encoding utf8
    Set-Content -LiteralPath (Join-Path $transactionStaging 'data\user\state.txt') -Value 'restored-user' -Encoding utf8

    $transactionContext = New-HermesRestoreContext `
        -Root $transactionRoot `
        -TaskId 'transaction-success' `
        -BackupPath $validArchive
    $rollbackRoot = Join-Path $tempRoot 'transaction-rollback'
    $journal = @(
        Invoke-HermesRestorePromotion `
            -Context $transactionContext `
            -StagingRoot $transactionStaging `
            -RollbackRoot $rollbackRoot
    )
    Assert-RestoreContract `
        ((Get-Content -Raw -LiteralPath (Join-Path $transactionRoot 'config\state.txt')).Trim() -eq 'restored') `
        'Restore promotion did not activate staged configuration.'

    $rollback = Invoke-HermesRestoreRollback `
        -Context $transactionContext `
        -Journal $journal `
        -FailedStateRoot (Join-Path $tempRoot 'transaction-failed-state')
    Assert-RestoreContract ([bool]$rollback.Succeeded) 'A valid rollback did not succeed.'
    Assert-RestoreContract `
        ((Get-Content -Raw -LiteralPath (Join-Path $transactionRoot 'config\state.txt')).Trim() -eq 'original') `
        'Rollback did not restore the original configuration.'

    $failureRoot = Join-Path $tempRoot 'rollback-failure-root'
    $failureStaging = Join-Path $tempRoot 'rollback-failure-staging'
    [IO.Directory]::CreateDirectory((Join-Path $failureRoot 'config')) | Out-Null
    [IO.Directory]::CreateDirectory((Join-Path $failureStaging 'config')) | Out-Null
    Set-Content -LiteralPath (Join-Path $failureRoot 'config\state.txt') -Value 'original' -Encoding utf8
    Set-Content -LiteralPath (Join-Path $failureStaging 'config\state.txt') -Value 'restored' -Encoding utf8
    $failureContext = New-HermesRestoreContext `
        -Root $failureRoot `
        -TaskId 'transaction-failure' `
        -BackupPath $validArchive
    $failureJournal = @(
        Invoke-HermesRestorePromotion `
            -Context $failureContext `
            -StagingRoot $failureStaging `
            -RollbackRoot (Join-Path $tempRoot 'rollback-failure-rollback')
    )
    $failedRollback = Invoke-HermesRestoreRollback `
        -Context $failureContext `
        -Journal $failureJournal `
        -FailedStateRoot (Join-Path $tempRoot 'rollback-failure-evidence') `
        -BeforeScope {
            param($phase, $scope)
            if ($phase -eq 'rollback' -and $scope -eq 'config') {
                throw 'simulated rollback failure'
            }
        }
    Assert-RestoreContract (-not [bool]$failedRollback.Succeeded) 'Rollback failure was not surfaced.'
    Assert-RestoreContract `
        (($failedRollback.Errors -join '; ') -match 'simulated rollback failure') `
        'Rollback failure evidence did not retain its cause.'

    $context = New-HermesRestoreContext `
        -Root $tempRoot `
        -TaskId 'terminal-report' `
        -BackupPath $validArchive
    $context.Backup = $plan
    Complete-HermesRestore `
        -Context $context `
        -Status 'failed' `
        -Message "Restore failed at $env:USERPROFILE with token=secret-value." `
        -FailureCode 'restore-contract-failure' `
        -FailureMessage "password=secret-value in $env:USERPROFILE"
    $progress = Get-Content -Raw -LiteralPath $context.ProgressPath | ConvertFrom-Json -Depth 32
    $report = Get-Content -Raw -LiteralPath $context.ReportPath | ConvertFrom-Json -Depth 32
    Assert-RestoreContract ($progress.status -eq 'failed') 'Terminal progress did not persist failure state.'
    Assert-RestoreContract ($progress.result.report -eq 'logs/restore/restore-terminal-report.json') 'Progress report link is not relative.'
    Assert-RestoreContract ($report.failure.code -eq 'restore-contract-failure') 'Report failure identity was lost.'
    Assert-RestoreContract `
        (-not ((Get-Content -Raw -LiteralPath $context.ReportPath).Contains($env:USERPROFILE))) `
        'Restore report leaked a private path.'
    Assert-RestoreContract `
        (-not ((Get-Content -Raw -LiteralPath $context.ReportPath).Contains('secret-value'))) `
        'Restore report leaked a credential value.'

    $entryScript = Get-Content -Raw -LiteralPath (Join-Path $repositoryRoot 'Restore-Hermes-Local.ps1')
    Assert-RestoreContract ($entryScript.Contains('New-HermesRestoreSafetySnapshot')) 'Restore no longer creates a safety snapshot.'
    Assert-RestoreContract ($entryScript.Contains('Invoke-HermesRestoreRollback')) 'Restore no longer contains transactional rollback.'
    Assert-RestoreContract ($entryScript.Contains('HERMES_LOCAL_TASK_ID')) 'Restore task identity is not bridged from Desktop.'

    Write-Host 'Hermes Local durable restore contract tests passed.'
} finally {
    if ($originalProfile -eq $null) {
        Remove-Item Env:USERPROFILE -ErrorAction SilentlyContinue
    } else {
        $env:USERPROFILE = $originalProfile
    }
    if (Test-Path -LiteralPath $tempRoot) {
        Remove-Item -LiteralPath $tempRoot -Recurse -Force
    }
}
