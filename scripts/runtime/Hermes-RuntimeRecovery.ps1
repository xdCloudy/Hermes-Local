function Test-HermesRuntimeAtPath {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $Path,
        [switch] $SmokeTest
    )

    $manifestPath = Join-Path $Path 'runtime-manifest.json'
    if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
        return [pscustomobject]@{
            Managed = $false
            Valid = $false
            Reason = 'Managed runtime manifest is absent.'
            Identity = $null
            Manifest = $null
        }
    }
    $manifest = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json -Depth 64
    [void](Test-HermesRuntimePayload -Path $Path -SmokeTest:$SmokeTest)
    $prefix = [System.IO.Path]::GetFullPath($Path).TrimEnd('\') + '\'
    $seen = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
    foreach ($file in @($manifest.files)) {
        $relative = [string]$file.path
        if ([string]::IsNullOrWhiteSpace($relative) -or -not $seen.Add($relative)) {
            throw "Managed runtime inventory contains an invalid or duplicate path: $relative"
        }
        $candidate = [System.IO.Path]::GetFullPath((Join-Path $Path $relative))
        if (-not $candidate.StartsWith($prefix, [System.StringComparison]::OrdinalIgnoreCase) -or
            -not (Test-Path -LiteralPath $candidate -PathType Leaf)) {
            throw "Managed runtime file is missing or unsafe: $relative"
        }
        if ((Get-Item -LiteralPath $candidate).Length -ne [int64]$file.sizeBytes -or
            (Get-FileHash -LiteralPath $candidate -Algorithm SHA256).Hash.ToLowerInvariant() -ne [string]$file.sha256) {
            throw "Managed runtime integrity verification failed: $relative"
        }
    }
    $identity = Get-HermesRuntimeManifestIdentity -Manifest $manifest
    return [pscustomobject]@{
        Managed = $true
        Valid = $true
        PackageId = [string]$manifest.packageId
        Identity = $identity
        Manifest = $manifest
    }
}

function Assert-HermesRetainedRuntimeCompatible {
    [CmdletBinding()]
    param([Parameter(Mandatory)][pscustomobject] $Validation)

    if (-not $Validation.Managed -or -not $Validation.Valid) {
        return $null
    }
    $catalog = Get-HermesRuntimeCatalog
    $packageId = [string]$Validation.PackageId
    $matches = @($catalog.packages | Where-Object { [string]$_.id -eq $packageId })
    if ($matches.Count -ne 1) {
        throw "Retained managed runtime '$packageId' is no longer present in the authoritative catalog."
    }
    $package = $matches[0]
    $expected = Get-HermesLlamaRuntimePackageIdentity -Package $package -Catalog $catalog
    if ([string]$Validation.Identity.fingerprint -ne [string]$expected.fingerprint) {
        throw "Retained runtime identity '$packageId' differs from the authoritative catalog."
    }

    $configuration = Get-HermesConfiguration
    $requested = Get-HermesRequestedAcceleration -Configuration $configuration
    $hardware = Assert-HermesMachine -Acceleration $(if ([string]$package.acceleration -eq 'cuda') { 'cuda' } else { 'auto' })
    $cpuFeatures = @(Get-HermesCpuFeatures)
    $modelFormat = Get-HermesSelectedModelFormat -Configuration $configuration
    $compatibility = Test-HermesRuntimePackageCompatibility `
        -Package $package `
        -Hardware $hardware `
        -CpuFeatures $cpuFeatures `
        -ModelFormat $modelFormat
    if (-not $compatibility.Compatible) {
        throw "Retained runtime '$packageId' is incompatible with the current workstation: $($compatibility.Reasons -join '; ')"
    }
    [pscustomobject]@{
        Identity = $expected
        Hardware = $hardware
        ModelFormat = $modelFormat
        RequestedAcceleration = $requested
        ResolvedAcceleration = [string]$package.acceleration
    }
}

function Restore-HermesLlamaRuntime {
    [CmdletBinding()]
    param()

    Assert-HermesRuntimeProcessesStopped
    if (-not (Test-Path -LiteralPath $script:StatePath -PathType Leaf)) {
        throw 'No managed runtime state exists to roll back.'
    }
    $state = Get-Content -Raw -LiteralPath $script:StatePath | ConvertFrom-Json -Depth 64
    $previousPath = [string]$state.previousPath
    if ([string]::IsNullOrWhiteSpace($previousPath) -or -not (Test-Path -LiteralPath $previousPath -PathType Container)) {
        throw 'No previous runtime is available for rollback.'
    }

    # Verify every recorded file and re-check catalog/hardware/model compatibility
    # before mutating the active runtime location.
    $retained = Test-HermesRuntimeAtPath -Path $previousPath -SmokeTest
    $retainedCompatibility = if ($retained.Managed) {
        Assert-HermesRetainedRuntimeCompatible -Validation $retained
    } else {
        [void](Test-HermesRuntimePayload -Path $previousPath -SmokeTest)
        $null
    }

    [System.IO.Directory]::CreateDirectory($script:RollbackRoot) | Out-Null
    $displaced = Join-Path $script:RollbackRoot ("rollback-{0}-{1}" -f (Get-Date).ToUniversalTime().ToString('yyyyMMddTHHmmssZ'), [guid]::NewGuid().ToString('N'))
    if (Test-Path -LiteralPath $script:BuildRoot -PathType Container) {
        Move-Item -LiteralPath $script:BuildRoot -Destination $displaced
    }
    try {
        Move-Item -LiteralPath $previousPath -Destination $script:BuildRoot
    } catch {
        if (Test-Path -LiteralPath $displaced -PathType Container) {
            Move-Item -LiteralPath $displaced -Destination $script:BuildRoot
        }
        throw
    }

    $manifestPath = Join-Path $script:BuildRoot 'runtime-manifest.json'
    $manifest = if (Test-Path -LiteralPath $manifestPath -PathType Leaf) {
        Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json -Depth 64
    } else { $null }
    $identity = if ($manifest) { Get-HermesRuntimeManifestIdentity -Manifest $manifest } else { $null }
    $requestedAcceleration = if ($retainedCompatibility) {
        [string]$retainedCompatibility.RequestedAcceleration
    } else {
        [string]$state.requestedAcceleration
    }
    $resolvedAcceleration = if ($retainedCompatibility) {
        [string]$retainedCompatibility.ResolvedAcceleration
    } elseif ($manifest) {
        [string]$manifest.acceleration
    } else {
        [string]$state.resolvedAcceleration
    }

    $updated = [ordered]@{
        schemaVersion = 2
        packageId = $(if ($manifest) { [string]$manifest.packageId } else { 'source-build-or-legacy' })
        installedIdentity = $identity
        requestedAcceleration = $requestedAcceleration
        resolvedAcceleration = $resolvedAcceleration
        modelFormat = $(if ($retainedCompatibility) { [string]$retainedCompatibility.ModelFormat } else { $null })
        selectionState = 'Rolled back runtime'
        selectionReason = 'The previous validated runtime was restored explicitly.'
        lifecycle = [ordered]@{
            stagingRoot = $script:StagingRoot
            activePath = $script:BuildRoot
            retainedRoot = $script:RollbackRoot
        }
        activePath = $script:BuildRoot
        previousPath = $(if (Test-Path -LiteralPath $displaced -PathType Container) { $displaced } else { $null })
        installedAt = (Get-Date).ToUniversalTime().ToString('o')
        integrityState = $(if ($manifest) { 'verified' } else { 'legacy-source-build' })
    }
    Write-HermesAtomicText -Path $script:StatePath -Content (($updated | ConvertTo-Json -Depth 64) + [Environment]::NewLine)

    $rollbackDiagnostic = [ordered]@{
        schemaVersion = 2
        selection = $updated
        package = $(if ($manifest) {
            [ordered]@{
                identity = $identity
                id = [string](Get-HermesRuntimeObjectProperty $manifest 'packageId')
                version = [string](Get-HermesRuntimeObjectProperty $manifest 'version')
                distribution = [string](Get-HermesRuntimeObjectProperty $manifest 'distribution')
                sourceRepository = [string](Get-HermesRuntimeObjectProperty $manifest 'sourceRepository')
                sourceCommit = [string](Get-HermesRuntimeObjectProperty $manifest 'sourceCommit')
                buildFlags = @(Get-HermesRuntimeObjectProperty $manifest 'buildFlags' @())
                cudaArchitectures = @(Get-HermesRuntimeObjectProperty $manifest 'cudaArchitectures' @())
                modelFormats = @(Get-HermesRuntimeObjectProperty $manifest 'modelFormats' @('gguf'))
                compatibility = Get-HermesRuntimeObjectProperty $manifest 'compatibility'
                licenses = @(Get-HermesRuntimeObjectProperty $manifest 'licenses' @())
                artifacts = @(Get-HermesRuntimeObjectProperty $manifest 'artifacts' @())
                dependencyInventory = Get-HermesRuntimeObjectProperty $manifest 'dependencyInventory'
                integrity = Get-HermesRuntimeObjectProperty $manifest 'integrity'
                provenance = Get-HermesRuntimeObjectProperty $manifest 'provenance'
                manifestPath = $manifestPath
            }
        } else { $null })
    }
    Write-HermesAtomicText -Path $script:DiagnosticPath -Content (($rollbackDiagnostic | ConvertTo-Json -Depth 64) + [Environment]::NewLine)
    Write-HermesRuntimeHistory -Entry ([ordered]@{
        action = 'rollback'
        packageId = [string]$updated.packageId
        identity = $identity
        previousPath = $updated.previousPath
        completedAt = (Get-Date).ToUniversalTime().ToString('o')
        integrityState = [string]$updated.integrityState
    })
    Set-HermesResolvedAcceleration -Requested $requestedAcceleration -Resolved $resolvedAcceleration
    return [pscustomobject]$updated
}

function Test-HermesManagedLlamaRuntime {
    [CmdletBinding()]
    param([switch] $SmokeTest)

    Test-HermesRuntimeAtPath -Path $script:BuildRoot -SmokeTest:$SmokeTest
}
