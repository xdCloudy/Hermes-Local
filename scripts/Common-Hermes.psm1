Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$script:HermesRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$script:HermesRootPrefix = $script:HermesRoot.TrimEnd('\') + '\'

function Get-HermesRoot {
    [CmdletBinding()]
    param()

    return $script:HermesRoot
}

function Assert-HermesRoot {
    [CmdletBinding()]
    param()

    $resolved = [System.IO.Path]::GetFullPath($script:HermesRoot)
    $rootPath = [System.IO.Path]::GetPathRoot($resolved)
    if (-not $rootPath -or $resolved.TrimEnd('\') -eq $rootPath.TrimEnd('\')) {
        throw "Refusing to use a filesystem root as the Hermes Local project: $resolved"
    }
    foreach ($marker in @('VERSION.json', 'scripts\Common-Hermes.psm1')) {
        if (-not (Test-Path -LiteralPath (Join-Path $resolved $marker))) {
            throw "The selected directory is not a Hermes Local project (missing $marker): $resolved"
        }
    }
}

function Resolve-HermesPath {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string] $RelativePath
    )

    $candidate = [System.IO.Path]::GetFullPath((Join-Path $script:HermesRoot $RelativePath))
    if ($candidate -ne $script:HermesRoot -and
        -not $candidate.StartsWith($script:HermesRootPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Path escapes the Hermes root: $RelativePath"
    }
    return $candidate
}

function Get-HermesVersionManifest {
    [CmdletBinding()]
    param()

    $manifestPath = Resolve-HermesPath 'VERSION.json'
    if (-not (Test-Path -LiteralPath $manifestPath)) {
        throw "Version manifest is missing: $manifestPath"
    }
    $manifest = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json -Depth 32
    $overridePath = Resolve-HermesPath 'config\launcher\source-overrides.json'
    if (-not (Test-Path -LiteralPath $overridePath -PathType Leaf)) {
        return $manifest
    }

    $override = Get-Content -Raw -LiteralPath $overridePath | ConvertFrom-Json -Depth 16
    if ([int]$override.schemaVersion -ne 1 -or -not $override.sources.hermesAgent) {
        throw "Hermes source override is invalid: $overridePath"
    }
    foreach ($name in @('commit', 'integrationCommit', 'integrationTree')) {
        $value = [string]$override.sources.hermesAgent.$name
        if ($value -notmatch '^[0-9a-fA-F]{40}$') {
            throw "Hermes source override field '$name' is not a 40-character Git identity: $overridePath"
        }
        $manifest.sources.hermesAgent.$name = $value.ToLowerInvariant()
    }
    return $manifest
}

function Protect-HermesLogText {
    [CmdletBinding()]
    param(
        [AllowEmptyString()]
        [string] $Text
    )

    if ($null -eq $Text) {
        return ''
    }

    $redacted = $Text
    $patterns = @(
        '(?i)\b(sk|pk|rk)-[A-Za-z0-9_-]{16,}\b',
        '(?i)\b(Bearer|token|api[_-]?key|password|secret)\s*[:=]\s*[^\s,;]+',
        '(?i)"(token|apiKey|api_key|password|secret)"\s*:\s*"[^"]+"'
    )
    foreach ($pattern in $patterns) {
        $redacted = [regex]::Replace($redacted, $pattern, '$1=[REDACTED]')
    }
    return $redacted
}

function Write-HermesLog {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string] $Component,

        [Parameter(Mandatory)]
        [string] $Message,

        [ValidateSet('DEBUG', 'INFO', 'WARN', 'ERROR')]
        [string] $Level = 'INFO'
    )

    $logDirectory = Resolve-HermesPath "logs\$Component"
    [System.IO.Directory]::CreateDirectory($logDirectory) | Out-Null
    $logPath = Join-Path $logDirectory "$Component.log"
    $safeMessage = Protect-HermesLogText $Message
    $line = '{0} [{1}] {2}' -f (Get-Date).ToUniversalTime().ToString('o'), $Level, $safeMessage
    [System.IO.File]::AppendAllText($logPath, $line + [Environment]::NewLine, [System.Text.UTF8Encoding]::new($false))
    if ($Level -eq 'ERROR') {
        # Logging is diagnostic output, not control flow. Callers already own
        # the failure and must remain able to print context and return an exit code.
        Write-Error $safeMessage -ErrorAction Continue
    } elseif ($Level -eq 'WARN') {
        Write-Warning $safeMessage
    } else {
        Write-Verbose $safeMessage
    }
}

function Write-HermesAtomicText {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string] $Path,

        [Parameter(Mandatory)]
        [AllowEmptyString()]
        [string] $Content,

        [switch] $Backup
    )

    $absolute = [System.IO.Path]::GetFullPath($Path)
    if ($absolute -ne $script:HermesRoot -and
        -not $absolute.StartsWith($script:HermesRootPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Atomic write target is outside the Hermes root: $absolute"
    }

    $directory = [System.IO.Path]::GetDirectoryName($absolute)
    [System.IO.Directory]::CreateDirectory($directory) | Out-Null
    if ($Backup -and (Test-Path -LiteralPath $absolute)) {
        $backupRoot = Resolve-HermesPath 'backups\config'
        [System.IO.Directory]::CreateDirectory($backupRoot) | Out-Null
        $stamp = (Get-Date).ToUniversalTime().ToString('yyyyMMddTHHmmssZ')
        $backupPath = Join-Path $backupRoot "$([System.IO.Path]::GetFileName($absolute)).$stamp.bak"
        Copy-Item -LiteralPath $absolute -Destination $backupPath
    }

    $temporary = "$absolute.$([guid]::NewGuid().ToString('N')).tmp"
    try {
        [System.IO.File]::WriteAllText($temporary, $Content, [System.Text.UTF8Encoding]::new($false))
        [System.IO.File]::Move($temporary, $absolute, $true)
    } finally {
        if (Test-Path -LiteralPath $temporary) {
            Remove-Item -LiteralPath $temporary -Force
        }
    }
}

function Initialize-HermesLayout {
    [CmdletBinding()]
    param()

    Assert-HermesRoot
    $directories = @(
        'config\hermes', 'config\llama', 'config\launcher', 'config\profiles', 'config\templates',
        'data\sessions', 'data\memory', 'data\skills', 'data\cron', 'data\databases', 'data\user', 'data\hermes',
        'models', 'models\draft', 'models\manifests',
        'runtimes\llama.cpp', 'runtimes\python', 'runtimes\node', 'runtimes\git', 'runtimes\tools',
        'source\hermes-launcher', 'source\native-helpers',
        'scripts\setup', 'scripts\build', 'scripts\launch', 'scripts\update',
        'scripts\benchmark', 'scripts\security', 'scripts\backup', 'scripts\diagnostics',
        'logs\launcher', 'logs\model-server', 'logs\hermes', 'logs\dashboard',
        'logs\security', 'logs\benchmarks',
        'cache\huggingface', 'cache\npm', 'cache\uv', 'cache\build',
        'backups', 'benchmarks\inputs', 'benchmarks\results', 'benchmarks\reports',
        'security\scans', 'security\threat-model', 'security\findings',
        'security\patches', 'security\sbom', 'security\reports',
        'docs', 'build', 'dist', 'temp'
    )
    foreach ($relative in $directories) {
        [System.IO.Directory]::CreateDirectory((Resolve-HermesPath $relative)) | Out-Null
    }
}

function Set-HermesProcessEnvironment {
    [CmdletBinding()]
    param()

    $env:HERMES_HOME = Resolve-HermesPath 'data\hermes'
    $env:HF_HOME = Resolve-HermesPath 'cache\huggingface'
    $env:HUGGINGFACE_HUB_CACHE = Resolve-HermesPath 'cache\huggingface\hub'
    $env:UV_CACHE_DIR = Resolve-HermesPath 'cache\uv'
    $env:npm_config_cache = Resolve-HermesPath 'cache\npm'
    $env:XDG_CACHE_HOME = Resolve-HermesPath 'cache'
    $env:PLAYWRIGHT_BROWSERS_PATH = Resolve-HermesPath 'runtimes\tools\playwright'
    $env:HERMES_DESKTOP_HERMES_ROOT = Resolve-HermesPath 'source\hermes-agent'
    $env:HERMES_DESKTOP_CWD = Resolve-HermesPath 'data\user'

    $cudaBase = Join-Path $env:ProgramFiles 'NVIDIA GPU Computing Toolkit\CUDA'
    if (Test-Path -LiteralPath $cudaBase) {
        $nvcc = Get-ChildItem -LiteralPath $cudaBase -Recurse -Filter nvcc.exe -File -ErrorAction SilentlyContinue |
            Sort-Object FullName -Descending |
            Select-Object -First 1
        if ($nvcc) {
            $cudaRoot = [System.IO.Directory]::GetParent(
                [System.IO.Directory]::GetParent($nvcc.FullName).FullName
            ).FullName
            $cudaRuntime = Join-Path $cudaRoot 'bin\x64'
            $cudaCompiler = Join-Path $cudaRoot 'bin'
            $env:CUDA_PATH = $cudaRoot
            $runtimeSegments = @($cudaRuntime, $cudaCompiler) |
                Where-Object { Test-Path -LiteralPath $_ }
            foreach ($segment in $runtimeSegments) {
                if (($env:PATH -split ';') -notcontains $segment) {
                    $env:PATH = "$segment;$env:PATH"
                }
            }
        }
    }
}

function Get-HermesPropertyValue {
    [CmdletBinding()]
    param(
        [AllowNull()]
        [object] $InputObject,

        [Parameter(Mandatory)]
        [string] $Name
    )

    if ($null -eq $InputObject) {
        return $null
    }

    $property = $InputObject.PSObject.Properties[$Name]
    if ($null -eq $property) {
        return $null
    }

    return $property.Value
}

function Get-HermesHardwareSnapshot {
    [CmdletBinding()]
    param(
        [scriptblock] $CimInstanceProvider = {
            param([string] $ClassName)
            Get-CimInstance -ClassName $ClassName
        },

        [scriptblock] $NvidiaSmiProvider = {
            $nvidiaSmi = Get-Command nvidia-smi -ErrorAction SilentlyContinue |
                Select-Object -First 1
            if (-not $nvidiaSmi) {
                return
            }

            $raw = & $nvidiaSmi.Source `
                --query-gpu=name,driver_version,memory.total,compute_cap `
                --format=csv,noheader,nounits 2>$null
            if ($LASTEXITCODE -eq 0 -and $raw) {
                return $raw
            }
        }
    )

    $os = & $CimInstanceProvider 'Win32_OperatingSystem' |
        Select-Object -First 1
    $cpu = & $CimInstanceProvider 'Win32_Processor' |
        Select-Object -First 1
    $system = & $CimInstanceProvider 'Win32_ComputerSystem' |
        Select-Object -First 1
    $videoControllers = @(& $CimInstanceProvider 'Win32_VideoController')
    $gpu = $videoControllers |
        Where-Object {
            [string](Get-HermesPropertyValue -InputObject $_ -Name 'Name') -match 'NVIDIA'
        } |
        Select-Object -First 1

    $nvidia = $null
    $raw = @(& $NvidiaSmiProvider) | Select-Object -First 1
    if ($null -ne $raw) {
        $parts = @(
            ([string] $raw -split ',') |
                ForEach-Object { $_.Trim() }
        )
        $memoryMiB = 0
        if ($parts.Count -ge 4 -and
            $parts[0] -and
            [int]::TryParse($parts[2], [ref] $memoryMiB)) {
            $nvidia = [pscustomobject]@{
                Name = $parts[0]
                DriverVersion = $parts[1]
                MemoryMiB = $memoryMiB
                ComputeCapability = $parts[3]
            }
        }
    }

    $memoryBytes = Get-HermesPropertyValue `
        -InputObject $system `
        -Name 'TotalPhysicalMemory'
    if ($null -ne $memoryBytes) {
        $memoryBytes = [int64] $memoryBytes
    }

    return [pscustomobject]@{
        OperatingSystem = Get-HermesPropertyValue -InputObject $os -Name 'Caption'
        Version = Get-HermesPropertyValue -InputObject $os -Name 'Version'
        Build = Get-HermesPropertyValue -InputObject $os -Name 'BuildNumber'
        Architecture = Get-HermesPropertyValue -InputObject $os -Name 'OSArchitecture'
        Cpu = Get-HermesPropertyValue -InputObject $cpu -Name 'Name'
        PhysicalCores = Get-HermesPropertyValue -InputObject $cpu -Name 'NumberOfCores'
        LogicalProcessors = Get-HermesPropertyValue -InputObject $cpu -Name 'NumberOfLogicalProcessors'
        MemoryBytes = $memoryBytes
        DisplayGpu = Get-HermesPropertyValue -InputObject $gpu -Name 'Name'
        Nvidia = $nvidia
    }
}

function Assert-HermesMachine {
    [CmdletBinding()]
    param(
        [int64] $RequiredFreeBytes = 16GB,
        [ValidateSet('auto', 'cpu', 'cuda')]
        [string] $Acceleration = 'auto',
        [scriptblock] $HardwareSnapshotProvider = {
            Get-HermesHardwareSnapshot
        }
    )

    $snapshot = & $HardwareSnapshotProvider
    $operatingSystem = [string](
        Get-HermesPropertyValue -InputObject $snapshot -Name 'OperatingSystem'
    )
    $architecture = [string](
        Get-HermesPropertyValue -InputObject $snapshot -Name 'Architecture'
    )
    $nvidia = Get-HermesPropertyValue -InputObject $snapshot -Name 'Nvidia'

    if ($operatingSystem -notmatch 'Windows (10|11)') {
        throw "Windows 10 or newer is required. Detected: $operatingSystem"
    }
    if ($architecture -notmatch '64') {
        throw "A 64-bit OS is required. Detected: $architecture"
    }
    if ($Acceleration -eq 'cuda' -and -not $nvidia) {
        throw 'CUDA acceleration was requested, but an NVIDIA GPU was not detected by nvidia-smi.'
    }

    $driveName = [System.IO.Path]::GetPathRoot($script:HermesRoot).TrimEnd('\').TrimEnd(':')
    $drive = Get-PSDrive -Name $driveName -PSProvider FileSystem
    if ($drive.Free -lt $RequiredFreeBytes) {
        throw "$($drive.Name): needs at least $RequiredFreeBytes free bytes; detected $($drive.Free)."
    }
    return $snapshot
}

function Get-HermesToolSnapshot {
    [CmdletBinding()]
    param()

    $toolSpecs = [ordered]@{
        pwsh = @('--version')
        git = @('--version')
        python = @('--version')
        uv = @('--version')
        node = @('--version')
        'npm.cmd' = @('--version')
        cmake = @('--version')
        ninja = @('--version')
        nvcc = @('--version')
        rg = @('--version')
        ffmpeg = @('-version')
    }
    $result = [ordered]@{}
    foreach ($entry in $toolSpecs.GetEnumerator()) {
        $command = Get-Command $entry.Key -ErrorAction SilentlyContinue | Select-Object -First 1
        if (-not $command) {
            $result[$entry.Key] = [pscustomobject]@{ Found = $false; Path = $null; Version = $null }
            continue
        }
        $version = (& $command.Source @($entry.Value) 2>&1 | Select-Object -First 2) -join ' '
        $result[$entry.Key] = [pscustomobject]@{
            Found = $true
            Path = $command.Source
            Version = Protect-HermesLogText $version
        }
    }
    return [pscustomobject]$result
}

function Invoke-HermesProcess {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string] $FilePath,

        [string[]] $ArgumentList = @(),

        [string] $WorkingDirectory = $script:HermesRoot,

        [hashtable] $Environment = @{},

        [int[]] $AcceptExitCode = @(0),

        [string] $LogComponent = 'setup',

        [switch] $PassThruOutput
    )

    $command = Get-Command $FilePath -ErrorAction SilentlyContinue | Select-Object -First 1
    $resolvedFile = if ($command) { $command.Source } elseif (Test-Path -LiteralPath $FilePath) {
        [System.IO.Path]::GetFullPath($FilePath)
    } else {
        throw "Executable not found: $FilePath"
    }

    $resolvedWorkingDirectory = [System.IO.Path]::GetFullPath($WorkingDirectory)
    if (-not (Test-Path -LiteralPath $resolvedWorkingDirectory -PathType Container)) {
        throw "Working directory does not exist: $resolvedWorkingDirectory"
    }

    $safeArgs = ($ArgumentList | ForEach-Object {
        if ($_ -match '(?i)(token|secret|password|api[_-]?key)') { '[REDACTED-ARG]' } else { $_ }
    }) -join ' '
    Write-HermesLog -Component $LogComponent -Message "Starting $resolvedFile $safeArgs"

    $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $resolvedFile
    $startInfo.WorkingDirectory = $resolvedWorkingDirectory
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    foreach ($argument in $ArgumentList) {
        $startInfo.ArgumentList.Add([string]$argument)
    }
    foreach ($entry in $Environment.GetEnumerator()) {
        $startInfo.Environment[[string]$entry.Key] = [string]$entry.Value
    }

    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $startInfo
    if (-not $process.Start()) {
        throw "Failed to start $resolvedFile"
    }

    $stdoutTask = $process.StandardOutput.ReadToEndAsync()
    $stderrTask = $process.StandardError.ReadToEndAsync()
    $process.WaitForExit()
    $stdout = $stdoutTask.GetAwaiter().GetResult()
    $stderr = $stderrTask.GetAwaiter().GetResult()
    $combined = (($stdout, $stderr | Where-Object { $_ }) -join [Environment]::NewLine).Trim()
    if ($combined) {
        Write-HermesLog -Component $LogComponent -Message $combined
        if ($PassThruOutput) {
            Write-Output $combined
        }
    }
    if ($AcceptExitCode -notcontains $process.ExitCode) {
        throw "$resolvedFile exited with code $($process.ExitCode). See logs\$LogComponent\$LogComponent.log."
    }
    # Success is silent by default. Callers that need command output opt in via
    # PassThruOutput; unsuccessful exit codes are raised above.
    return
}

function Get-OrCreateHermesApiToken {
    [CmdletBinding()]
    param()

    $secretPath = Resolve-HermesPath 'config\launcher\api-token.dpapi'
    if (Test-Path -LiteralPath $secretPath) {
        $protected = (Get-Content -Raw -LiteralPath $secretPath).Trim()
        $secure = ConvertTo-SecureString $protected
        return [System.Net.NetworkCredential]::new('', $secure).Password
    }

    $bytes = [byte[]]::new(48)
    [System.Security.Cryptography.RandomNumberGenerator]::Fill($bytes)
    $token = [Convert]::ToBase64String($bytes).TrimEnd('=').Replace('+', '-').Replace('/', '_')
    $secureToken = ConvertTo-SecureString $token -AsPlainText -Force
    $protectedToken = ConvertFrom-SecureString $secureToken
    Write-HermesAtomicText -Path $secretPath -Content ($protectedToken + [Environment]::NewLine)
    Write-HermesLog -Component setup -Message 'Generated a per-user DPAPI-protected local API token.'
    return $token
}

function Test-HermesFile {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string] $Path,

        [Parameter(Mandatory)]
        [int64] $ExpectedSize,

        [Parameter(Mandatory)]
        [string] $ExpectedSha256
    )

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return $false
    }
    $file = Get-Item -LiteralPath $Path
    if ($file.Length -ne $ExpectedSize) {
        return $false
    }
    $actual = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
    return $actual -eq $ExpectedSha256.ToLowerInvariant()
}

function Test-HermesLoopbackAddress {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string] $Address
    )

    $parsed = $null
    if (-not [System.Net.IPAddress]::TryParse($Address, [ref]$parsed)) {
        return $false
    }
    return [System.Net.IPAddress]::IsLoopback($parsed)
}

Export-ModuleMember -Function @(
    'Get-HermesRoot',
    'Assert-HermesRoot',
    'Resolve-HermesPath',
    'Get-HermesVersionManifest',
    'Protect-HermesLogText',
    'Write-HermesLog',
    'Write-HermesAtomicText',
    'Initialize-HermesLayout',
    'Set-HermesProcessEnvironment',
    'Get-HermesHardwareSnapshot',
    'Assert-HermesMachine',
    'Get-HermesToolSnapshot',
    'Invoke-HermesProcess',
    'Get-OrCreateHermesApiToken',
    'Test-HermesFile',
    'Test-HermesLoopbackAddress'
)
