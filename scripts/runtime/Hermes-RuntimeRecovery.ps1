function Restore-HermesLlamaRuntime {
    [CmdletBinding()]
    param()

    Assert-HermesRuntimeProcessesStopped
    if (-not (Test-Path -LiteralPath $script:StatePath -PathType Leaf)) {
        throw 'No managed runtime state exists to roll back.'
    }
    $state = Get-Content -Raw -LiteralPath $script:StatePath | ConvertFrom-Json -Depth 32
    $previousPath = [string]$state.previousPath
    if ([string]::IsNullOrWhiteSpace($previousPath) -or -not (Test-Path -LiteralPath $previousPath -PathType Container)) {
        throw 'No previous runtime is available for rollback.'
    }
    [void](Test-HermesRuntimePayload -Path $previousPath -SmokeTest)
    $rollbackRoot = Join-Path $script:ManagedRoot 'rollback'
    $displaced = Join-Path $rollbackRoot ("rollback-{0}-{1}" -f (Get-Date).ToUniversalTime().ToString('yyyyMMddTHHmmssZ'), [guid]::NewGuid().ToString('N'))
    Move-Item -LiteralPath $script:BuildRoot -Destination $displaced
    try {
        Move-Item -LiteralPath $previousPath -Destination $script:BuildRoot
    } catch {
        Move-Item -LiteralPath $displaced -Destination $script:BuildRoot
        throw
    }
    $manifestPath = Join-Path $script:BuildRoot 'runtime-manifest.json'
    $manifest = if (Test-Path -LiteralPath $manifestPath -PathType Leaf) {
        Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json -Depth 64
    } else { $null }
    $updated = [ordered]@{
        schemaVersion = 1
        packageId = $(if ($manifest) { [string]$manifest.packageId } else { 'source-build-or-legacy' })
        requestedAcceleration = [string]$state.requestedAcceleration
        resolvedAcceleration = $(if ($manifest) { [string]$manifest.acceleration } else { [string]$state.resolvedAcceleration })
        selectionState = 'Rolled back runtime'
        selectionReason = 'The previous validated runtime was restored explicitly.'
        activePath = $script:BuildRoot
        previousPath = $displaced
        installedAt = (Get-Date).ToUniversalTime().ToString('o')
        integrityState = $(if ($manifest) { 'verified' } else { 'legacy-source-build' })
    }
    Write-HermesAtomicText -Path $script:StatePath -Content (($updated | ConvertTo-Json -Depth 32) + [Environment]::NewLine)
    $rollbackDiagnostic = [ordered]@{
        schemaVersion = 1
        selection = $updated
        package = $(if ($manifest) {
            [ordered]@{
                id = [string]$manifest.packageId
                version = [string]$manifest.version
                distribution = [string]$manifest.distribution
                sourceRepository = [string]$manifest.sourceRepository
                sourceCommit = [string]$manifest.sourceCommit
                buildFlags = @($manifest.buildFlags)
                cudaArchitectures = @($manifest.cudaArchitectures)
                compatibility = $manifest.compatibility
                artifacts = @($manifest.artifacts)
                integrity = $manifest.integrity
                manifestPath = $manifestPath
            }
        } else { $null })
    }
    Write-HermesAtomicText -Path $script:DiagnosticPath -Content (($rollbackDiagnostic | ConvertTo-Json -Depth 64) + [Environment]::NewLine)
    Write-HermesRuntimeHistory -Entry ([ordered]@{
        action = 'rollback'
        packageId = [string]$updated.packageId
        previousPath = $displaced
        completedAt = (Get-Date).ToUniversalTime().ToString('o')
        integrityState = [string]$updated.integrityState
    })
    Set-HermesResolvedAcceleration -Requested ([string]$updated.requestedAcceleration) -Resolved ([string]$updated.resolvedAcceleration)
    return [pscustomobject]$updated
}

function Test-HermesManagedLlamaRuntime {
    [CmdletBinding()]
    param([switch] $SmokeTest)

    $manifestPath = Join-Path $script:BuildRoot 'runtime-manifest.json'
    if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
        return [pscustomobject]@{ Managed = $false; Valid = $false; Reason = 'Managed runtime manifest is absent.' }
    }
    $manifest = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json -Depth 64
    [void](Test-HermesRuntimePayload -Path $script:BuildRoot -SmokeTest:$SmokeTest)
    foreach ($file in @($manifest.files)) {
        $candidate = [System.IO.Path]::GetFullPath((Join-Path $script:BuildRoot ([string]$file.path)))
        $prefix = [System.IO.Path]::GetFullPath($script:BuildRoot).TrimEnd('\') + '\'
        if (-not $candidate.StartsWith($prefix, [System.StringComparison]::OrdinalIgnoreCase) -or
            -not (Test-Path -LiteralPath $candidate -PathType Leaf)) {
            throw "Managed runtime file is missing or unsafe: $($file.path)"
        }
        if ((Get-Item -LiteralPath $candidate).Length -ne [int64]$file.sizeBytes -or
            (Get-FileHash -LiteralPath $candidate -Algorithm SHA256).Hash.ToLowerInvariant() -ne [string]$file.sha256) {
            throw "Managed runtime integrity verification failed: $($file.path)"
        }
    }
    return [pscustomobject]@{ Managed = $true; Valid = $true; PackageId = [string]$manifest.packageId; Manifest = $manifest }
}
