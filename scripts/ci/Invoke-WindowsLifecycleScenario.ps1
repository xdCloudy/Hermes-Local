[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidatePattern('^[0-9a-fA-F]{40}$')][string] $Candidate,
    [Parameter(Mandatory)][string] $Scenario,
    [Parameter(Mandatory)][string] $EvidenceDirectory,
    [string] $PackageDirectory,
    [string] $PreviousInstaller,
    [string] $SandboxRoot,
    [string] $MatrixPath = 'config\validation\windows-lifecycle-matrix.json',
    [string] $HardwareSmokeScript
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repositoryRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$resolvedMatrix = [IO.Path]::GetFullPath($MatrixPath, $repositoryRoot)
$resolvedEvidence = [IO.Path]::GetFullPath($EvidenceDirectory, $repositoryRoot)
$sandboxBase = if ($SandboxRoot) {
    [IO.Path]::GetFullPath($SandboxRoot, $repositoryRoot)
} elseif ($env:RUNNER_TEMP) {
    Join-Path ([IO.Path]::GetFullPath($env:RUNNER_TEMP)) "hermes-lifecycle-$Scenario-$([guid]::NewGuid().ToString('N'))"
} else {
    Join-Path ([IO.Path]::GetFullPath((Join-Path $repositoryRoot 'temp\windows-lifecycle'))) "$Scenario-$([guid]::NewGuid().ToString('N'))"
}
$sandbox = [IO.Path]::GetFullPath($sandboxBase)
$fixtureRoot = Join-Path $sandbox 'user-owned-data'
$fixtureManifest = Join-Path $fixtureRoot '.lifecycle-fixture.json'
$logPath = Join-Path $resolvedEvidence "$Scenario.log"
$transcriptPath = Join-Path $resolvedEvidence "$Scenario.transcript.log"
$evidencePath = Join-Path $resolvedEvidence "$Scenario.json"
$startedAt = (Get-Date).ToUniversalTime().ToString('o')
$checks = [Collections.Generic.List[string]]::new()
$failures = [Collections.Generic.List[string]]::new()
$status = 'passed'
$skipReason = $null
$gpuName = $null
$gpuDriver = $null

function Invoke-LifecycleTool {
    param([Parameter(Mandatory)][string[]] $Arguments)

    & python (Join-Path $repositoryRoot 'scripts\ci\windows_lifecycle.py') `
        --matrix $resolvedMatrix @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "windows_lifecycle.py failed: $($Arguments -join ' ')"
    }
}

function Resolve-PackageArtifact {
    param([Parameter(Mandatory)][ValidateSet('installer', 'portable')][string] $Kind)

    if (-not $PackageDirectory) {
        throw "Scenario '$Scenario' requires -PackageDirectory."
    }
    $root = [IO.Path]::GetFullPath($PackageDirectory, $repositoryRoot)
    if (-not (Test-Path -LiteralPath $root -PathType Container)) {
        throw "Package directory does not exist: $root"
    }
    $pattern = if ($Kind -eq 'installer') { '*windows*x64*setup.exe' } else { '*windows*x64*portable.exe' }
    $matches = @(Get-ChildItem -LiteralPath $root -Filter $pattern -File -Recurse | Sort-Object FullName)
    if ($matches.Count -ne 1) {
        throw "Expected exactly one $Kind artifact matching '$pattern' under $root; found $($matches.Count)."
    }
    return $matches[0].FullName
}

function Resolve-InstalledExecutable {
    param([Parameter(Mandatory)][string] $InstallRoot)

    $candidates = @(
        (Join-Path $InstallRoot 'Hermes Launcher.exe'),
        (Join-Path $InstallRoot 'Hermes.exe')
    )
    $match = $candidates | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Select-Object -First 1
    if (-not $match) {
        throw "Installed launcher was not found under $InstallRoot."
    }
    return $match
}

function Invoke-SilentInstall {
    param(
        [Parameter(Mandatory)][string] $Installer,
        [Parameter(Mandatory)][string] $InstallRoot
    )

    $null = New-Item -ItemType Directory -Path ([IO.Path]::GetDirectoryName($InstallRoot)) -Force
    $process = Start-Process -FilePath $Installer -ArgumentList @('/S', "/D=$InstallRoot") -Wait -PassThru
    if ($process.ExitCode -ne 0) {
        throw "Installer exited $($process.ExitCode): $Installer"
    }
    $checks.Add("silent install exited 0: $InstallRoot")
    return Resolve-InstalledExecutable -InstallRoot $InstallRoot
}

function Stop-OwnedProcessTree {
    param([Parameter(Mandatory)][Diagnostics.Process] $Process)

    if ($Process.HasExited) {
        return
    }
    $taskkill = Join-Path ($env:SystemRoot ?? 'C:\Windows') 'System32\taskkill.exe'
    & $taskkill /PID ([string]$Process.Id) /T /F 2>&1 | Add-Content -LiteralPath $logPath
    if ($LASTEXITCODE -ne 0 -and -not $Process.HasExited) {
        throw "Unable to stop owned launcher process tree $($Process.Id)."
    }
}

function Invoke-IsolatedLauncherSmoke {
    param(
        [Parameter(Mandatory)][string] $Executable,
        [Parameter(Mandatory)][string] $Label,
        [switch] $Offline,
        [switch] $CpuOnly
    )

    $userData = Join-Path $sandbox "$Label-electron-user-data"
    $null = New-Item -ItemType Directory -Path $userData -Force
    $info = [Diagnostics.ProcessStartInfo]::new()
    $info.FileName = $Executable
    $info.WorkingDirectory = $sandbox
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.Environment['HERMES_DESKTOP_BOOT_FAKE'] = '1'
    $info.Environment['HERMES_DESKTOP_BOOT_FAKE_STEP_MS'] = '20'
    $info.Environment['HERMES_DESKTOP_IGNORE_EXISTING'] = '1'
    $info.Environment['HERMES_DESKTOP_TEST_MODE'] = 'windows-lifecycle'
    $info.Environment['HERMES_DESKTOP_USER_DATA_DIR'] = $userData
    $info.Environment['HERMES_HOME'] = $fixtureRoot
    if ($Offline) {
        $info.Environment['HTTP_PROXY'] = 'http://127.0.0.1:9'
        $info.Environment['HTTPS_PROXY'] = 'http://127.0.0.1:9'
        $info.Environment['NO_PROXY'] = '127.0.0.1,localhost'
    }
    if ($CpuOnly) {
        $info.Environment['CUDA_VISIBLE_DEVICES'] = '-1'
    }

    $process = [Diagnostics.Process]::Start($info)
    if (-not $process) {
        throw "Launcher did not start: $Executable"
    }
    try {
        Start-Sleep -Seconds 8
        if ($process.HasExited -and $process.ExitCode -ne 0) {
            throw "Launcher smoke exited $($process.ExitCode): $Executable"
        }
        $checks.Add("isolated fake-boot launcher smoke passed: $Label")
    } finally {
        Stop-OwnedProcessTree -Process $process
        $process.Dispose()
    }
}

function Invoke-SilentUninstall {
    param([Parameter(Mandatory)][string] $InstallRoot)

    $uninstallers = @(Get-ChildItem -LiteralPath $InstallRoot -Filter '*ninstall*.exe' -File -ErrorAction SilentlyContinue)
    if ($uninstallers.Count -ne 1) {
        throw "Expected one uninstaller under $InstallRoot; found $($uninstallers.Count)."
    }
    $process = Start-Process -FilePath $uninstallers[0].FullName -ArgumentList '/S' -Wait -PassThru
    if ($process.ExitCode -ne 0) {
        throw "Uninstaller exited $($process.ExitCode)."
    }
    $checks.Add('silent uninstall exited 0')
}

function Invoke-StandardScenario {
    param(
        [Parameter(Mandatory)][string] $InstallLeaf,
        [switch] $Offline,
        [switch] $CpuOnly,
        [switch] $Uninstall,
        [switch] $Reinstall
    )

    $installer = Resolve-PackageArtifact -Kind installer
    $installRoot = Join-Path $sandbox $InstallLeaf
    $executable = Invoke-SilentInstall -Installer $installer -InstallRoot $installRoot
    Invoke-IsolatedLauncherSmoke -Executable $executable -Label 'installed' -Offline:$Offline -CpuOnly:$CpuOnly
    if ($Uninstall -or $Reinstall) {
        Invoke-SilentUninstall -InstallRoot $installRoot
        Start-Sleep -Milliseconds 500
        if (Test-Path -LiteralPath $executable) {
            throw "Uninstall retained the managed launcher binary: $executable"
        }
        $checks.Add('managed launcher removed while fixture remained external')
    }
    if ($Reinstall) {
        $executable = Invoke-SilentInstall -Installer $installer -InstallRoot $installRoot
        Invoke-IsolatedLauncherSmoke -Executable $executable -Label 'reinstalled'
    }
}

function Invoke-PhysicalSmoke {
    if (-not $HardwareSmokeScript) {
        throw "Physical scenario '$Scenario' requires -HardwareSmokeScript."
    }
    $script = [IO.Path]::GetFullPath($HardwareSmokeScript, $repositoryRoot)
    if (-not (Test-Path -LiteralPath $script -PathType Leaf) -or [IO.Path]::GetExtension($script) -ne '.ps1') {
        throw "Hardware smoke script must be an existing PowerShell file: $script"
    }
    if ($Scenario -eq 'physical-nvidia') {
        $gpu = Get-CimInstance Win32_VideoController | Where-Object { $_.Name -match 'NVIDIA' } | Select-Object -First 1
        if (-not $gpu) {
            throw 'The physical-nvidia runner does not expose an NVIDIA adapter.'
        }
        $script:gpuName = [string]$gpu.Name
        $script:gpuDriver = [string]$gpu.DriverVersion
    }
    & pwsh.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File $script 2>&1 |
        Add-Content -LiteralPath $logPath
    if ($LASTEXITCODE -ne 0) {
        throw "Hardware smoke script exited $LASTEXITCODE."
    }
    $checks.Add("trusted hardware smoke passed: $Scenario")
}

$null = New-Item -ItemType Directory -Path $resolvedEvidence -Force
$null = New-Item -ItemType Directory -Path $sandbox -Force
Set-Content -LiteralPath (Join-Path $sandbox '.hermes-lifecycle-sandbox') -Value $Scenario -Encoding utf8NoBOM
Start-Transcript -LiteralPath $transcriptPath -Force | Out-Null

try {
    Invoke-LifecycleTool -Arguments @('validate')
    Invoke-LifecycleTool -Arguments @('create-fixture', '--root', $fixtureRoot)

    switch ($Scenario) {
        'clean-standard' { Invoke-StandardScenario -InstallLeaf 'standard-install' }
        'clean-portable' {
            $portable = Resolve-PackageArtifact -Kind portable
            Invoke-IsolatedLauncherSmoke -Executable $portable -Label 'portable'
        }
        'clean-offline' { Invoke-StandardScenario -InstallLeaf 'offline-install' -Offline }
        'clean-cpu' { Invoke-StandardScenario -InstallLeaf 'cpu-install' -CpuOnly }
        'clean-path-spaces' { Invoke-StandardScenario -InstallLeaf 'path with spaces\Hermes Launcher' }
        'clean-path-unicode' { Invoke-StandardScenario -InstallLeaf '日本語\Hermes Launcher' }
        'clean-no-dev-tools' { Invoke-StandardScenario -InstallLeaf 'no-dev-tools' }
        'clean-secondary-drive' {
            $secondary = Get-PSDrive -PSProvider FileSystem | Where-Object { $_.Name -ne ([IO.Path]::GetPathRoot($repositoryRoot)).TrimEnd(':\') } | Select-Object -First 1
            if (-not $secondary) {
                $status = 'skipped'
                $skipReason = 'No secondary filesystem drive is attached to this runner.'
            } else {
                $secondaryRoot = Join-Path $secondary.Root "hermes-lifecycle-$([guid]::NewGuid().ToString('N'))"
                $originalSandbox = $sandbox
                try {
                    $script:sandbox = $secondaryRoot
                    $null = New-Item -ItemType Directory -Path $script:sandbox -Force
                    Invoke-StandardScenario -InstallLeaf 'secondary-drive-install'
                } finally {
                    $script:sandbox = $originalSandbox
                }
            }
        }
        'upgrade-stable' {
            if (-not $PreviousInstaller) {
                $status = 'skipped'
                $skipReason = 'No previous Stable installer was supplied.'
            } else {
                $previous = [IO.Path]::GetFullPath($PreviousInstaller, $repositoryRoot)
                $installRoot = Join-Path $sandbox 'upgrade-install'
                $null = Invoke-SilentInstall -Installer $previous -InstallRoot $installRoot
                $current = Resolve-PackageArtifact -Kind installer
                $executable = Invoke-SilentInstall -Installer $current -InstallRoot $installRoot
                Invoke-IsolatedLauncherSmoke -Executable $executable -Label 'upgraded'
            }
        }
        'uninstall-app-only' { Invoke-StandardScenario -InstallLeaf 'uninstall-app-only' -Uninstall }
        'uninstall-preserve-data' { Invoke-StandardScenario -InstallLeaf 'uninstall-preserve-data' -Uninstall }
        'reinstall-preserved' { Invoke-StandardScenario -InstallLeaf 'reinstall-preserved' -Reinstall }
        'clean-interrupted-download' {
            & (Join-Path $repositoryRoot 'tests\Test-ModelArtifactContract.ps1') 2>&1 | Add-Content -LiteralPath $logPath
            if ($LASTEXITCODE -ne 0) { throw 'Model artifact resume contract failed.' }
            $checks.Add('interrupted model artifact resume contract passed')
        }
        'physical-cpu' {
            Invoke-StandardScenario -InstallLeaf 'physical-cpu-package' -CpuOnly
            Invoke-PhysicalSmoke
        }
        'physical-nvidia' {
            Invoke-StandardScenario -InstallLeaf 'physical-nvidia-package'
            Invoke-PhysicalSmoke
        }
        default {
            $status = 'skipped'
            $skipReason = "Scenario '$Scenario' requires a lifecycle implementation or controlled environment not yet available to this runner."
        }
    }
} catch {
    $status = 'failed'
    $failures.Add($_.Exception.Message)
    $_ | Out-String | Add-Content -LiteralPath $logPath
} finally {
    Stop-Transcript | Out-Null
}

$arguments = @(
    'record', '--scenario', $Scenario, '--candidate', $Candidate, '--status', $status,
    '--output', $evidencePath, '--started-at', $startedAt, '--log', $logPath
)
if ((Test-Path -LiteralPath $fixtureRoot) -and (Test-Path -LiteralPath $fixtureManifest)) {
    $arguments += @('--fixture-root', $fixtureRoot, '--fixture-manifest', $fixtureManifest)
}
foreach ($check in $checks) { $arguments += @('--check', $check) }
foreach ($failure in $failures) { $arguments += @('--failure', $failure) }
$arguments += @('--log', $transcriptPath)
if ($skipReason) { $arguments += @('--skip-reason', $skipReason) }
if ($gpuName) { $arguments += @('--gpu-name', $gpuName) }
if ($gpuDriver) { $arguments += @('--gpu-driver', $gpuDriver) }

Invoke-LifecycleTool -Arguments $arguments
if ($status -eq 'failed') { exit 1 }
exit 0
