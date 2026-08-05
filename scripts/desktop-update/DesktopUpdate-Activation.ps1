$partsRoot = if (
    Get-Variable -Name desktopUpdatePartsRoot -Scope Script -ErrorAction SilentlyContinue
) {
    [string]$script:desktopUpdatePartsRoot
} else {
    $PSScriptRoot
}

foreach ($component in @(
    'DesktopUpdate-Activation-Core.ps1',
    'DesktopUpdate-StackDrain.ps1',
    'DesktopUpdate-ZStackDrainSafety.ps1'
)) {
    $path = Join-Path $partsRoot $component
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Hermes Desktop activation component is missing: $path"
    }
    . $path
}
