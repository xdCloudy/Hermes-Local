[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$root = Split-Path $PSScriptRoot -Parent
Import-Module (Join-Path $root 'scripts\Common-Hermes.psm1') -Force
. (Join-Path $root 'scripts\model-download\ModelDownload-Common.ps1')

$failures = [System.Collections.Generic.List[string]]::new()
$cleanup = [System.Collections.Generic.List[string]]::new()

function Assert-True {
    param([bool] $Condition, [string] $Message)
    if (-not $Condition) {
        $script:failures.Add($Message)
    }
}

function Assert-Throws {
    param([scriptblock] $Operation, [string] $Pattern, [string] $Message)
    try {
        & $Operation
        $script:failures.Add("$Message (operation did not throw)")
    } catch {
        if ($_.Exception.Message -notmatch $Pattern) {
            $script:failures.Add("$Message (unexpected message: $($_.Exception.Message))")
        }
    }
}

try {
    $taskId = "issue107-$([guid]::NewGuid().ToString('N'))"
    $modelId = "issue107-$([guid]::NewGuid().ToString('N').Substring(0, 12))"
    $folder = "models\issue107-$([guid]::NewGuid().ToString('N'))"
    $targetRelative = "$folder\fixture.gguf"
    $targetPath = Resolve-HermesPath $targetRelative
    $manifestPath = Resolve-HermesPath "models\manifests\$modelId.json"
    $cleanup.Add((Resolve-HermesPath $folder))
    $cleanup.Add($manifestPath)

    $identity = ConvertTo-HermesSafeSourceIdentity -SourceUrl 'https://example.invalid/models/fixture.gguf'
    Assert-True ($identity.url -eq 'https://example.invalid/models/fixture.gguf') 'Safe source identity changed a public immutable URL.'
    Assert-Throws { ConvertTo-HermesSafeSourceIdentity -SourceUrl 'http://example.invalid/model.gguf' } 'HTTPS' 'HTTP sources must be rejected.'
    Assert-Throws { ConvertTo-HermesSafeSourceIdentity -SourceUrl 'https://user:secret@example.invalid/model.gguf' } 'Credentials' 'Embedded credentials must be rejected.'
    Assert-Throws { ConvertTo-HermesSafeSourceIdentity -SourceUrl 'https://example.invalid/model.gguf?token=secret' } 'query-bearing' 'Signed query URLs must not enter durable task state.'
    Assert-Throws { Resolve-HermesModelDownloadPath -RelativePath 'models\..\outside.gguf' -Kind model } 'traversal' 'Traversal targets must be rejected.'

    $context = New-HermesModelDownloadContext `
        -TaskId $taskId `
        -SourceUrl 'https://example.invalid/models/fixture.gguf' `
        -Repository 'example/fixture' `
        -Revision ('1' * 40) `
        -ModelId $modelId `
        -DisplayName 'Issue 107 fixture' `
        -Alias $modelId `
        -Filename 'fixture.gguf' `
        -TargetRelativePath $targetRelative `
        -AuxiliaryFiles @() `
        -KeepPartialOnCancel $true
    $cleanup.Add($context.ProgressPath)
    $cleanup.Add($context.ControlPath)
    $cleanup.Add($context.ReportPath)
    $cleanup.Add($context.LogPath)
    $cleanup.Add($context.LockPath)

    [System.IO.Directory]::CreateDirectory([System.IO.Path]::GetDirectoryName($context.Primary.partialPath)) | Out-Null
    [System.IO.File]::WriteAllBytes($context.Primary.partialPath, [System.Text.Encoding]::UTF8.GetBytes('verified replacement payload'))
    Test-HermesModelDownloadFile -Context $context -File $context.Primary -Index 0

    $progress = Write-HermesModelDownloadProgress `
        -Context $context `
        -Stage download `
        -Message 'Fixture progress' `
        -Status running `
        -BytesCompleted 10 `
        -BytesTotal 20 `
        -RateBytesPerSecond 5 `
        -EtaSeconds 2
    Assert-True ($progress.taskId -eq $taskId) 'Progress did not retain the stable task id.'
    Assert-True ($progress.progress.percent -eq 50) 'Determinate byte progress was not calculated.'
    Assert-True ($progress.progress.rateBytesPerSecond -eq 5) 'Transfer rate was not persisted.'
    Assert-True ($progress.progress.etaSeconds -eq 2) 'ETA was not persisted.'
    Assert-True ($progress.source.identity.url -notmatch 'secret|token') 'Progress leaked source credentials.'

    Enter-HermesModelDownloadLock -Context $context
    $duplicate = New-HermesModelDownloadContext `
        -TaskId "issue107-$([guid]::NewGuid().ToString('N'))" `
        -SourceUrl 'https://example.invalid/models/fixture.gguf' `
        -Repository 'example/fixture' `
        -Revision ('1' * 40) `
        -ModelId "$modelId-duplicate" `
        -DisplayName 'Issue 107 duplicate' `
        -Alias "$modelId-duplicate" `
        -Filename 'fixture.gguf' `
        -TargetRelativePath $targetRelative `
        -AuxiliaryFiles @() `
        -KeepPartialOnCancel $true
    Assert-Throws { Enter-HermesModelDownloadLock -Context $duplicate } 'already owned' 'Concurrent tasks must not own the same target.'
    Exit-HermesModelDownloadLock -Context $context

    [System.IO.File]::WriteAllText($targetPath, 'existing valid model', [System.Text.UTF8Encoding]::new($false))
    $manifest = New-HermesModelDownloadManifest -Context $context
    Invoke-HermesModelDownloadPromotion -Context $context -Manifest $manifest
    Assert-True ((Get-Content -Raw -LiteralPath $targetPath) -eq 'verified replacement payload') 'Verified model was not atomically promoted.'
    Assert-True (Test-Path -LiteralPath $manifestPath -PathType Leaf) 'Verified model manifest was not registered.'
    $registered = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json -Depth 32
    Assert-True ($registered.id -eq $modelId) 'Registered manifest model id is incorrect.'
    Assert-True ($registered.sha256 -eq $context.Primary.actualSha256) 'Registered manifest hash does not match the promoted artifact.'

    Complete-HermesModelDownload -Context $context -Status succeeded -Message 'Fixture completed.'
    $terminal = Get-Content -Raw -LiteralPath $context.ProgressPath | ConvertFrom-Json -Depth 32
    Assert-True ($terminal.status -eq 'succeeded') 'Terminal progress was not persisted.'
    Assert-True ($terminal.result.model -eq $targetRelative.Replace('\', '/')) 'Terminal result does not link the installed model.'
    Assert-True ($terminal.result.manifest -eq "models/manifests/$modelId.json") 'Terminal result does not link the manifest.'

    $entryScript = Get-Content -Raw -LiteralPath (Join-Path $root 'Invoke-Hermes-ModelDownload.ps1')
    $commonScript = (Get-ChildItem -LiteralPath (Join-Path $root 'scripts\model-download') -Filter '*.ps1' -File |
        Sort-Object Name | ForEach-Object { Get-Content -Raw -LiteralPath $_.FullName }) -join [Environment]::NewLine
    foreach ($contract in @(
        'RangeHeaderValue',
        '.partial',
        'hash-verification',
        'auxiliary-file-verification',
        'manifest-generation',
        'promotion',
        'registration',
        'AvailableFreeSpace',
        'KeepPartialOnCancel'
    )) {
        Assert-True ($commonScript.Contains($contract) -or $entryScript.Contains($contract)) "Download backend is missing contract marker '$contract'."
    }
} finally {
    foreach ($path in $cleanup | Select-Object -Unique | Sort-Object Length -Descending) {
        Remove-Item -LiteralPath $path -Recurse -Force -ErrorAction SilentlyContinue
    }
}

if ($failures.Count -gt 0) {
    throw ($failures -join [Environment]::NewLine)
}

Write-Host 'Durable model download contract tests passed.'
