# Lexical part loaders encounter DesktopUpdate-Activation.ps1 before the core
# promotion and state functions exist. Reinitialize the captured delegates after
# every core part has been loaded. The production entrypoint already loads the
# activation layer through DesktopUpdate-Reliability-Platform.ps1, so this file
# is a no-op there unless a caller sources all parts alphabetically.
$requiredCoreFunctions = @(
    'Get-HermesDesktopUpdateStatus',
    'Promote-HermesDesktopPendingLauncher',
    'Start-HermesDesktopPromotionHelper'
)
$missingCoreFunctions = @(
    $requiredCoreFunctions |
        Where-Object {
            -not (Get-Command `
                -Name $_ `
                -CommandType Function `
                -ErrorAction SilentlyContinue
            )
        }
)

if ($missingCoreFunctions.Count -eq 0) {
    foreach ($variableName in @(
        'hermesDesktopOriginalPromotePendingLauncher',
        'hermesDesktopOriginalStartPromotionHelper',
        'hermesDesktopOriginalGetUpdateStatus'
    )) {
        Remove-Variable `
            -Name $variableName `
            -Scope Script `
            -Force `
            -ErrorAction SilentlyContinue
    }

    . (Join-Path $PSScriptRoot 'DesktopUpdate-Activation.ps1')
}
