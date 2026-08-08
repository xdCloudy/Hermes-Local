Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# These are dependencies of this module, not modules owned exclusively by it.
# Importing them with -Force from a nested module scope unloads caller-visible
# exports such as Write-HermesLog and Get-HermesConfiguration. Reuse an
# existing session import when present and add the commands to this module's
# scope without mutating the caller's module table.
Import-Module (Join-Path $PSScriptRoot 'Common-Hermes.psm1')
Import-Module (Join-Path $PSScriptRoot 'Hermes-Configuration.psm1')

$script:CatalogPath = Resolve-HermesPath 'config\runtime\llama-runtime-catalog.json'
. (Join-Path $PSScriptRoot 'runtime\Hermes-RuntimeIdentity.ps1')

$script:Lifecycle = Get-HermesRuntimeLifecyclePaths -CatalogPath $script:CatalogPath
$script:StagingRoot = $script:Lifecycle.StagingRoot
$script:BuildRoot = $script:Lifecycle.ActivePath
$script:RollbackRoot = $script:Lifecycle.RetainedRoot
$script:StatePath = $script:Lifecycle.StatePath
$script:HistoryPath = $script:Lifecycle.HistoryPath
$script:DiagnosticPath = $script:Lifecycle.DiagnosticPath
$script:ManagedRoot = [System.IO.Path]::GetDirectoryName($script:StatePath)

. (Join-Path $PSScriptRoot 'runtime\Hermes-RuntimeCatalog.ps1')
. (Join-Path $PSScriptRoot 'runtime\Hermes-RuntimeInstall.ps1')
. (Join-Path $PSScriptRoot 'runtime\Hermes-RuntimeRecovery.ps1')

Export-ModuleMember -Function @(
    'Get-HermesCpuFeatures',
    'Get-HermesRuntimeCatalog',
    'Get-HermesRuntimeLifecyclePaths',
    'Get-HermesSelectedModelFormat',
    'Get-HermesLlamaRuntimePackageIdentity',
    'Get-HermesInstalledLlamaRuntimeIdentity',
    'Get-HermesLlamaRuntimeUpdateSnapshot',
    'Assert-HermesLlamaRuntimeDecision',
    'Get-HermesRequestedAcceleration',
    'Resolve-HermesLlamaRuntimePackage',
    'Install-HermesLlamaRuntime',
    'Restore-HermesLlamaRuntime',
    'Test-HermesManagedLlamaRuntime'
)
