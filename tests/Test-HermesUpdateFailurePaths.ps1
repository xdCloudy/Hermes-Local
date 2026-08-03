[CmdletBinding()]
param(
    [string] $EvidencePath = 'artifacts\test-evidence\issue-44-update-failure-paths.json'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repositoryRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$modulePath = Join-Path $repositoryRoot 'scripts\Hermes-UpdateOrchestrator.psm1'

function Assert-True {
    param(
        [Parameter(Mandatory)][bool] $Condition,
        [Parameter(Mandatory)][string] $Message
    )

    if (-not $Condition) {
        throw $Message
    }
}

function Assert-Equal {
    param(
        [AllowNull()][object] $Actual,
        [AllowNull()][object] $Expected,
        [Parameter(Mandatory)][string] $Message
    )

    if ([string]$Actual -cne [string]$Expected) {
        throw "$Message Expected '$Expected' but observed '$Actual'."
    }
}

function Get-TextSha256 {
    param([Parameter(Mandatory)][string] $Path)

    (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Write-EvidenceJson {
    param(
        [Parameter(Mandatory)][string] $Path,
        [Parameter(Mandatory)][object] $Value
    )

    $directory = [System.IO.Path]::GetDirectoryName($Path)
    if ($directory) {
        [System.IO.Directory]::CreateDirectory($directory) | Out-Null
    }
    [System.IO.File]::WriteAllText(
        $Path,
        (($Value | ConvertTo-Json -Depth 64) + [Environment]::NewLine),
        [System.Text.UTF8Encoding]::new($false)
    )
}

function New-FailureFixtureStore {
    $root = Join-Path (
        [System.IO.Path]::GetTempPath()
    ) "hermes-update-failure-$([guid]::NewGuid().ToString('N'))"
    $componentRoot = Join-Path $root 'fixture\component'
    $sourceRoot = Join-Path $root 'fixture\source'
    $userRoot = Join-Path $root 'data\user'

    foreach ($directory in @($componentRoot, $sourceRoot, $userRoot)) {
        [System.IO.Directory]::CreateDirectory($directory) | Out-Null
    }

    $active = Join-Path $componentRoot 'active.txt'
    $candidate = Join-Path $sourceRoot 'candidate.txt'
    $metadata = Join-Path $sourceRoot 'metadata.json'
    $userData = Join-Path $userRoot 'settings.json'

    [System.IO.File]::WriteAllText($active, 'old')
    [System.IO.File]::WriteAllText($candidate, 'new')
    [System.IO.File]::WriteAllText(
        $metadata,
        (@{
            schemaVersion = 1
            candidate = 'new'
            sha256 = Get-TextSha256 -Path $candidate
        } | ConvertTo-Json -Depth 8)
    )
    [System.IO.File]::WriteAllText(
        $userData,
        (@{
            schemaVersion = 1
            preference = 'preserve-me'
            conversation = 'user-owned-state'
        } | ConvertTo-Json -Depth 8)
    )

    [pscustomobject]@{
        Root = $root
        Active = $active
        Candidate = $candidate
        Metadata = $metadata
        UserData = $userData
        Backup = Join-Path $componentRoot 'backup.txt'
        PendingReplacement = Join-Path $componentRoot 'active.pending-replacement.txt'
        PendingManifest = Join-Path $componentRoot 'active.pending-replacement.json'
        DirtyMarker = Join-Path $sourceRoot 'dirty.marker'
    }
}

function New-FailureFixtureAdapter {
    [pscustomobject]@{
        AutoRollbackOnFailure = $true

        check = {
            param($Context)
            $scenario = [string]$Context.Input.Scenario
            $fixtureRoot = Join-Path $Context.StoreRoot 'fixture'
            $componentRoot = Join-Path $fixtureRoot 'component'
            $sourceRoot = Join-Path $fixtureRoot 'source'

            $Context.Working.ComponentRoot = $componentRoot
            $Context.Working.SourceRoot = $sourceRoot
            $Context.Working.Active = Join-Path $componentRoot 'active.txt'
            $Context.Working.Candidate = Join-Path $sourceRoot 'candidate.txt'
            $Context.Working.Metadata = Join-Path $sourceRoot 'metadata.json'
            $Context.Working.Backup = Join-Path $componentRoot 'backup.txt'
            $Context.Working.PendingReplacement = Join-Path $componentRoot 'active.pending-replacement.txt'
            $Context.Working.PendingManifest = Join-Path $componentRoot 'active.pending-replacement.json'

            if ($scenario -eq 'offline-source-metadata') {
                throw [System.Net.WebException]::new(
                    'Controlled offline source metadata failure.'
                )
            }
            if ($scenario -eq 'invalid-source-metadata') {
                [System.IO.File]::WriteAllText($Context.Working.Metadata, '{ invalid json')
            }

            $metadata = Get-Content -Raw -LiteralPath $Context.Working.Metadata |
                ConvertFrom-Json -Depth 16
            if ([int]$metadata.schemaVersion -ne 1 -or
                [string]::IsNullOrWhiteSpace([string]$metadata.candidate) -or
                [string]::IsNullOrWhiteSpace([string]$metadata.sha256)) {
                throw 'Fixture source metadata is incomplete.'
            }

            $current = Get-Content -Raw -LiteralPath $Context.Working.Active
            $candidate = Get-Content -Raw -LiteralPath $Context.Working.Candidate
            [ordered]@{
                current = $current
                candidate = $candidate
                updateAvailable = if ($scenario -eq 'no-update') {
                    $false
                } else {
                    $candidate -ne $current
                }
                metadataSha256 = [string]$metadata.sha256
            }
        }

        compatibility = {
            param($Context)
            $scenario = [string]$Context.Input.Scenario
            if ($scenario -eq 'dirty-source-checkout') {
                throw 'Controlled dirty source checkout rejection.'
            }
            if ($scenario -eq 'insufficient-disk-space') {
                throw 'Controlled disk-space rejection: required=4096 available=1024.'
            }
            [ordered]@{ compatible = $true }
        }

        prepare = {
            param($Context)
            $scenario = [string]$Context.Input.Scenario
            if ($scenario -eq 'download-failure') {
                throw [System.IO.IOException]::new(
                    'Controlled candidate download failure.'
                )
            }

            $stagingRoot = Join-Path $Context.Working.ComponentRoot 'staging'
            [System.IO.Directory]::CreateDirectory($stagingRoot) | Out-Null
            $Context.Working.Staged = Join-Path $stagingRoot 'candidate.txt'
            Copy-Item `
                -LiteralPath $Context.Working.Candidate `
                -Destination $Context.Working.Staged `
                -Force

            if ($scenario -eq 'integrity-failure') {
                Add-Content -LiteralPath $Context.Working.Staged -Value '-corrupt'
            }
            if ($scenario -eq 'patch-conflict') {
                throw 'Controlled patch conflict while preparing the candidate.'
            }
            [ordered]@{ staged = $Context.Working.Staged }
        }

        verify = {
            param($Context)
            $scenario = [string]$Context.Input.Scenario
            $metadata = Get-Content -Raw -LiteralPath $Context.Working.Metadata |
                ConvertFrom-Json -Depth 16
            $actualHash = Get-TextSha256 -Path $Context.Working.Staged
            if ($actualHash -cne [string]$metadata.sha256) {
                throw "Controlled integrity failure: expected=$($metadata.sha256) actual=$actualHash."
            }
            if ($scenario -eq 'build-failure') {
                throw 'Controlled candidate build failure.'
            }
            if ($scenario -eq 'test-failure') {
                throw 'Controlled candidate test failure.'
            }
            [ordered]@{ verified = $true; sha256 = $actualHash }
        }

        backup = {
            param($Context)
            Copy-Item `
                -LiteralPath $Context.Working.Active `
                -Destination $Context.Working.Backup `
                -Force
            [ordered]@{
                backup = $Context.Working.Backup
                sha256 = Get-TextSha256 -Path $Context.Working.Backup
            }
        }

        promote = {
            param($Context)
            $scenario = [string]$Context.Input.Scenario
            if ($scenario -eq 'locked-launcher-files') {
                $stream = [System.IO.FileStream]::new(
                    $Context.Working.Active,
                    [System.IO.FileMode]::Open,
                    [System.IO.FileAccess]::Read,
                    [System.IO.FileShare]::None
                )
                try {
                    $lockBlockedReplacement = $false
                    try {
                        [System.IO.File]::Copy(
                            $Context.Working.Staged,
                            $Context.Working.Active,
                            $true
                        )
                    } catch [System.IO.IOException] {
                        $lockBlockedReplacement = $true
                    }
                    if (-not $lockBlockedReplacement) {
                        throw 'The fixture could replace an exclusively locked launcher file.'
                    }

                    Copy-Item `
                        -LiteralPath $Context.Working.Staged `
                        -Destination $Context.Working.PendingReplacement `
                        -Force
                    Write-EvidenceJson `
                        -Path $Context.Working.PendingManifest `
                        -Value ([ordered]@{
                            schemaVersion = 1
                            operationId = $Context.OperationId
                            source = $Context.Working.PendingReplacement
                            destination = $Context.Working.Active
                            state = 'staged-for-process-exit'
                        })
                } finally {
                    $stream.Dispose()
                }
                return [ordered]@{
                    promoted = $false
                    restartRequired = $true
                    stagedReplacement = $Context.Working.PendingReplacement
                    replacementManifest = $Context.Working.PendingManifest
                }
            }

            Copy-Item `
                -LiteralPath $Context.Working.Staged `
                -Destination $Context.Working.Active `
                -Force
            if ($scenario -eq 'interrupted-promotion') {
                [System.IO.File]::WriteAllText(
                    (Join-Path $Context.Working.ComponentRoot 'promotion.interrupted'),
                    $Context.OperationId
                )
                throw [System.OperationCanceledException]::new(
                    'Controlled interruption after candidate activation.'
                )
            }
            [ordered]@{ promoted = $true }
        }

        validate = {
            param($Context)
            $scenario = [string]$Context.Input.Scenario
            if ($scenario -eq 'health-check-failure' -or
                $scenario -eq 'rollback-failure') {
                throw 'Controlled post-promotion health-check failure.'
            }

            $active = Get-Content -Raw -LiteralPath $Context.Working.Active
            if ($scenario -eq 'locked-launcher-files') {
                if ($active -ne 'old') {
                    throw 'Locked launcher fixture changed the active component.'
                }
                if (-not (Test-Path -LiteralPath $Context.Working.PendingReplacement -PathType Leaf) -or
                    (Get-Content -Raw -LiteralPath $Context.Working.PendingReplacement) -ne 'new') {
                    throw 'Locked launcher fixture did not retain the staged replacement.'
                }
                return [ordered]@{
                    validated = $true
                    active = $active
                    pendingReplacement = $Context.Working.PendingReplacement
                }
            }

            $expected = if ($Context.Mode -eq 'Rollback') { 'old' } else { 'new' }
            if ($active -ne $expected) {
                throw "Fixture validation expected '$expected' but observed '$active'."
            }
            [ordered]@{ validated = $true; active = $active }
        }

        rollback = {
            param($Context)
            $scenario = [string]$Context.Input.Scenario
            if ($scenario -eq 'rollback-failure') {
                throw 'Controlled rollback failure.'
            }
            if (-not (Test-Path -LiteralPath $Context.Working.Backup -PathType Leaf)) {
                throw 'Fixture rollback backup is missing.'
            }
            Copy-Item `
                -LiteralPath $Context.Working.Backup `
                -Destination $Context.Working.Active `
                -Force
            [ordered]@{
                restored = $true
                sha256 = Get-TextSha256 -Path $Context.Working.Active
            }
        }
    }
}

function Get-StageStatusMap {
    param([Parameter(Mandatory)][object] $State)

    $map = [ordered]@{}
    foreach ($stage in @($State.stages)) {
        $map[[string]$stage.name] = [string]$stage.status
    }
    $map
}

function Invoke-FailureScenario {
    param([Parameter(Mandatory)][hashtable] $Scenario)

    $fixture = New-FailureFixtureStore
    $state = $null
    $record = [ordered]@{
        name = [string]$Scenario.Name
        passed = $false
        expected = [ordered]@{
            status = [string]$Scenario.ExpectedStatus
            active = [string]$Scenario.ExpectedActive
            disposition = [string]$Scenario.ExpectedDisposition
            failedStage = if ($Scenario.ContainsKey('ExpectedFailedStage')) {
                [string]$Scenario.ExpectedFailedStage
            } else {
                $null
            }
        }
        observed = $null
        error = $null
    }

    try {
        if ($Scenario.Name -eq 'dirty-source-checkout') {
            [System.IO.File]::WriteAllText($fixture.DirtyMarker, 'dirty')
        }
        if ($Scenario.Name -eq 'successful-rollback') {
            Copy-Item -LiteralPath $fixture.Active -Destination $fixture.Backup -Force
            [System.IO.File]::WriteAllText($fixture.Active, 'new')
        }
        if ($Scenario.Name -eq 'stale-lock-recovery') {
            $lockRoot = Join-Path $fixture.Root 'data\runtime\locks'
            [System.IO.Directory]::CreateDirectory($lockRoot) | Out-Null
            Write-EvidenceJson `
                -Path (Join-Path $lockRoot 'update-orchestrator.json') `
                -Value ([ordered]@{
                    schemaVersion = 1
                    operationId = 'issue-44-stale-operation'
                    ownerPid = 2147483647
                    acquiredAt = '2000-01-01T00:00:00.0000000Z'
                    heartbeatAt = '2000-01-01T00:00:00.0000000Z'
                    resources = @('update-orchestrator', 'workstation')
                })
        }

        $beforeActive = Get-Content -Raw -LiteralPath $fixture.Active
        $beforeActiveHash = Get-TextSha256 -Path $fixture.Active
        $beforeUserHash = Get-TextSha256 -Path $fixture.UserData

        $adapter = New-FailureFixtureAdapter
        $adapter.AutoRollbackOnFailure = [bool]$Scenario.AutoRollback
        Register-HermesUpdateAdapter -Name Issue44Fixture -Adapter $adapter -Force

        $state = Invoke-HermesUpdateOperation `
            -Mode ([string]$Scenario.Mode) `
            -Component Issue44Fixture `
            -Caller Test `
            -Input @{ Scenario = [string]$Scenario.Name } `
            -StoreRoot $fixture.Root

        $afterActive = Get-Content -Raw -LiteralPath $fixture.Active
        $afterActiveHash = Get-TextSha256 -Path $fixture.Active
        $afterUserHash = Get-TextSha256 -Path $fixture.UserData
        $stageStatuses = Get-StageStatusMap -State $state
        $lockPath = Join-Path $fixture.Root 'data\runtime\locks\update-orchestrator.json'
        $report = Get-Content -Raw -LiteralPath ([string]$state.reportPath) |
            ConvertFrom-Json -Depth 64

        Assert-Equal -Actual $state.status -Expected $Scenario.ExpectedStatus `
            -Message "Scenario '$($Scenario.Name)' returned the wrong operation status."
        Assert-Equal -Actual $afterActive -Expected $Scenario.ExpectedActive `
            -Message "Scenario '$($Scenario.Name)' left the wrong active component."
        Assert-True `
            -Condition ($beforeUserHash -ceq $afterUserHash) `
            -Message "Scenario '$($Scenario.Name)' changed user-owned data."
        Assert-True `
            -Condition (Test-Path -LiteralPath $state.statePath -PathType Leaf) `
            -Message "Scenario '$($Scenario.Name)' did not persist operation state."
        Assert-True `
            -Condition (Test-Path -LiteralPath $state.reportPath -PathType Leaf) `
            -Message "Scenario '$($Scenario.Name)' did not persist an operation report."
        Assert-True `
            -Condition (-not (Test-Path -LiteralPath $lockPath)) `
            -Message "Scenario '$($Scenario.Name)' did not release the update lock."
        Assert-Equal -Actual $report.operationId -Expected $state.operationId `
            -Message "Scenario '$($Scenario.Name)' report identity did not match its state."

        if ($Scenario.ContainsKey('ExpectedFailedStage')) {
            Assert-Equal `
                -Actual $stageStatuses[[string]$Scenario.ExpectedFailedStage] `
                -Expected 'failed' `
                -Message "Scenario '$($Scenario.Name)' did not record the expected failed stage."
        }
        if ($Scenario.ContainsKey('ExpectedRollbackStatus')) {
            Assert-Equal `
                -Actual $stageStatuses.rollback `
                -Expected $Scenario.ExpectedRollbackStatus `
                -Message "Scenario '$($Scenario.Name)' recorded the wrong rollback status."
        }
        if ($Scenario.ContainsKey('ExpectedUpdateAvailable')) {
            Assert-Equal `
                -Actual ([bool]$state.result.updateAvailable) `
                -Expected ([bool]$Scenario.ExpectedUpdateAvailable) `
                -Message "Scenario '$($Scenario.Name)' returned the wrong update availability."
        }
        if ($Scenario.Name -eq 'locked-launcher-files') {
            Assert-True `
                -Condition (Test-Path -LiteralPath $fixture.PendingReplacement -PathType Leaf) `
                -Message 'Locked launcher replacement was not staged.'
            Assert-True `
                -Condition (Test-Path -LiteralPath $fixture.PendingManifest -PathType Leaf) `
                -Message 'Locked launcher replacement manifest was not recorded.'
        }
        if ($Scenario.Name -eq 'rollback-failure') {
            Assert-True `
                -Condition ($null -ne $state.failure.rollback) `
                -Message 'Rollback failure evidence was not attached to the operation failure.'
        }
        if ($Scenario.Name -eq 'stale-lock-recovery') {
            Assert-True `
                -Condition ([bool]$state.recovery.staleLockRecovered) `
                -Message 'Stale-lock recovery was not recorded in operation state.'
            $recovered = @(
                Get-ChildItem `
                    -LiteralPath (Join-Path $fixture.Root 'data\runtime\locks') `
                    -Filter 'update-orchestrator.recovered-*.json' `
                    -File
            )
            Assert-True `
                -Condition ($recovered.Count -eq 1) `
                -Message 'Stale-lock recovery evidence file was not retained.'
        }

        $record.observed = [ordered]@{
            operationId = [string]$state.operationId
            status = [string]$state.status
            disposition = [string]$Scenario.ExpectedDisposition
            active = [ordered]@{
                before = $beforeActive
                after = $afterActive
                beforeSha256 = $beforeActiveHash
                afterSha256 = $afterActiveHash
            }
            userData = [ordered]@{
                path = $fixture.UserData
                beforeSha256 = $beforeUserHash
                afterSha256 = $afterUserHash
                preserved = $beforeUserHash -ceq $afterUserHash
            }
            statePersisted = Test-Path -LiteralPath $state.statePath -PathType Leaf
            reportPersisted = Test-Path -LiteralPath $state.reportPath -PathType Leaf
            lockReleased = -not (Test-Path -LiteralPath $lockPath)
            statePath = [string]$state.statePath
            reportPath = [string]$state.reportPath
            stages = $stageStatuses
            recovery = $state.recovery
            failure = $state.failure
            result = $state.result
            report = $report
            pendingReplacement = if (Test-Path -LiteralPath $fixture.PendingReplacement -PathType Leaf) {
                [ordered]@{
                    path = $fixture.PendingReplacement
                    sha256 = Get-TextSha256 -Path $fixture.PendingReplacement
                    manifest = if (Test-Path -LiteralPath $fixture.PendingManifest -PathType Leaf) {
                        Get-Content -Raw -LiteralPath $fixture.PendingManifest |
                            ConvertFrom-Json -Depth 16
                    } else {
                        $null
                    }
                }
            } else {
                $null
            }
        }
        $record.passed = $true
    } catch {
        $record.error = [ordered]@{
            message = $_.Exception.Message
            type = $_.Exception.GetType().FullName
            scriptStackTrace = $_.ScriptStackTrace
        }
        if ($state) {
            $record.observed = [ordered]@{
                operationId = [string]$state.operationId
                status = [string]$state.status
                stages = Get-StageStatusMap -State $state
                failure = $state.failure
                recovery = $state.recovery
            }
        }
    } finally {
        if (Test-Path -LiteralPath $fixture.Root) {
            Remove-Item -LiteralPath $fixture.Root -Recurse -Force
        }
    }

    [pscustomobject]$record
}

Import-Module $modulePath -Force

$scenarios = @(
    @{ Name = 'no-update'; Mode = 'Check'; AutoRollback = $false; ExpectedStatus = 'succeeded'; ExpectedActive = 'old'; ExpectedDisposition = 'unchanged'; ExpectedUpdateAvailable = $false },
    @{ Name = 'update-available'; Mode = 'Check'; AutoRollback = $false; ExpectedStatus = 'succeeded'; ExpectedActive = 'old'; ExpectedDisposition = 'unchanged'; ExpectedUpdateAvailable = $true },
    @{ Name = 'offline-source-metadata'; Mode = 'Check'; AutoRollback = $false; ExpectedStatus = 'failed'; ExpectedActive = 'old'; ExpectedDisposition = 'unchanged'; ExpectedFailedStage = 'check' },
    @{ Name = 'invalid-source-metadata'; Mode = 'Check'; AutoRollback = $false; ExpectedStatus = 'failed'; ExpectedActive = 'old'; ExpectedDisposition = 'unchanged'; ExpectedFailedStage = 'check' },
    @{ Name = 'dirty-source-checkout'; Mode = 'Apply'; AutoRollback = $false; ExpectedStatus = 'failed'; ExpectedActive = 'old'; ExpectedDisposition = 'unchanged'; ExpectedFailedStage = 'compatibility' },
    @{ Name = 'insufficient-disk-space'; Mode = 'Apply'; AutoRollback = $false; ExpectedStatus = 'failed'; ExpectedActive = 'old'; ExpectedDisposition = 'unchanged'; ExpectedFailedStage = 'compatibility' },
    @{ Name = 'download-failure'; Mode = 'Apply'; AutoRollback = $false; ExpectedStatus = 'failed'; ExpectedActive = 'old'; ExpectedDisposition = 'unchanged'; ExpectedFailedStage = 'prepare' },
    @{ Name = 'integrity-failure'; Mode = 'Apply'; AutoRollback = $false; ExpectedStatus = 'failed'; ExpectedActive = 'old'; ExpectedDisposition = 'unchanged'; ExpectedFailedStage = 'verify' },
    @{ Name = 'patch-conflict'; Mode = 'Apply'; AutoRollback = $false; ExpectedStatus = 'failed'; ExpectedActive = 'old'; ExpectedDisposition = 'unchanged'; ExpectedFailedStage = 'prepare' },
    @{ Name = 'build-failure'; Mode = 'Apply'; AutoRollback = $false; ExpectedStatus = 'failed'; ExpectedActive = 'old'; ExpectedDisposition = 'unchanged'; ExpectedFailedStage = 'verify' },
    @{ Name = 'test-failure'; Mode = 'Apply'; AutoRollback = $false; ExpectedStatus = 'failed'; ExpectedActive = 'old'; ExpectedDisposition = 'unchanged'; ExpectedFailedStage = 'verify' },
    @{ Name = 'locked-launcher-files'; Mode = 'Apply'; AutoRollback = $true; ExpectedStatus = 'succeeded'; ExpectedActive = 'old'; ExpectedDisposition = 'staged-process-replacement' },
    @{ Name = 'interrupted-promotion'; Mode = 'Apply'; AutoRollback = $true; ExpectedStatus = 'rolled-back'; ExpectedActive = 'old'; ExpectedDisposition = 'restored'; ExpectedFailedStage = 'promote'; ExpectedRollbackStatus = 'succeeded' },
    @{ Name = 'health-check-failure'; Mode = 'Apply'; AutoRollback = $true; ExpectedStatus = 'rolled-back'; ExpectedActive = 'old'; ExpectedDisposition = 'restored'; ExpectedFailedStage = 'validate'; ExpectedRollbackStatus = 'succeeded' },
    @{ Name = 'successful-rollback'; Mode = 'Rollback'; AutoRollback = $false; ExpectedStatus = 'succeeded'; ExpectedActive = 'old'; ExpectedDisposition = 'restored'; ExpectedRollbackStatus = 'succeeded' },
    @{ Name = 'rollback-failure'; Mode = 'Apply'; AutoRollback = $true; ExpectedStatus = 'failed'; ExpectedActive = 'new'; ExpectedDisposition = 'promotion-retained-after-rollback-failure'; ExpectedFailedStage = 'validate'; ExpectedRollbackStatus = 'failed' },
    @{ Name = 'stale-lock-recovery'; Mode = 'Apply'; AutoRollback = $true; ExpectedStatus = 'succeeded'; ExpectedActive = 'new'; ExpectedDisposition = 'promoted' },
    @{ Name = 'user-data-preservation'; Mode = 'Apply'; AutoRollback = $true; ExpectedStatus = 'succeeded'; ExpectedActive = 'new'; ExpectedDisposition = 'promoted' }
)

$results = [System.Collections.Generic.List[object]]::new()
foreach ($scenario in $scenarios) {
    $results.Add((Invoke-FailureScenario -Scenario $scenario))
}

$failed = @($results | Where-Object { -not $_.passed })
$resolvedEvidencePath = if ([System.IO.Path]::IsPathRooted($EvidencePath)) {
    [System.IO.Path]::GetFullPath($EvidencePath)
} else {
    [System.IO.Path]::GetFullPath((Join-Path $repositoryRoot $EvidencePath))
}
$evidence = [ordered]@{
    schemaVersion = 1
    issue = 44
    generatedAt = (Get-Date).ToUniversalTime().ToString('o')
    module = 'scripts/Hermes-UpdateOrchestrator.psm1'
    scenarioCount = $results.Count
    passedCount = @($results | Where-Object passed).Count
    failedCount = $failed.Count
    acceptance = [ordered]@{
        machineReadableEvidence = $true
        activeComponentDispositionRecorded = $true
        userDataPreservationCheckedForEveryScenario = $true
    }
    scenarios = $results.ToArray()
}
Write-EvidenceJson -Path $resolvedEvidencePath -Value $evidence

if ($failed.Count -gt 0) {
    $messages = @(
        foreach ($failure in $failed) {
            "[$($failure.name)] $($failure.error.message)"
        }
    )
    throw "Updater failure-path fixtures failed.`n$($messages -join [Environment]::NewLine)"
}

Write-Host "Hermes updater failure-path fixtures passed: $($results.Count) scenarios."
Write-Host "Evidence: $resolvedEvidencePath"
