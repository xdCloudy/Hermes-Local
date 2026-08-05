function Invoke-HermesModelDownloadPromotion {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] $Context,
        [Parameter(Mandatory)] $Manifest
    )

    $null = Write-HermesModelDownloadProgress `
        -Context $Context `
        -Stage 'promotion' `
        -Message 'Promoting verified model artifacts atomically.' `
        -Status running `
        -BytesCompleted (Get-HermesModelDownloadCompletedBytes -Context $Context) `
        -BytesTotal (Get-HermesModelDownloadExpectedTotal -Context $Context) `
        -Cancellable $false `
        -PauseSupported $false

    $journal = [System.Collections.Generic.List[object]]::new()
    $manifestBackup = $null
    try {
        foreach ($file in $Context.Files) {
            [System.IO.Directory]::CreateDirectory([System.IO.Path]::GetDirectoryName($file.targetPath)) | Out-Null
            $backup = "$($file.targetPath).previous-$($Context.TaskId)"
            Remove-Item -LiteralPath $backup -Force -ErrorAction SilentlyContinue
            if (Test-Path -LiteralPath $file.targetPath -PathType Leaf) {
                [System.IO.File]::Move($file.targetPath, $backup)
            }
            try {
                [System.IO.File]::Move($file.partialPath, $file.targetPath)
            } catch {
                if ((Test-Path -LiteralPath $backup -PathType Leaf) -and -not (Test-Path -LiteralPath $file.targetPath)) {
                    [System.IO.File]::Move($backup, $file.targetPath)
                }
                throw
            }
            $journal.Add([ordered]@{ target = $file.targetPath; backup = $backup })
        }

        $null = Write-HermesModelDownloadProgress `
            -Context $Context `
            -Stage 'registration' `
            -Message 'Registering the verified model manifest.' `
            -Status running `
            -BytesCompleted (Get-HermesModelDownloadExpectedTotal -Context $Context) `
            -BytesTotal (Get-HermesModelDownloadExpectedTotal -Context $Context) `
            -Cancellable $false `
            -PauseSupported $false

        if (Test-Path -LiteralPath $Context.ManifestPath -PathType Leaf) {
            $manifestBackup = "$($Context.ManifestPath).previous-$($Context.TaskId)"
            Remove-Item -LiteralPath $manifestBackup -Force -ErrorAction SilentlyContinue
            [System.IO.File]::Move($Context.ManifestPath, $manifestBackup)
        }
        Write-HermesModelDownloadJson -Path $Context.ManifestPath -Value $Manifest

        foreach ($entry in $journal) {
            Remove-Item -LiteralPath $entry.backup -Force -ErrorAction SilentlyContinue
        }
        if ($manifestBackup) {
            Remove-Item -LiteralPath $manifestBackup -Force -ErrorAction SilentlyContinue
        }
        $Context.PromotionJournal = $journal.ToArray()
    } catch {
        Remove-Item -LiteralPath $Context.ManifestPath -Force -ErrorAction SilentlyContinue
        if ($manifestBackup -and (Test-Path -LiteralPath $manifestBackup -PathType Leaf)) {
            [System.IO.File]::Move($manifestBackup, $Context.ManifestPath)
        }
        foreach ($entry in @($journal.ToArray() | Sort-Object { $_.target } -Descending)) {
            Remove-Item -LiteralPath $entry.target -Force -ErrorAction SilentlyContinue
            if (Test-Path -LiteralPath $entry.backup -PathType Leaf) {
                [System.IO.File]::Move($entry.backup, $entry.target)
            }
        }
        throw
    }
}

function Remove-HermesModelDownloadPartials {
    [CmdletBinding()]
    param([Parameter(Mandatory)] $Context)

    foreach ($file in $Context.Files) {
        Remove-Item -LiteralPath $file.partialPath -Force -ErrorAction SilentlyContinue
    }
}

function Complete-HermesModelDownload {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] $Context,
        [ValidateSet('paused', 'cancelled', 'failed', 'succeeded')][string] $Status,
        [Parameter(Mandatory)][string] $Message,
        $Failure = $null
    )

    $completedAt = (Get-Date).ToUniversalTime().ToString('o')
    $result = [ordered]@{
        model = $Context.Primary.targetRelativePath.Replace('\', '/')
        manifest = $Context.ManifestRelativePath.Replace('\', '/')
        report = ([System.IO.Path]::GetRelativePath($Context.Root, $Context.ReportPath)).Replace('\', '/')
        log = ([System.IO.Path]::GetRelativePath($Context.Root, $Context.LogPath)).Replace('\', '/')
        source = $Context.Source
    }
    $resumeSupported = $Status -eq 'paused'
    $progress = Write-HermesModelDownloadProgress `
        -Context $Context `
        -Stage $(if ($Status -eq 'succeeded') { 'complete' } else { $Status }) `
        -Message $Message `
        -Status $Status `
        -BytesCompleted (Get-HermesModelDownloadCompletedBytes -Context $Context) `
        -BytesTotal (Get-HermesModelDownloadExpectedTotal -Context $Context) `
        -Cancellable $false `
        -PauseSupported $false `
        -ResumeSupported $resumeSupported `
        -Result $result `
        -Failure $Failure `
        -CompletedAt $(if ($Status -eq 'paused') { $null } else { $completedAt })

    Write-HermesModelDownloadJson -Path $Context.ReportPath -Value ([ordered]@{
        schemaVersion = 1
        taskId = $Context.TaskId
        status = $Status
        startedAt = $Context.StartedAt
        completedAt = $(if ($Status -eq 'paused') { $null } else { $completedAt })
        source = $progress.source
        target = $progress.target
        files = $progress.files
        result = $result
        failure = $Failure
        retention = $progress.retention
    })
}
