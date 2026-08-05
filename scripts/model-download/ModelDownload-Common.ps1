Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

foreach ($component in @(
    'ModelDownload-Core.ps1',
    'ModelDownload-State.ps1',
    'ModelDownload-Transfer.ps1',
    'ModelDownload-Promotion.ps1'
)) {
    . (Join-Path $PSScriptRoot $component)
}
