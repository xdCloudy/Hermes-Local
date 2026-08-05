function Expand-HermesRuntimeArchive {
    param(
        [Parameter(Mandatory)][string] $Archive,
        [Parameter(Mandatory)][string] $Destination
    )

    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $destinationRoot = [System.IO.Path]::GetFullPath($Destination).TrimEnd('\') + '\'
    [System.IO.Directory]::CreateDirectory($Destination) | Out-Null
    $zip = [System.IO.Compression.ZipFile]::OpenRead($Archive)
    try {
        foreach ($entry in $zip.Entries) {
            $target = [System.IO.Path]::GetFullPath((Join-Path $Destination $entry.FullName))
            if (-not $target.StartsWith($destinationRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
                throw "Archive entry escapes the staging directory: $($entry.FullName)"
            }
            if ([string]::IsNullOrEmpty($entry.Name)) {
                [System.IO.Directory]::CreateDirectory($target) | Out-Null
                continue
            }
            [System.IO.Directory]::CreateDirectory([System.IO.Path]::GetDirectoryName($target)) | Out-Null
            $input = $entry.Open()
            try {
                $output = [System.IO.File]::Open($target, [System.IO.FileMode]::Create, [System.IO.FileAccess]::Write, [System.IO.FileShare]::None)
                try { $input.CopyTo($output) } finally { $output.Dispose() }
            } finally {
                $input.Dispose()
            }
        }
    } finally {
        $zip.Dispose()
    }
}

function Assert-HermesRuntimeProcessesStopped {
    $blocking = @(Get-Process -Name 'llama-server', 'llama-cli', 'llama-bench' -ErrorAction SilentlyContinue)
    if ($blocking.Count -gt 0) {
        $summary = @($blocking | ForEach-Object { "$($_.ProcessName) PID $($_.Id)" }) -join ', '
        throw "The inference runtime cannot be changed while native tools are running: $summary"
    }
}

function Test-HermesRuntimePayload {
    param(
        [Parameter(Mandatory)][string] $Path,
        [switch] $SmokeTest
    )

    $roles = [ordered]@{
        server = 'llama-server.exe'
        cli = 'llama-cli.exe'
        benchmark = 'llama-bench.exe'
    }
    $resolved = [ordered]@{}
    foreach ($entry in $roles.GetEnumerator()) {
        $matches = @(Get-ChildItem -LiteralPath $Path -Recurse -Filter $entry.Value -File)
        if ($matches.Count -ne 1) {
            throw "Expected exactly one $($entry.Value) in the runtime package; found $($matches.Count)."
        }
        $resolved[$entry.Key] = $matches[0].FullName
        if ($SmokeTest) {
            Invoke-HermesProcess -FilePath $matches[0].FullName -ArgumentList @('--version') `
                -WorkingDirectory $matches[0].DirectoryName -LogComponent setup
        }
    }
    return [pscustomobject]$resolved
}

function Get-HermesRuntimeFileInventory {
    param([Parameter(Mandatory)][string] $Path)

    $prefix = [System.IO.Path]::GetFullPath($Path).TrimEnd('\') + '\'
    return @(Get-ChildItem -LiteralPath $Path -Recurse -File |
        Where-Object Name -ne 'runtime-manifest.json' |
        Sort-Object FullName |
        ForEach-Object {
            [ordered]@{
                path = $_.FullName.Substring($prefix.Length).Replace('\', '/')
                sizeBytes = $_.Length
                sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
            }
        })
}

function Write-HermesRuntimeHistory {
    param([Parameter(Mandatory)][System.Collections.IDictionary] $Entry)

    $history = if (Test-Path -LiteralPath $script:HistoryPath -PathType Leaf) {
        try { @(Get-Content -Raw -LiteralPath $script:HistoryPath | ConvertFrom-Json -Depth 64) }
        catch { throw "Runtime history is invalid: $($_.Exception.Message)" }
    } else { @() }
    $updated = @($history) + @([pscustomobject]$Entry)
    Write-HermesAtomicText -Path $script:HistoryPath -Content (($updated | ConvertTo-Json -Depth 64) + [Environment]::NewLine)
}

function Set-HermesResolvedAcceleration {
    param(
        [Parameter(Mandatory)][string] $Requested,
        [Parameter(Mandatory)][ValidateSet('cpu', 'cuda')][string] $Resolved
    )

    $settings = Get-HermesUserSettings
    if (-not $settings.Contains('runtime') -or -not $settings.runtime) {
        $settings.runtime = [ordered]@{}
    }
    $settings.runtime.acceleration = $Resolved
    Save-HermesUserSettings -Settings $settings
    Write-HermesLog -Component setup -Message "Resolved runtime acceleration '$Requested' to '$Resolved'."
}

function Install-HermesLlamaRuntime {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][pscustomobject] $Decision,
        [switch] $Force
    )

    if (-not $Decision.Package) {
        throw "$($Decision.SelectionState): $($Decision.Reason)"
    }
    Assert-HermesRuntimeProcessesStopped
    [System.IO.Directory]::CreateDirectory($script:ManagedRoot) | Out-Null
    $package = $Decision.Package
    if (-not $Force -and (Test-Path -LiteralPath (Join-Path $script:BuildRoot 'runtime-manifest.json') -PathType Leaf)) {
        try {
            $installed = Get-Content -Raw -LiteralPath (Join-Path $script:BuildRoot 'runtime-manifest.json') | ConvertFrom-Json -Depth 64
            if ([string]$installed.packageId -eq [string]$package.id) {
                [void](Test-HermesRuntimePayload -Path $script:BuildRoot)
                Set-HermesResolvedAcceleration -Requested $Decision.RequestedAcceleration -Resolved $Decision.ResolvedAcceleration
                return $installed
            }
        } catch {
            Write-HermesLog -Component setup -Level WARN -Message "Existing managed runtime requires replacement: $($_.Exception.Message)"
        }
    }

    $transactionId = [guid]::NewGuid().ToString('N')
    $stage = Join-Path $script:ManagedRoot "staging\$transactionId"
    $payload = Join-Path $stage 'payload'
    $downloads = Join-Path $stage 'downloads'
    [System.IO.Directory]::CreateDirectory($downloads) | Out-Null
    [System.IO.Directory]::CreateDirectory($payload) | Out-Null
    $resolvedArtifacts = [System.Collections.Generic.List[object]]::new()
    try {
        foreach ($artifactSpec in @($package.artifacts)) {
            $asset = Get-HermesReleaseAsset -Artifact $artifactSpec
            $archive = Join-Path $downloads $asset.Name
            Invoke-HermesProcess -FilePath 'curl.exe' -ArgumentList @(
                '--location', '--fail', '--show-error', '--retry', '8', '--retry-all-errors',
                '--output', $archive, $asset.Url
            ) -LogComponent setup
            $item = Get-Item -LiteralPath $archive
            if ($item.Length -ne $asset.Size) {
                throw "Downloaded asset '$($asset.Name)' has size $($item.Length); expected $($asset.Size)."
            }
            $actualHash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
            if ($actualHash -ne $asset.Sha256) {
                throw "Downloaded asset '$($asset.Name)' failed SHA-256 verification."
            }
            Expand-HermesRuntimeArchive -Archive $archive -Destination $payload
            $resolvedArtifacts.Add([ordered]@{
                repository = [string]$artifactSpec.repository
                tag = [string]$artifactSpec.tag
                asset = $asset.Name
                sizeBytes = $asset.Size
                sha256 = $asset.Sha256
                releaseId = $asset.ReleaseId
                publishedAt = $asset.PublishedAt
            })
        }
        $executables = Test-HermesRuntimePayload -Path $payload -SmokeTest
        $notice = @(
            'Hermes Local managed inference runtime',
            "Package: $($package.id)",
            "Source: $($package.sourceRepository)@$($package.sourceCommit)",
            "Licences: $(@($package.licenses) -join ', ')",
            'The original licence files included by upstream remain in this package.'
        ) -join [Environment]::NewLine
        [System.IO.File]::WriteAllText(
            (Join-Path $payload 'HERMES-RUNTIME-NOTICE.txt'),
            $notice + [Environment]::NewLine,
            [System.Text.UTF8Encoding]::new($false)
        )
        $manifest = [ordered]@{
            schemaVersion = 1
            component = 'llama.cpp'
            packageId = [string]$package.id
            version = [string]$package.version
            distribution = [string]$package.distribution
            sourceRepository = [string]$package.sourceRepository
            sourceCommit = [string]$package.sourceCommit
            platform = [string]$package.platform
            acceleration = [string]$package.acceleration
            buildFlags = @($package.buildFlags)
            cudaArchitectures = @($package.cudaArchitectures)
            compatibility = $package.compatibility
            artifacts = $resolvedArtifacts.ToArray()
            executables = [ordered]@{
                server = [System.IO.Path]::GetRelativePath($payload, $executables.server).Replace('\', '/')
                cli = [System.IO.Path]::GetRelativePath($payload, $executables.cli).Replace('\', '/')
                benchmark = [System.IO.Path]::GetRelativePath($payload, $executables.benchmark).Replace('\', '/')
            }
            files = @(Get-HermesRuntimeFileInventory -Path $payload)
            integrity = [ordered]@{
                state = 'verified'
                verifiedAt = (Get-Date).ToUniversalTime().ToString('o')
                smokeTests = @('llama-server --version', 'llama-cli --version', 'llama-bench --version')
            }
            provenance = [ordered]@{
                provider = 'github-release-assets'
                catalog = 'config/runtime/llama-runtime-catalog.json'
            }
        }
        Write-HermesAtomicText -Path (Join-Path $payload 'runtime-manifest.json') `
            -Content (($manifest | ConvertTo-Json -Depth 64) + [Environment]::NewLine)

        $rollbackRoot = Join-Path $script:ManagedRoot 'rollback'
        [System.IO.Directory]::CreateDirectory($rollbackRoot) | Out-Null
        $previousPath = $null
        if (Test-Path -LiteralPath $script:BuildRoot -PathType Container) {
            $stamp = (Get-Date).ToUniversalTime().ToString('yyyyMMddTHHmmssZ')
            $previousPath = Join-Path $rollbackRoot "$stamp-$transactionId"
            Move-Item -LiteralPath $script:BuildRoot -Destination $previousPath
        }
        try {
            Move-Item -LiteralPath $payload -Destination $script:BuildRoot
        } catch {
            if ($previousPath -and (Test-Path -LiteralPath $previousPath) -and
                -not (Test-Path -LiteralPath $script:BuildRoot)) {
                Move-Item -LiteralPath $previousPath -Destination $script:BuildRoot
            }
            throw
        }
        $state = [ordered]@{
            schemaVersion = 1
            packageId = [string]$package.id
            requestedAcceleration = [string]$Decision.RequestedAcceleration
            resolvedAcceleration = [string]$Decision.ResolvedAcceleration
            selectionState = [string]$Decision.SelectionState
            selectionReason = [string]$Decision.Reason
            activePath = $script:BuildRoot
            previousPath = $previousPath
            installedAt = (Get-Date).ToUniversalTime().ToString('o')
            integrityState = 'verified'
        }
        Write-HermesAtomicText -Path $script:StatePath -Content (($state | ConvertTo-Json -Depth 32) + [Environment]::NewLine)
        $diagnostic = [ordered]@{
            schemaVersion = 1
            selection = $state
            hardware = [ordered]@{
                operatingSystem = [string]$Decision.Hardware.OperatingSystem
                windowsBuild = [string]$Decision.Hardware.Build
                architecture = [string]$Decision.Hardware.Architecture
                cpu = [string]$Decision.Hardware.Cpu
                cpuFeatures = @($Decision.CpuFeatures)
                memoryBytes = [int64]$Decision.Hardware.MemoryBytes
                nvidia = $Decision.Hardware.Nvidia
            }
            package = [ordered]@{
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
                manifestPath = (Join-Path $script:BuildRoot 'runtime-manifest.json')
            }
        }
        Write-HermesAtomicText -Path $script:DiagnosticPath -Content (($diagnostic | ConvertTo-Json -Depth 64) + [Environment]::NewLine)
        Write-HermesRuntimeHistory -Entry ([ordered]@{
            action = 'install'
            packageId = [string]$package.id
            previousPath = $previousPath
            completedAt = (Get-Date).ToUniversalTime().ToString('o')
            integrityState = 'verified'
        })
        Set-HermesResolvedAcceleration -Requested $Decision.RequestedAcceleration -Resolved $Decision.ResolvedAcceleration
        Write-HermesLog -Component setup -Message "Promoted verified runtime package '$($package.id)' atomically."
        return [pscustomobject]$manifest
    } finally {
        if (Test-Path -LiteralPath $stage) {
            Remove-Item -LiteralPath $stage -Recurse -Force
        }
    }
}
