# Some validation and recovery callers dot-source every Desktop updater part in
# lexical order instead of using Invoke-Hermes-DesktopUpdate.ps1's explicit
# dependency order. Establish the parts root early without changing the value
# supplied by the real updater entrypoint.
if (-not (Get-Variable `
    -Name desktopUpdatePartsRoot `
    -Scope Script `
    -ErrorAction SilentlyContinue
)) {
    $script:desktopUpdatePartsRoot = $PSScriptRoot
}
