[CmdletBinding()]
param(
    [ValidatePattern('^[A-Za-z][A-Za-z0-9 ]{0,31}$')]
    [string] $Profile = 'Daily',
    [ValidateRange(30, 1200)]
    [int] $TimeoutSeconds = 960,
    [switch] $NonInteractive
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'scripts\Common-Hermes.psm1') -Force

try {
    Assert-HermesRoot
    Initialize-HermesLayout
    $runtimeDirectory = Resolve-HermesPath 'data\runtime'
    $statusPath = Join-Path $runtimeDirectory 'status.json'
    $pidPath = Join-Path $runtimeDirectory 'supervisor.pid'
    $existingPid = if (Test-Path -LiteralPath $pidPath) {
        [int](Get-Content -Raw -LiteralPath $pidPath).Trim()
    } else {
        0
    }
    if ($existingPid -and (Get-Process -Id $existingPid -ErrorAction SilentlyContinue)) {
        $status = if (Test-Path -LiteralPath $statusPath) {
            Get-Content -Raw -LiteralPath $statusPath | ConvertFrom-Json
        } else {
            $null
        }
        if ($status -and $status.phase -eq 'running') {
            Write-Host "Hermes Local is already running with profile '$($status.profile)' (supervisor PID $existingPid)."
            exit 0
        }
        throw "Hermes Local supervisor PID $existingPid exists but is not ready. Inspect $statusPath."
    }

    [System.IO.Directory]::CreateDirectory($runtimeDirectory) | Out-Null
    $supervisor = Resolve-HermesPath 'scripts\supervisor\Hermes-Supervisor.ps1'
    $pwsh = (Get-Command pwsh.exe -ErrorAction Stop).Source
    $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $pwsh
    $startInfo.WorkingDirectory = Get-HermesRoot
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    foreach ($argument in @(
        '-NoLogo', '-NoProfile', '-NonInteractive',
        '-ExecutionPolicy', 'Bypass',
        '-File', $supervisor,
        '-Profile', $Profile
    )) {
        $startInfo.ArgumentList.Add($argument)
    }
    $process = [System.Diagnostics.Process]::Start($startInfo)
    if (-not $process) {
        throw 'Failed to launch Hermes Local supervisor.'
    }
    Write-HermesLog -Component supervisor -Message "Launched supervisor PID $($process.Id) for profile '$Profile'."

    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    $lastPhase = ''
    while ((Get-Date) -lt $deadline) {
        if ($process.HasExited -and $process.ExitCode -ne 0) {
            $failure = if (Test-Path -LiteralPath $statusPath) {
                Get-Content -Raw -LiteralPath $statusPath | ConvertFrom-Json
            } else {
                $null
            }
            $detail = if ($failure) { $failure.message } else { 'No status was written.' }
            throw "Supervisor exited with code $($process.ExitCode): $detail"
        }
        if (Test-Path -LiteralPath $statusPath) {
            try {
                $status = Get-Content -Raw -LiteralPath $statusPath | ConvertFrom-Json
                if ($status.phase -ne $lastPhase) {
                    Write-Verbose "Hermes Local phase: $($status.phase) — $($status.message)"
                    $lastPhase = $status.phase
                }
                if ($status.phase -eq 'running' -and $status.model.healthy -and $status.hermes.healthy) {
                    Write-Host "Hermes Local is ready with profile '$Profile'. Model PID $($status.model.pid); Hermes PID $($status.hermes.pid)."
                    exit 0
                }
                if ($status.phase -eq 'failed') {
                    throw "Hermes Local startup failed: $($status.message)"
                }
            } catch [System.Management.Automation.RuntimeException] {
                throw
            } catch {
                Write-Verbose "Waiting for an atomic status update: $($_.Exception.Message)"
            }
        }
        Start-Sleep -Milliseconds 750
    }
    throw "Hermes Local did not become ready within $TimeoutSeconds seconds."
} catch {
    Write-HermesLog -Component supervisor -Level ERROR -Message $_.Exception.ToString()
    Write-Host "Hermes Local start failed: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
}
