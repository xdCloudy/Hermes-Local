[CmdletBinding()]
param(
    [switch] $Force
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'scripts\Common-Hermes.psm1') -Force

function Test-SecurityArtifactHash {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string] $Path,

        [Parameter(Mandatory)]
        [string] $Sha256
    )

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return $false
    }

    $actual = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash
    return $actual.Equals($Sha256, [System.StringComparison]::OrdinalIgnoreCase)
}

function Save-SecurityArtifact {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string] $Name,

        [Parameter(Mandatory)]
        [string] $Uri,

        [Parameter(Mandatory)]
        [string] $Sha256,

        [Parameter(Mandatory)]
        [string] $Destination,

        [switch] $Reinstall
    )

    if (-not $Reinstall -and (Test-SecurityArtifactHash -Path $Destination -Sha256 $Sha256)) {
        Write-HermesLog -Component security -Message "$Name download is already present and verified."
        return
    }

    $parent = [System.IO.Path]::GetDirectoryName($Destination)
    [System.IO.Directory]::CreateDirectory($parent) | Out-Null

    $partial = "$Destination.partial"
    Remove-Item -LiteralPath $partial -Force -ErrorAction SilentlyContinue

    try {
        Invoke-HermesProcess -FilePath 'curl.exe' -ArgumentList @(
            '--location',
            '--fail',
            '--show-error',
            '--retry', '8',
            '--retry-all-errors',
            '--output', $partial,
            $Uri
        ) -LogComponent security

        if (-not (Test-SecurityArtifactHash -Path $partial -Sha256 $Sha256)) {
            $actual = if (Test-Path -LiteralPath $partial -PathType Leaf) {
                (Get-FileHash -LiteralPath $partial -Algorithm SHA256).Hash
            } else {
                '<missing>'
            }
            throw "$Name checksum verification failed. Expected $Sha256; received $actual."
        }

        Move-Item -LiteralPath $partial -Destination $Destination -Force
    } finally {
        Remove-Item -LiteralPath $partial -Force -ErrorAction SilentlyContinue
    }

    Write-HermesLog -Component security -Message "$Name downloaded and verified."
}

function Test-GitleaksInstallation {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string] $Executable,

        [Parameter(Mandatory)]
        [string] $Version
    )

    if (-not (Test-Path -LiteralPath $Executable -PathType Leaf)) {
        return $false
    }

    try {
        $output = (@(& $Executable version 2>$null) -join [Environment]::NewLine).Trim()
        return $LASTEXITCODE -eq 0 -and $output -match [regex]::Escape($Version)
    } catch {
        return $false
    }
}

function Install-Gitleaks {
    [CmdletBinding()]
    param([switch] $Reinstall)

    $version = '8.30.1'
    $archiveSha256 = 'D29144DEFF3A68AA93CED33DDDF84B7FDC26070ADD4AA0F4513094C8332AFC4E'
    $archiveUri = "https://github.com/gitleaks/gitleaks/releases/download/v$version/gitleaks_${version}_windows_x64.zip"
    $target = Resolve-HermesPath "runtimes\tools\security\gitleaks-$version"
    $executable = Join-Path $target 'gitleaks.exe'

    if (-not $Reinstall -and (Test-GitleaksInstallation -Executable $executable -Version $version)) {
        Write-HermesLog -Component security -Message "Gitleaks $version is already installed."
        return
    }

    $archive = Resolve-HermesPath "cache\security-tools\gitleaks_${version}_windows_x64.zip"
    Save-SecurityArtifact `
        -Name "Gitleaks $version" `
        -Uri $archiveUri `
        -Sha256 $archiveSha256 `
        -Destination $archive `
        -Reinstall:$Reinstall

    $staging = "$target.staging-$([guid]::NewGuid().ToString('N'))"
    try {
        [System.IO.Directory]::CreateDirectory($staging) | Out-Null
        Expand-Archive -LiteralPath $archive -DestinationPath $staging -Force

        $stagedExecutable = Join-Path $staging 'gitleaks.exe'
        if (-not (Test-GitleaksInstallation -Executable $stagedExecutable -Version $version)) {
            throw "Gitleaks $version archive did not produce a valid gitleaks.exe."
        }

        Remove-Item -LiteralPath $target -Recurse -Force -ErrorAction SilentlyContinue
        Move-Item -LiteralPath $staging -Destination $target
    } finally {
        Remove-Item -LiteralPath $staging -Recurse -Force -ErrorAction SilentlyContinue
    }

    Write-HermesLog -Component security -Message "Installed Gitleaks $version at $executable."
}

function Install-OsvScanner {
    [CmdletBinding()]
    param([switch] $Reinstall)

    $version = '2.4.0'
    $sha256 = '0CDD113610126D5DFD5E12AD0E0B4F3E879291FF19BB43B0C52ED2F2C2DF1A37'
    $uri = "https://github.com/google/osv-scanner/releases/download/v$version/osv-scanner_windows_amd64.exe"
    $destination = Resolve-HermesPath "runtimes\tools\security\osv-scanner-$version\osv-scanner.exe"

    Save-SecurityArtifact `
        -Name "OSV-Scanner $version" `
        -Uri $uri `
        -Sha256 $sha256 `
        -Destination $destination `
        -Reinstall:$Reinstall

    Write-HermesLog -Component security -Message "Installed OSV-Scanner $version at $destination."
}

try {
    Assert-HermesRoot
    Initialize-HermesLayout
    Set-HermesProcessEnvironment

    Install-Gitleaks -Reinstall:$Force
    Install-OsvScanner -Reinstall:$Force

    Write-Host 'Hermes Local security tools are installed and verified.'
} catch {
    try {
        Write-HermesLog -Component security -Level ERROR -Message $_.Exception.ToString()
    } catch {
    }
    throw
}
