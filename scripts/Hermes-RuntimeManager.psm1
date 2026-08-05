Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

Import-Module (Join-Path $PSScriptRoot 'Common-Hermes.psm1') -Force
Import-Module (Join-Path $PSScriptRoot 'Hermes-Configuration.psm1') -Force

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
