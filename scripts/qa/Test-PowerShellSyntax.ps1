[CmdletBinding()]
param(
    [string] $OutputPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$tracked = @(& git.exe -C $root ls-files -- '*.ps1' '*.psm1')
$qaFiles = @(
    Get-ChildItem -LiteralPath $PSScriptRoot -File |
        Where-Object Extension -In @('.ps1', '.psm1') |
        ForEach-Object { [IO.Path]::GetRelativePath($root, $_.FullName).Replace('\', '/') }
)
$files = @($tracked + $qaFiles | Sort-Object -Unique)
$results = [System.Collections.Generic.List[object]]::new()

foreach ($relativePath in $files) {
    $excluded = $relativePath -eq 'Security-Scan-Hermes-Local.ps1' -or
        $relativePath.StartsWith('scripts/security/', [StringComparison]::OrdinalIgnoreCase)

    if ($excluded) {
        $results.Add([pscustomobject]@{
            file = $relativePath
            scope = 'security-excluded'
            passed = $null
            errors = @()
        })
        continue
    }

    $absolutePath = Join-Path $root $relativePath
    $tokens = $null
    $parseErrors = $null
    $null = [Management.Automation.Language.Parser]::ParseFile(
        $absolutePath,
        [ref] $tokens,
        [ref] $parseErrors
    )

    $results.Add([pscustomobject]@{
        file = $relativePath
        scope = 'functional-qa'
        passed = $parseErrors.Count -eq 0
        errors = @(
            $parseErrors | ForEach-Object {
                [pscustomobject]@{
                    line = $_.Extent.StartLineNumber
                    column = $_.Extent.StartColumnNumber
                    message = $_.Message
                }
            }
        )
    })
}

$document = [ordered]@{
    schemaVersion = 1
    generatedAt = (Get-Date).ToUniversalTime().ToString('o')
    summary = [ordered]@{
        total = $results.Count
        functionalQa = @($results | Where-Object scope -EQ 'functional-qa').Count
        securityExcluded = @($results | Where-Object scope -EQ 'security-excluded').Count
        failed = @($results | Where-Object passed -EQ $false).Count
    }
    entries = @($results)
}

if ($OutputPath) {
    $resolvedOutput = [IO.Path]::GetFullPath($OutputPath, $root)
    $outputDirectory = Split-Path -Parent $resolvedOutput
    $null = New-Item -ItemType Directory -Path $outputDirectory -Force
    [IO.File]::WriteAllText(
        $resolvedOutput,
        (($document | ConvertTo-Json -Depth 8) + [Environment]::NewLine),
        [Text.UTF8Encoding]::new($false)
    )
}

$document.summary | Format-List | Out-Host

if ($document.summary.failed -gt 0) {
    $results |
        Where-Object passed -EQ $false |
        ForEach-Object {
            foreach ($parseError in $_.errors) {
                Write-Error "$($_.file):$($parseError.line):$($parseError.column): $($parseError.message)"
            }
        }

    exit 1
}

exit 0
