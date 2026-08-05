# The updater process and its invoking shell must survive the drain. Electron's
# launcher process is different: it may be an ancestor of the updater child,
# but it owns the files that activation must replace, so it must remain eligible
# for graceful shutdown and bounded force termination.
function Get-HermesDesktopProtectedProcessIds {
    [CmdletBinding()]
    param()

    $protected = [System.Collections.Generic.HashSet[int]]::new()
    $candidate = [int]$PID
    for ($depth = 0; $depth -lt 16 -and $candidate -gt 0; $depth += 1) {
        $record = Get-CimInstance Win32_Process `
            -Filter "ProcessId = $candidate" `
            -ErrorAction SilentlyContinue
        if (-not $record) {
            $null = $protected.Add($candidate)
            break
        }

        if ([string]$record.Name -ne 'Hermes Launcher.exe') {
            $null = $protected.Add($candidate)
        }

        $parent = [int]$record.ParentProcessId
        if ($parent -le 0 -or $protected.Contains($parent)) {
            break
        }
        $candidate = $parent
    }

    Write-Output -NoEnumerate $protected
}
