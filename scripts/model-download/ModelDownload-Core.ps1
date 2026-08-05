Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Get-HermesModelDownloadValue {
    [CmdletBinding()]
    param(
        $Record,
        [Parameter(Mandatory)][string] $Name,
        $Default = $null
    )

    if ($null -eq $Record) {
        return $Default
    }
    if ($Record -is [System.Collections.IDictionary]) {
        return $(if ($Record.Contains($Name)) { $Record[$Name] } else { $Default })
    }
    $property = $Record.PSObject.Properties[$Name]
    return $(if ($property) { $property.Value } else { $Default })
}

function Get-HermesModelDownloadTaskId {
    [CmdletBinding()]
    param([string] $RequestedTaskId)

    $candidate = [string]$RequestedTaskId
    if ([string]::IsNullOrWhiteSpace($candidate)) {
        $candidate = [guid]::NewGuid().ToString('N')
    }
    if ($candidate -notmatch '^[A-Za-z0-9][A-Za-z0-9._-]{7,127}$') {
        throw 'Model download task identity is invalid.'
    }
    return $candidate
}

function Protect-HermesModelDownloadText {
    [CmdletBinding()]
    param(
        [AllowEmptyString()][string] $Text,
        [string] $Root = (Get-HermesRoot)
    )

    if ($null -eq $Text) {
        return ''
    }

    $safe = Protect-HermesLogText -Text $Text
    $safe = [regex]::Replace($safe, '(?i)https://[^/\s:@]+:[^@/\s]+@', 'https://[REDACTED]@')
    $safe = [regex]::Replace($safe, '(?i)([?&](?:token|access_token|auth|signature|sig|key|api_key)=)[^&#\s]+', '$1[REDACTED]')
    if (-not [string]::IsNullOrWhiteSpace($Root)) {
        $safe = $safe.Replace($Root, '[HERMES_ROOT]', [System.StringComparison]::OrdinalIgnoreCase)
    }
    return $safe
}

function ConvertTo-HermesSafeSourceIdentity {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string] $SourceUrl)

    $uri = [uri]$SourceUrl
    if ($uri.Scheme -ne 'https') {
        throw 'Model downloads require HTTPS sources.'
    }
    if (-not [string]::IsNullOrWhiteSpace($uri.UserInfo)) {
        throw 'Credentials must not be embedded in a model source URL.'
    }
    if (-not [string]::IsNullOrWhiteSpace($uri.Query) -or -not [string]::IsNullOrWhiteSpace($uri.Fragment)) {
        throw 'Signed or query-bearing model URLs are not accepted because task state must remain safe to persist.'
    }
    if ([string]::IsNullOrWhiteSpace($uri.Host)) {
        throw 'Model source URL is missing a host.'
    }

    $builder = [UriBuilder]::new($uri)
    $builder.Query = ''
    $builder.Fragment = ''
    return [ordered]@{
        scheme = $builder.Scheme
        host = $builder.Host.ToLowerInvariant()
        port = $(if ($builder.Uri.IsDefaultPort) { $null } else { $builder.Port })
        path = $builder.Path
        url = $builder.Uri.AbsoluteUri
    }
}

function Resolve-HermesModelDownloadPath {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $RelativePath,
        [ValidateSet('model', 'manifest', 'runtime', 'log')][string] $Kind = 'model'
    )

    $normalized = $RelativePath.Replace('/', '\').TrimStart('\')
    if ([string]::IsNullOrWhiteSpace($normalized) -or [System.IO.Path]::IsPathFullyQualified($normalized)) {
        throw "Model download $Kind path must be relative to the Hermes Local root."
    }
    if ($normalized -match '(^|\\)\.\.(\\|$)') {
        throw "Model download $Kind path contains traversal segments."
    }

    $absolute = Resolve-HermesPath $normalized
    $requiredRoot = switch ($Kind) {
        'model' { Resolve-HermesPath 'models' }
        'manifest' { Resolve-HermesPath 'models\manifests' }
        'runtime' { Resolve-HermesPath 'data\runtime' }
        'log' { Resolve-HermesPath 'logs\model-downloads' }
    }
    $prefix = $requiredRoot.TrimEnd('\') + '\'
    if ($absolute -ne $requiredRoot -and
        -not $absolute.StartsWith($prefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Model download $Kind path escapes its managed directory."
    }
    return $absolute
}

function Write-HermesModelDownloadJson {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $Path,
        [Parameter(Mandatory)] $Value
    )

    Write-HermesAtomicText -Path $Path -Content (($Value | ConvertTo-Json -Depth 64) + [Environment]::NewLine)
}

function New-HermesModelDownloadContext {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $TaskId,
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
        [object[]] $AuxiliaryFiles,
        [bool] $KeepPartialOnCancel
    )

    $root = Get-HermesRoot
    $runtimeDirectory = Resolve-HermesPath 'data\runtime\model-downloads'
    $controlDirectory = Resolve-HermesPath 'data\runtime\model-download-controls'
    $logDirectory = Resolve-HermesPath 'logs\model-downloads'
    $lockDirectory = Resolve-HermesPath 'data\runtime\model-download-locks'
    foreach ($directory in @($runtimeDirectory, $controlDirectory, $logDirectory, $lockDirectory)) {
        [System.IO.Directory]::CreateDirectory($directory) | Out-Null
    }

    $source = ConvertTo-HermesSafeSourceIdentity -SourceUrl $SourceUrl
    $targetPath = Resolve-HermesModelDownloadPath -RelativePath $TargetRelativePath -Kind model
    if ([System.IO.Path]::GetExtension($targetPath) -ne '.gguf') {
        throw 'Primary model download target must be a GGUF file.'
    }
    if ([System.IO.Path]::GetFileName($targetPath) -ne $Filename) {
        throw 'Model filename and target path do not identify the same file.'
    }
    if ($ModelId -notmatch '^[a-z0-9][a-z0-9._-]{0,63}$') {
        throw 'Model id is invalid.'
    }
    if ($Alias -notmatch '^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$') {
        throw 'Model alias is invalid.'
    }
    if ($Sha256 -and $Sha256 -notmatch '^[0-9a-fA-F]{64}$') {
        throw 'Primary model SHA-256 is invalid.'
    }
    if ($null -ne $SizeBytes -and $SizeBytes -le 0) {
        throw 'Primary model size must be positive when supplied.'
    }

    $files = [System.Collections.Generic.List[object]]::new()
    $files.Add([ordered]@{
        kind = 'model'
        filename = $Filename
        source = $source
        targetRelativePath = $TargetRelativePath.Replace('/', '\')
        targetPath = $targetPath
        partialPath = "$targetPath.partial"
        expectedSha256 = $(if ($Sha256) { $Sha256.ToLowerInvariant() } else { $null })
        expectedSizeBytes = $(if ($null -ne $SizeBytes) { [long]$SizeBytes } else { $null })
    })

    foreach ($raw in @($AuxiliaryFiles)) {
        $kind = [string](Get-HermesModelDownloadValue -Record $raw -Name kind -Default 'auxiliary')
        $auxFilename = [string](Get-HermesModelDownloadValue -Record $raw -Name filename -Default '')
        $auxSourceUrl = [string](Get-HermesModelDownloadValue -Record $raw -Name sourceUrl -Default '')
        $auxTargetRelativePath = [string](Get-HermesModelDownloadValue -Record $raw -Name targetRelativePath -Default '')
        $auxSha256 = [string](Get-HermesModelDownloadValue -Record $raw -Name sha256 -Default '')
        $auxSize = Get-HermesModelDownloadValue -Record $raw -Name sizeBytes -Default $null
        if ([string]::IsNullOrWhiteSpace($auxFilename) -or [string]::IsNullOrWhiteSpace($auxSourceUrl) -or
            [string]::IsNullOrWhiteSpace($auxTargetRelativePath)) {
            throw 'Auxiliary model file entries require filename, sourceUrl and targetRelativePath.'
        }
        if ($auxSha256 -and $auxSha256 -notmatch '^[0-9a-fA-F]{64}$') {
            throw "Auxiliary file '$auxFilename' has an invalid SHA-256."
        }
        $auxTargetPath = Resolve-HermesModelDownloadPath -RelativePath $auxTargetRelativePath -Kind model
        if ([System.IO.Path]::GetFileName($auxTargetPath) -ne $auxFilename) {
            throw "Auxiliary filename '$auxFilename' does not match its target path."
        }
        $files.Add([ordered]@{
            kind = $(if ([string]::IsNullOrWhiteSpace($kind)) { 'auxiliary' } else { $kind })
            filename = $auxFilename
            source = ConvertTo-HermesSafeSourceIdentity -SourceUrl $auxSourceUrl
            targetRelativePath = $auxTargetRelativePath.Replace('/', '\')
            targetPath = $auxTargetPath
            partialPath = "$auxTargetPath.partial"
            expectedSha256 = $(if ($auxSha256) { $auxSha256.ToLowerInvariant() } else { $null })
            expectedSizeBytes = $(if ($null -ne $auxSize -and [long]$auxSize -gt 0) { [long]$auxSize } else { $null })
        })
    }

    $targetIdentity = [Convert]::ToHexString(
        [System.Security.Cryptography.SHA256]::HashData(
            [System.Text.Encoding]::UTF8.GetBytes($targetPath.ToLowerInvariant())
        )
    ).ToLowerInvariant().Substring(0, 24)

    return [pscustomobject]@{
        Root = $root
        TaskId = $TaskId
        Repository = $Repository
        Revision = $Revision
        ModelId = $ModelId
        DisplayName = $DisplayName
        Alias = $Alias
        Filename = $Filename
        License = $License
        Source = $source
        Files = $files.ToArray()
        Primary = $files[0]
        TargetIdentity = $targetIdentity
        ManifestRelativePath = "models\manifests\$ModelId.json"
        ManifestPath = Resolve-HermesModelDownloadPath -RelativePath "models\manifests\$ModelId.json" -Kind manifest
        ProgressPath = Join-Path $runtimeDirectory "$TaskId.json"
        ControlPath = Join-Path $controlDirectory "$TaskId.json"
        ReportPath = Join-Path $logDirectory "$TaskId.json"
        LogPath = Join-Path $logDirectory "$TaskId.log"
        LockPath = Join-Path $lockDirectory "$targetIdentity.json"
        KeepPartialOnCancel = $KeepPartialOnCancel
        StartedAt = (Get-Date).ToUniversalTime().ToString('o')
        CurrentStage = 'metadata-resolution'
        Progress = $null
        LockOwned = $false
        LockStream = $null
        PromotionJournal = @()
    }
}
