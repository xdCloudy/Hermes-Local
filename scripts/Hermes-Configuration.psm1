Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

Import-Module (Join-Path $PSScriptRoot 'Common-Hermes.psm1')

$script:DefaultsPath = Resolve-HermesPath 'config\defaults\workstation.json'
$script:ProfilesPath = Resolve-HermesPath 'config\profiles\profiles.json'
$script:UserSettingsPath = Resolve-HermesPath 'config\launcher\user-settings.json'
$script:ModelManifestDirectory = Resolve-HermesPath 'models\manifests'

function Read-HermesJsonHashtable {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string] $Path,
        [switch] $Optional
    )

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        if ($Optional) {
            return $null
        }
        throw "Required configuration file is missing: $Path"
    }
    try {
        return Get-Content -Raw -LiteralPath $Path | ConvertFrom-Json -AsHashtable -Depth 64
    } catch {
        throw "Invalid JSON in $Path`: $($_.Exception.Message)"
    }
}

function Copy-HermesValue {
    param([Parameter(Mandatory)] $Value)

    return ($Value | ConvertTo-Json -Depth 64 | ConvertFrom-Json -AsHashtable -Depth 64)
}

function Get-HermesUserSettings {
    [CmdletBinding()]
    param()

    $settings = Read-HermesJsonHashtable -Path $script:UserSettingsPath -Optional
    if (-not $settings) {
        return [ordered]@{ schemaVersion = 1 }
    }
    if ([int]$settings.schemaVersion -ne 1) {
        throw "Unsupported user settings schema version: $($settings.schemaVersion)"
    }
    return $settings
}

function Get-HermesAutoTuning {
    [CmdletBinding()]
    param()

    $logicalProcessors = [Environment]::ProcessorCount
    $vramMiB = 0
    try {
        $hardware = Get-HermesHardwareSnapshot
        if ($hardware.LogicalProcessors) {
            $logicalProcessors = [int]$hardware.LogicalProcessors
        }
        if ($hardware.Nvidia) {
            $vramMiB = [int]$hardware.Nvidia.MemoryMiB
        }
    } catch {
        Write-HermesLog -Component setup -Level WARN -Message "Hardware auto-tuning used fallback values: $($_.Exception.Message)"
    }

    $generationThreads = [math]::Max(1, [math]::Min(8, [math]::Floor($logicalProcessors / 2)))
    $batchThreads = [math]::Max($generationThreads, [math]::Min($logicalProcessors, [math]::Floor($logicalProcessors * 0.75)))
    $vramReserveMiB = if ($vramMiB -gt 0) {
        [math]::Max(512, [math]::Min(4096, [math]::Round($vramMiB * 0.15 / 128) * 128))
    } else {
        1024
    }

    return [ordered]@{
        logicalProcessors = $logicalProcessors
        generationThreads = [int]$generationThreads
        batchThreads = [int]$batchThreads
        vramMiB = $vramMiB
        vramReserveMiB = [int]$vramReserveMiB
    }
}

function Resolve-HermesProfile {
    param(
        [Parameter(Mandatory)]
        [System.Collections.IDictionary] $Profile,
        [Parameter(Mandatory)]
        [System.Collections.IDictionary] $AutoTuning
    )

    $resolved = Copy-HermesValue $Profile
    if ([string]$resolved.threads.generation -eq 'auto') {
        $resolved.threads.generation = $AutoTuning.generationThreads
    }
    if ([string]$resolved.threads.batch -eq 'auto') {
        $resolved.threads.batch = $AutoTuning.batchThreads
    }
    if ([string]$resolved.gpu.vramReserveMiB -eq 'auto') {
        $resolved.gpu.vramReserveMiB = $AutoTuning.vramReserveMiB
    }
    return $resolved
}

function Resolve-HermesModelPath {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string] $Path
    )

    $candidate = [Environment]::ExpandEnvironmentVariables($Path.Trim())
    if ([System.IO.Path]::IsPathFullyQualified($candidate)) {
        return [System.IO.Path]::GetFullPath($candidate)
    }
    return Resolve-HermesPath $candidate
}

function Test-HermesModelRecord {
    param(
        [Parameter(Mandatory)]
        [System.Collections.IDictionary] $Model
    )

    foreach ($required in @('id', 'displayName', 'alias', 'filename', 'localPath')) {
        if (-not $Model.Contains($required) -or [string]::IsNullOrWhiteSpace([string]$Model[$required])) {
            throw "Model registration is missing '$required'."
        }
    }
    if ([string]$Model.id -notmatch '^[a-z0-9][a-z0-9._-]{0,63}$') {
        throw "Invalid model id: $($Model.id)"
    }
    if ([string]$Model.alias -notmatch '^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$') {
        throw "Invalid model alias: $($Model.alias)"
    }
    $extension = [System.IO.Path]::GetExtension([string]$Model.localPath)
    if ($extension -ne '.gguf') {
        throw "Only GGUF model files are supported: $($Model.localPath)"
    }
    if ($Model.Contains('sha256') -and $Model.sha256 -and [string]$Model.sha256 -notmatch '^[a-fA-F0-9]{64}$') {
        throw "Invalid SHA-256 for model '$($Model.id)'."
    }
    if ($Model.Contains('server') -and $Model.server -and $Model.server.Contains('extraArguments')) {
        $blocked = '^(?:-m|--model|--host|--port|--api-key|--api-key-file|--log-file)(?:=|$)'
        foreach ($argument in @($Model.server.extraArguments)) {
            if ([string]$argument -match $blocked) {
                throw "Model '$($Model.id)' uses a reserved llama-server argument: $argument"
            }
        }
    }
}

function Get-HermesModelCatalog {
    [CmdletBinding()]
    param(
        [System.Collections.IDictionary] $UserSettings = (Get-HermesUserSettings)
    )

    $byId = [ordered]@{}
    if (Test-Path -LiteralPath $script:ModelManifestDirectory -PathType Container) {
        foreach ($file in @(Get-ChildItem -LiteralPath $script:ModelManifestDirectory -Filter '*.json' -File | Sort-Object Name)) {
            $model = Read-HermesJsonHashtable -Path $file.FullName
            Test-HermesModelRecord -Model $model
            $byId[[string]$model.id] = $model
        }
    }
    if ($UserSettings.Contains('models')) {
        foreach ($modelValue in @($UserSettings.models)) {
            $model = if ($modelValue -is [System.Collections.IDictionary]) {
                $modelValue
            } else {
                Copy-HermesValue $modelValue
            }
            Test-HermesModelRecord -Model $model
            $byId[[string]$model.id] = $model
        }
    }

    $catalog = [System.Collections.Generic.List[object]]::new()
    foreach ($model in $byId.Values) {
        $copy = Copy-HermesValue $model
        if (-not $copy.Contains('server') -or -not $copy.server) {
            $copy.server = [ordered]@{ jinja = $true; extraArguments = @() }
        }
        if (-not $copy.server.Contains('jinja')) {
            $copy.server.jinja = $true
        }
        if (-not $copy.server.Contains('extraArguments')) {
            $copy.server.extraArguments = @()
        }
        if (-not $copy.server.Contains('chatTemplate')) {
            $copy.server.chatTemplate = $null
        }
        if (-not $copy.Contains('metadata') -or -not $copy.metadata) {
            $copy.metadata = [ordered]@{}
        }
        foreach ($optional in @('source', 'repository', 'revision', 'license', 'sizeBytes', 'sha256')) {
            if (-not $copy.Contains($optional)) {
                $copy[$optional] = $null
            }
        }
        $resolvedPath = Resolve-HermesModelPath ([string]$copy.localPath)
        $copy.resolvedPath = $resolvedPath
        $copy.installed = Test-Path -LiteralPath $resolvedPath -PathType Leaf
        if ($copy.installed) {
            $copy.actualSizeBytes = (Get-Item -LiteralPath $resolvedPath).Length
        }
        $catalog.Add($copy)
    }
    return $catalog.ToArray()
}

function Test-HermesSettings {
    param(
        [Parameter(Mandatory)]
        [System.Collections.IDictionary] $Settings,
        [Parameter(Mandatory)]
        [object[]] $Models,
        [Parameter(Mandatory)]
        [object[]] $Profiles
    )

    $hostName = [string]$Settings.network.host
    if ($hostName -eq 'localhost') {
        $hostName = '127.0.0.1'
        $Settings.network.host = $hostName
    }
    if (-not (Test-HermesLoopbackAddress -Address $hostName)) {
        throw "Hermes Local services must use a loopback host, not '$hostName'."
    }
    foreach ($portName in @('modelPort', 'hermesPort')) {
        $port = [int]$Settings.network[$portName]
        if ($port -lt 1024 -or $port -gt 65535) {
            throw "$portName must be between 1024 and 65535."
        }
    }
    if ([int]$Settings.network.modelPort -eq [int]$Settings.network.hermesPort) {
        throw 'The model and Hermes services must use different ports.'
    }
    if ([string]$Settings.runtime.acceleration -notin @('auto', 'cpu', 'cuda')) {
        throw "Unsupported acceleration mode: $($Settings.runtime.acceleration)"
    }
    if (-not @($Models | Where-Object id -eq $Settings.selectedModelId)) {
        throw "Selected model '$($Settings.selectedModelId)' is not registered."
    }
    if (-not @($Profiles | Where-Object name -eq $Settings.selectedProfile)) {
        throw "Selected profile '$($Settings.selectedProfile)' does not exist."
    }
}

function Get-HermesConfiguration {
    [CmdletBinding()]
    param()

    $defaults = Read-HermesJsonHashtable -Path $script:DefaultsPath
    $user = Get-HermesUserSettings
    $settings = Copy-HermesValue $defaults

    foreach ($name in @('selectedModelId', 'selectedProfile')) {
        if ($user.Contains($name)) {
            $settings[$name] = $user[$name]
        }
    }
    foreach ($sectionName in @('network', 'runtime')) {
        if ($user.Contains($sectionName)) {
            foreach ($entry in $user[$sectionName].GetEnumerator()) {
                $settings[$sectionName][$entry.Key] = $entry.Value
            }
        }
    }

    $profileDocument = Read-HermesJsonHashtable -Path $script:ProfilesPath
    $rawProfiles = if ($user.Contains('profiles') -and @($user.profiles).Count -gt 0) {
        @($user.profiles)
    } else {
        @($profileDocument.profiles)
    }
    $autoTuning = Get-HermesAutoTuning
    $profiles = @($rawProfiles | ForEach-Object {
        $profileValue = if ($_ -is [System.Collections.IDictionary]) { $_ } else { Copy-HermesValue $_ }
        Resolve-HermesProfile -Profile $profileValue -AutoTuning $autoTuning
    })
    $models = @(Get-HermesModelCatalog -UserSettings $user)

    Test-HermesSettings -Settings $settings -Models $models -Profiles $profiles
    $selectedModel = @($models | Where-Object id -eq $settings.selectedModelId)[0]
    $selectedProfile = @($profiles | Where-Object name -eq $settings.selectedProfile)[0]

    $result = [ordered]@{
        schemaVersion = 1
        network = $settings.network
        runtime = $settings.runtime
        selectedModelId = [string]$settings.selectedModelId
        selectedProfile = [string]$settings.selectedProfile
        models = $models
        profiles = $profiles
        selectedModel = $selectedModel
        selectedProfileConfiguration = $selectedProfile
        autoTuning = $autoTuning
        userSettingsPath = $script:UserSettingsPath
    }
    return ($result | ConvertTo-Json -Depth 64 | ConvertFrom-Json -Depth 64)
}

function Save-HermesUserSettings {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [System.Collections.IDictionary] $Settings
    )

    $copy = Copy-HermesValue $Settings
    $copy.schemaVersion = 1
    Write-HermesAtomicText -Path $script:UserSettingsPath -Content (
        ($copy | ConvertTo-Json -Depth 64) + [Environment]::NewLine
    )
}

function Set-HermesSelectedProfile {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string] $Name
    )

    $configuration = Get-HermesConfiguration
    if (-not @($configuration.profiles | Where-Object name -eq $Name)) {
        throw "Profile '$Name' does not exist."
    }
    $user = Get-HermesUserSettings
    $user.selectedProfile = $Name
    Save-HermesUserSettings -Settings $user
}

function Set-HermesSelectedModel {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string] $Id
    )

    $configuration = Get-HermesConfiguration
    if (-not @($configuration.models | Where-Object id -eq $Id)) {
        throw "Model '$Id' is not registered."
    }
    $user = Get-HermesUserSettings
    $user.selectedModelId = $Id
    Save-HermesUserSettings -Settings $user
}

function Get-HermesEffectiveAcceleration {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [pscustomobject] $Configuration
    )

    $requested = [string]$Configuration.runtime.acceleration
    if ($requested -ne 'auto') {
        return $requested
    }
    $nvcc = Get-Command nvcc -ErrorAction SilentlyContinue | Select-Object -First 1
    $nvidiaSmi = Get-Command nvidia-smi -ErrorAction SilentlyContinue | Select-Object -First 1
    return $(if ($nvcc -and $nvidiaSmi) { 'cuda' } else { 'cpu' })
}

function Get-HermesBuildParallelism {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [pscustomobject] $Configuration
    )

    if ([string]$Configuration.runtime.buildParallelism -eq 'auto') {
        return [math]::Max(1, [Environment]::ProcessorCount)
    }
    return [int]$Configuration.runtime.buildParallelism
}

function Get-HermesCudaArchitecture {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [pscustomobject] $Configuration
    )

    if ([string]$Configuration.runtime.cudaArchitecture -ne 'auto') {
        return [string]$Configuration.runtime.cudaArchitecture
    }
    $hardware = Get-HermesHardwareSnapshot
    if (-not $hardware.Nvidia -or -not $hardware.Nvidia.ComputeCapability) {
        throw 'CUDA architecture auto-detection requires nvidia-smi compute-cap output.'
    }
    return ([string]$hardware.Nvidia.ComputeCapability).Replace('.', '')
}

function Test-HermesSelectedModel {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [pscustomobject] $Model,
        [switch] $Hash
    )

    if (-not (Test-Path -LiteralPath $Model.resolvedPath -PathType Leaf)) {
        return $false
    }
    $item = Get-Item -LiteralPath $Model.resolvedPath
    if ($Model.sizeBytes -and $item.Length -ne [int64]$Model.sizeBytes) {
        return $false
    }
    if ($Hash -and $Model.sha256) {
        return (Get-FileHash -LiteralPath $Model.resolvedPath -Algorithm SHA256).Hash.ToLowerInvariant() -eq
            ([string]$Model.sha256).ToLowerInvariant()
    }
    return $true
}

function Sync-HermesRuntimeConfiguration {
    [CmdletBinding()]
    param(
        [pscustomobject] $Configuration = (Get-HermesConfiguration)
    )

    $pythonCandidates = @(
        (Resolve-HermesPath 'runtimes\python\hermes\Scripts\python.exe'),
        (Get-Command python.exe -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty Source),
        (Get-Command python -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty Source)
    ) | Where-Object { $_ -and (Test-Path -LiteralPath $_ -PathType Leaf) }
    $python = $pythonCandidates | Select-Object -First 1
    if (-not $python) {
        throw 'Python is required to merge the Hermes YAML configuration.'
    }

    $modelBaseUrl = "http://$($Configuration.network.host):$($Configuration.network.modelPort)/v1"
    $arguments = @(
        (Resolve-HermesPath 'scripts\configure_hermes.py'),
        '--config', (Resolve-HermesPath 'data\hermes\config.yaml'),
        '--template', (Resolve-HermesPath 'config\templates\hermes.local.yaml'),
        '--provider', 'local-llama',
        '--model', [string]$Configuration.selectedModel.alias,
        '--base-url', $modelBaseUrl,
        '--context', [string]$Configuration.selectedProfileConfiguration.contextTokens,
        '--cwd', (Resolve-HermesPath 'data\user'),
        '--root', (Get-HermesRoot)
    )
    Invoke-HermesProcess -FilePath $python -ArgumentList $arguments -LogComponent setup
}

Export-ModuleMember -Function @(
    'Get-HermesUserSettings',
    'Get-HermesAutoTuning',
    'Resolve-HermesModelPath',
    'Get-HermesModelCatalog',
    'Get-HermesConfiguration',
    'Save-HermesUserSettings',
    'Set-HermesSelectedProfile',
    'Set-HermesSelectedModel',
    'Get-HermesEffectiveAcceleration',
    'Get-HermesBuildParallelism',
    'Get-HermesCudaArchitecture',
    'Test-HermesSelectedModel',
    'Sync-HermesRuntimeConfiguration'
)
