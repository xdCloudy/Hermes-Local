[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot '..\Common-Hermes.psm1') -Force

Assert-HermesRoot
$token = Get-OrCreateHermesApiToken
if ($token -notmatch '^[A-Za-z0-9_-]{40,128}$') {
    throw 'The protected Hermes Local token is invalid.'
}
[Console]::Out.Write($token)
