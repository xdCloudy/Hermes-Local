[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$root = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
Import-Module (Join-Path $root 'scripts\Common-Hermes.psm1') -Force

Assert-HermesRoot
Set-HermesProcessEnvironment
$env:HERMES_LOCAL_API_TOKEN = Get-OrCreateHermesApiToken
$env:LLAMA_API_KEY = $env:HERMES_LOCAL_API_TOKEN
$hermes = Resolve-HermesPath 'runtimes\python\hermes\Scripts\hermes.exe'

if (-not (Test-Path -LiteralPath $hermes -PathType Leaf)) {
    throw "Hermes executable not found: $hermes"
}

& $hermes --tui
exit $LASTEXITCODE
