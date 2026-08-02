[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidatePattern('^[a-z0-9][a-z0-9._-]{0,63}$')]
    [string] $TargetModelId,
    [ValidatePattern('^[a-z0-9][a-z0-9._-]{0,63}$')]
    [string] $PreviousModelId,
    [ValidatePattern('^[A-Za-z][A-Za-z0-9 ]{0,31}$')]
    [string] $Profile,
    [switch] $NonInteractive
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'scripts\Common-Hermes.psm1') -Force
Import-Module (Join-Path $PSScriptRoot 'scripts\Hermes-Configuration.psm1') -Force

function Write-ModelSwitchStage {
    param(
        [Parameter(Mandatory)]
        [string] $Stage,
        [Parameter(Mandatory)]
        [string] $Message
    )

    Write-Host "::hermes-model-switch-stage::$Stage::$Message"
    Write-HermesLog -Component model-switch -Message $Message
}

function Invoke-HermesStackRestart {
    param([Parameter(Mandatory)][string] $SelectedProfile)

    $pwsh = (Get-Command pwsh.exe -ErrorAction Stop).Source
    Invoke-HermesProcess -FilePath $pwsh -ArgumentList @(
        '-NoLogo', '-NoProfile', '-NonInteractive',
        '-ExecutionPolicy', 'Bypass',
        '-File', (Join-Path $PSScriptRoot 'Restart-Hermes-Local.ps1'),
        '-Profile', $SelectedProfile,
        '-NonInteractive'
    ) -LogComponent model-switch
}

try {
    Assert-HermesRoot
    Initialize-HermesLayout

    $initial = Get-HermesConfiguration
    if (-not $Profile) {
        $Profile = [string]$initial.selectedProfile
    }
    if (-not $PreviousModelId) {
        $PreviousModelId = [string]$initial.selectedModelId
    }
    if ([string]$initial.selectedModelId -ne $PreviousModelId) {
        throw "Model selection changed before the switch acquired lifecycle ownership. Expected '$PreviousModelId', found '$($initial.selectedModelId)'."
    }

    $previousModel = @($initial.models | Where-Object id -eq $PreviousModelId)[0]
    $targetModel = @($initial.models | Where-Object id -eq $TargetModelId)[0]
    if (-not $targetModel) {
        throw "Target model '$TargetModelId' is not registered."
    }
    if ($TargetModelId -eq $PreviousModelId) {
        Write-ModelSwitchStage -Stage complete -Message "Model '$TargetModelId' is already selected."
        exit 0
    }

    Write-ModelSwitchStage -Stage validating-target -Message (
        "Model switch requested: $($previousModel.alias) -> $($targetModel.alias)."
    )
    $verifyHash = [bool]$initial.runtime.verifyModelOnStart
    if (-not (Test-HermesSelectedModel -Model $targetModel -Hash:$verifyHash)) {
        throw "Target model '$($targetModel.displayName)' failed file or integrity validation at '$($targetModel.resolvedPath)'."
    }

    $selectionChanged = $false
    $failedStage = 'persisting-selection'
    try {
        Write-ModelSwitchStage -Stage persisting-selection -Message (
            "Persisting selected model '$($targetModel.id)' before managed restart."
        )
        Set-HermesSelectedModel -Id $TargetModelId
        $selectionChanged = $true

        $failedStage = 'restarting-stack'
        Write-ModelSwitchStage -Stage stopping-services -Message (
            'Stopping dependent Hermes Local services before model replacement.'
        )
        Write-ModelSwitchStage -Stage starting-target -Message (
            "Starting llama-server with selected model '$($targetModel.alias)' and its manifest arguments."
        )
        Invoke-HermesStackRestart -SelectedProfile $Profile

        $failedStage = 'verifying-identity'
        Write-ModelSwitchStage -Stage verifying-model -Message 'Selected model health passed; verifying runtime identity.'
        $active = Get-HermesConfiguration
        [void](Assert-HermesModelIdentity -Configuration $active)
        Write-ModelSwitchStage -Stage verifying-hermes -Message 'Hermes provider health passed.'
        Write-ModelSwitchStage -Stage complete -Message 'Model switch completed and active identities agree.'
        exit 0
    } catch {
        $targetFailure = $_
        if (-not $selectionChanged -or -not $previousModel) {
            throw
        }

        Write-HermesLog -Component model-switch -Level ERROR -Message (
            "Model switch failed during $failedStage; restoring previous model. $($targetFailure.Exception.Message)"
        )
        Write-ModelSwitchStage -Stage rollback-selection -Message (
            "Model switch failed during $failedStage; restoring previous model '$($previousModel.alias)'."
        )
        try {
            Set-HermesSelectedModel -Id $PreviousModelId
            Write-ModelSwitchStage -Stage rollback-restart -Message 'Restarting the previous known-working stack.'
            Invoke-HermesStackRestart -SelectedProfile $Profile
            $restored = Get-HermesConfiguration
            [void](Assert-HermesModelIdentity -Configuration $restored)
            Write-ModelSwitchStage -Stage rollback-complete -Message 'Previous model restored successfully.'
            throw "Model switch failed during $failedStage. Previous model restored successfully. $($targetFailure.Exception.Message)"
        } catch {
            $rollbackFailure = $_
            if ($rollbackFailure.Exception.Message -like 'Model switch failed during*Previous model restored successfully*') {
                throw
            }
            throw (
                "Model switch failed during $failedStage and rollback could not restore service. " +
                "Target failure: $($targetFailure.Exception.Message) Rollback failure: $($rollbackFailure.Exception.Message)"
            )
        }
    }
} catch {
    Write-HermesLog -Component model-switch -Level ERROR -Message $_.Exception.ToString()
    Write-Host "Hermes Local model switch failed: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
}
