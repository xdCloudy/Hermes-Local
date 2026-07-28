[CmdletBinding(SupportsShouldProcess, ConfirmImpact = 'High')]
param(
    [Parameter(Mandatory)]
    [string] $BackupPath,
    [switch] $NonInteractive
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'scripts\Common-Hermes.psm1') -Force

$staging = $null

try {
    Assert-HermesRoot
    Initialize-HermesLayout
    $resolvedBackup = [System.IO.Path]::GetFullPath($BackupPath)
    $backupRoot = (Resolve-HermesPath 'backups').TrimEnd('\') + '\'
    if (-not $resolvedBackup.StartsWith($backupRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Restore archives must be selected from $backupRoot"
    }
    if (-not (Test-Path -LiteralPath $resolvedBackup -PathType Leaf) -or
        [System.IO.Path]::GetExtension($resolvedBackup) -ne '.zip') {
        throw "Backup archive is missing or not a zip file: $resolvedBackup"
    }
    $sidecar = "$resolvedBackup.sha256"
    if (Test-Path -LiteralPath $sidecar) {
        $expected = ((Get-Content -Raw -LiteralPath $sidecar).Trim() -split '\s+')[0]
        $actual = (Get-FileHash -LiteralPath $resolvedBackup -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($actual -ne $expected.ToLowerInvariant()) {
            throw 'Backup archive SHA-256 validation failed.'
        }
    }

    $staging = Resolve-HermesPath "temp\restore-$([guid]::NewGuid().ToString('N'))"
    [System.IO.Directory]::CreateDirectory($staging) | Out-Null
    $stagingPrefix = $staging.TrimEnd('\') + '\'
    $archive = [System.IO.Compression.ZipFile]::OpenRead($resolvedBackup)
    try {
        foreach ($entry in $archive.Entries) {
            if ([System.IO.Path]::IsPathRooted($entry.FullName) -or $entry.FullName -match '(^|[\\/])\.\.([\\/]|$)') {
                throw "Unsafe archive entry: $($entry.FullName)"
            }
            $target = [System.IO.Path]::GetFullPath((Join-Path $staging $entry.FullName))
            if ($target -ne $staging -and -not $target.StartsWith($stagingPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
                throw "Archive entry escapes the restore staging directory: $($entry.FullName)"
            }
        }
    } finally {
        $archive.Dispose()
    }
    [System.IO.Compression.ZipFile]::ExtractToDirectory($resolvedBackup, $staging)
    $manifestPath = Join-Path $staging 'backup-manifest.json'
    if (-not (Test-Path -LiteralPath $manifestPath)) {
        throw 'Backup manifest is missing.'
    }
    $manifest = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
    if ($manifest.schemaVersion -ne 1 -or $manifest.product -ne 'Hermes Local') {
        throw 'Backup manifest is not a supported Hermes Local archive.'
    }

    $null = Invoke-HermesProcess -FilePath 'pwsh.exe' -ArgumentList @(
        '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
        '-File', (Resolve-HermesPath 'Backup-Hermes-Local.ps1'),
        '-Name', "pre-restore-$((Get-Date).ToUniversalTime().ToString('yyyyMMddTHHmmssZ'))",
        '-NonInteractive'
    ) -LogComponent backup
    $null = Invoke-HermesProcess -FilePath 'pwsh.exe' -ArgumentList @(
        '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
        '-File', (Resolve-HermesPath 'Stop-Hermes-Local.ps1'), '-NonInteractive'
    ) -LogComponent backup

    $restoreTarget = "$(Resolve-HermesPath 'config') and $(Resolve-HermesPath 'data')"
    if (-not $NonInteractive -and -not $PSCmdlet.ShouldProcess($restoreTarget, "Restore $resolvedBackup")) {
        Write-Host 'Restore cancelled.'
        exit 2
    }
    foreach ($relative in @('config', 'data\hermes', 'data\sessions', 'data\memory', 'data\skills', 'data\cron', 'data\databases', 'data\user')) {
        $source = Join-Path $staging $relative
        if (-not (Test-Path -LiteralPath $source)) {
            continue
        }
        $target = Resolve-HermesPath $relative
        if (Test-Path -LiteralPath $target) {
            Remove-Item -LiteralPath $target -Recurse -Force
        }
        [System.IO.Directory]::CreateDirectory([System.IO.Path]::GetDirectoryName($target)) | Out-Null
        Copy-Item -LiteralPath $source -Destination $target -Recurse -Force
    }
    Write-HermesLog -Component backup -Message "Restored backup $resolvedBackup."
    Write-Host "Hermes Local restored from: $resolvedBackup"
    exit 0
} catch {
    Write-HermesLog -Component backup -Level ERROR -Message $_.Exception.ToString()
    Write-Host "Hermes Local restore failed: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
} finally {
    if ($staging -and (Test-Path -LiteralPath $staging)) {
        $resolvedStaging = [System.IO.Path]::GetFullPath($staging)
        $restorePrefix = Resolve-HermesPath 'temp\restore-'
        if ($resolvedStaging.StartsWith($restorePrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
            Remove-Item -LiteralPath $resolvedStaging -Recurse -Force
        }
    }
}
