function ConvertTo-HermesComparableVersion {
    param([AllowNull()][string] $Value)

    if ([string]::IsNullOrWhiteSpace($Value)) {
        return [version]'0.0'
    }
    $match = [regex]::Match($Value, '^\s*(\d+)(?:\.(\d+))?(?:\.(\d+))?(?:\.(\d+))?')
    if (-not $match.Success) {
        throw "Invalid version value: $Value"
    }
    $parts = 1..4 | ForEach-Object {
        $group = $match.Groups[$_]
        if ($group.Success) { [int]$group.Value } else { 0 }
    }
    return [version]::new($parts[0], $parts[1], $parts[2], $parts[3])
}

function Get-HermesCpuFeatures {
    [CmdletBinding()]
    param()

    $features = [System.Collections.Generic.List[string]]::new()
    if ([System.Runtime.Intrinsics.X86.Sse42]::IsSupported) { $features.Add('sse4.2') }
    if ([System.Runtime.Intrinsics.X86.Avx]::IsSupported) { $features.Add('avx') }
    if ([System.Runtime.Intrinsics.X86.Avx2]::IsSupported) { $features.Add('avx2') }
    if ([System.Runtime.Intrinsics.X86.Fma]::IsSupported) { $features.Add('fma') }
    if ([System.Runtime.Intrinsics.X86.Bmi2]::IsSupported) { $features.Add('bmi2') }
    return $features.ToArray()
}

function Get-HermesRuntimeCatalog {
    [CmdletBinding()]
    param([string] $Path = $script:CatalogPath)

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "Runtime catalog is missing: $Path"
    }
    try {
        $catalog = Get-Content -Raw -LiteralPath $Path | ConvertFrom-Json -Depth 64
    } catch {
        throw "Runtime catalog JSON is invalid: $($_.Exception.Message)"
    }
    if ([int]$catalog.schemaVersion -ne 1 -or [string]$catalog.component -ne 'llama.cpp') {
        throw "Unsupported runtime catalog: $Path"
    }
    $ids = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
    foreach ($package in @($catalog.packages)) {
        if ([string]$package.id -notmatch '^[a-z0-9][a-z0-9._-]{0,95}$') {
            throw "Invalid runtime package id: $($package.id)"
        }
        if (-not $ids.Add([string]$package.id)) {
            throw "Duplicate runtime package id: $($package.id)"
        }
        if ([string]$package.platform -ne 'windows-x64' -or
            [string]$package.acceleration -notin @('cpu', 'cuda')) {
            throw "Unsupported runtime package platform/backend: $($package.id)"
        }
        if ([string]$package.sourceCommit -notmatch '^[0-9a-f]{40}$') {
            throw "Runtime package '$($package.id)' has an invalid source commit."
        }
        if (@($package.artifacts).Count -lt 1) {
            throw "Runtime package '$($package.id)' has no artifacts."
        }
        foreach ($artifact in @($package.artifacts)) {
            if ([string]$artifact.asset -notmatch '^[A-Za-z0-9][A-Za-z0-9._+-]{0,159}\.zip$') {
                throw "Runtime package '$($package.id)' has an unsafe asset name."
            }
            if ([string]$artifact.repository -notmatch '^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$' -or
                [string]$artifact.tag -notmatch '^[A-Za-z0-9][A-Za-z0-9._-]{0,79}$') {
                throw "Runtime package '$($package.id)' has an invalid release identity."
            }
            $expectedHashProperty = $artifact.PSObject.Properties['expectedSha256']
            if ($expectedHashProperty -and [string]$expectedHashProperty.Value -notmatch '^[0-9a-f]{64}$') {
                throw "Runtime package '$($package.id)' has an invalid expected SHA-256."
            }
        }
    }
    return $catalog
}

function Test-HermesRuntimePackageCompatibility {
    param(
        [Parameter(Mandatory)][pscustomobject] $Package,
        [Parameter(Mandatory)][pscustomobject] $Hardware,
        [Parameter(Mandatory)][string[]] $CpuFeatures
    )

    $reasons = [System.Collections.Generic.List[string]]::new()
    $minimumBuild = [int]$Package.compatibility.minimumWindowsBuild
    if ([int]$Hardware.Build -lt $minimumBuild) {
        $reasons.Add("Windows build $($Hardware.Build) is below $minimumBuild")
    }
    foreach ($feature in @($Package.compatibility.cpuFeatures)) {
        if ($CpuFeatures -notcontains [string]$feature) {
            $reasons.Add("CPU feature '$feature' is unavailable")
        }
    }
    if ([string]$Package.acceleration -eq 'cuda') {
        if (-not $Hardware.Nvidia) {
            $reasons.Add('NVIDIA hardware was not detected')
        } else {
            $compute = [decimal]([string]$Hardware.Nvidia.ComputeCapability)
            if ($Package.compatibility.minimumComputeCapability -and
                $compute -lt [decimal]([string]$Package.compatibility.minimumComputeCapability)) {
                $reasons.Add("compute capability $compute is below $($Package.compatibility.minimumComputeCapability)")
            }
            if ($Package.compatibility.maximumComputeCapability -and
                $compute -gt [decimal]([string]$Package.compatibility.maximumComputeCapability)) {
                $reasons.Add("compute capability $compute exceeds $($Package.compatibility.maximumComputeCapability)")
            }
            if ($Package.compatibility.minimumDriver) {
                $driver = ConvertTo-HermesComparableVersion ([string]$Hardware.Nvidia.DriverVersion)
                $minimumDriver = ConvertTo-HermesComparableVersion ([string]$Package.compatibility.minimumDriver)
                if ($driver -lt $minimumDriver) {
                    $reasons.Add("NVIDIA driver $driver is below $minimumDriver")
                }
            }
        }
    }
    return [pscustomobject]@{
        Compatible = $reasons.Count -eq 0
        Reasons = $reasons.ToArray()
    }
}

function Get-HermesRequestedAcceleration {
    [CmdletBinding()]
    param([Parameter(Mandatory)][pscustomobject] $Configuration)

    $requested = [string]$Configuration.runtime.acceleration
    if ($requested -ne 'auto' -and (Test-Path -LiteralPath $script:StatePath -PathType Leaf)) {
        try {
            $state = Get-Content -Raw -LiteralPath $script:StatePath | ConvertFrom-Json -Depth 32
            if ([string]$state.requestedAcceleration -eq 'auto' -and
                [string]$state.resolvedAcceleration -eq $requested) {
                return 'auto'
            }
        } catch {
            Write-HermesLog -Component setup -Level WARN -Message "Ignoring invalid runtime state while resolving acceleration: $($_.Exception.Message)"
        }
    }
    return $requested
}

function Resolve-HermesLlamaRuntimePackage {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][pscustomobject] $Configuration,
        [Parameter(Mandatory)][pscustomobject] $Hardware,
        [pscustomobject] $Catalog = (Get-HermesRuntimeCatalog)
    )

    $requested = Get-HermesRequestedAcceleration -Configuration $Configuration
    $preferred = if ($requested -eq 'auto') {
        if ($Hardware.Nvidia) { 'cuda' } else { 'cpu' }
    } else {
        $requested
    }
    $cpuFeatures = @(Get-HermesCpuFeatures)
    $evaluations = @($Catalog.packages | ForEach-Object {
        $compatibility = Test-HermesRuntimePackageCompatibility -Package $_ -Hardware $Hardware -CpuFeatures $cpuFeatures
        [pscustomobject]@{
            Package = $_
            Compatible = $compatibility.Compatible
            Reasons = $compatibility.Reasons
        }
    })
    $selected = @($evaluations |
        Where-Object { $_.Compatible -and [string]$_.Package.acceleration -eq $preferred } |
        Sort-Object { [int]$_.Package.priority } -Descending |
        Select-Object -First 1)
    $state = 'Recommended prebuilt runtime'
    $reason = "Selected the highest-priority compatible $preferred package."

    if ($selected.Count -eq 0 -and $requested -eq 'auto' -and $preferred -eq 'cuda') {
        $selected = @($evaluations |
            Where-Object { $_.Compatible -and [string]$_.Package.acceleration -eq 'cpu' } |
            Sort-Object { [int]$_.Package.priority } -Descending |
            Select-Object -First 1)
        if ($selected.Count -gt 0) {
            $state = 'CPU fallback available'
            $reason = 'No compatible CUDA package matched; selected the verified CPU fallback.'
        }
    }
    if ($selected.Count -eq 0) {
        $details = @($evaluations | ForEach-Object {
            "$($_.Package.id): $($_.Reasons -join '; ')"
        }) -join ' | '
        return [pscustomobject]@{
            SelectionState = $(if ($requested -eq 'auto') { 'Source build required' } else { 'Unsupported configuration' })
            Reason = "No compatible prebuilt runtime matched. $details"
            RequestedAcceleration = $requested
            ResolvedAcceleration = $null
            Package = $null
            Hardware = $Hardware
            CpuFeatures = $cpuFeatures
        }
    }
    return [pscustomobject]@{
        SelectionState = $state
        Reason = $reason
        RequestedAcceleration = $requested
        ResolvedAcceleration = [string]$selected[0].Package.acceleration
        Package = $selected[0].Package
        Hardware = $Hardware
        CpuFeatures = $cpuFeatures
    }
}

function Get-HermesReleaseAsset {
    param([Parameter(Mandatory)][pscustomobject] $Artifact)

    $parts = ([string]$Artifact.repository).Split('/')
    $uri = "https://api.github.com/repos/$($parts[0])/$($parts[1])/releases/tags/$($Artifact.tag)"
    $headers = @{ Accept = 'application/vnd.github+json'; 'User-Agent' = 'Hermes-Local-RuntimeManager' }
    $release = Invoke-RestMethod -Uri $uri -Headers $headers -Method Get -TimeoutSec 60
    $matches = @($release.assets | Where-Object name -eq [string]$Artifact.asset)
    if ($matches.Count -ne 1) {
        throw "Release asset '$($Artifact.asset)' was not found exactly once in $($Artifact.repository) tag $($Artifact.tag)."
    }
    $asset = $matches[0]
    $digest = [string]$asset.digest
    if ($digest -notmatch '^sha256:([0-9a-f]{64})$') {
        throw "Release asset '$($Artifact.asset)' does not publish a SHA-256 digest."
    }
    $sha256 = $Matches[1]
    $expectedHashProperty = $Artifact.PSObject.Properties['expectedSha256']
    if ($expectedHashProperty -and $sha256 -ne [string]$expectedHashProperty.Value) {
        throw "Published digest for '$($Artifact.asset)' does not match the pinned catalog digest."
    }
    if ([string]$asset.browser_download_url -notmatch '^https://github\.com/[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+/releases/download/[A-Za-z0-9._-]+/[A-Za-z0-9._+-]+\.zip$') {
        throw "Release asset '$($Artifact.asset)' returned an unsafe download URL."
    }
    return [pscustomobject]@{
        Name = [string]$asset.name
        Url = [string]$asset.browser_download_url
        Size = [int64]$asset.size
        Sha256 = $sha256
        ReleaseId = [int64]$release.id
        PublishedAt = [string]$asset.updated_at
    }
}
