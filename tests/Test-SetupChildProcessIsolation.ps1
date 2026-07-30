[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$root = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$setupPath = Join-Path $root 'Setup-Hermes-Local.ps1'
$setup = [System.IO.File]::ReadAllText($setupPath)

if ($setup -notmatch 'function Invoke-IsolatedPowerShellScript') {
    throw 'Setup does not define the isolated PowerShell child runner.'
}
if ($setup -match '&\s+\$launcherBuild') {
    throw 'Launcher build is still invoked directly in the setup process.'
}
if ($setup -match '&\s+\$diagnosticScript') {
    throw 'Bootstrap diagnostics are still invoked directly in the setup process.'
}

$temp = Join-Path ([System.IO.Path]::GetTempPath()) ("hermes-module-scope-" + [guid]::NewGuid().ToString('N'))
[System.IO.Directory]::CreateDirectory($temp) | Out-Null
try {
    $modulePath = Join-Path $temp 'ScopeProbe.psm1'
    $childPath = Join-Path $temp 'Child.ps1'
    @'
function Get-ScopeProbe { 'parent-command-still-loaded' }
Export-ModuleMember -Function Get-ScopeProbe
'@ | Set-Content -LiteralPath $modulePath -Encoding utf8
    @"
Import-Module '$modulePath' -Force
if ((Get-ScopeProbe) -ne 'parent-command-still-loaded') { exit 2 }
exit 0
"@ | Set-Content -LiteralPath $childPath -Encoding utf8

    Import-Module $modulePath -Force
    $hostExecutable = (Get-Process -Id $PID -ErrorAction Stop).Path
    & $hostExecutable -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File $childPath
    if ($LASTEXITCODE -ne 0) {
        throw "Isolated child process failed with exit code $LASTEXITCODE."
    }
    if ((Get-ScopeProbe) -ne 'parent-command-still-loaded') {
        throw 'Parent module command disappeared after isolated child execution.'
    }
} finally {
    Remove-Item -LiteralPath $temp -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Host 'Setup child-process isolation tests passed.'
