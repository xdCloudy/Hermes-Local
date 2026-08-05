[CmdletBinding()]
param(
    [Parameter(Mandatory)][string] $SourceUrl,
    [Parameter(Mandatory)][string] $Repository,
    [Parameter(Mandatory)][string] $Revision,
    [Parameter(Mandatory)][string] $ModelId,
    [Parameter(Mandatory)][string] $DisplayName,
    [Parameter(Mandatory)][string] $Alias,
    [Parameter(Mandatory)][string] $Filename,
    [Parameter(Mandatory)][string] $TargetRelativePath,
    [string] $Sha256,
    [Nullable[long]] $SizeBytes,
    [string] $License,
    [string] $AuxiliaryFilesJson = '[]',
    [switch] $ConsentConfirmed,
    [switch] $RequiresConsent,
    [ValidateSet('keep', 'discard')][string] $PartialRetention = 'keep',
    [switch] $NonInteractive
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'scripts\Common-Hermes.psm1') -Force
. (Join-Path $PSScriptRoot 'scripts\model-download\ModelDownload-Common.ps1')

$context = $null
$paused = $false
$exitCode = 1

try {
    Assert-HermesRoot
    Initialize-HermesLayout

    $taskId = Get-HermesModelDownloadTaskId -RequestedTaskId $env:HERMES_LOCAL_TASK_ID
    $auxiliaryFiles = @()
    try {
        $parsedAuxiliary = $AuxiliaryFilesJson | ConvertFrom-Json -Depth 32
        $auxiliaryFiles = @($parsedAuxiliary)
    } catch {
        throw "Auxiliary file metadata is invalid JSON: $($_.Exception.Message)"
    }

    $context = New-HermesModelDownloadContext `
        -TaskId $taskId `
        -SourceUrl $SourceUrl `
        -Repository $Repository `
        -Revision $Revision `
        -ModelId $ModelId `
        -DisplayName $DisplayName `
        -Alias $Alias `
        -Filename $Filename `
        -TargetRelativePath $TargetRelativePath `
        -Sha256 $Sha256 `
        -SizeBytes $SizeBytes `
        -License $License `
        -AuxiliaryFiles $auxiliaryFiles `
        -KeepPartialOnCancel ($PartialRetention -eq 'keep')

    Remove-HermesModelDownloadControl -Context $context
    $null = Write-HermesModelDownloadProgress `
        -Context $context `
        -Stage 'metadata-resolution' `
        -Message 'Resolved source, revision, selected files and managed target identity.' `
        -Status running `
        -BytesCompleted (Get-HermesModelDownloadCompletedBytes -Context $context) `
        -BytesTotal (Get-HermesModelDownloadExpectedTotal -Context $context)

    if ($RequiresConsent -and -not $ConsentConfirmed) {
        throw 'License or access consent is required before this model can be downloaded.'
    }
    if (-not $NonInteractive -and -not $ConsentConfirmed) {
        throw 'Interactive download confirmation is not available through the unattended task backend.'
    }

    $null = Write-HermesModelDownloadProgress `
        -Context $context `
        -Stage 'consent' `
        -Message 'Source access and licence consent requirements are satisfied.' `
        -Status running `
        -BytesCompleted (Get-HermesModelDownloadCompletedBytes -Context $context) `
        -BytesTotal (Get-HermesModelDownloadExpectedTotal -Context $context)

    Enter-HermesModelDownloadLock -Context $context
    Assert-HermesModelDownloadDiskSpace -Context $context
    $null = Write-HermesModelDownloadProgress `
        -Context $context `
        -Stage 'disk-preflight' `
        -Message 'Validated target ownership and available disk capacity.' `
        -Status running `
        -BytesCompleted (Get-HermesModelDownloadCompletedBytes -Context $context) `
        -BytesTotal (Get-HermesModelDownloadExpectedTotal -Context $context)

    $overallTotal = Get-HermesModelDownloadExpectedTotal -Context $context
    $completedBefore = [long]0
    for ($index = 0; $index -lt $context.Files.Count; $index += 1) {
        $file = $context.Files[$index]
        $transferred = Invoke-HermesModelDownloadTransfer `
            -Context $context `
            -File $file `
            -CompletedBefore $completedBefore `
            -OverallTotal $overallTotal
        Test-HermesModelDownloadFile -Context $context -File $file -Index $index
        $completedBefore += $transferred
    }

    $null = Write-HermesModelDownloadProgress `
        -Context $context `
        -Stage 'manifest-generation' `
        -Message 'Generating a verified portable model manifest.' `
        -Status running `
        -BytesCompleted $completedBefore `
        -BytesTotal $overallTotal `
        -Cancellable $true `
        -PauseSupported $false
    $manifest = New-HermesModelDownloadManifest -Context $context

    Assert-HermesModelDownloadNotControlled -Context $context
    Invoke-HermesModelDownloadPromotion -Context $context -Manifest $manifest
    Complete-HermesModelDownload `
        -Context $context `
        -Status succeeded `
        -Message "Downloaded and registered $DisplayName."
    $exitCode = 0
} catch [OperationCanceledException] {
    $action = [string]$_.Exception.Data['HermesModelDownloadAction']
    if ($context -and $action -eq 'pause') {
        $paused = $true
        Complete-HermesModelDownload `
            -Context $context `
            -Status paused `
            -Message 'Download paused safely. Verified target files were not replaced and partial data was retained.'
        $exitCode = 75
    } elseif ($context) {
        if (-not $context.KeepPartialOnCancel) {
            Remove-HermesModelDownloadPartials -Context $context
        }
        Complete-HermesModelDownload `
            -Context $context `
            -Status cancelled `
            -Message $(if ($context.KeepPartialOnCancel) {
                'Download cancelled safely. Partial data was retained for an explicit retry.'
            } else {
                'Download cancelled safely. Partial data was discarded by policy.'
            })
        $exitCode = 130
    }
} catch {
    $failureMessage = Protect-HermesModelDownloadText `
        -Text $_.Exception.Message `
        -Root $(if ($context) { $context.Root } else { $PSScriptRoot })
    if ($context) {
        Add-HermesModelDownloadLog -Context $context -Level ERROR -Message $_.Exception.ToString()
        Complete-HermesModelDownload `
            -Context $context `
            -Status failed `
            -Message 'Model download failed without replacing an existing verified model.' `
            -Failure ([ordered]@{
                code = 'model-download-failed'
                message = $failureMessage
                stage = $context.CurrentStage
            })
    }
    Write-Error $failureMessage -ErrorAction Continue
    $exitCode = 1
} finally {
    if ($context) {
        Exit-HermesModelDownloadLock -Context $context -Paused:$paused
        Remove-HermesModelDownloadControl -Context $context
    }
}

exit $exitCode
