Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

Import-Module (Join-Path $PSScriptRoot 'Common-Hermes.psm1')
Import-Module (Join-Path $PSScriptRoot 'Hermes-Configuration.psm1')
Import-Module (Join-Path $PSScriptRoot 'Hermes-UpdateOrchestrator.psm1')
Import-Module (Join-Path $PSScriptRoot 'Hermes-RuntimeManager.psm1')

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
            $decision = Get-HermesRuntimeUpdateDecision
            $Context.Working.RuntimeDecision = $decision
            $snapshot = Get-HermesLlamaRuntimeUpdateSnapshot -Decision $decision
            $Context.Working.RuntimeSnapshot = $snapshot
            $snapshot
        }

        compatibility = {
            param($Context)
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
            $snapshot = $Context.Working.RuntimeSnapshot
            [ordered]@{
                installedIdentity = $snapshot.installedIdentity
                retainedRoot = $snapshot.lifecycle.retained
                policy = 'The validated active runtime is retained atomically during package promotion.'
            }
        }

        promote = {
            param($Context)
            $decision = $Context.Working.RuntimeDecision
            $force = $Context.Input.ContainsKey('Force') -and [bool]$Context.Input.Force
            $manifest = Install-HermesLlamaRuntime -Decision $decision -Force:$force
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
            }
        }

        validate = {
            param($Context)
            $validation = Test-HermesManagedLlamaRuntime -SmokeTest
            if (-not $validation.Valid) {
                throw "Managed runtime validation failed after promotion: $($validation.Reason)"
            }
            if ($Context.Mode -eq 'Rollback') {
                return [ordered]@{
                    rollbackValidated = $true
                    identity = $validation.Identity
                }
            }
            $expected = $Context.Working.RuntimeDecision.PackageIdentity
            if ([string]$validation.Identity.fingerprint -ne [string]$expected.fingerprint) {
                throw 'The promoted runtime identity differs from the package selected during the check stage.'
            }
            [ordered]@{
                validated = $true
                identity = $validation.Identity
                integrity = 'verified'
            }
        }

        rollback = {
            param($Context)
            if ($Context.Mode -ne 'Rollback' -and
                (-not $Context.Working.ContainsKey('RuntimePromoted') -or -not [bool]$Context.Working.RuntimePromoted)) {
                return [ordered]@{
                    performed = $false
                    activePreserved = $true
                    reason = 'Package promotion had not completed; the active runtime was not mutated.'
                }
            }
            $restored = Restore-HermesLlamaRuntime
            $Context.Working.RuntimePromoted = $false
            [ordered]@{
                performed = $true
                restoredIdentity = $restored.installedIdentity
                displacedPath = $restored.previousPath
                integrityState = $restored.integrityState
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
