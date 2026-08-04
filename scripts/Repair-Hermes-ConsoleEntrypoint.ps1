[CmdletBinding()]
param(
    [string] $RepositoryRoot = (Split-Path $PSScriptRoot -Parent),
    [switch] $Force
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

Import-Module (Join-Path $RepositoryRoot 'scripts\Common-Hermes.psm1') -Force

function Test-HermesConsoleEntrypoint {
    param(
        [Parameter(Mandatory)][string] $Executable,
        [Parameter(Mandatory)][string] $WorkingDirectory
    )

    if (-not (Test-Path -LiteralPath $Executable -PathType Leaf)) {
        return [pscustomobject]@{
            Healthy = $false
            Detail = "Executable not found: $Executable"
        }
    }

    $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = [System.IO.Path]::GetFullPath($Executable)
    $startInfo.WorkingDirectory = [System.IO.Path]::GetFullPath($WorkingDirectory)
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    $startInfo.ArgumentList.Add('--help')

    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $startInfo

    try {
        if (-not $process.Start()) {
            return [pscustomobject]@{
                Healthy = $false
                Detail = "Could not start $Executable"
            }
        }

        $stdout = $process.StandardOutput.ReadToEnd()
        $stderr = $process.StandardError.ReadToEnd()
        $process.WaitForExit()

        return [pscustomobject]@{
            Healthy = $process.ExitCode -eq 0
            Detail = (@($stdout.Trim(), $stderr.Trim()) | Where-Object { $_ }) -join [Environment]::NewLine
        }
    } finally {
        $process.Dispose()
    }
}

try {
    Assert-HermesRoot
    Set-HermesProcessEnvironment

    $source = Join-Path $RepositoryRoot 'source\hermes-agent'
    $runtime = Join-Path $RepositoryRoot 'runtimes\python\hermes'
    $python = Join-Path $runtime 'Scripts\python.exe'
    $hermes = Join-Path $runtime 'Scripts\hermes.exe'

    if (-not (Test-Path -LiteralPath $source -PathType Container)) {
        throw "Hermes Agent source directory was not found: $source"
    }
    if (-not (Test-Path -LiteralPath $python -PathType Leaf)) {
        throw "Hermes runtime Python was not found: $python"
    }

    $probe = Test-HermesConsoleEntrypoint -Executable $hermes -WorkingDirectory $source
    if ($probe.Healthy -and -not $Force) {
        exit 0
    }

    $reason = if ($probe.Detail) { $probe.Detail } else { 'The console entrypoint probe failed without output.' }
    Write-HermesLog -Component setup -Level WARN -Message (
        "Hermes console entrypoint is unhealthy and will be regenerated in place: $reason"
    )

    $uv = (Get-Command uv.exe -ErrorAction Stop).Source
    $env:UV_CACHE_DIR = Join-Path $RepositoryRoot 'cache\uv'
    $env:VIRTUAL_ENV = $runtime
    $env:UV_PROJECT_ENVIRONMENT = $runtime

    $null = Invoke-HermesProcess `
        -FilePath $uv `
        -ArgumentList @(
            'pip', 'install',
            '--python', $python,
            '--reinstall',
            '--no-deps',
            '--editable', $source
        ) `
        -WorkingDirectory $RepositoryRoot `
        -LogComponent setup

    $verification = Test-HermesConsoleEntrypoint -Executable $hermes -WorkingDirectory $source
    if (-not $verification.Healthy) {
        $detail = if ($verification.Detail) { $verification.Detail } else { 'No verification output was produced.' }
        throw "Hermes console entrypoint regeneration did not produce a working executable:`n$detail"
    }

    Write-HermesLog -Component setup -Message (
        'Regenerated the Hermes console entrypoint against the final runtime path.'
    )
    Write-Host 'Hermes console entrypoint repaired.'
    exit 0
} catch {
    Write-HermesLog -Component setup -Level ERROR -Message $_.Exception.ToString()
    Write-Host "Hermes console entrypoint repair failed: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
}
