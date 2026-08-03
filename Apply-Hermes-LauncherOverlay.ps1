[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidateSet('Apply', 'Restore')][string] $Mode,
    [Parameter(Mandatory)][string] $StatePath,
    [string] $RepositoryRoot = $PSScriptRoot
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$payload = @(
    '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'
) -join ''
$compressed = [Convert]::FromBase64String($payload)
$input = [System.IO.MemoryStream]::new($compressed, $false)
$gzip = [System.IO.Compression.GzipStream]::new($input, [System.IO.Compression.CompressionMode]::Decompress)
$output = [System.IO.MemoryStream]::new()
try {
    $gzip.CopyTo($output)
} finally {
    $gzip.Dispose()
    $input.Dispose()
}
$source = [System.Text.UTF8Encoding]::new($false).GetString($output.ToArray())
$output.Dispose()
$transformer = [scriptblock]::Create($source)
& $transformer -Mode $Mode -StatePath $StatePath -RepositoryRoot $RepositoryRoot
