[CmdletBinding()]
param(
    [ValidatePattern('^[A-Za-z][A-Za-z0-9 ]{0,31}$')]
    [string] $Profile,
    [ValidateRange(30, 1200)]
    [int] $TimeoutSeconds = 960,
    [switch] $NonInteractive
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'scripts\Common-Hermes.psm1') -Force
Import-Module (Join-Path $PSScriptRoot 'scripts\Hermes-Configuration.psm1') -Force

function Test-HermesDesktopReadyState {
    param(
        [AllowNull()]
        [psobject] $Status,
        [Parameter(Mandatory)]
        [int] $ControllerPid,
        [Parameter(Mandatory)]
        [string] $ExpectedModelId,
        [Parameter(Mandatory)]
        [string] $ExpectedModelAlias
    )

    if (-not $Status -or
        $Status.PSObject.Properties.Name -notcontains 'phase' -or
        $Status.PSObject.Properties.Name -notcontains 'controllerPid' -or
        [int]$Status.controllerPid -ne $ControllerPid) {
        return $false
    }
    $gatewayReady = $Status.PSObject.Properties.Name -notcontains 'gateway' -or
        -not $Status.gateway.required -or $Status.gateway.healthy
    $desktopReady = $Status.hermes.healthy -and $gatewayReady
    $inferenceReady = $Status.phase -eq 'running' -and
        $Status.model.healthy -and
        [string]$Status.selectedModelId -eq $ExpectedModelId -and
        [string]$Status.model.alias -eq $ExpectedModelAlias
    $benchmarkReady = $Status.phase -in @('benchmark-preparing', 'benchmarking', 'starting-model') -and $desktopReady
    return $desktopReady -and ($inferenceReady -or $benchmarkReady)
}

function Get-RunningHermesSupervisor {
    param(
        [Parameter(Mandatory)]
        [string] $PidPath,
        [int] $ExcludePid = 0
    )

    if (-not (Test-Path -LiteralPath $PidPath)) {
        return $null
    }

    try {
        $candidatePid = 0
        $rawPid = (Get-Content -Raw -LiteralPath $PidPath).Trim()
        if (-not [int]::TryParse($rawPid, [ref] $candidatePid) -or
            $candidatePid -le 0 -or
            $candidatePid -eq $ExcludePid) {
            return $null
        }
        return Get-Process -Id $candidatePid -ErrorAction SilentlyContinue
    } catch {
        return $null
    }
}

try {
    Assert-HermesRoot
    Initialize-HermesLayout

    $updateStateRepair = Resolve-HermesPath 'scripts\Repair-Hermes-DesktopUpdateState.ps1'
    & $updateStateRepair -RepositoryRoot (Get-HermesRoot)
    if ($LASTEXITCODE -ne 0) {
        throw "Desktop update-state repair failed with exit code $LASTEXITCODE."
    }

    $expectedConfiguration = Get-HermesConfiguration
    if (-not $Profile) {
        $Profile = [string]$expectedConfiguration.selectedProfile
    }
    $runtimeDirectory = Resolve-HermesPath 'data\runtime'
    $statusPath = Join-Path $runtimeDirectory 'status.json'
    $pidPath = Join-Path $runtimeDirectory 'supervisor.pid'
    $process = Get-RunningHermesSupervisor -PidPath $pidPath
    $existingPid = if ($process) { $process.Id } else { 0 }
    if ($process) {
        $status = if (Test-Path -LiteralPath $statusPath) {
            try {
                Get-Content -Raw -LiteralPath $statusPath | ConvertFrom-Json
            } catch {
                $null
            }
        } else {
            $null
        }
        if (Test-HermesDesktopReadyState -Status $status -ControllerPid $existingPid -ExpectedModelId ([string]$expectedConfiguration.selectedModelId) -ExpectedModelAlias ([string]$expectedConfiguration.selectedModel.alias)) {
            if ($status.phase -eq 'running') {
                [void](Assert-HermesModelIdentity -Configuration $expectedConfiguration -StatusPath $statusPath)
                Write-Host "Hermes Local is already running with profile '$($status.profile)' (supervisor PID $existingPid)."
            } else {
                Write-Host "Hermes Local Desktop services are ready while benchmark lifecycle state is '$($status.phase)' (supervisor PID $existingPid)."
            }
            exit 0
        }
        Write-HermesLog -Component supervisor -Message (
            "Waiting for existing supervisor PID $existingPid to finish starting profile '$Profile'."
        )
    } else {
        $entrypointRepair = Resolve-HermesPath 'scripts\Repair-Hermes-ConsoleEntrypoint.ps1'
        & $entrypointRepair -RepositoryRoot (Get-HermesRoot)
        if ($LASTEXITCODE -ne 0) {
            throw "Hermes console entrypoint repair failed with exit code $LASTEXITCODE."
        }

        [System.IO.Directory]::CreateDirectory($runtimeDirectory) | Out-Null
        $supervisor = Resolve-HermesPath 'scripts\supervisor\Hermes-Supervisor.ps1'
        $pwsh = (Get-Command pwsh.exe -ErrorAction Stop).Source
        $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
        $startInfo.FileName = $pwsh
        $startInfo.WorkingDirectory = Get-HermesRoot
        # Shell execution prevents the long-lived supervisor from inheriting the
        # caller's redirected stdout/stderr pipes. Without this, noninteractive
        # callers wait forever for EOF even after this short launcher exits.
        $startInfo.UseShellExecute = $true
        $startInfo.CreateNoWindow = $true
        $startInfo.WindowStyle = [System.Diagnostics.ProcessWindowStyle]::Hidden
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
    }

    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    $lastPhase = ''
    while ((Get-Date) -lt $deadline) {
        if ($process.HasExited -and $process.ExitCode -ne 0) {
            if ($process.ExitCode -eq 16) {
                # Two callers can both launch before the supervisor mutex is
                # acquired. The loser exits with the documented code 16; join
                # the winning process instead of reporting a false failure.
                $losingPid = $process.Id
                $winnerDeadline = (Get-Date).AddSeconds(5)
                $winner = $null
                while (-not $winner -and (Get-Date) -lt $winnerDeadline) {
                    $winner = Get-RunningHermesSupervisor -PidPath $pidPath -ExcludePid $losingPid
                    if (-not $winner) {
                        Start-Sleep -Milliseconds 100
                    }
                }
                if ($winner) {
                    $process = $winner
                    Write-HermesLog -Component supervisor -Message (
                        "Supervisor PID $losingPid lost the startup race; joining supervisor PID $($winner.Id)."
                    )
                    continue
                }
            }
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
                if (-not $status -or $status.PSObject.Properties.Name -notcontains 'phase') {
                    Write-Verbose 'Waiting for a complete atomic supervisor status update.'
                    Start-Sleep -Milliseconds 250
                    continue
                }
                if ($status.PSObject.Properties.Name -notcontains 'controllerPid' -or
                    [int]$status.controllerPid -ne $process.Id) {
                    Write-Verbose "Ignoring stale supervisor status while waiting for PID $($process.Id)."
                    Start-Sleep -Milliseconds 250
                    continue
                }
                if ($status.phase -ne $lastPhase) {
                    Write-Verbose "Hermes Local phase: $($status.phase) — $($status.message)"
                    $lastPhase = $status.phase
                }
                if (Test-HermesDesktopReadyState -Status $status -ControllerPid $process.Id -ExpectedModelId ([string]$expectedConfiguration.selectedModelId) -ExpectedModelAlias ([string]$expectedConfiguration.selectedModel.alias)) {
                    $gatewayDetail = if ($status.PSObject.Properties.Name -contains 'gateway' -and $status.gateway.required) {
                        " Gateway PID $($status.gateway.pid) ($($status.gateway.ownership))."
                    } else {
                        ' Messaging gateway disabled.'
                    }
                    if ($status.phase -eq 'running') {
                        [void](Assert-HermesModelIdentity -Configuration $expectedConfiguration -StatusPath $statusPath)
                        Write-Host "Hermes Local is ready with profile '$Profile'. Model PID $($status.model.pid); Hermes PID $($status.hermes.pid).$gatewayDetail"
                    } else {
                        Write-Host "Hermes Local Desktop services are ready while benchmark lifecycle state is '$($status.phase)'. Hermes PID $($status.hermes.pid).$gatewayDetail"
                    }
                    exit 0
                }
                if ($status.phase -eq 'failed') {
                    throw "Hermes Local startup failed: $($status.message)"
                }
            } catch {
                if ($_.Exception.Message -match 'identity|selected GGUF|selected alias|provider configuration') {
                    throw
                }
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
