[CmdletBinding()]
param(
    [ValidatePattern('^[A-Za-z][A-Za-z0-9 ]{0,31}$')]
    [string] $Profile = 'Daily',
    [switch] $NonInteractive
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$verboseEnabled = $PSBoundParameters.ContainsKey('Verbose')

& (Join-Path $PSScriptRoot 'Stop-Hermes-Local.ps1') -NonInteractive:$NonInteractive -Verbose:$verboseEnabled
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}
& (Join-Path $PSScriptRoot 'Start-Hermes-Local.ps1') -Profile $Profile -NonInteractive:$NonInteractive -Verbose:$verboseEnabled
exit $LASTEXITCODE
