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
$script:ManagedRoot = Resolve-HermesPath 'runtimes\llama.cpp\managed'
$script:BuildRoot = Resolve-HermesPath 'runtimes\llama.cpp\build'
$script:StatePath = Join-Path $script:ManagedRoot 'current.json'
$script:HistoryPath = Join-Path $script:ManagedRoot 'history.json'
$script:DiagnosticPath = Resolve-HermesPath 'data\runtime\llama-runtime.json'

. (Join-Path $PSScriptRoot 'runtime\Hermes-RuntimeCatalog.ps1')
. (Join-Path $PSScriptRoot 'runtime\Hermes-RuntimeInstall.ps1')
. (Join-Path $PSScriptRoot 'runtime\Hermes-RuntimeRecovery.ps1')

Export-ModuleMember -Function @(
    'Get-HermesCpuFeatures',
    'Get-HermesRuntimeCatalog',
    'Get-HermesRequestedAcceleration',
    'Resolve-HermesLlamaRuntimePackage',
    'Install-HermesLlamaRuntime',
    'Restore-HermesLlamaRuntime',
    'Test-HermesManagedLlamaRuntime'
)
