function Get-HermesTargetPythonMinorVersion {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string] $ManifestPath
    )

    if (-not (Test-Path -LiteralPath $ManifestPath -PathType Leaf)) {
        throw "Hermes version manifest is missing: $ManifestPath"
    }

    $manifest = Get-Content -Raw -LiteralPath $ManifestPath | ConvertFrom-Json -Depth 32
    $declaredVersion = [string]$manifest.runtime.python
    if ($declaredVersion -notmatch '^(?<minor>\d+\.\d+)(?:\.|$)') {
        throw "Hermes version manifest contains an invalid runtime.python value: '$declaredVersion'."
    }

    return [string]$Matches.minor
}

function Get-HermesInstalledPythonMinorVersion {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string] $PythonExecutable
    )

    if (-not (Test-Path -LiteralPath $PythonExecutable -PathType Leaf)) {
        return $null
    }

    try {
        $output = (@(
            & $PythonExecutable -c 'import sys; print(f"{sys.version_info.major}.{sys.version_info.minor}")' 2>$null
        ) -join [Environment]::NewLine).Trim()
        if ($LASTEXITCODE -ne 0 -or $output -notmatch '^\d+\.\d+$') {
            return $null
        }
        return $output
    } catch {
        return $null
    }
}

function New-HermesPythonRollbackPath {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string] $Runtime,

        [AllowNull()]
        [AllowEmptyString()]
        [string] $RuntimeVersion,

        [datetime] $Timestamp = (Get-Date).ToUniversalTime()
    )

    $parent = [System.IO.Path]::GetDirectoryName([System.IO.Path]::GetFullPath($Runtime))
    $versionLabel = if ([string]::IsNullOrWhiteSpace($RuntimeVersion)) {
        'unknown'
    } else {
        $RuntimeVersion -replace '[^0-9]', ''
    }
    if ([string]::IsNullOrWhiteSpace($versionLabel)) {
        $versionLabel = 'unknown'
    }

    $stamp = $Timestamp.ToUniversalTime().ToString('yyyyMMdd-HHmmss')
    $basePath = Join-Path $parent "hermes-python$versionLabel-$stamp"
    $candidate = $basePath
    $suffix = 2
    while (Test-Path -LiteralPath $candidate) {
        $candidate = "$basePath-$suffix"
        $suffix++
    }

    return $candidate
}

function Move-HermesPythonRuntimeToRollback {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string] $Runtime,

        [AllowNull()]
        [AllowEmptyString()]
        [string] $RuntimeVersion,

        [datetime] $Timestamp = (Get-Date).ToUniversalTime()
    )

    if (-not (Test-Path -LiteralPath $Runtime -PathType Container)) {
        return $null
    }

    $rollbackPath = New-HermesPythonRollbackPath `
        -Runtime $Runtime `
        -RuntimeVersion $RuntimeVersion `
        -Timestamp $Timestamp

    if (Get-Command Write-HermesLog -ErrorAction SilentlyContinue) {
        Write-HermesLog -Component setup -Level WARN -Message (
            "Preserving incompatible Hermes Python runtime as rollback copy: $rollbackPath"
        )
    }

    try {
        Move-Item -LiteralPath $Runtime -Destination $rollbackPath -ErrorAction Stop
    } catch {
        throw (
            "Unable to preserve the existing Hermes Python runtime at '$Runtime'. " +
            "Stop Hermes Local with '.\Stop-Hermes-Local.ps1 -NonInteractive' and retry. " +
            "Original error: $($_.Exception.Message)"
        )
    }

    return $rollbackPath
}

function Invoke-HermesPythonRuntimeMigration {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string] $Runtime,

        [Parameter(Mandatory)]
        [string] $ManifestPath
    )

    if (-not (Test-Path -LiteralPath $Runtime -PathType Container)) {
        return $null
    }

    $targetVersion = Get-HermesTargetPythonMinorVersion -ManifestPath $ManifestPath
    $pythonExecutable = Join-Path $Runtime 'Scripts\python.exe'
    $runtimeVersion = Get-HermesInstalledPythonMinorVersion -PythonExecutable $pythonExecutable

    if ($runtimeVersion -eq $targetVersion) {
        return $null
    }

    $description = if ([string]::IsNullOrWhiteSpace($runtimeVersion)) {
        'an incomplete or unreadable Python environment'
    } else {
        "Python $runtimeVersion"
    }
    if (Get-Command Write-HermesLog -ErrorAction SilentlyContinue) {
        Write-HermesLog -Component setup -Level WARN -Message (
            "The active Hermes runtime uses $description; the project requires Python $targetVersion. " +
            'Setup will preserve the old environment and rebuild it automatically.'
        )
    }

    $rollbackPath = Move-HermesPythonRuntimeToRollback `
        -Runtime $Runtime `
        -RuntimeVersion $runtimeVersion

    if (Get-Command Write-HermesLog -ErrorAction SilentlyContinue) {
        Write-HermesLog -Component setup -Message (
            "Hermes Python runtime migration prepared. Rollback copy: $rollbackPath"
        )
    }

    return $rollbackPath
}
