[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$root = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$wrapperPath = Join-Path $root 'Setup-Hermes-Local.ps1'
$implementationPath = Join-Path $root 'Setup-Hermes-Local.Impl.ps1'
$migrationPath = Join-Path $root 'scripts\setup\Python-RuntimeMigration.ps1'

foreach ($path in @($wrapperPath, $implementationPath, $migrationPath)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Required setup migration file is missing: $path"
    }

    $tokens = $null
    $parseErrors = $null
    [System.Management.Automation.Language.Parser]::ParseFile(
        $path,
        [ref]$tokens,
        [ref]$parseErrors
    ) | Out-Null
    if (@($parseErrors).Count -gt 0) {
        $details = @($parseErrors | ForEach-Object { $_.Message }) -join '; '
        throw "PowerShell parser errors in ${path}: $details"
    }
}

. $migrationPath

$temp = Join-Path ([System.IO.Path]::GetTempPath()) ("hermes-python-migration-" + [guid]::NewGuid().ToString('N'))
[System.IO.Directory]::CreateDirectory($temp) | Out-Null
try {
    $runtime = Join-Path $temp 'runtimes\python\hermes'
    [System.IO.Directory]::CreateDirectory((Join-Path $runtime 'Scripts')) | Out-Null
    Set-Content -LiteralPath (Join-Path $runtime 'runtime-marker.txt') -Value 'preserve-me' -Encoding utf8

    $fixedTimestamp = [datetime]::SpecifyKind(
        [datetime]::ParseExact('20260803-171900', 'yyyyMMdd-HHmmss', $null),
        [System.DateTimeKind]::Utc
    )
    $rollback = Move-HermesPythonRuntimeToRollback `
        -Runtime $runtime `
        -RuntimeVersion '3.11' `
        -Timestamp $fixedTimestamp

    $expectedRollback = Join-Path $temp 'runtimes\python\hermes-python311-20260803-171900'
    if ($rollback -ne $expectedRollback) {
        throw "Unexpected rollback path. Expected '$expectedRollback'; received '$rollback'."
    }
    if (Test-Path -LiteralPath $runtime) {
        throw 'The incompatible active runtime was not moved out of the live path.'
    }
    if (-not (Test-Path -LiteralPath (Join-Path $rollback 'runtime-marker.txt') -PathType Leaf)) {
        throw 'The rollback copy did not preserve the previous runtime contents.'
    }

    $manifestPath = Join-Path $temp 'VERSION.json'
    @{
        runtime = @{
            python = '3.13.14'
        }
    } | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $manifestPath -Encoding utf8

    if ((Get-HermesTargetPythonMinorVersion -ManifestPath $manifestPath) -ne '3.13') {
        throw 'The target Python minor version was not derived from VERSION.json.'
    }

    $incompleteRuntime = Join-Path $temp 'second\runtimes\python\hermes'
    [System.IO.Directory]::CreateDirectory($incompleteRuntime) | Out-Null
    Set-Content -LiteralPath (Join-Path $incompleteRuntime 'partial.txt') -Value 'partial' -Encoding utf8
    $unknownRollback = Invoke-HermesPythonRuntimeMigration `
        -Runtime $incompleteRuntime `
        -ManifestPath $manifestPath
    if (-not $unknownRollback -or -not (Test-Path -LiteralPath $unknownRollback -PathType Container)) {
        throw 'An incomplete runtime was not preserved before rebuild.'
    }
    if ([System.IO.Path]::GetFileName($unknownRollback) -notmatch '^hermes-pythonunknown-') {
        throw "Incomplete runtime used an unexpected rollback name: $unknownRollback"
    }
} finally {
    Remove-Item -LiteralPath $temp -Recurse -Force -ErrorAction SilentlyContinue
}

$wrapper = [System.IO.File]::ReadAllText($wrapperPath)
$implementation = [System.IO.File]::ReadAllText($implementationPath)
$migrationIndex = $wrapper.IndexOf('Invoke-HermesPythonRuntimeMigration', [System.StringComparison]::Ordinal)
$implementationIndex = $wrapper.IndexOf('& $implementation @forwardedParameters', [System.StringComparison]::Ordinal)
if ($migrationIndex -lt 0 -or $implementationIndex -lt 0 -or $migrationIndex -ge $implementationIndex) {
    throw 'Setup must migrate the Python runtime before invoking the preserved implementation.'
}
if (-not $wrapper.Contains('if (-not $SkipHermesDependencies)', [System.StringComparison]::Ordinal)) {
    throw 'Setup does not respect -SkipHermesDependencies when deciding whether to migrate the runtime.'
}
if (-not $implementation.Contains('The existing Hermes runtime uses Python', [System.StringComparison]::Ordinal)) {
    throw 'The setup implementation no longer retains its incompatible-runtime safety check.'
}

Write-Host 'Python runtime migration contract passed.'
