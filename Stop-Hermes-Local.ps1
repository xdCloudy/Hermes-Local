[CmdletBinding()]
param(
    [ValidateRange(5, 120)]
    [int] $TimeoutSeconds = 45,
    [switch] $NonInteractive
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'scripts\Common-Hermes.psm1') -Force

try {
    Assert-HermesRoot
    $runtimeDirectory = Resolve-HermesPath 'data\runtime'
    $pidPath = Join-Path $runtimeDirectory 'supervisor.pid'
    $statusPath = Join-Path $runtimeDirectory 'status.json'
    if (-not (Test-Path -LiteralPath $pidPath)) {
        Write-Host 'Hermes Local is already stopped.'
        exit 0
    }

    $controllerPid = [int](Get-Content -Raw -LiteralPath $pidPath).Trim()
    $controller = Get-Process -Id $controllerPid -ErrorAction SilentlyContinue
    if (-not $controller) {
        Remove-Item -LiteralPath $pidPath -Force
        Write-HermesLog -Component supervisor -Level WARN -Message "Removed stale supervisor PID record $controllerPid."
        Write-Host 'Hermes Local was not running; a stale PID record was removed.'
        exit 0
    }

    [System.IO.File]::WriteAllText(
        (Join-Path $runtimeDirectory 'stop.request'),
        (Get-Date).ToUniversalTime().ToString('o'),
        [System.Text.UTF8Encoding]::new($false)
    )
    Write-HermesLog -Component supervisor -Message "Requested graceful stop from supervisor PID $controllerPid."
    if (-not $controller.WaitForExit($TimeoutSeconds * 1000)) {
        Write-HermesLog -Component supervisor -Level WARN -Message 'Graceful supervisor timeout; invoking process-tree fallback.'
        & taskkill.exe /PID $controllerPid /T | Out-Null
        Start-Sleep -Seconds 2
        if (Get-Process -Id $controllerPid -ErrorAction SilentlyContinue) {
            & taskkill.exe /PID $controllerPid /T /F | Out-Null
        }
    }
    if (Get-Process -Id $controllerPid -ErrorAction SilentlyContinue) {
        throw "Supervisor PID $controllerPid is still running after the bounded fallback."
    }
    Write-Host 'Hermes Local stopped cleanly.'
    exit 0
} catch {
    Write-HermesLog -Component supervisor -Level ERROR -Message $_.Exception.ToString()
    Write-Host "Hermes Local stop failed: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
}
