[CmdletBinding()]
param(
    [ValidateRange(5, 120)]
    [int] $TimeoutSeconds = 45,
    [switch] $NonInteractive
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'scripts\Common-Hermes.psm1') -Force

function Get-DescendantProcessIds {
    param([Parameter(Mandatory)][int] $RootProcessId)

    $processes = @(Get-CimInstance Win32_Process -ErrorAction SilentlyContinue)
    $childrenByParent = @{}
    foreach ($process in $processes) {
        $parent = [int]$process.ParentProcessId
        if (-not $childrenByParent.ContainsKey($parent)) {
            $childrenByParent[$parent] = [System.Collections.Generic.List[int]]::new()
        }
        $childrenByParent[$parent].Add([int]$process.ProcessId)
    }

    $result = [System.Collections.Generic.List[int]]::new()
    $pending = [System.Collections.Generic.Queue[int]]::new()
    $pending.Enqueue($RootProcessId)
    while ($pending.Count) {
        $parent = $pending.Dequeue()
        if (-not $childrenByParent.ContainsKey($parent)) {
            continue
        }
        foreach ($child in $childrenByParent[$parent]) {
            if (-not $result.Contains($child)) {
                $result.Add($child)
                $pending.Enqueue($child)
            }
        }
    }
    return $result.ToArray()
}

function Test-AnyProcessAlive {
    param([int[]] $ProcessIds)
    foreach ($processId in @($ProcessIds)) {
        if ($processId -gt 0 -and (Get-Process -Id $processId -ErrorAction SilentlyContinue)) {
            return $true
        }
    }
    return $false
}

try {
    Assert-HermesRoot
    $runtimeDirectory = Resolve-HermesPath 'data\runtime'
    $pidPath = Join-Path $runtimeDirectory 'supervisor.pid'
    $statusPath = Join-Path $runtimeDirectory 'status.json'
    $stopRequestPath = Join-Path $runtimeDirectory 'stop.request'

    if (-not (Test-Path -LiteralPath $pidPath)) {
        Write-Host 'Hermes Local is already stopped.'
        exit 0
    }

    $controllerPid = 0
    $rawPid = (Get-Content -Raw -LiteralPath $pidPath).Trim()
    if (-not [int]::TryParse($rawPid, [ref]$controllerPid) -or $controllerPid -le 0) {
        Remove-Item -LiteralPath $pidPath -Force -ErrorAction SilentlyContinue
        throw "Supervisor PID record is invalid: '$rawPid'."
    }

    $controller = Get-Process -Id $controllerPid -ErrorAction SilentlyContinue
    if (-not $controller) {
        Remove-Item -LiteralPath $pidPath -Force -ErrorAction SilentlyContinue
        Write-HermesLog -Component supervisor -Level WARN -Message "Removed stale supervisor PID record $controllerPid."
        Write-Host 'Hermes Local was not running; a stale PID record was removed.'
        exit 0
    }

    # Capture the complete tree before requesting shutdown. Windows can orphan
    # descendants when the root exits, so checking the root PID alone is unsafe.
    $trackedPids = [System.Collections.Generic.HashSet[int]]::new()
    [void]$trackedPids.Add($controllerPid)
    foreach ($processId in @(Get-DescendantProcessIds -RootProcessId $controllerPid)) {
        [void]$trackedPids.Add($processId)
    }

    [System.IO.File]::WriteAllText(
        $stopRequestPath,
        (Get-Date).ToUniversalTime().ToString('o'),
        [System.Text.UTF8Encoding]::new($false)
    )
    Write-HermesLog -Component supervisor -Message "Requested graceful stop from supervisor PID $controllerPid."

    [void]$controller.WaitForExit($TimeoutSeconds * 1000)
    Start-Sleep -Milliseconds 500

    # Capture descendants again in case the supervisor created a final cleanup
    # child after the first snapshot.
    foreach ($processId in @(Get-DescendantProcessIds -RootProcessId $controllerPid)) {
        [void]$trackedPids.Add($processId)
    }

    if (Test-AnyProcessAlive -ProcessIds @($trackedPids)) {
        Write-HermesLog -Component supervisor -Level WARN -Message 'Graceful supervisor timeout or orphaned descendants detected; invoking forced process-tree fallback.'
        if (Get-Process -Id $controllerPid -ErrorAction SilentlyContinue) {
            try {
                & taskkill.exe /PID $controllerPid /T /F 2>$null | Out-Null
            } catch {
                Write-Verbose "Root process-tree fallback returned: $($_.Exception.Message)"
            }
        }
        Start-Sleep -Seconds 2

        # taskkill can lose descendants after their parent exits. Force each
        # captured PID independently, then verify every one is gone.
        foreach ($processId in @($trackedPids | Sort-Object -Descending)) {
            if ($processId -ne $PID -and (Get-Process -Id $processId -ErrorAction SilentlyContinue)) {
                try {
                    & taskkill.exe /PID $processId /T /F 2>$null | Out-Null
                } catch {
                    Write-Verbose "PID $processId fallback returned: $($_.Exception.Message)"
                }
            }
        }
        Start-Sleep -Seconds 2
    }

    $survivors = @(
        $trackedPids |
            Where-Object { Get-Process -Id $_ -ErrorAction SilentlyContinue }
    )
    if ($survivors.Count) {
        throw "Hermes Local process tree still has live PID(s): $($survivors -join ', ')."
    }

    # Detect an unexpected replacement supervisor rather than reporting a false
    # clean stop. This usually means an open Launcher or failed benchmark cleanup
    # immediately started the stack again.
    Start-Sleep -Seconds 1
    if (Test-Path -LiteralPath $pidPath) {
        $replacementPid = 0
        $replacementRaw = (Get-Content -Raw -LiteralPath $pidPath).Trim()
        if ([int]::TryParse($replacementRaw, [ref]$replacementPid) -and
            $replacementPid -gt 0 -and
            $replacementPid -ne $controllerPid -and
            (Get-Process -Id $replacementPid -ErrorAction SilentlyContinue)) {
            throw "A replacement Hermes Local supervisor started as PID $replacementPid during shutdown. Close the Launcher and retry."
        }
    }

    Remove-Item -LiteralPath $pidPath -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $stopRequestPath -Force -ErrorAction SilentlyContinue
    if (Test-Path -LiteralPath $statusPath) {
        try {
            $status = Get-Content -Raw -LiteralPath $statusPath | ConvertFrom-Json
            if ([int]$status.controllerPid -eq $controllerPid) {
                Remove-Item -LiteralPath $statusPath -Force -ErrorAction SilentlyContinue
            }
        } catch {
        }
    }

    Write-Host 'Hermes Local stopped cleanly.'
    exit 0
} catch {
    Write-HermesLog -Component supervisor -Level ERROR -Message $_.Exception.ToString()
    Write-Host "Hermes Local stop failed: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
}
