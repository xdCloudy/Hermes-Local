[CmdletBinding(SupportsShouldProcess, ConfirmImpact = 'High')]
param(
    [ValidateSet('Check', 'Compatibility', 'Apply', 'Rollback')]
    [string] $Mode = 'Check',

    [ValidateSet(
        'All', 'HermesAgent', 'Launcher', 'LlamaCpp', 'Model',
        'PythonLock', 'NodeLock', 'BrowserBinaries', 'OptionalTools'
    )]
    [string] $Component = 'All',

    [ValidateSet('Cli', 'Desktop', 'Installer', 'Recovery')]
    [string] $Caller = 'Cli',

    [ValidatePattern('^[0-9a-fA-F]{40}$')]
    [string] $TargetCommit,

    [ValidatePattern('^[A-Za-z0-9._/-]+$')]
    [string] $TargetBranch,

    [string] $ReleaseManifestPath,

    [string] $ArtifactRoot,

    [string] $AttestationBundleDirectory,

    [string] $TrustedRootPath,

    [switch] $NonInteractive
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

Import-Module (Join-Path $PSScriptRoot 'scripts\Common-Hermes.psm1') -Force
Import-Module (Join-Path $PSScriptRoot 'scripts\Hermes-UpdateOrchestrator.psm1') -Force
Import-Module (Join-Path $PSScriptRoot 'scripts\Hermes-RuntimeUpdateAdapter.psm1') -Force

function Invoke-HermesReleasePreflight {
    param(
        [Parameter(Mandatory)][string] $ManifestPath,
        [string] $Root,
        [string] $BundleDirectory,
        [string] $TrustedRoot
    )

    $manifest = [System.IO.Path]::GetFullPath($ManifestPath)
    if (-not (Test-Path -LiteralPath $manifest -PathType Leaf)) {
        throw "Release manifest is missing: $manifest"
    }
    $artifactDirectory = if ($Root) {
        [System.IO.Path]::GetFullPath($Root)
    } else {
        [System.IO.Path]::GetDirectoryName($manifest)
    }
    if (-not (Test-Path -LiteralPath $artifactDirectory -PathType Container)) {
        throw "Release artifact root is missing: $artifactDirectory"
    }

    $managedPython = Resolve-HermesPath 'runtimes\python\hermes\Scripts\python.exe'
    $pythonPath = if (Test-Path -LiteralPath $managedPython -PathType Leaf) {
        $managedPython
    } else {
        $pythonCommand = Get-Command python.exe -ErrorAction SilentlyContinue
        if (-not $pythonCommand) {
            $pythonCommand = Get-Command python -ErrorAction Stop
        }
        $pythonCommand.Source
    }
    $tool = Resolve-HermesPath 'scripts\ci\release_integrity.py'
    if (-not (Test-Path -LiteralPath $tool -PathType Leaf)) {
        throw "Release integrity verifier is missing: $tool"
    }
    $report = Resolve-HermesPath 'build\release-integrity\LATEST.json'
    $arguments = @(
        $tool, 'verify',
        '--manifest', $manifest,
        '--artifact-root', $artifactDirectory,
        '--require-attestation',
        '--report', $report
    )
    if ($BundleDirectory) {
        $arguments += @('--attestation-bundle-dir', [System.IO.Path]::GetFullPath($BundleDirectory))
    }
    if ($TrustedRoot) {
        $arguments += @('--trusted-root', [System.IO.Path]::GetFullPath($TrustedRoot))
    }

    $null = Invoke-HermesProcess `
        -FilePath $pythonPath `
        -ArgumentList $arguments `
        -WorkingDirectory (Get-HermesRoot) `
        -LogComponent update

    $verification = Get-Content -Raw -LiteralPath $report | ConvertFrom-Json -Depth 64
    if ([string]$verification.status -ne 'verified') {
        throw 'Release integrity verification did not produce a verified result.'
    }
    Write-Host "Release integrity verified for $([string]$verification.release.version) ($([string]$verification.release.channel))." -ForegroundColor Green
    [ordered]@{
        manifest = $manifest
        artifactRoot = $artifactDirectory
        report = $report
        status = [string]$verification.status
        release = $verification.release
    }
}

try {
    Assert-HermesRoot
    Initialize-HermesLayout
    Set-HermesProcessEnvironment

    if ($Mode -in @('Apply', 'Rollback') -and -not $NonInteractive) {
        if (-not $PSCmdlet.ShouldProcess("Hermes Local $Component", $Mode)) {
            Write-Host "$Mode cancelled."
            exit 2
        }
    }

    $inputRecord = @{}
    if ($TargetCommit) {
        $inputRecord.TargetCommit = $TargetCommit.ToLowerInvariant()
    }
    if ($TargetBranch) {
        $inputRecord.TargetBranch = $TargetBranch
    }
    $desktopTaskId = [Environment]::GetEnvironmentVariable('HERMES_LOCAL_TASK_ID')
    if ($desktopTaskId) {
        if ($desktopTaskId -notmatch '^[0-9a-fA-F-]{16,64}$') {
            throw 'HERMES_LOCAL_TASK_ID contains an invalid task identity.'
        }
        $inputRecord.TaskId = $desktopTaskId
    }

    # Installer/update-package promotion is blocked before lock acquisition,
    # backup or mutation unless every required release control verifies.
    if ($Mode -eq 'Apply' -and $Caller -eq 'Installer' -and -not $ReleaseManifestPath) {
        throw 'Installer promotion requires -ReleaseManifestPath and verified release provenance.'
    }
    if ($Mode -eq 'Apply' -and $ReleaseManifestPath) {
        $inputRecord.ReleaseIntegrity = Invoke-HermesReleasePreflight `
            -ManifestPath $ReleaseManifestPath `
            -Root $ArtifactRoot `
            -BundleDirectory $AttestationBundleDirectory `
            -TrustedRoot $TrustedRootPath
    }

    $result = Invoke-HermesUpdateOperation `
        -Mode $Mode `
        -Component $Component `
        -Caller $Caller `
        -Input $inputRecord

    $result | ConvertTo-Json -Depth 64

    if ($result.status -eq 'succeeded') {
        Write-Host "Update operation $($result.operationId) completed. State: $($result.statePath)"
        exit 0
    }

    if ($result.status -eq 'rolled-back') {
        Write-Host "Update operation $($result.operationId) failed and was rolled back. State: $($result.statePath)" -ForegroundColor Yellow
        exit 1
    }

    Write-Host "Update operation $($result.operationId) failed. State: $($result.statePath)" -ForegroundColor Red
    exit 1
} catch {
    try {
        Write-HermesLog -Component update -Level ERROR -Message $_.Exception.ToString()
    } catch {
        Write-Warning "Could not write the update failure log: $($_.Exception.Message)"
    }
    Write-Host "Hermes Local update $Mode failed: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
}
