[CmdletBinding(SupportsShouldProcess, ConfirmImpact = 'High')]
param(
    [switch] $RemoveUserData,
    [switch] $RemoveModels,
    [switch] $RemoveSource,
    [switch] $RemoveRuntimes,
    [switch] $RemoveBuilds
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'scripts\Common-Hermes.psm1') -Force

function Remove-VerifiedHermesPath {
    param(
        [Parameter(Mandatory)]
        [string] $RelativePath
    )

    $target = Resolve-HermesPath $RelativePath
    $root = Get-HermesRoot
    if ($target -eq $root) {
        throw 'Refusing to remove the Hermes project root.'
    }
    if (-not (Test-Path -LiteralPath $target)) {
        return
    }
    if ($PSCmdlet.ShouldProcess($target, 'Remove recursively')) {
        Remove-Item -LiteralPath $target -Recurse -Force
        Write-HermesLog -Component setup -Message "Removed $target."
    }
}

try {
    Assert-HermesRoot
    $stopScript = Resolve-HermesPath 'Stop-Hermes-Local.ps1'
    if (Test-Path -LiteralPath $stopScript) {
        & $stopScript
    }

    if ($RemoveBuilds) {
        Remove-VerifiedHermesPath 'build'
        Remove-VerifiedHermesPath 'dist'
    }
    if ($RemoveRuntimes) {
        Remove-VerifiedHermesPath 'runtimes'
    }
    if ($RemoveSource) {
        Remove-VerifiedHermesPath 'source\hermes-agent'
    }
    if ($RemoveModels) {
        Remove-VerifiedHermesPath 'models'
    }
    if ($RemoveUserData) {
        Remove-VerifiedHermesPath 'data'
        Remove-VerifiedHermesPath 'config\launcher\api-token.dpapi'
    }

    Write-Host 'Hermes Local uninstall completed. Unselected data and project files were preserved.'
    exit 0
} catch {
    Write-HermesLog -Component setup -Level ERROR -Message $_.Exception.ToString()
    Write-Host 'Hermes Local uninstall failed before completion.' -ForegroundColor Red
    exit 1
}
