[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$root = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$runtimeModule = Join-Path $root 'scripts\Hermes-RuntimeManager.psm1'
$orchestratorModule = Join-Path $root 'scripts\Hermes-UpdateOrchestrator.psm1'
$adapterModule = Join-Path $root 'scripts\Hermes-RuntimeUpdateAdapter.psm1'

function Assert-Contract {
    param(
        [Parameter(Mandatory)][bool] $Condition,
        [Parameter(Mandatory)][string] $Message
    )
    if (-not $Condition) { throw $Message }
}

function Assert-Throws {
    param(
        [Parameter(Mandatory)][scriptblock] $Action,
        [Parameter(Mandatory)][string] $Message
    )
    $threw = $false
    try { & $Action } catch { $threw = $true }
    if (-not $threw) { throw $Message }
}

Import-Module $runtimeModule -Force

$catalog = Get-HermesRuntimeCatalog
$lifecycle = Get-HermesRuntimeLifecyclePaths -Catalog $catalog
$rootPrefix = if ($root.EndsWith([string][System.IO.Path]::DirectorySeparatorChar)) {
    $root
} else {
    $root + [System.IO.Path]::DirectorySeparatorChar
}
foreach ($path in @(
    $lifecycle.StagingRoot,
    $lifecycle.ActivePath,
    $lifecycle.RetainedRoot,
    $lifecycle.StatePath,
    $lifecycle.HistoryPath,
    $lifecycle.DiagnosticPath
)) {
    Assert-Contract `
        -Condition ([System.IO.Path]::GetFullPath($path).StartsWith($rootPrefix, [System.StringComparison]::OrdinalIgnoreCase)) `
        -Message "Runtime lifecycle path escaped the repository root: $path"
}

$identities = @($catalog.packages | ForEach-Object {
    Get-HermesLlamaRuntimePackageIdentity -Package $_ -Catalog $catalog
})
Assert-Contract ($identities.Count -eq @($catalog.packages).Count) 'Not every catalog package produced an identity.'
Assert-Contract `
    (@($identities | Where-Object { [string]$_.fingerprint -notmatch '^sha256:[0-9a-f]{64}$' }).Count -eq 0) `
    'A runtime identity fingerprint is malformed.'
Assert-Contract `
    ((@($identities.fingerprint | Sort-Object -Unique)).Count -eq $identities.Count) `
    'Runtime package fingerprints are not unique.'

$cpuPackage = @($catalog.packages | Where-Object acceleration -eq 'cpu' | Select-Object -First 1)[0]
$cpuIdentity = Get-HermesLlamaRuntimePackageIdentity -Package $cpuPackage -Catalog $catalog
$hardware = [pscustomobject]@{
    OperatingSystem = 'Windows test fixture'
    Build = 22631
    Architecture = 'x64'
    Cpu = 'Fixture CPU'
    MemoryBytes = 32GB
    Nvidia = $null
}
$decision = [pscustomobject]@{
    SelectionState = 'Recommended prebuilt runtime'
    Reason = 'fixture'
    RequestedAcceleration = 'cpu'
    ResolvedAcceleration = 'cpu'
    ModelFormat = 'gguf'
    Package = $cpuPackage
    PackageIdentity = $cpuIdentity
    Hardware = $hardware
    CpuFeatures = @('avx2')
}
$validated = Assert-HermesLlamaRuntimeDecision -Decision $decision -Catalog $catalog
Assert-Contract `
    ([string]$validated.fingerprint -eq [string]$cpuIdentity.fingerprint) `
    'A valid runtime decision did not retain its canonical identity.'

$tamperedPackage = ($cpuPackage | ConvertTo-Json -Depth 64 | ConvertFrom-Json -Depth 64)
$tamperedPackage.sourceCommit = 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'
$tamperedDecision = [pscustomobject]@{
    SelectionState = 'Recommended prebuilt runtime'
    Reason = 'tampered fixture'
    RequestedAcceleration = 'cpu'
    ResolvedAcceleration = 'cpu'
    ModelFormat = 'gguf'
    Package = $tamperedPackage
    PackageIdentity = Get-HermesLlamaRuntimePackageIdentity -Package $tamperedPackage -Catalog $catalog
    Hardware = $hardware
    CpuFeatures = @('avx2')
}
Assert-Throws `
    -Action { $null = Assert-HermesLlamaRuntimeDecision -Decision $tamperedDecision -Catalog $catalog } `
    -Message 'A runtime decision with tampered source identity was accepted.'

$oldWindowsDecision = [pscustomobject]@{
    SelectionState = $decision.SelectionState
    Reason = $decision.Reason
    RequestedAcceleration = 'cpu'
    ResolvedAcceleration = 'cpu'
    ModelFormat = 'gguf'
    Package = $cpuPackage
    PackageIdentity = $cpuIdentity
    Hardware = [pscustomobject]@{
        OperatingSystem = 'Windows old fixture'
        Build = 19044
        Architecture = 'x64'
        Cpu = 'Fixture CPU'
        MemoryBytes = 32GB
        Nvidia = $null
    }
    CpuFeatures = @('avx2')
}
Assert-Throws `
    -Action { $null = Assert-HermesLlamaRuntimeDecision -Decision $oldWindowsDecision -Catalog $catalog } `
    -Message 'A runtime package incompatible with the Windows build was accepted.'

$wrongFormatDecision = [pscustomobject]@{
    SelectionState = $decision.SelectionState
    Reason = $decision.Reason
    RequestedAcceleration = 'cpu'
    ResolvedAcceleration = 'cpu'
    ModelFormat = 'safetensors'
    Package = $cpuPackage
    PackageIdentity = $cpuIdentity
    Hardware = $hardware
    CpuFeatures = @('avx2')
}
Assert-Throws `
    -Action { $null = Assert-HermesLlamaRuntimeDecision -Decision $wrongFormatDecision -Catalog $catalog } `
    -Message 'A runtime package was accepted for an undeclared model format.'

$unsafeCatalog = ($catalog | ConvertTo-Json -Depth 64 | ConvertFrom-Json -Depth 64)
$unsafeCatalog.lifecycle.activePath = '../outside-runtime'
Assert-Throws `
    -Action { $null = Get-HermesRuntimeLifecyclePaths -Catalog $unsafeCatalog } `
    -Message 'An escaping runtime lifecycle path was accepted.'

# Import the orchestrator in caller scope so its exported registry commands are
# available to inspect after the runtime adapter registers itself.
Import-Module $orchestratorModule -Force
Import-Module $adapterModule -Force
$adapter = Get-HermesUpdateAdapter -Name LlamaCpp
Assert-Contract ($null -ne $adapter) 'The managed LlamaCpp update adapter was not registered.'
foreach ($stage in @('check', 'compatibility', 'prepare', 'verify', 'backup', 'promote', 'validate', 'rollback')) {
    Assert-Contract `
        -Condition ($adapter.PSObject.Properties[$stage].Value -is [scriptblock]) `
        -Message "The LlamaCpp adapter does not implement '$stage'."
}

$prePromotionContext = [pscustomobject]@{
    Mode = 'Apply'
    Working = @{}
    Input = @{}
}
$prePromotionRollback = & ([scriptblock]$adapter.rollback) $prePromotionContext
Assert-Contract `
    (-not [bool]$prePromotionRollback.performed -and [bool]$prePromotionRollback.activePreserved) `
    'Pre-promotion failure recovery attempted to mutate the active runtime.'

Write-Host 'Hermes runtime identity and lifecycle contract passed.'
