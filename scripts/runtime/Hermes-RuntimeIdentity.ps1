function Get-HermesRuntimeObjectProperty {
    param(
        [AllowNull()][object] $Object,
        [Parameter(Mandatory)][string] $Name,
        [AllowNull()][object] $Default = $null
    )

    if ($null -eq $Object) { return $Default }
    if ($Object -is [System.Collections.IDictionary]) {
        if ($Object.Contains($Name)) { return $Object[$Name] }
        return $Default
    }
    $property = $Object.PSObject.Properties[$Name]
    if ($property) { return $property.Value }
    $Default
}

function Resolve-HermesRuntimeLifecyclePath {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string] $RelativePath)

    if ([string]::IsNullOrWhiteSpace($RelativePath) -or
        [System.IO.Path]::IsPathRooted($RelativePath) -or
        $RelativePath -match '(^|[\\/])\.\.([\\/]|$)' -or
        $RelativePath -match "[`r`n`0]") {
        throw "Runtime lifecycle path is unsafe: '$RelativePath'."
    }

    $normalized = $RelativePath.Replace('/', [System.IO.Path]::DirectorySeparatorChar)
    $resolved = [System.IO.Path]::GetFullPath((Resolve-HermesPath $normalized))
    $root = [System.IO.Path]::GetFullPath((Get-HermesRoot)).TrimEnd('\\', '/')
    $prefix = $root + [System.IO.Path]::DirectorySeparatorChar
    if (-not $resolved.StartsWith($prefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Runtime lifecycle path escapes the Hermes Local root: '$RelativePath'."
    }
    $resolved
}

function Get-HermesRuntimeLifecyclePaths {
    [CmdletBinding()]
    param(
        [pscustomobject] $Catalog,
        [string] $CatalogPath = $script:CatalogPath
    )

    if (-not $Catalog) {
        if (-not (Test-Path -LiteralPath $CatalogPath -PathType Leaf)) {
            throw "Runtime catalog is missing: $CatalogPath"
        }
        try {
            $Catalog = Get-Content -Raw -LiteralPath $CatalogPath | ConvertFrom-Json -Depth 64
        } catch {
            throw "Runtime catalog JSON is invalid: $($_.Exception.Message)"
        }
    }
    $lifecycle = Get-HermesRuntimeObjectProperty -Object $Catalog -Name lifecycle
    if (-not $lifecycle) {
        throw 'Runtime catalog does not declare lifecycle paths.'
    }

    $paths = [ordered]@{
        StagingRoot = Resolve-HermesRuntimeLifecyclePath ([string](Get-HermesRuntimeObjectProperty $lifecycle 'stagingRoot'))
        ActivePath = Resolve-HermesRuntimeLifecyclePath ([string](Get-HermesRuntimeObjectProperty $lifecycle 'activePath'))
        RetainedRoot = Resolve-HermesRuntimeLifecyclePath ([string](Get-HermesRuntimeObjectProperty $lifecycle 'retainedRoot'))
        StatePath = Resolve-HermesRuntimeLifecyclePath ([string](Get-HermesRuntimeObjectProperty $lifecycle 'statePath'))
        HistoryPath = Resolve-HermesRuntimeLifecyclePath ([string](Get-HermesRuntimeObjectProperty $lifecycle 'historyPath'))
        DiagnosticPath = Resolve-HermesRuntimeLifecyclePath ([string](Get-HermesRuntimeObjectProperty $lifecycle 'diagnosticPath'))
    }
    if ($paths.StagingRoot -eq $paths.ActivePath -or
        $paths.RetainedRoot -eq $paths.ActivePath -or
        $paths.StagingRoot -eq $paths.RetainedRoot) {
        throw 'Runtime lifecycle staging, active and retained locations must be distinct.'
    }
    [pscustomobject]$paths
}

function Get-HermesSelectedModelFormat {
    [CmdletBinding()]
    param([pscustomobject] $Configuration = (Get-HermesConfiguration))

    $model = Get-HermesRuntimeObjectProperty -Object $Configuration -Name selectedModel
    if (-not $model) {
        throw 'A selected model is required to resolve an inference runtime package.'
    }
    $filename = [string](Get-HermesRuntimeObjectProperty -Object $model -Name filename)
    $localPath = [string](Get-HermesRuntimeObjectProperty -Object $model -Name localPath)
    $candidate = if ($filename) { $filename } else { $localPath }
    $extension = [System.IO.Path]::GetExtension($candidate).TrimStart('.').ToLowerInvariant()
    if ([string]::IsNullOrWhiteSpace($extension)) {
        throw "The selected model '$([string](Get-HermesRuntimeObjectProperty $model 'displayName' 'selected model'))' does not declare a model format."
    }
    $extension
}

function Get-HermesRuntimeIdentityFingerprint {
    [CmdletBinding()]
    param([Parameter(Mandatory)][System.Collections.IDictionary] $Material)

    $json = $Material | ConvertTo-Json -Depth 64 -Compress
    $bytes = [System.Text.UTF8Encoding]::new($false).GetBytes($json)
    $hash = [System.Security.Cryptography.SHA256]::HashData($bytes)
    'sha256:' + [System.Convert]::ToHexString($hash).ToLowerInvariant()
}

function Get-HermesLlamaRuntimePackageIdentity {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][pscustomobject] $Package,
        [pscustomobject] $Catalog
    )

    if (-not $Catalog) {
        $Catalog = Get-HermesRuntimeCatalog
    }
    $artifacts = @(
        foreach ($artifact in @($Package.artifacts)) {
            [ordered]@{
                repository = [string]$artifact.repository
                tag = [string]$artifact.tag
                asset = [string]$artifact.asset
                expectedSha256 = [string](Get-HermesRuntimeObjectProperty -Object $artifact -Name expectedSha256)
            }
        }
    )
    $material = [ordered]@{
        schemaVersion = 1
        family = [string]$Catalog.component
        packageId = [string]$Package.id
        packageVersion = [string]$Package.version
        distribution = [string]$Package.distribution
        revision = [string]$Package.sourceCommit
        sourceRepository = [string]$Package.sourceRepository
        platform = [string]$Package.platform
        hardwareBackend = [string]$Package.acceleration
        buildFlags = @($Package.buildFlags | ForEach-Object { [string]$_ })
        cudaArchitectures = @($Package.cudaArchitectures | ForEach-Object { [string]$_ })
        modelFormats = @($Package.modelFormats | ForEach-Object { [string]$_ })
        artifacts = $artifacts
        integrity = [ordered]@{
            algorithm = [string]$Package.integrity.algorithm
            requirePublishedDigests = [bool]$Package.integrity.requirePublishedDigests
            recordPayloadInventory = [bool]$Package.integrity.recordPayloadInventory
            smokeTests = @($Package.integrity.smokeTests | ForEach-Object { [string]$_ })
        }
        dependencyInventory = [ordered]@{
            mode = [string]$Package.dependencyInventory.mode
            hashAlgorithm = [string]$Package.dependencyInventory.hashAlgorithm
            includeExtensions = @($Package.dependencyInventory.includeExtensions | ForEach-Object { [string]$_ })
        }
        provenance = [ordered]@{
            provider = [string]$Package.provenance.provider
            repository = [string]$Package.provenance.repository
            tag = [string]$Package.provenance.tag
            sourceCommit = [string]$Package.provenance.sourceCommit
        }
        licenses = @($Package.licenses | ForEach-Object { [string]$_ })
    }
    $fingerprint = Get-HermesRuntimeIdentityFingerprint -Material $material
    [pscustomobject]([ordered]@{
        schemaVersion = 1
        key = "{0}/{1}/{2}/{3}/{4}@{5}" -f (
            $material.family,
            $material.distribution,
            $material.platform,
            $material.hardwareBackend,
            $material.packageId,
            $material.revision
        )
        fingerprint = $fingerprint
        family = $material.family
        packageId = $material.packageId
        packageVersion = $material.packageVersion
        distribution = $material.distribution
        revision = $material.revision
        sourceRepository = $material.sourceRepository
        platform = $material.platform
        hardwareBackend = $material.hardwareBackend
        buildFlags = $material.buildFlags
        cudaArchitectures = $material.cudaArchitectures
        modelFormats = $material.modelFormats
        artifacts = $material.artifacts
        integrity = $material.integrity
        dependencyInventory = $material.dependencyInventory
        provenance = $material.provenance
        licenses = $material.licenses
    })
}

function Get-HermesRuntimeManifestIdentity {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][pscustomobject] $Manifest,
        [pscustomobject] $Catalog = (Get-HermesRuntimeCatalog)
    )

    $recordedIdentity = Get-HermesRuntimeObjectProperty -Object $Manifest -Name identity
    $recordedFingerprint = [string](Get-HermesRuntimeObjectProperty -Object $recordedIdentity -Name fingerprint)
    if ($recordedIdentity -and $recordedFingerprint -match '^sha256:[0-9a-f]{64}$') {
        return $recordedIdentity
    }

    $packageId = [string](Get-HermesRuntimeObjectProperty $Manifest 'packageId')
    $version = [string](Get-HermesRuntimeObjectProperty $Manifest 'version')
    $sourceCommit = [string](Get-HermesRuntimeObjectProperty $Manifest 'sourceCommit')
    $platform = [string](Get-HermesRuntimeObjectProperty $Manifest 'platform')
    $acceleration = [string](Get-HermesRuntimeObjectProperty $Manifest 'acceleration')
    $matches = @($Catalog.packages | Where-Object {
        [string]$_.id -eq $packageId -and
        [string]$_.version -eq $version -and
        [string]$_.sourceCommit -eq $sourceCommit -and
        [string]$_.platform -eq $platform -and
        [string]$_.acceleration -eq $acceleration
    })
    if ($matches.Count -eq 1) {
        return Get-HermesLlamaRuntimePackageIdentity -Package $matches[0] -Catalog $Catalog
    }

    $component = [string](Get-HermesRuntimeObjectProperty $Manifest 'component' 'llama.cpp')
    $formats = Get-HermesRuntimeObjectProperty $Manifest 'modelFormats' @('gguf')
    $legacy = [ordered]@{
        schemaVersion = 1
        family = $component
        packageId = $packageId
        packageVersion = $version
        distribution = [string](Get-HermesRuntimeObjectProperty $Manifest 'distribution')
        revision = $sourceCommit
        sourceRepository = [string](Get-HermesRuntimeObjectProperty $Manifest 'sourceRepository')
        platform = $platform
        hardwareBackend = $acceleration
        buildFlags = @(Get-HermesRuntimeObjectProperty $Manifest 'buildFlags' @() | ForEach-Object { [string]$_ })
        cudaArchitectures = @(Get-HermesRuntimeObjectProperty $Manifest 'cudaArchitectures' @() | ForEach-Object { [string]$_ })
        modelFormats = @($formats | ForEach-Object { [string]$_ })
    }
    $fingerprint = Get-HermesRuntimeIdentityFingerprint -Material $legacy
    [pscustomobject]([ordered]@{
        schemaVersion = 1
        key = "{0}/{1}/{2}/{3}/{4}@{5}" -f (
            $legacy.family, $legacy.distribution, $legacy.platform,
            $legacy.hardwareBackend, $legacy.packageId, $legacy.revision
        )
        fingerprint = $fingerprint
        family = $legacy.family
        packageId = $legacy.packageId
        packageVersion = $legacy.packageVersion
        distribution = $legacy.distribution
        revision = $legacy.revision
        sourceRepository = $legacy.sourceRepository
        platform = $legacy.platform
        hardwareBackend = $legacy.hardwareBackend
        buildFlags = $legacy.buildFlags
        cudaArchitectures = $legacy.cudaArchitectures
        modelFormats = $legacy.modelFormats
    })
}

function Get-HermesInstalledLlamaRuntimeIdentity {
    [CmdletBinding()]
    param([string] $Path = $script:BuildRoot)

    $manifestPath = Join-Path $Path 'runtime-manifest.json'
    if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
        return $null
    }
    $manifest = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json -Depth 64
    Get-HermesRuntimeManifestIdentity -Manifest $manifest
}

function Assert-HermesLlamaRuntimeDecision {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][pscustomobject] $Decision,
        [pscustomobject] $Catalog = (Get-HermesRuntimeCatalog)
    )

    $package = Get-HermesRuntimeObjectProperty -Object $Decision -Name Package
    if (-not $package) {
        throw "$([string](Get-HermesRuntimeObjectProperty $Decision 'SelectionState')): $([string](Get-HermesRuntimeObjectProperty $Decision 'Reason'))"
    }
    $packageId = [string]$package.id
    $matches = @($Catalog.packages | Where-Object { [string]$_.id -eq $packageId })
    if ($matches.Count -ne 1) {
        throw "Runtime decision references package '$packageId', which is not uniquely present in the authoritative catalog."
    }
    $catalogPackage = $matches[0]
    $expectedIdentity = Get-HermesLlamaRuntimePackageIdentity -Package $catalogPackage -Catalog $Catalog
    $decisionIdentity = Get-HermesRuntimeObjectProperty -Object $Decision -Name PackageIdentity
    if (-not $decisionIdentity) {
        $decisionIdentity = Get-HermesLlamaRuntimePackageIdentity -Package $package -Catalog $Catalog
    }
    if ([string]$decisionIdentity.fingerprint -ne [string]$expectedIdentity.fingerprint) {
        throw "Runtime decision identity for '$packageId' does not match the authoritative catalog."
    }

    $modelFormat = [string](Get-HermesRuntimeObjectProperty -Object $Decision -Name ModelFormat)
    if (-not $modelFormat) { $modelFormat = Get-HermesSelectedModelFormat }
    $hardware = Get-HermesRuntimeObjectProperty -Object $Decision -Name Hardware
    $cpuFeatures = @(Get-HermesRuntimeObjectProperty -Object $Decision -Name CpuFeatures -Default @())
    $compatibility = Test-HermesRuntimePackageCompatibility `
        -Package $catalogPackage `
        -Hardware $hardware `
        -CpuFeatures $cpuFeatures `
        -ModelFormat $modelFormat
    if (-not $compatibility.Compatible) {
        throw "Runtime package '$packageId' is incompatible: $($compatibility.Reasons -join '; ')"
    }
    if ([string](Get-HermesRuntimeObjectProperty $Decision 'ResolvedAcceleration') -ne [string]$catalogPackage.acceleration) {
        throw "Runtime decision backend '$([string](Get-HermesRuntimeObjectProperty $Decision 'ResolvedAcceleration'))' does not match package '$packageId'."
    }
    $expectedIdentity
}

function Get-HermesLlamaRuntimeUpdateSnapshot {
    [CmdletBinding()]
    param([Parameter(Mandatory)][pscustomobject] $Decision)

    $package = Get-HermesRuntimeObjectProperty -Object $Decision -Name Package
    $target = if ($package) { Assert-HermesLlamaRuntimeDecision -Decision $Decision } else { $null }
    $installed = Get-HermesInstalledLlamaRuntimeIdentity
    $lifecycle = Get-HermesRuntimeLifecyclePaths
    [ordered]@{
        name = 'llama.cpp managed runtime'
        current = if ($installed) { [string]$installed.key } else { $null }
        candidate = if ($target) { [string]$target.key } else { $null }
        updateAvailable = [bool]($target -and (-not $installed -or [string]$installed.fingerprint -ne [string]$target.fingerprint))
        installedIdentity = $installed
        targetIdentity = $target
        compatibility = [ordered]@{
            state = [string](Get-HermesRuntimeObjectProperty $Decision 'SelectionState')
            reason = [string](Get-HermesRuntimeObjectProperty $Decision 'Reason')
            requestedAcceleration = [string](Get-HermesRuntimeObjectProperty $Decision 'RequestedAcceleration')
            resolvedAcceleration = [string](Get-HermesRuntimeObjectProperty $Decision 'ResolvedAcceleration')
            modelFormat = [string](Get-HermesRuntimeObjectProperty $Decision 'ModelFormat')
        }
        lifecycle = [ordered]@{
            staging = $lifecycle.StagingRoot
            active = $lifecycle.ActivePath
            retained = $lifecycle.RetainedRoot
            state = $lifecycle.StatePath
            history = $lifecycle.HistoryPath
            diagnostics = $lifecycle.DiagnosticPath
        }
    }
}
