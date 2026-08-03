Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repositoryRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$modulePath = Join-Path $repositoryRoot 'scripts\Hermes-UpdateOrchestrator.psm1'

function Assert-True {
    param(
        [Parameter(Mandatory)]
        [bool] $Condition,

        [Parameter(Mandatory)]
        [string] $Message
    )

    if (-not $Condition) {
        throw $Message
    }
}

function New-FixtureStore {
    $root = Join-Path ([System.IO.Path]::GetTempPath()) "hermes-update-fixture-$([guid]::NewGuid().ToString('N'))"
    $fixture = Join-Path $root 'fixture'
    [System.IO.Directory]::CreateDirectory($fixture) | Out-Null
    [System.IO.File]::WriteAllText((Join-Path $fixture 'active.txt'), 'old', [System.Text.UTF8Encoding]::new($false))
    [System.IO.File]::WriteAllText((Join-Path $fixture 'candidate.txt'), 'new', [System.Text.UTF8Encoding]::new($false))
    return $root
}

function New-FixtureAdapter {
    return [pscustomobject]@{
        AutoRollbackOnFailure = $true

        check = {
            param($Context)
            $fixture = Join-Path $Context.StoreRoot 'fixture'
            $Context.Working.Fixture = $fixture
            $Context.Working.Active = Join-Path $fixture 'active.txt'
            $Context.Working.Candidate = Join-Path $fixture 'candidate.txt'
            $Context.Working.Backup = Join-Path $fixture 'backup.txt'
            [ordered]@{
                current = Get-Content -Raw -LiteralPath $Context.Working.Active
                candidate = Get-Content -Raw -LiteralPath $Context.Working.Candidate
                updateAvailable = $true
            }
        }

        compatibility = {
            param($Context)
            if (-not (Test-Path -LiteralPath $Context.Working.Active -PathType Leaf)) {
                throw 'Fixture active file is missing.'
            }
            [ordered]@{ compatible = $true }
        }

        prepare = {
            param($Context)
            $staging = Join-Path $Context.Working.Fixture 'staging'
            [System.IO.Directory]::CreateDirectory($staging) | Out-Null
            Copy-Item -LiteralPath $Context.Working.Candidate -Destination (Join-Path $staging 'candidate.txt') -Force
            $Context.Working.Staged = Join-Path $staging 'candidate.txt'
            [ordered]@{ staged = $Context.Working.Staged }
        }

        verify = {
            param($Context)
            if (-not (Test-Path -LiteralPath $Context.Working.Staged -PathType Leaf)) {
                throw 'Fixture candidate was not staged.'
            }
            [ordered]@{ verified = $true }
        }

        backup = {
            param($Context)
            Copy-Item -LiteralPath $Context.Working.Active -Destination $Context.Working.Backup -Force
            [ordered]@{ backup = $Context.Working.Backup }
        }

        promote = {
            param($Context)
            Copy-Item -LiteralPath $Context.Working.Staged -Destination $Context.Working.Active -Force
            if ($Context.Input.ContainsKey('FailPromote') -and [bool]$Context.Input.FailPromote) {
                throw 'Controlled fixture promotion failure.'
            }
            [ordered]@{ promoted = $true }
        }

        validate = {
            param($Context)
            $actual = Get-Content -Raw -LiteralPath $Context.Working.Active
            $expected = if ($Context.Mode -eq 'Rollback') { 'old' } else { 'new' }
            if ($actual -ne $expected) {
                throw "Fixture validation expected '$expected' but observed '$actual'."
            }
            [ordered]@{ validated = $true; value = $actual }
        }

        rollback = {
            param($Context)
            if (-not (Test-Path -LiteralPath $Context.Working.Backup -PathType Leaf)) {
                throw 'Fixture rollback backup is missing.'
            }
            Copy-Item -LiteralPath $Context.Working.Backup -Destination $Context.Working.Active -Force
            [ordered]@{ restored = $true }
        }
    }
}

function Register-FixtureAdapter {
    Register-HermesUpdateAdapter -Name Fixture -Adapter (New-FixtureAdapter) -Force
}

function Get-NormalizedOperation {
    param(
        [Parameter(Mandatory)]
        [object] $State
    )

    return [ordered]@{
        component = [string]$State.identity.component
        mode = [string]$State.identity.mode
        status = [string]$State.status
        stages = @(
            $State.stages | ForEach-Object {
                [ordered]@{
                    name = [string]$_.name
                    status = [string]$_.status
                }
            }
        )
    }
}

Import-Module $modulePath -Force
Register-FixtureAdapter

$stores = [System.Collections.Generic.List[string]]::new()
try {
    $callerStates = [ordered]@{}
    foreach ($caller in @('Cli', 'Desktop')) {
        $store = New-FixtureStore
        $stores.Add($store)
        $state = Invoke-HermesUpdateOperation `
            -Mode Apply `
            -Component Fixture `
            -Caller $caller `
            -StoreRoot $store

        Assert-True ($state.status -eq 'succeeded') "$caller fixture apply did not succeed."
        Assert-True ((Get-Content -Raw -LiteralPath (Join-Path $store 'fixture\active.txt')) -eq 'new') "$caller fixture was not promoted."
        Assert-True (Test-Path -LiteralPath $state.statePath -PathType Leaf) "$caller state was not persisted."
        Assert-True (Test-Path -LiteralPath $state.reportPath -PathType Leaf) "$caller report was not persisted."

        $callerStates[$caller] = Get-NormalizedOperation -State $state
    }

    $cliJson = $callerStates.Cli | ConvertTo-Json -Depth 16 -Compress
    $desktopJson = $callerStates.Desktop | ConvertTo-Json -Depth 16 -Compress
    Assert-True ($cliJson -eq $desktopJson) 'CLI and Desktop callers produced different normalized state.'

    $durableStore = New-FixtureStore
    $stores.Add($durableStore)
    $durable = Invoke-HermesUpdateOperation -Mode Apply -Component Fixture -Caller Desktop -StoreRoot $durableStore
    $durableId = [string]$durable.operationId

    Remove-Module Hermes-UpdateOrchestrator -Force
    Import-Module $modulePath -Force
    $reloaded = Get-HermesUpdateOperation -OperationId $durableId -StoreRoot $durableStore
    Assert-True ($null -ne $reloaded) 'The operation disappeared after the caller module exited.'
    Assert-True ($reloaded.status -eq 'succeeded') 'Reloaded operation state was not successful.'

    Register-FixtureAdapter
    $failedStore = New-FixtureStore
    $stores.Add($failedStore)
    $failed = Invoke-HermesUpdateOperation `
        -Mode Apply `
        -Component Fixture `
        -Caller Cli `
        -Input @{ FailPromote = $true } `
        -StoreRoot $failedStore

    Assert-True ($failed.status -eq 'rolled-back') 'A failed fixture promotion was not rolled back.'
    Assert-True ((Get-Content -Raw -LiteralPath (Join-Path $failedStore 'fixture\active.txt')) -eq 'old') 'Rollback did not restore the fixture.'
    $promoteStage = @($failed.stages | Where-Object name -eq 'promote')[0]
    $rollbackStage = @($failed.stages | Where-Object name -eq 'rollback')[0]
    Assert-True ($promoteStage.status -eq 'failed') 'The failed promotion stage was not recorded.'
    Assert-True ($rollbackStage.status -eq 'succeeded') 'The rollback stage was not recorded as successful.'

    $rollbackStore = New-FixtureStore
    $stores.Add($rollbackStore)
    Copy-Item -LiteralPath (Join-Path $rollbackStore 'fixture\active.txt') -Destination (Join-Path $rollbackStore 'fixture\backup.txt') -Force
    [System.IO.File]::WriteAllText(
        (Join-Path $rollbackStore 'fixture\active.txt'),
        'new',
        [System.Text.UTF8Encoding]::new($false)
    )
    $rolledBack = Invoke-HermesUpdateOperation -Mode Rollback -Component Fixture -Caller Desktop -StoreRoot $rollbackStore
    Assert-True ($rolledBack.status -eq 'succeeded') 'Explicit fixture rollback did not succeed.'
    Assert-True ((Get-Content -Raw -LiteralPath (Join-Path $rollbackStore 'fixture\active.txt')) -eq 'old') 'Explicit rollback did not restore old content.'

    $staleStore = New-FixtureStore
    $stores.Add($staleStore)
    $lockRoot = Join-Path $staleStore 'data\runtime\locks'
    [System.IO.Directory]::CreateDirectory($lockRoot) | Out-Null
    @{
        schemaVersion = 1
        operationId = 'stale-operation'
        ownerPid = 2147483647
        acquiredAt = '2000-01-01T00:00:00.0000000Z'
        heartbeatAt = '2000-01-01T00:00:00.0000000Z'
        resources = @('update-orchestrator', 'workstation')
    } | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $lockRoot 'update-orchestrator.json') -Encoding utf8

    $staleRecovered = Invoke-HermesUpdateOperation -Mode Apply -Component Fixture -Caller Cli -StoreRoot $staleStore
    Assert-True ($staleRecovered.status -eq 'succeeded') 'Operation did not continue after stale-lock recovery.'
    Assert-True ([bool]$staleRecovered.recovery.staleLockRecovered) 'Stale-lock recovery was not recorded.'
    Assert-True (
        @(Get-ChildItem -LiteralPath $lockRoot -Filter 'update-orchestrator.recovered-*.json' -File).Count -eq 1
    ) 'Recovered lock evidence was not retained.'

    $nativeRejected = $false
    try {
        $null = Assert-HermesUpdateNativeArguments -FilePath 'pwsh.exe' -ArgumentList @('safe', "bad`nvalue")
    } catch {
        $nativeRejected = $true
    }
    Assert-True $nativeRejected 'Unsafe native arguments were accepted.'

    Write-Host 'Hermes update orchestration contract tests passed.'
} finally {
    foreach ($store in $stores) {
        if (Test-Path -LiteralPath $store) {
            Remove-Item -LiteralPath $store -Recurse -Force
        }
    }
}
