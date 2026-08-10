Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

Import-Module (Join-Path $PSScriptRoot 'Common-Hermes.psm1')
Import-Module (Join-Path $PSScriptRoot 'Hermes-Configuration.psm1')
Import-Module (Join-Path $PSScriptRoot 'Hermes-UpdateOrchestrator.psm1')
Import-Module (Join-Path $PSScriptRoot 'Hermes-RuntimeManager.psm1')

function Write-HermesRuntimeUpdateStage {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [ValidateSet('check', 'compatibility', 'prepare', 'verify', 'backup', 'promote', 'validate', 'rollback')]
        [string] $Stage,
        [Parameter(Mandatory)][string] $Message
    )

    $safeMessage = ([string]$Message).Replace("`r", ' ').Replace("`n", ' ').Trim()
    Write-Host ("::hermes-update-stage::{0}::{1}" -f $Stage, $safeMessage)
}

function Get-HermesRuntimeUserProfileSnapshot {
    [CmdletBinding()]
    param()

    $path = Resolve-HermesPath 'config\launcher\user-settings.json'
    if (Test-Path -LiteralPath $path -PathType Leaf) {
        return [pscustomobject]@{
            Path = $path
            Existed = $true
            Bytes = [System.IO.File]::ReadAllBytes($path)
        }
    }

    [pscustomobject]@{
        Path = $path
        Existed = $false
        Bytes = $null
    }
}

function Restore-HermesRuntimeUserProfileSnapshot {
    [CmdletBinding()]
    param([Parameter(Mandatory)][pscustomobject] $Snapshot)

    $path = [System.IO.Path]::GetFullPath([string]$Snapshot.Path)
    if ([bool]$Snapshot.Existed) {
        [System.IO.Directory]::CreateDirectory([System.IO.Path]::GetDirectoryName($path)) | Out-Null
        $temporary = "$path.$PID.$([guid]::NewGuid().ToString('N')).tmp"
        try {
            [System.IO.File]::WriteAllBytes($temporary, [byte[]]$Snapshot.Bytes)
            [System.IO.File]::Move($temporary, $path, $true)
        } finally {
            if (Test-Path -LiteralPath $temporary -PathType Leaf) {
                Remove-Item -LiteralPath $temporary -Force
            }
        }
        return
    }

    if (Test-Path -LiteralPath $path -PathType Leaf) {
        Remove-Item -LiteralPath $path -Force
    }
}

function Invoke-HermesRuntimeProfilePreservingAction {
    [CmdletBinding()]
    param([Parameter(Mandatory)][scriptblock] $Action)

    $profileSnapshot = Get-HermesRuntimeUserProfileSnapshot
    try {
        & $Action
    } finally {
        Restore-HermesRuntimeUserProfileSnapshot -Snapshot $profileSnapshot
    }
}

function Get-HermesRuntimeUpdateDecision {
    [CmdletBinding()]
    param()

    $configuration = Get-HermesConfiguration
    $requested = Get-HermesRequestedAcceleration -Configuration $configuration
    $hardware = Assert-HermesMachine -Acceleration $(if ($requested -eq 'cuda') { 'cuda' } else { 'auto' })
    Resolve-HermesLlamaRuntimePackage -Configuration $configuration -Hardware $hardware
}

function New-HermesLlamaRuntimeUpdateAdapter {
    [CmdletBinding()]
    param()

    [pscustomobject]@{
        AutoRollbackOnFailure = $true

        check = {
            param($Context)
            Write-HermesRuntimeUpdateStage -Stage check -Message 'Resolving installed and candidate inference-runtime identities.'
            $decision = Get-HermesRuntimeUpdateDecision
            $Context.Working.RuntimeDecision = $decision
            $snapshot = Get-HermesLlamaRuntimeUpdateSnapshot -Decision $decision
            $Context.Working.RuntimeSnapshot = $snapshot
            $snapshot
        }

        compatibility = {
            param($Context)
            Write-HermesRuntimeUpdateStage -Stage compatibility -Message 'Validating hardware, model-format and runtime package compatibility.'
            if ($Context.Mode -eq 'Rollback') {
                return [ordered]@{
                    compatible = $true
                    validation = 'Retained package compatibility is rechecked immediately before rollback promotion.'
                }
            }
            $decision = $Context.Working.RuntimeDecision
            if (-not $decision -or -not $decision.Package) {
                throw "$($decision.SelectionState): $($decision.Reason)"
            }
            $identity = Assert-HermesLlamaRuntimeDecision -Decision $decision
            [ordered]@{
                compatible = $true
                targetIdentity = $identity
                modelFormat = [string]$decision.ModelFormat
                selectionState = [string]$decision.SelectionState
                reason = [string]$decision.Reason
            }
        }

        prepare = {
            param($Context)
            Write-HermesRuntimeUpdateStage -Stage prepare -Message 'Preparing isolated staging and retained-runtime locations.'
            $lifecycle = Get-HermesRuntimeLifecyclePaths
            $Context.Working.RuntimeLifecycle = $lifecycle
            [ordered]@{
                staging = $lifecycle.StagingRoot
                active = $lifecycle.ActivePath
                retained = $lifecycle.RetainedRoot
            }
        }

        verify = {
            param($Context)
            Write-HermesRuntimeUpdateStage -Stage verify -Message 'Verifying the currently active runtime before mutation.'
            $lifecycle = if ($Context.Working.ContainsKey('RuntimeLifecycle')) {
                $Context.Working.RuntimeLifecycle
            } else {
                Get-HermesRuntimeLifecyclePaths
            }
            $manifestPath = Join-Path $lifecycle.ActivePath 'runtime-manifest.json'
            if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
                return [ordered]@{ installed = $false; valid = $true; identity = $null }
            }
            $validation = Test-HermesManagedLlamaRuntime
            if (-not $validation.Valid) {
                throw "The active managed runtime failed validation: $($validation.Reason)"
            }
            [ordered]@{
                installed = $true
                valid = $true
                identity = $validation.Identity
            }
        }

        backup = {
            param($Context)
            Write-HermesRuntimeUpdateStage -Stage backup -Message 'Recording the active runtime identity for transactional retention.'
            $snapshot = $Context.Working.RuntimeSnapshot
            [ordered]@{
                installedIdentity = $snapshot.installedIdentity
                retainedRoot = $snapshot.lifecycle.retained
                policy = 'The validated active runtime is retained atomically during package promotion.'
            }
        }

        promote = {
            param($Context)
            Write-HermesRuntimeUpdateStage -Stage promote -Message 'Verifying, smoke-testing and promoting the staged runtime package.'
            $decision = $Context.Working.RuntimeDecision
            $force = $Context.Input.ContainsKey('Force') -and [bool]$Context.Input.Force
            $manifest = Invoke-HermesRuntimeProfilePreservingAction -Action {
                Install-HermesLlamaRuntime -Decision $decision -Force:$force
            }
            $Context.Working.RuntimePromoted = $true
            $Context.Working.PromotedIdentity = Get-HermesRuntimeManifestIdentity -Manifest $manifest
            $lifecycle = $Context.Working.RuntimeLifecycle
            $state = if (Test-Path -LiteralPath $lifecycle.StatePath -PathType Leaf) {
                Get-Content -Raw -LiteralPath $lifecycle.StatePath | ConvertFrom-Json -Depth 64
            } else {
                $null
            }
            [ordered]@{
                promoted = $true
                identity = $Context.Working.PromotedIdentity
                previousIdentity = $(if ($state) { $state.previousIdentity } else { $null })
                previousPath = $(if ($state) { [string]$state.previousPath } else { $null })
                userProfilePreserved = $true
            }
        }

        validate = {
            param($Context)
            Write-HermesRuntimeUpdateStage -Stage validate -Message 'Running post-promotion runtime integrity and backend smoke validation.'
            $validation = Test-HermesManagedLlamaRuntime -SmokeTest
            if ($Context.Mode -eq 'Rollback') {
                # Restore-HermesLlamaRuntime already smoke-tests and validates the retained
                # payload before swapping it into the active path. A legacy source-build
                # rollback has no managed manifest by design, so preserve that explicit
                # state instead of treating the absent manifest as a post-swap failure.
                if ($validation.Managed -and -not $validation.Valid) {
                    throw "Managed runtime validation failed after rollback: $($validation.Reason)"
                }
                return [ordered]@{
                    rollbackValidated = $true
                    managed = [bool]$validation.Managed
                    identity = $validation.Identity
                    integrity = $(if ($validation.Managed) { 'verified' } else { 'legacy-source-build' })
                    userProfilePreserved = $true
                }
            }
            if (-not $validation.Valid) {
                throw "Managed runtime validation failed after promotion: $($validation.Reason)"
            }
            $expected = $Context.Working.RuntimeDecision.PackageIdentity
            if ([string]$validation.Identity.fingerprint -ne [string]$expected.fingerprint) {
                throw 'The promoted runtime identity differs from the package selected during the check stage.'
            }
            [ordered]@{
                validated = $true
                identity = $validation.Identity
                integrity = 'verified'
                userProfilePreserved = $true
            }
        }

        rollback = {
            param($Context)
            Write-HermesRuntimeUpdateStage -Stage rollback -Message 'Restoring the retained runtime after compatibility and integrity revalidation.'
            if ($Context.Mode -ne 'Rollback' -and
                (-not $Context.Working.ContainsKey('RuntimePromoted') -or -not [bool]$Context.Working.RuntimePromoted)) {
                return [ordered]@{
                    performed = $false
                    activePreserved = $true
                    userProfilePreserved = $true
                    reason = 'Package promotion had not completed; the active runtime was not mutated.'
                }
            }
            $restored = Invoke-HermesRuntimeProfilePreservingAction -Action {
                Restore-HermesLlamaRuntime
            }
            $Context.Working.RuntimePromoted = $false
            [ordered]@{
                performed = $true
                restoredIdentity = $restored.installedIdentity
                displacedPath = $restored.previousPath
                integrityState = $restored.integrityState
                userProfilePreserved = $true
            }
        }
    }
}

function Register-HermesRuntimeUpdateAdapters {
    [CmdletBinding()]
    param()

    $allAdapter = Get-HermesUpdateAdapter -Name All
    $allCheck = [scriptblock]$allAdapter.check
    $allCompatibility = $allAdapter.compatibility
    $allPrepare = $allAdapter.prepare
    $allVerify = $allAdapter.verify
    $allBackup = $allAdapter.backup
    $allPromote = $allAdapter.promote
    $allValidate = $allAdapter.validate
    $allRollback = $allAdapter.rollback

    $managedAll = [pscustomobject]@{
        AutoRollbackOnFailure = [bool]$allAdapter.AutoRollbackOnFailure
        check = {
            param($Context)
            $inventory = & $allCheck $Context
            $decision = Get-HermesRuntimeUpdateDecision
            $inventory.components.LlamaCpp = Get-HermesLlamaRuntimeUpdateSnapshot -Decision $decision
            $Context.Working.Inventory = $inventory
            $inventory
        }.GetNewClosure()
        compatibility = $allCompatibility
        prepare = $allPrepare
        verify = $allVerify
        backup = $allBackup
        promote = $allPromote
        validate = $allValidate
        rollback = $allRollback
    }

    Register-HermesUpdateAdapter -Name All -Adapter $managedAll -Force
    Register-HermesUpdateAdapter -Name LlamaCpp -Adapter (New-HermesLlamaRuntimeUpdateAdapter) -Force
}

Register-HermesRuntimeUpdateAdapters

Export-ModuleMember -Function @(
    'Get-HermesRuntimeUpdateDecision',
    'New-HermesLlamaRuntimeUpdateAdapter',
    'Register-HermesRuntimeUpdateAdapters'
)
