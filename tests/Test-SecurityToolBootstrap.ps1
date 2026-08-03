[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$root = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$installerPath = Join-Path $root 'Install-Hermes-SecurityTools.ps1'
$wrapperPath = Join-Path $root 'Security-Scan-Hermes-Local.ps1'
$implementationPath = Join-Path $root 'Security-Scan-Hermes-Local.Impl.ps1'

foreach ($path in @($installerPath, $wrapperPath, $implementationPath)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Required script is missing: $path"
    }

    $tokens = $null
    $errors = $null
    [System.Management.Automation.Language.Parser]::ParseFile(
        $path,
        [ref]$tokens,
        [ref]$errors
    ) | Out-Null
    if (@($errors).Count -gt 0) {
        $details = @($errors | ForEach-Object { $_.Message }) -join '; '
        throw "PowerShell parser errors in ${path}: $details"
    }
}

$installer = [System.IO.File]::ReadAllText($installerPath)
$wrapper = [System.IO.File]::ReadAllText($wrapperPath)

$requiredInstallerFragments = @(
    'gitleaks_8.30.1_windows_x64.zip',
    'D29144DEFF3A68AA93CED33DDDF84B7FDC26070ADD4AA0F4513094C8332AFC4E',
    'osv-scanner_windows_amd64.exe',
    '0CDD113610126D5DFD5E12AD0E0B4F3E879291FF19BB43B0C52ED2F2C2DF1A37',
    'Test-SecurityArtifactHash',
    'Expand-Archive'
)
foreach ($fragment in $requiredInstallerFragments) {
    if (-not $installer.Contains($fragment, [System.StringComparison]::Ordinal)) {
        throw "Security tool installer is missing required fragment: $fragment"
    }
}

$installerIndex = $wrapper.IndexOf('& $installer', [System.StringComparison]::Ordinal)
$implementationIndex = $wrapper.IndexOf('& $implementation @arguments', [System.StringComparison]::Ordinal)
if ($installerIndex -lt 0) {
    throw 'Security scan wrapper does not invoke the security tool installer.'
}
if ($implementationIndex -lt 0) {
    throw 'Security scan wrapper does not invoke the preserved implementation.'
}
if ($installerIndex -ge $implementationIndex) {
    throw 'Security tool bootstrap must run before the security scan implementation.'
}

Write-Host 'Security tool bootstrap contract tests passed.'
