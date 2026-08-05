Set-StrictMode -Version Latest

$script:HermesRestoreScopes = @(
    'config',
    'data\hermes',
    'data\sessions',
    'data\memory',
    'data\skills',
    'data\cron',
    'data\databases',
    'data\user'
)
$script:HermesRestoreStages = @(
    'validation',
    'archive-inspection',
    'safety-snapshot',
    'service-shutdown',
    'extraction',
    'data-restoration',
    'configuration-migration',
    'validation-after-restore',
    'service-restart',
    'rollback',
    'complete'
)

function Get-HermesRestoreValue {
    [CmdletBinding()]
    param(
        [AllowNull()][object] $Record,
        [Parameter(Mandatory)][string] $Name,
        [AllowNull()][object] $Default = $null
    )

    if ($null -eq $Record) {
        return $Default
    }
    if ($Record -is [System.Collections.IDictionary]) {
        if ($Record.Contains($Name)) {
            return $Record[$Name]
        }
        return $Default
    }
    $property = $Record.PSObject.Properties[$Name]
    if ($property) {
        return $property.Value
    }
    $Default
}

function Get-HermesRestoreTaskId {
    [CmdletBinding()]
    param([AllowNull()][string] $RequestedTaskId)

    $candidate = if ([string]::IsNullOrWhiteSpace($RequestedTaskId)) {
        [string]$env:HERMES_LOCAL_TASK_ID
    } else {
        $RequestedTaskId
    }
    if ($candidate -and $candidate -match '^[A-Za-z0-9_-]{1,128}$') {
        return $candidate
    }
    [guid]::NewGuid().ToString('N')
}

function ConvertTo-HermesRestoreRelativePath {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $Root,
        [Parameter(Mandatory)][string] $Path
    )

    $rootFull = [IO.Path]::GetFullPath($Root).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    $pathFull = [IO.Path]::GetFullPath($Path)
    if (-not $pathFull.StartsWith($rootFull, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Path is outside the Hermes Local root: $pathFull"
    }
    $pathFull.Substring($rootFull.Length).Replace('\', '/')
}

function Protect-HermesRestoreText {
    [CmdletBinding()]
    param(
        [AllowNull()][string] $Text,
        [AllowNull()][string] $Root
    )

    if ([string]::IsNullOrEmpty($Text)) {
        return ''
    }

    $safe = if (Get-Command Protect-HermesLogText -ErrorAction SilentlyContinue) {
        Protect-HermesLogText $Text
    } else {
        $Text
    }
    $safe = $safe `
        -replace '(?i)(authorization\s*[:=]\s*bearer\s+)[^\s"'']+', '$1[REDACTED]' `
        -replace '(?i)((?:api[_-]?key|password|secret|token|credential)\s*[:=]\s*)[^\s,"'']+', '$1[REDACTED]'

    foreach ($privatePath in @($env:USERPROFILE, $Root) | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }) {
        $safe = $safe.Replace([string]$privatePath, '[PRIVATE-PATH]', [StringComparison]::OrdinalIgnoreCase)
        $safe = $safe.Replace(([string]$privatePath).Replace('\', '/'), '[PRIVATE-PATH]', [StringComparison]::OrdinalIgnoreCase)
    }
    $safe
}

function Write-HermesRestoreAtomicJson {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $Path,
        [Parameter(Mandatory)][object] $Document
    )

    $content = ($Document | ConvertTo-Json -Depth 32) + [Environment]::NewLine
    if (Get-Command Write-HermesAtomicText -ErrorAction SilentlyContinue) {
        Write-HermesAtomicText -Path $Path -Content $content
        return
    }

    [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($Path)) | Out-Null
    $temporary = "$Path.$PID.$([guid]::NewGuid().ToString('N')).tmp"
    [IO.File]::WriteAllText($temporary, $content, [Text.UTF8Encoding]::new($false))
    Move-Item -LiteralPath $temporary -Destination $Path -Force
}

function Add-HermesRestoreLog {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][object] $Context,
        [Parameter(Mandatory)][string] $Message,
        [ValidateSet('INFO', 'WARN', 'ERROR')][string] $Level = 'INFO'
    )

    $line = '[{0}] [{1}] {2}' -f (
        (Get-Date).ToUniversalTime().ToString('o'),
        $Level,
        (Protect-HermesRestoreText -Text $Message -Root ([string]$Context.Root))
    )
    [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName([string]$Context.LogPath)) | Out-Null
    [IO.File]::AppendAllText(
        [string]$Context.LogPath,
        $line + [Environment]::NewLine,
        [Text.UTF8Encoding]::new($false)
    )
    if (Get-Command Write-HermesLog -ErrorAction SilentlyContinue) {
        Write-HermesLog -Component restore -Level $Level -Message $line
    }
}

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
    [IO.Directory]::CreateDirectory($logDirectory) | Out-Null
    $runtimeDirectory = Join-Path $rootFull 'data\runtime'
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
        RestorePromoted = $false
        RollbackAttempted = $false
        RollbackSucceeded = $null
        RollbackRoot = $null
        FailedStateRoot = $null
        Cancellable = $true
    }
}

function Get-HermesRestorePercent {
    [CmdletBinding()]
    param(
        [AllowNull()][object] $CompletedUnits,
        [AllowNull()][object] $TotalUnits
    )

    if ($null -eq $CompletedUnits -or $null -eq $TotalUnits) {
        return $null
    }
    $completed = [double]$CompletedUnits
    $total = [double]$TotalUnits
    if ($completed -lt 0 -or $total -le 0) {
        return $null
    }
    [math]::Round(([math]::Min($completed, $total) / $total) * 100, 1)
}

function Get-HermesRestoreResultDocument {
    [CmdletBinding()]
    param([Parameter(Mandatory)][object] $Context)

    [ordered]@{
        report = ConvertTo-HermesRestoreRelativePath -Root $Context.Root -Path $Context.ReportPath
        log = ConvertTo-HermesRestoreRelativePath -Root $Context.Root -Path $Context.LogPath
        backupIdentity = if ($Context.Backup) { [string]$Context.Backup.Id } else { $null }
        restoredBackup = if ($Context.Backup) { [string]$Context.Backup.RelativePath } else { $null }
        safetySnapshot = if ($Context.SafetySnapshot) { [string]$Context.SafetySnapshot.RelativePath } else { $null }
        activeState = [string]$Context.ActiveState
        restorePromoted = [bool]$Context.RestorePromoted
        rollbackAttempted = [bool]$Context.RollbackAttempted
        rollbackSucceeded = $Context.RollbackSucceeded
    }
}

function Write-HermesRestoreProgress {
    [CmdletBinding(DefaultParameterSetName = 'Indeterminate')]
    param(
        [Parameter(Mandatory)][object] $Context,
        [Parameter(Mandatory)]
        [ValidateSet(
            'validation', 'archive-inspection', 'safety-snapshot', 'service-shutdown',
            'extraction', 'data-restoration', 'configuration-migration',
            'validation-after-restore', 'service-restart', 'rollback', 'complete'
        )]
        [string] $Stage,
        [Parameter(Mandatory)][string] $Message,
        [ValidateSet('running', 'cancelling', 'succeeded', 'failed', 'cancelled')]
        [string] $Status = 'running',
        [Parameter(ParameterSetName = 'Determinate', Mandatory)]
        [ValidateRange(0, 2147483647)][int] $CompletedUnits,
        [Parameter(ParameterSetName = 'Determinate', Mandatory)]
        [ValidateRange(1, 2147483647)][int] $TotalUnits,
        [Parameter(ParameterSetName = 'Indeterminate')][switch] $Indeterminate,
        [bool] $Cancellable = $Context.Cancellable,
        [AllowNull()][string] $FailureCode,
        [AllowNull()][string] $FailureMessage
    )

    $Context.Cancellable = $Cancellable
    $now = (Get-Date).ToUniversalTime()
    $determinate = $PSCmdlet.ParameterSetName -eq 'Determinate'
    $completed = if ($determinate) { $CompletedUnits } else { $null }
    $total = if ($determinate) { $TotalUnits } else { $null }
    $percent = if ($determinate) {
        Get-HermesRestorePercent -CompletedUnits $CompletedUnits -TotalUnits $TotalUnits
    } else {
        $null
    }
    $safeMessage = Protect-HermesRestoreText -Text $Message -Root $Context.Root
    $document = [ordered]@{
        schemaVersion = 1
        taskId = [string]$Context.TaskId
        ownerPid = $PID
        status = $Status
        stage = $Stage
        mode = if ($determinate) { 'determinate' } else { 'indeterminate' }
        completedUnits = $completed
        totalUnits = $total
        percent = $percent
        counters = [ordered]@{
            restoredItems = if ($determinate) { $CompletedUnits } else { 0 }
            totalItems = if ($determinate) { $TotalUnits } else { 0 }
        }
        cancellable = [bool]$Cancellable
        elapsedSeconds = [math]::Round(($now - $Context.StartedAt).TotalSeconds, 3)
        message = $safeMessage
        backup = if ($Context.Backup) {
            [ordered]@{
                id = [string]$Context.Backup.Id
                name = [string]$Context.Backup.Name
                path = [string]$Context.Backup.RelativePath
                sha256 = [string]$Context.Backup.Sha256
                createdAt = [string]$Context.Backup.CreatedAt
            }
        } else {
            $null
        }
        safetySnapshot = if ($Context.SafetySnapshot) {
            [ordered]@{
                id = [string]$Context.SafetySnapshot.Id
                path = [string]$Context.SafetySnapshot.RelativePath
                sha256 = [string]$Context.SafetySnapshot.Sha256
            }
        } else {
            $null
        }
        result = Get-HermesRestoreResultDocument -Context $Context
        failure = if ($FailureMessage) {
            [ordered]@{
                code = if ($FailureCode) { $FailureCode } else { 'restore-failed' }
                message = Protect-HermesRestoreText -Text $FailureMessage -Root $Context.Root
            }
        } else {
            $null
        }
        startedAt = $Context.StartedAt.ToString('o')
        updatedAt = $now.ToString('o')
        completedAt = if ($Status -in @('succeeded', 'failed', 'cancelled')) { $now.ToString('o') } else { $null }
    }

    Write-HermesRestoreAtomicJson -Path $Context.ProgressPath -Document $document
    Add-HermesRestoreLog -Context $Context -Message "$Stage · $safeMessage"
    Write-Host "::hermes-restore-stage::$Stage::$safeMessage"
    $units = if ($determinate) { " $CompletedUnits/$TotalUnits" } else { '' }
    Write-Host "Restore progress: $Stage$units · $safeMessage"

    if ($Status -in @('succeeded', 'failed', 'cancelled')) {
        $Context.TerminalStatus = $Status
    }
    $document
}

function Remove-HermesRestoreCancellationRequest {
    [CmdletBinding()]
    param([Parameter(Mandatory)][object] $Context)

    if (-not (Test-Path -LiteralPath $Context.CancelPath -PathType Leaf)) {
        return
    }
    try {
        $request = Get-Content -Raw -LiteralPath $Context.CancelPath | ConvertFrom-Json
        $requestTaskId = [string](Get-HermesRestoreValue -Record $request -Name taskId -Default '')
        if (-not $requestTaskId -or $requestTaskId -eq $Context.TaskId) {
            Remove-Item -LiteralPath $Context.CancelPath -Force -ErrorAction SilentlyContinue
        }
    } catch {
        Remove-Item -LiteralPath $Context.CancelPath -Force -ErrorAction SilentlyContinue
    }
}

function Test-HermesRestoreCancellationRequested {
    [CmdletBinding()]
    param([Parameter(Mandatory)][object] $Context)

    if (-not $Context.Cancellable -or -not (Test-Path -LiteralPath $Context.CancelPath -PathType Leaf)) {
        return $false
    }
    try {
        $request = Get-Content -Raw -LiteralPath $Context.CancelPath | ConvertFrom-Json
        $requestTaskId = [string](Get-HermesRestoreValue -Record $request -Name taskId -Default '')
        $ownerPid = [int](Get-HermesRestoreValue -Record $request -Name ownerPid -Default 0)
        return $requestTaskId -eq $Context.TaskId -and ($ownerPid -eq 0 -or $ownerPid -eq $PID)
    } catch {
        Add-HermesRestoreLog -Context $Context -Level WARN -Message "Ignoring unreadable restore cancellation request: $($_.Exception.Message)"
        return $false
    }
}

function Assert-HermesRestoreNotCancelled {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][object] $Context,
        [string] $Message = 'Restore cancellation was requested.'
    )

    if (-not (Test-HermesRestoreCancellationRequested -Context $Context)) {
        return
    }
    $null = Write-HermesRestoreProgress `
        -Context $Context `
        -Stage 'complete' `
        -Message 'Cancellation accepted at a safe boundary; the active installation was not replaced.' `
        -Status 'cancelling' `
        -Cancellable $false `
        -Indeterminate
    throw [OperationCanceledException]::new($Message)
}

function Resolve-HermesRestoreBackupPath {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $Root,
        [Parameter(Mandatory)][string] $BackupPath
    )

    $rootFull = [IO.Path]::GetFullPath($Root)
    $backups = [IO.Path]::GetFullPath((Join-Path $rootFull 'backups')).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    $candidate = if ([IO.Path]::IsPathFullyQualified($BackupPath)) {
        [IO.Path]::GetFullPath($BackupPath)
    } else {
        [IO.Path]::GetFullPath((Join-Path $rootFull $BackupPath))
    }
    if (-not $candidate.StartsWith($backups, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Restore archives must be selected from $backups"
    }
    if (-not (Test-Path -LiteralPath $candidate -PathType Leaf)) {
        throw "Backup archive is missing: $candidate"
    }
    if (-not [IO.Path]::GetExtension($candidate).Equals('.zip', [StringComparison]::OrdinalIgnoreCase)) {
        throw "Backup archive must use the .zip format: $candidate"
    }
    $candidate
}

function Test-HermesRestoreArchiveEntryName {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string] $Name)

    if ([string]::IsNullOrWhiteSpace($Name) -or $Name.Length -gt 1024) {
        return $false
    }
    if ($Name.IndexOf([char]0) -ge 0 -or $Name -match '[\x00-\x1f]') {
        return $false
    }
    $normalized = $Name.Replace('\', '/')
    if ($normalized.StartsWith('/') -or $normalized.StartsWith('//') -or $normalized -match '^[A-Za-z]:') {
        return $false
    }
    if ($normalized.Contains(':')) {
        return $false
    }
    $segments = @($normalized.TrimEnd('/').Split('/'))
    if ($segments.Count -eq 0) {
        return $false
    }
    foreach ($segment in $segments) {
        if ([string]::IsNullOrWhiteSpace($segment) -or $segment -in @('.', '..')) {
            return $false
        }
        if ($segment.EndsWith('.') -or $segment.EndsWith(' ')) {
            return $false
        }
        $stem = ($segment -split '\.')[0]
        if ($stem -match '^(?i:CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])$') {
            return $false
        }
    }
    $allowedExact = @('backup-manifest.json', 'VERSION.json')
    if ($normalized.TrimEnd('/') -in $allowedExact) {
        return $true
    }
    foreach ($scope in $script:HermesRestoreScopes) {
        $scopeNormalized = $scope.Replace('\', '/')
        if (
            $normalized.TrimEnd('/').Equals($scopeNormalized, [StringComparison]::OrdinalIgnoreCase) -or
            $normalized.StartsWith($scopeNormalized + '/', [StringComparison]::OrdinalIgnoreCase)
        ) {
            return $true
        }
    }
    $false
}

function Get-HermesRestoreSidecarHash {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string] $ArchivePath)

    $sidecar = "$ArchivePath.sha256"
    if (-not (Test-Path -LiteralPath $sidecar -PathType Leaf)) {
        throw 'Backup archive integrity sidecar is missing.'
    }
    $text = (Get-Content -Raw -LiteralPath $sidecar).Trim()
    $expected = ($text -split '\s+')[0]
    if ($expected -notmatch '^[0-9a-fA-F]{64}$') {
        throw 'Backup archive integrity sidecar is malformed.'
    }
    $expected.ToLowerInvariant()
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
                try {
                    $totalBytes = [math]::Add($totalBytes, [int64]$entry.Length)
                } catch {
                    throw 'Backup archive expanded size is invalid.'
                }
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
        $reader = [IO.StreamReader]::new($manifestEntry.Open(), [Text.UTF8Encoding]::new($false, $true), $true)
        try {
            $manifestText = $reader.ReadToEnd()
        } finally {
            $reader.Dispose()
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
        $compressedBytes = [math]::Max(1, (Get-Item -LiteralPath $resolved).Length)
        if ($totalBytes -gt 1073741824 -and ($totalBytes / $compressedBytes) -gt 1000) {
            throw 'Backup archive expansion ratio is unsafe.'
        }

        $name = [IO.Path]::GetFileName($resolved)
        $relative = ConvertTo-HermesRestoreRelativePath -Root $Root -Path $resolved
        [pscustomobject]@{
            Path = $resolved
            RelativePath = $relative
            Name = $name
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

function Expand-HermesRestoreArchive {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][object] $Context,
        [Parameter(Mandatory)][object] $Plan,
        [Parameter(Mandatory)][string] $Destination
    )

    if (Test-Path -LiteralPath $Destination) {
        throw "Restore staging directory already exists: $Destination"
    }
    [IO.Directory]::CreateDirectory($Destination) | Out-Null
    [IO.Compression.ZipFile]::ExtractToDirectory([string]$Plan.Path, $Destination)
    foreach ($scope in $script:HermesRestoreScopes) {
        $candidate = Join-Path $Destination $scope
        if (Test-Path -LiteralPath $candidate) {
            $full = [IO.Path]::GetFullPath($candidate)
            $prefix = [IO.Path]::GetFullPath($Destination).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
            if (-not $full.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) {
                throw "Extracted restore scope escapes staging: $scope"
            }
        }
    }
}

function Get-HermesRestorePreviousState {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string] $Root)

    $statusPath = Join-Path $Root 'data\runtime\status.json'
    $status = $null
    if (Test-Path -LiteralPath $statusPath -PathType Leaf) {
        try {
            $status = Get-Content -Raw -LiteralPath $statusPath | ConvertFrom-Json -Depth 64
        } catch {
            $status = $null
        }
    }
    $phase = [string](Get-HermesRestoreValue -Record $status -Name phase -Default '')
    $profile = [string](Get-HermesRestoreValue -Record $status -Name profile -Default '')
    [pscustomobject]@{
        WasRunning = $phase -in @('running', 'starting-model', 'benchmark-preparing', 'benchmarking')
        Profile = $profile
        Phase = $phase
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

    $output = @(
        & pwsh.exe `
            -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass `
            -File $ScriptPath @Arguments 2>&1 |
            ForEach-Object { [string]$_ }
    )
    $exitCode = $LASTEXITCODE
    foreach ($line in $output) {
        Add-HermesRestoreLog -Context $Context -Message "$Description: $line"
    }
    if ($exitCode -ne 0) {
        throw "$Description failed with exit code $exitCode. See $($Context.LogPath)"
    }
    [pscustomobject]@{ ExitCode = $exitCode; Output = ($output -join [Environment]::NewLine) }
}

function New-HermesRestoreSafetySnapshot {
    [CmdletBinding()]
    param([Parameter(Mandatory)][object] $Context)

    $name = "pre-restore-$($Context.TaskId)"
    $backupScript = Join-Path $Context.Root 'Backup-Hermes-Local.ps1'
    $null = Invoke-HermesRestoreProcess `
        -Context $Context `
        -ScriptPath $backupScript `
        -Arguments @('-Name', $name, '-NonInteractive') `
        -Description 'Pre-restore safety snapshot'

    $archive = Get-ChildItem -LiteralPath (Join-Path $Context.Root 'backups') -File -Filter "Hermes-Local-*-$name.zip" |
        Sort-Object LastWriteTimeUtc -Descending |
        Select-Object -First 1
    if (-not $archive) {
        throw 'Pre-restore safety snapshot did not produce an archive.'
    }
    $sha = Get-HermesRestoreSidecarHash -ArchivePath $archive.FullName
    [pscustomobject]@{
        Id = $sha.Substring(0, 16)
        Path = $archive.FullName
        RelativePath = ConvertTo-HermesRestoreRelativePath -Root $Context.Root -Path $archive.FullName
        Sha256 = $sha
    }
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
    $journal = [Collections.Generic.List[object]]::new()
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
        $journal.Add([pscustomobject]@{
            Scope = $scope
            HadOriginal = $hadOriginal
            HasReplacement = $hasReplacement
            Target = $target
            Rollback = $rollback
        })
        $null = Write-HermesRestoreProgress `
            -Context $Context `
            -Stage 'data-restoration' `
            -Message "Restored declared data scope $scope." `
            -Status 'running' `
            -Cancellable $false `
            -CompletedUnits $index `
            -TotalUnits $script:HermesRestoreScopes.Count
    }
    $journal
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
    foreach ($entry in @($Journal | Select-Object -Reverse)) {
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

function Test-HermesRestorePromotedState {
    [CmdletBinding()]
    param([Parameter(Mandatory)][object] $Context)

    $manifest = Join-Path $Context.Root 'config'
    if (-not (Test-Path -LiteralPath $manifest -PathType Container)) {
        throw 'Restored configuration directory is missing.'
    }
    if (Get-Command Get-HermesConfiguration -ErrorAction SilentlyContinue) {
        $configuration = Get-HermesConfiguration
        $profile = [string](Get-HermesRestoreValue -Record $configuration -Name selectedProfile -Default '')
        if ([string]::IsNullOrWhiteSpace($profile)) {
            throw 'Restored configuration does not identify a selected profile.'
        }
        return $profile
    }
    [string](Get-HermesRestoreValue -Record $Context.Backup -Name Profile -Default '')
}

function Write-HermesRestoreReport {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][object] $Context,
        [Parameter(Mandatory)][ValidateSet('succeeded', 'failed', 'cancelled')][string] $Status,
        [Parameter(Mandatory)][string] $Message,
        [AllowNull()][string] $FailureCode,
        [AllowNull()][string] $FailureMessage
    )

    $now = (Get-Date).ToUniversalTime()
    $document = [ordered]@{
        schemaVersion = 1
        taskId = [string]$Context.TaskId
        status = $Status
        message = Protect-HermesRestoreText -Text $Message -Root $Context.Root
        backup = if ($Context.Backup) {
            [ordered]@{
                id = [string]$Context.Backup.Id
                name = [string]$Context.Backup.Name
                path = [string]$Context.Backup.RelativePath
                sha256 = [string]$Context.Backup.Sha256
                createdAt = [string]$Context.Backup.CreatedAt
            }
        } else { $null }
        safetySnapshot = if ($Context.SafetySnapshot) {
            [ordered]@{
                id = [string]$Context.SafetySnapshot.Id
                path = [string]$Context.SafetySnapshot.RelativePath
                sha256 = [string]$Context.SafetySnapshot.Sha256
            }
        } else { $null }
        originalInstallationRemainedActive = -not [bool]$Context.RestorePromoted
        restorePromoted = [bool]$Context.RestorePromoted
        activeState = [string]$Context.ActiveState
        rollback = [ordered]@{
            attempted = [bool]$Context.RollbackAttempted
            succeeded = $Context.RollbackSucceeded
            preservedState = if ($Context.FailedStateRoot -and (Test-Path -LiteralPath $Context.FailedStateRoot)) {
                ConvertTo-HermesRestoreRelativePath -Root $Context.Root -Path $Context.FailedStateRoot
            } else { $null }
        }
        result = Get-HermesRestoreResultDocument -Context $Context
        failure = if ($FailureMessage) {
            [ordered]@{
                code = if ($FailureCode) { $FailureCode } else { 'restore-failed' }
                message = Protect-HermesRestoreText -Text $FailureMessage -Root $Context.Root
            }
        } else { $null }
        startedAt = $Context.StartedAt.ToString('o')
        completedAt = $now.ToString('o')
        elapsedSeconds = [math]::Round(($now - $Context.StartedAt).TotalSeconds, 3)
    }
    Write-HermesRestoreAtomicJson -Path $Context.ReportPath -Document $document
    Write-HermesRestoreAtomicJson -Path $Context.LatestReportPath -Document $document
    $document
}

function Complete-HermesRestore {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][object] $Context,
        [Parameter(Mandatory)][ValidateSet('succeeded', 'failed', 'cancelled')][string] $Status,
        [Parameter(Mandatory)][string] $Message,
        [AllowNull()][string] $FailureCode,
        [AllowNull()][string] $FailureMessage
    )

    if ($Context.TerminalStatus) {
        return
    }
    $null = Write-HermesRestoreReport `
        -Context $Context `
        -Status $Status `
        -Message $Message `
        -FailureCode $FailureCode `
        -FailureMessage $FailureMessage
    $null = Write-HermesRestoreProgress `
        -Context $Context `
        -Stage 'complete' `
        -Message $Message `
        -Status $Status `
        -Cancellable $false `
        -FailureCode $FailureCode `
        -FailureMessage $FailureMessage `
        -Indeterminate
}
