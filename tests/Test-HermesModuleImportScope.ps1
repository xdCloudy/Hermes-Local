[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$root = [IO.Path]::GetFullPath((Split-Path $PSScriptRoot -Parent))
$common = Join-Path $root 'scripts\Common-Hermes.psm1'
$configuration = Join-Path $root 'scripts\Hermes-Configuration.psm1'
$runtimeManager = Join-Path $root 'scripts\Hermes-RuntimeManager.psm1'

foreach ($path in @($common, $configuration, $runtimeManager)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Required module is missing: $path"
    }
}

function Assert-CommandAvailable {
    param(
        [Parameter(Mandatory)][string] $Name,
        [Parameter(Mandatory)][string] $ExpectedModule
    )

    $command = Get-Command $Name -CommandType Function -ErrorAction SilentlyContinue |
        Select-Object -First 1
    if (-not $command) {
        throw "Expected command '$Name' is unavailable after setup module imports."
    }
    if ([string]$command.ModuleName -ne $ExpectedModule) {
        throw (
            "Command '$Name' resolved from module '$($command.ModuleName)'; " +
            "expected '$ExpectedModule'."
        )
    }
}

# Match Setup-Hermes-Local.Prebuilt.ps1 exactly. The runtime manager may import
# dependencies into its own scope, but must never unload the exports already
# visible to the setup entrypoint.
Import-Module $common -Force
Assert-CommandAvailable -Name Write-HermesLog -ExpectedModule 'Common-Hermes'
Assert-CommandAvailable -Name Invoke-HermesProcess -ExpectedModule 'Common-Hermes'

Import-Module $configuration -Force
Assert-CommandAvailable -Name Write-HermesLog -ExpectedModule 'Common-Hermes'
Assert-CommandAvailable -Name Get-HermesConfiguration -ExpectedModule 'Hermes-Configuration'

Import-Module $runtimeManager -Force
Assert-CommandAvailable -Name Write-HermesLog -ExpectedModule 'Common-Hermes'
Assert-CommandAvailable -Name Invoke-HermesProcess -ExpectedModule 'Common-Hermes'
Assert-CommandAvailable -Name Get-HermesConfiguration -ExpectedModule 'Hermes-Configuration'
Assert-CommandAvailable -Name Get-HermesRequestedAcceleration -ExpectedModule 'Hermes-RuntimeManager'
Assert-CommandAvailable -Name Resolve-HermesLlamaRuntimePackage -ExpectedModule 'Hermes-RuntimeManager'

Write-Host 'Hermes setup module import-scope tests passed.'
