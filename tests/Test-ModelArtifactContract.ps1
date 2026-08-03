[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$root = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$manifestPath = Join-Path $root 'models\manifests\Qwen3.6-35B-A3B-APEX-MTP-I-Quality.json'
$setupPath = Join-Path $root 'Setup-Hermes-Local.Impl.ps1'

function Assert-Contract {
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

$manifest = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
$metadata = $manifest.metadata

foreach ($property in @(
    'visionProjectorFilename',
    'visionProjectorLocalPath',
    'visionProjectorSource',
    'visionProjectorSizeBytes',
    'visionProjectorSha256'
)) {
    Assert-Contract `
        -Condition ($null -ne $metadata.PSObject.Properties[$property]) `
        -Message "Starter manifest is missing metadata.$property."
}

Assert-Contract `
    -Condition ([string]$metadata.visionProjectorSha256 -match '^[a-f0-9]{64}$') `
    -Message 'Vision projector SHA-256 is invalid.'
Assert-Contract `
    -Condition ([int64]$metadata.visionProjectorSizeBytes -gt 0) `
    -Message 'Vision projector size must be positive.'
Assert-Contract `
    -Condition ([uri]::IsWellFormedUriString([string]$metadata.visionProjectorSource, [UriKind]::Absolute)) `
    -Message 'Vision projector source must be an absolute URL.'

$arguments = @($manifest.server.extraArguments | ForEach-Object { [string]$_ })
Assert-Contract `
    -Condition ($arguments -notcontains '--mmproj-url') `
    -Message 'The starter manifest must not depend on llama.cpp HTTPS projector downloads.'

$projectorArgumentIndex = [Array]::IndexOf($arguments, '--mmproj')
Assert-Contract `
    -Condition ($projectorArgumentIndex -ge 0 -and $projectorArgumentIndex + 1 -lt $arguments.Count) `
    -Message 'The starter manifest must pass a local --mmproj path.'

$workingDirectory = Join-Path $root 'data\user'
$resolvedArgumentPath = [System.IO.Path]::GetFullPath(
    (Join-Path $workingDirectory $arguments[$projectorArgumentIndex + 1])
)
$resolvedManifestPath = [System.IO.Path]::GetFullPath(
    (Join-Path $root ([string]$metadata.visionProjectorLocalPath))
)
Assert-Contract `
    -Condition ($resolvedArgumentPath -eq $resolvedManifestPath) `
    -Message 'The portable --mmproj argument does not resolve to the provisioned projector path.'

$setupSource = Get-Content -Raw -LiteralPath $setupPath
foreach ($marker in @(
    'function Install-HermesModelArtifact',
    'visionProjectorSource',
    'visionProjectorLocalPath',
    'Cannot rebuild llama.cpp while its native tools are running'
)) {
    Assert-Contract `
        -Condition ($setupSource.Contains($marker)) `
        -Message "Setup provisioning contract is missing marker: $marker"
}

Write-Host 'Model artifact provisioning contract passed.'
