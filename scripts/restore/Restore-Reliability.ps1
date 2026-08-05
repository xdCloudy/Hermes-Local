# Loaded after Restore-Common.ps1. These implementations deliberately replace
# the lower-level primitives whose correctness depends on Windows process and
# filesystem behaviour.

function New-HermesRestoreContext {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $Root,
        [Parameter(Mandatory)][string] $TaskId,
        [Parameter(Mandatory)][string] $BackupPath,
        [bool] $VerifyIntegrity = $true
    )

    $rootFull = [IO.Path]::GetFullPath($Root)
    $logDirectory = Join-Path $rootFull 'logs\restore'
    $runtimeDirectory = Join-Path $rootFull 'data\runtime'
    [IO.Directory]::CreateDirectory($logDirectory) | Out-Null
    [IO.Directory]::CreateDirectory($runtimeDirectory) | Out-Null

    [pscustomobject]@{
        Root = $rootFull
        TaskId = $TaskId
        BackupPath = [IO.Path]::GetFullPath($BackupPath)
        VerifyIntegrity = $VerifyIntegrity
        StartedAt = (Get-Date).ToUniversalTime()
        ProgressPath = Join-Path $runtimeDirectory 'restore-progress.json'
        CancelPath = Join-Path $runtimeDirectory 'restore-cancel.json'
        LogPath = Join-Path $logDirectory "restore-$TaskId.log"
        ReportPath = Join-Path $logDirectory "restore-$TaskId.json"
        LatestReportPath = Join-Path $logDirectory 'LATEST.json'
        TerminalStatus = $null
        Backup = $null
        SafetySnapshot = $null
        PreviousState = $null
        ActiveState = 'original-active'
        PromotionAttempted = $false
        PromotionJournal = [Collections.Generic.List[object]]::new()
        RestorePromoted = $false
        RollbackAttempted = $false
        RollbackSucceeded = $null
        RollbackRoot = $null
        FailedStateRoot = $null
        Cancellable = $true
    }
}

function Get-HermesRestoreArchivePlan {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $Root,
        [Parameter(Mandatory)][string] $BackupPath,
        [bool] $VerifyIntegrity = $true
    )

    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $resolved = Resolve-HermesRestoreBackupPath -Root $Root -BackupPath $BackupPath
    $expectedHash = Get-HermesRestoreSidecarHash -ArchivePath $resolved
    $actualHash = (Get-FileHash -LiteralPath $resolved -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($VerifyIntegrity -and $actualHash -ne $expectedHash) {
        throw 'Backup archive SHA-256 validation failed.'
    }

    $archive = [IO.Compression.ZipFile]::OpenRead($resolved)
    try {
        if ($archive.Entries.Count -lt 1 -or $archive.Entries.Count -gt 250000) {
            throw 'Backup archive contains an unsafe number of entries.'
        }

        $seen = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
        $manifestEntry = $null
        $totalBytes = [int64]0
        $fileCount = 0

        foreach ($entry in $archive.Entries) {
            if (-not (Test-HermesRestoreArchiveEntryName -Name $entry.FullName)) {
                throw "Unsafe or unexpected archive entry: $($entry.FullName)"
            }

            $normalized = $entry.FullName.Replace('\', '/').TrimEnd('/')
            if (-not $seen.Add($normalized)) {
                throw "Backup archive contains a duplicate path: $normalized"
            }

            $unixMode = ([int64]$entry.ExternalAttributes -shr 16) -band 0xF000
            if ($unixMode -eq 0xA000) {
                throw "Backup archive contains an unsupported symbolic link: $normalized"
            }

            if (-not [string]::IsNullOrEmpty($entry.Name)) {
                $fileCount += 1
                $length = [int64]$entry.Length
                if ($length -lt 0 -or $totalBytes -gt ([int64]::MaxValue - $length)) {
                    throw 'Backup archive expanded size is invalid.'
                }
                $totalBytes += $length
            }

            if ($normalized.Equals('backup-manifest.json', [StringComparison]::OrdinalIgnoreCase)) {
                $manifestEntry = $entry
            }
        }

        if (-not $manifestEntry) {
            throw 'Backup manifest is missing.'
        }
        if ($manifestEntry.Length -le 0 -or $manifestEntry.Length -gt 1048576) {
            throw 'Backup manifest size is invalid.'
        }

        $manifestStream = $manifestEntry.Open()
        $reader = [IO.StreamReader]::new($manifestStream, [Text.UTF8Encoding]::new($false, $true), $true)
        try {
            $manifestText = $reader.ReadToEnd()
        } finally {
            $reader.Dispose()
            $manifestStream.Dispose()
        }

        try {
            $manifest = $manifestText | ConvertFrom-Json -Depth 64
        } catch {
            throw "Backup manifest is not valid JSON: $($_.Exception.Message)"
        }

        if ([int](Get-HermesRestoreValue -Record $manifest -Name schemaVersion -Default 0) -ne 1) {
            throw 'Backup manifest schema is not supported.'
        }
        if ([string](Get-HermesRestoreValue -Record $manifest -Name product -Default '') -ne 'Hermes Local') {
            throw 'Backup manifest is not a Hermes Local archive.'
        }

        $createdAt = [DateTimeOffset]::MinValue
        if (-not [DateTimeOffset]::TryParse(
            [string](Get-HermesRestoreValue -Record $manifest -Name createdAt -Default ''),
            [ref]$createdAt
        )) {
            throw 'Backup manifest creation time is invalid.'
        }
        if ($null -eq (Get-HermesRestoreValue -Record $manifest -Name version -Default $null)) {
            throw 'Backup manifest version identity is missing.'
        }

        $driveRoot = [IO.Path]::GetPathRoot([IO.Path]::GetFullPath($Root))
        $freeBytes = ([IO.DriveInfo]::new($driveRoot)).AvailableFreeSpace
        if ($totalBytes -gt [int64]($freeBytes * 0.90)) {
            throw 'Backup archive does not fit safely in the available staging space.'
        }

        $compressedBytes = [math]::Max([int64]1, [int64](Get-Item -LiteralPath $resolved).Length)
        if ($totalBytes -gt 1073741824 -and ([double]$totalBytes / [double]$compressedBytes) -gt 1000) {
            throw 'Backup archive expansion ratio is unsafe.'
        }

        [pscustomobject]@{
            Path = $resolved
            RelativePath = ConvertTo-HermesRestoreRelativePath -Root $Root -Path $resolved
            Name = [IO.Path]::GetFileName($resolved)
            Id = $actualHash.Substring(0, 16)
            Sha256 = $actualHash
            CreatedAt = $createdAt.ToUniversalTime().ToString('o')
            Profile = [string](Get-HermesRestoreValue -Record $manifest -Name profile -Default '')
            Manifest = $manifest
            FileCount = $fileCount
            EntryCount = $archive.Entries.Count
            ExpandedBytes = $totalBytes
        }
    } finally {
        $archive.Dispose()
    }
}

function Invoke-HermesRestoreNativeProcess {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $FilePath,
        [Parameter(Mandatory)][string[]] $ArgumentList,
        [Parameter(Mandatory)][string] $WorkingDirectory
    )

    $startInfo = [Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $FilePath
    $startInfo.WorkingDirectory = $WorkingDirectory
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    foreach ($argument in $ArgumentList) {
        $startInfo.ArgumentList.Add([string]$argument)
    }

    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $startInfo
    try {
        if (-not $process.Start()) {
            throw "Could not start process $FilePath"
        }
        $stdoutTask = $process.StandardOutput.ReadToEndAsync()
        $stderrTask = $process.StandardError.ReadToEndAsync()
        $process.WaitForExit()
        $stdout = $stdoutTask.GetAwaiter().GetResult()
        $stderr = $stderrTask.GetAwaiter().GetResult()
        [pscustomobject]@{
            ExitCode = $process.ExitCode
            Output = (@($stdout.TrimEnd(), $stderr.TrimEnd()) | Where-Object { $_ }) -join [Environment]::NewLine
        }
    } finally {
        $process.Dispose()
    }
}

function Invoke-HermesRestoreProcess {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][object] $Context,
        [Parameter(Mandatory)][string] $ScriptPath,
        [string[]] $Arguments = @(),
        [Parameter(Mandatory)][string] $Description
    )

    $pwsh = (Get-Command pwsh.exe -ErrorAction Stop).Source
    $result = Invoke-HermesRestoreNativeProcess `
        -FilePath $pwsh `
        -ArgumentList @(
            '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
            '-File', $ScriptPath
        ) + @($Arguments) `
        -WorkingDirectory $Context.Root

    foreach ($line in @($result.Output -split '\r?\n') | Where-Object { $_ }) {
        Add-HermesRestoreLog -Context $Context -Message "$Description: $line"
    }
    if ($result.ExitCode -ne 0) {
        throw "$Description failed with exit code $($result.ExitCode). See $($Context.LogPath)"
    }
    $result
}

function Invoke-HermesRestorePromotion {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][object] $Context,
        [Parameter(Mandatory)][string] $StagingRoot,
        [Parameter(Mandatory)][string] $RollbackRoot,
        [scriptblock] $BeforeScope
    )

    if (Test-Path -LiteralPath $RollbackRoot) {
        throw "Rollback staging already exists: $RollbackRoot"
    }
    [IO.Directory]::CreateDirectory($RollbackRoot) | Out-Null
    $Context.PromotionJournal.Clear()

    $index = 0
    foreach ($scope in $script:HermesRestoreScopes) {
        $index += 1
        if ($BeforeScope) {
            & $BeforeScope 'promote' $scope $index
        }

        $source = Join-Path $StagingRoot $scope
        $target = Join-Path $Context.Root $scope
        $rollback = Join-Path $RollbackRoot $scope
        $hadOriginal = Test-Path -LiteralPath $target
        $hasReplacement = Test-Path -LiteralPath $source

        if ($hadOriginal) {
            [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($rollback)) | Out-Null
            Move-Item -LiteralPath $target -Destination $rollback
        }

        try {
            if ($hasReplacement) {
                [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($target)) | Out-Null
                Move-Item -LiteralPath $source -Destination $target
            }
        } catch {
            if ($hadOriginal -and -not (Test-Path -LiteralPath $target) -and (Test-Path -LiteralPath $rollback)) {
                Move-Item -LiteralPath $rollback -Destination $target -ErrorAction SilentlyContinue
            }
            throw
        }

        $entry = [pscustomobject]@{
            Scope = $scope
            HadOriginal = $hadOriginal
            HasReplacement = $hasReplacement
            Target = $target
            Rollback = $rollback
        }
        $Context.PromotionJournal.Add($entry)

        $null = Write-HermesRestoreProgress `
            -Context $Context `
            -Stage 'data-restoration' `
            -Message "Restored declared data scope $scope." `
            -Status 'running' `
            -Cancellable $false `
            -CompletedUnits $index `
            -TotalUnits $script:HermesRestoreScopes.Count
    }

    @($Context.PromotionJournal)
}

function Invoke-HermesRestoreRollback {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][object] $Context,
        [Parameter(Mandatory)][object[]] $Journal,
        [Parameter(Mandatory)][string] $FailedStateRoot,
        [scriptblock] $BeforeScope
    )

    [IO.Directory]::CreateDirectory($FailedStateRoot) | Out-Null
    $errors = [Collections.Generic.List[string]]::new()
    $entries = @($Journal)
    [array]::Reverse($entries)

    foreach ($entry in $entries) {
        try {
            if ($BeforeScope) {
                & $BeforeScope 'rollback' ([string]$entry.Scope)
            }
            if (Test-Path -LiteralPath $entry.Target) {
                $failed = Join-Path $FailedStateRoot ([string]$entry.Scope)
                [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($failed)) | Out-Null
                Move-Item -LiteralPath $entry.Target -Destination $failed
            }
            if ([bool]$entry.HadOriginal -and (Test-Path -LiteralPath $entry.Rollback)) {
                [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName([string]$entry.Target)) | Out-Null
                Move-Item -LiteralPath $entry.Rollback -Destination $entry.Target
            }
        } catch {
            $errors.Add("$($entry.Scope): $($_.Exception.Message)")
        }
    }

    [pscustomobject]@{
        Succeeded = $errors.Count -eq 0
        Errors = @($errors)
    }
}
