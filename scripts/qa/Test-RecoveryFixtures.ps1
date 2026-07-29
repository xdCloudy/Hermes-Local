[CmdletBinding()]
param(
    [string] $OutputPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$fixtureParent = [IO.Path]::GetFullPath((Join-Path $root 'temp\qa-recovery-fixtures'))
$fixture = Join-Path $fixtureParent ([guid]::NewGuid().ToString('N'))
$results = [Collections.Generic.List[object]]::new()

function Add-Result {
    param(
        [Parameter(Mandatory)][string] $Id,
        [Parameter(Mandatory)][bool] $Passed,
        [Parameter(Mandatory)][string] $Detail
    )

    $results.Add([pscustomobject]@{
        id = $Id
        passed = $Passed
        detail = $Detail
    })
}

function Invoke-FixtureScript {
    param(
        [Parameter(Mandatory)][string] $Name,
        [Parameter(Mandatory)][string[]] $Arguments
    )

    $output = & pwsh.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass `
        -File (Join-Path $fixture $Name) @Arguments 2>&1

    return [pscustomobject]@{
        exitCode = $LASTEXITCODE
        output = (($output | ForEach-Object { [string]$_ }) -join [Environment]::NewLine)
    }
}

try {
    $null = New-Item -ItemType Directory -Path $fixture -Force
    $null = New-Item -ItemType Directory -Path (Join-Path $fixture 'scripts') -Force
    Copy-Item -LiteralPath (Join-Path $root 'VERSION.json') -Destination $fixture
    Copy-Item -LiteralPath (Join-Path $root 'Backup-Hermes-Local.ps1') -Destination $fixture
    Copy-Item -LiteralPath (Join-Path $root 'Restore-Hermes-Local.ps1') -Destination $fixture
    Copy-Item -LiteralPath (Join-Path $root 'Stop-Hermes-Local.ps1') -Destination $fixture
    Copy-Item -LiteralPath (Join-Path $root 'scripts\Common-Hermes.psm1') -Destination (Join-Path $fixture 'scripts')
    Copy-Item -LiteralPath (Join-Path $root 'scripts\Hermes-Configuration.psm1') -Destination (Join-Path $fixture 'scripts')
    Copy-Item -LiteralPath (Join-Path $root 'config') -Destination $fixture -Recurse
    $null = New-Item -ItemType Directory -Path (Join-Path $fixture 'models') -Force
    Copy-Item -LiteralPath (Join-Path $root 'models\manifests') -Destination (Join-Path $fixture 'models') -Recurse

    $userDirectory = Join-Path $fixture 'data\user'
    $null = New-Item -ItemType Directory -Path $userDirectory -Force
    $markerPath = Join-Path $userDirectory 'fixture-state.txt'
    [IO.File]::WriteAllText($markerPath, 'original fixture state', [Text.UTF8Encoding]::new($false))
    $settingsPath = Join-Path $fixture 'config\launcher\user-settings.json'
    $settingsHash = (Get-FileHash -LiteralPath $settingsPath -Algorithm SHA256).Hash

    $backup = Invoke-FixtureScript -Name 'Backup-Hermes-Local.ps1' -Arguments @(
        '-Name', 'qa-fixture', '-NonInteractive'
    )
    $archives = @(Get-ChildItem -LiteralPath (Join-Path $fixture 'backups') -Filter '*-qa-fixture.zip' -File)
    $archive = $archives | Select-Object -First 1
    Add-Result -Id 'backup-custom-name' -Passed ($backup.exitCode -eq 0 -and $archives.Count -eq 1) `
        -Detail "exit=$($backup.exitCode); archives=$($archives.Count)"
    if (-not $archive) {
        throw 'Fixture backup did not produce an archive for the remaining assertions.'
    }

    $sidecarHash = ((Get-Content -Raw -LiteralPath "$($archive.FullName).sha256").Trim() -split '\s+')[0]
    $archiveHash = (Get-FileHash -LiteralPath $archive.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    Add-Result -Id 'backup-hash-sidecar' -Passed ($sidecarHash -eq $archiveHash) `
        -Detail 'Archive hash matches its sidecar.'

    [IO.File]::WriteAllText($markerPath, 'changed after backup', [Text.UTF8Encoding]::new($false))
    [IO.File]::AppendAllText($settingsPath, [Environment]::NewLine)
    $restore = Invoke-FixtureScript -Name 'Restore-Hermes-Local.ps1' -Arguments @(
        '-BackupPath', $archive.FullName, '-NonInteractive'
    )
    $restoredMarker = Get-Content -Raw -LiteralPath $markerPath
    $restoredSettingsHash = (Get-FileHash -LiteralPath $settingsPath -Algorithm SHA256).Hash
    Add-Result -Id 'restore-valid' `
        -Passed (
            $restore.exitCode -eq 0 -and
            $restoredMarker -eq 'original fixture state' -and
            $restoredSettingsHash -eq $settingsHash
        ) `
        -Detail "exit=$($restore.exitCode); state and settings restored byte-for-byte."

    $missing = Invoke-FixtureScript -Name 'Restore-Hermes-Local.ps1' -Arguments @(
        '-BackupPath', (Join-Path $fixture 'backups\missing.zip'), '-NonInteractive'
    )
    Add-Result -Id 'restore-missing-backup' -Passed ($missing.exitCode -eq 1) `
        -Detail "exit=$($missing.exitCode); missing archive rejected."

    $invalidName = Invoke-FixtureScript -Name 'Backup-Hermes-Local.ps1' -Arguments @(
        '-Name', 'ordinary name with spaces', '-NonInteractive'
    )
    Add-Result -Id 'backup-invalid-name' -Passed ($invalidName.exitCode -eq 1) `
        -Detail "exit=$($invalidName.exitCode); invalid ordinary name rejected."

    $badHashArchive = Join-Path $fixture 'backups\bad-hash.zip'
    Copy-Item -LiteralPath $archive.FullName -Destination $badHashArchive
    [IO.File]::WriteAllText(
        "$badHashArchive.sha256",
        ('0' * 64) + '  bad-hash.zip' + [Environment]::NewLine,
        [Text.UTF8Encoding]::new($false)
    )
    $badHash = Invoke-FixtureScript -Name 'Restore-Hermes-Local.ps1' -Arguments @(
        '-BackupPath', $badHashArchive, '-NonInteractive'
    )
    Add-Result -Id 'restore-hash-mismatch' -Passed ($badHash.exitCode -eq 1) `
        -Detail "exit=$($badHash.exitCode); hash mismatch rejected before restore."

    $manifestFixture = Join-Path $fixture 'temp\missing-manifest-source'
    $null = New-Item -ItemType Directory -Path (Join-Path $manifestFixture 'data\user') -Force
    [IO.File]::WriteAllText(
        (Join-Path $manifestFixture 'data\user\state.txt'),
        'ordinary incomplete backup',
        [Text.UTF8Encoding]::new($false)
    )
    $missingManifestArchive = Join-Path $fixture 'backups\missing-manifest.zip'
    [IO.Compression.ZipFile]::CreateFromDirectory($manifestFixture, $missingManifestArchive)
    $missingManifest = Invoke-FixtureScript -Name 'Restore-Hermes-Local.ps1' -Arguments @(
        '-BackupPath', $missingManifestArchive, '-NonInteractive'
    )
    Add-Result -Id 'restore-missing-manifest' -Passed ($missingManifest.exitCode -eq 1) `
        -Detail "exit=$($missingManifest.exitCode); incomplete archive rejected."

    $failed = @($results | Where-Object { -not $_.passed })
    $summary = [ordered]@{
        schemaVersion = 1
        generatedAt = (Get-Date).ToUniversalTime().ToString('o')
        passed = $failed.Count -eq 0
        tests = $results.Count
        passedTests = @($results | Where-Object passed).Count
        failedTests = $failed.Count
        results = @($results)
    }

    if ($OutputPath) {
        $resolvedOutput = [IO.Path]::GetFullPath($OutputPath, $root)
        $null = New-Item -ItemType Directory -Path ([IO.Path]::GetDirectoryName($resolvedOutput)) -Force
        [IO.File]::WriteAllText(
            $resolvedOutput,
            (($summary | ConvertTo-Json -Depth 8) + [Environment]::NewLine),
            [Text.UTF8Encoding]::new($false)
        )
    }

    $summary | ConvertTo-Json -Depth 8
    if (-not $summary.passed) {
        exit 1
    }
} finally {
    $resolvedFixture = [IO.Path]::GetFullPath($fixture)
    $fixturePrefix = $fixtureParent.TrimEnd('\') + '\'
    if (
        (Test-Path -LiteralPath $resolvedFixture) -and
        $resolvedFixture.StartsWith($fixturePrefix, [StringComparison]::OrdinalIgnoreCase)
    ) {
        Remove-Item -LiteralPath $resolvedFixture -Recurse -Force
    }
}

exit 0
