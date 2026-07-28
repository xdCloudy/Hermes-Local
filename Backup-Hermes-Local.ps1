[CmdletBinding()]
param(
    [string] $Name,
    [switch] $NonInteractive
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'scripts\Common-Hermes.psm1') -Force

$wasRunning = $false
$profile = 'Daily'
$staging = $null

try {
    Assert-HermesRoot
    Initialize-HermesLayout
    $statusPath = Resolve-HermesPath 'data\runtime\status.json'
    if (Test-Path -LiteralPath $statusPath) {
        $status = Get-Content -Raw -LiteralPath $statusPath | ConvertFrom-Json
        $wasRunning = $status.phase -eq 'running'
        if ($status.profile) {
            $profile = [string]$status.profile
        }
    }

    if ($wasRunning) {
        $null = Invoke-HermesProcess -FilePath 'pwsh.exe' -ArgumentList @(
            '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
            '-File', (Resolve-HermesPath 'Stop-Hermes-Local.ps1'), '-NonInteractive'
        ) -LogComponent backup
    }

    $stamp = (Get-Date).ToUniversalTime().ToString('yyyyMMddTHHmmssZ')
    $safeName = if ($Name) {
        if ($Name -notmatch '^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$') {
            throw 'Backup name must contain only letters, numbers, dot, underscore, or hyphen.'
        }
        "-$Name"
    } else {
        ''
    }
    $archive = Resolve-HermesPath "backups\Hermes-Local-$stamp$safeName.zip"
    $staging = Resolve-HermesPath "temp\backup-$([guid]::NewGuid().ToString('N'))"
    [System.IO.Directory]::CreateDirectory($staging) | Out-Null

    foreach ($relative in @(
        'VERSION.json',
        'config',
        'data\hermes',
        'data\sessions',
        'data\memory',
        'data\skills',
        'data\cron',
        'data\databases',
        'data\user'
    )) {
        $source = Resolve-HermesPath $relative
        if (Test-Path -LiteralPath $source) {
            $destination = Join-Path $staging $relative
            [System.IO.Directory]::CreateDirectory([System.IO.Path]::GetDirectoryName($destination)) | Out-Null
            Copy-Item -LiteralPath $source -Destination $destination -Recurse -Force
        }
    }

    $manifest = [ordered]@{
        schemaVersion = 1
        product = 'Hermes Local'
        createdAt = (Get-Date).ToUniversalTime().ToString('o')
        sourceRoot = 'D:\Hermes-Local'
        profile = $profile
        version = Get-HermesVersionManifest
    }
    [System.IO.File]::WriteAllText(
        (Join-Path $staging 'backup-manifest.json'),
        (($manifest | ConvertTo-Json -Depth 32) + [Environment]::NewLine),
        [System.Text.UTF8Encoding]::new($false)
    )
    [System.IO.Compression.ZipFile]::CreateFromDirectory(
        $staging,
        $archive,
        [System.IO.Compression.CompressionLevel]::Optimal,
        $false
    )
    $hash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
    Write-HermesAtomicText -Path "$archive.sha256" -Content ("$hash  $([System.IO.Path]::GetFileName($archive))" + [Environment]::NewLine)
    Write-HermesLog -Component backup -Message "Created backup $archive with SHA-256 $hash."
    Write-Host "Hermes Local backup created: $archive"
    Write-Host "SHA-256: $hash"
} catch {
    Write-HermesLog -Component backup -Level ERROR -Message $_.Exception.ToString()
    Write-Host "Hermes Local backup failed: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
} finally {
    if ($staging -and (Test-Path -LiteralPath $staging)) {
        $resolvedStaging = [System.IO.Path]::GetFullPath($staging)
        if ($resolvedStaging.StartsWith('D:\Hermes-Local\temp\backup-', [System.StringComparison]::OrdinalIgnoreCase)) {
            Remove-Item -LiteralPath $resolvedStaging -Recurse -Force
        }
    }
    if ($wasRunning) {
        try {
            $null = Invoke-HermesProcess -FilePath 'pwsh.exe' -ArgumentList @(
                '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
                '-File', (Resolve-HermesPath 'Start-Hermes-Local.ps1'),
                '-Profile', $profile, '-NonInteractive'
            ) -LogComponent backup
        } catch {
            Write-HermesLog -Component backup -Level ERROR -Message "Backup completed but stack restart failed: $($_.Exception.Message)"
        }
    }
}

exit 0
