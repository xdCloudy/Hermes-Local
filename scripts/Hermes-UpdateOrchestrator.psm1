Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

Import-Module (Join-Path $PSScriptRoot 'Common-Hermes.psm1') -Force
Import-Module (Join-Path $PSScriptRoot 'Hermes-Configuration.psm1') -Force

$script:UpdateStages = @(
    'check', 'compatibility', 'prepare', 'verify',
    'backup', 'promote', 'validate', 'rollback'
)
$script:UpdateAdapters = [ordered]@{}

function Get-HermesUpdatePaths {
    [CmdletBinding()]
    param([string] $StoreRoot)

    $root = if ($StoreRoot) {
        [System.IO.Path]::GetFullPath($StoreRoot)
    } else {
        Get-HermesRoot
    }

    [pscustomobject]@{
        Root = $root
        StateRoot = Join-Path $root 'data\runtime\update-operations'
        LockRoot = Join-Path $root 'data\runtime\locks'
        LockPath = Join-Path $root 'data\runtime\locks\update-orchestrator.json'
        ReportRoot = Join-Path $root 'build\updates'
        OperationReportRoot = Join-Path $root 'build\updates\operations'
    }
}

function Write-HermesUpdateJson {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $Path,
        [Parameter(Mandatory)][object] $Value
    )

    [System.IO.Directory]::CreateDirectory(
        [System.IO.Path]::GetDirectoryName($Path)
    ) | Out-Null
    $temporary = "$Path.$PID.$([guid]::NewGuid().ToString('N')).tmp"
    $json = ($Value | ConvertTo-Json -Depth 64) + [Environment]::NewLine
    [System.IO.File]::WriteAllText(
        $temporary,
        $json,
        [System.Text.UTF8Encoding]::new($false)
    )
    [System.IO.File]::Move($temporary, $Path, $true)
}

function Read-HermesUpdateJson {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string] $Path)

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return $null
    }
    Get-Content -Raw -LiteralPath $Path | ConvertFrom-Json -Depth 64
}

function Test-HermesUpdateProcessAlive {
    [CmdletBinding()]
    param([AllowNull()][object] $ProcessId)

    $parsed = 0
    if ($null -eq $ProcessId -or
        -not [int]::TryParse([string]$ProcessId, [ref]$parsed) -or
        $parsed -le 0) {
        return $false
    }

    try {
        $null = Get-Process -Id $parsed -ErrorAction Stop
        return $true
    } catch {
        return $false
    }
}

function Assert-HermesUpdateNativeArguments {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $FilePath,
        [Parameter(Mandatory)][AllowEmptyCollection()][object[]] $ArgumentList
    )

    if ([string]::IsNullOrWhiteSpace($FilePath) -or $FilePath -match "[`r`n`0]") {
        throw 'Native executable path is empty or contains a control character.'
    }

    $validated = [System.Collections.Generic.List[string]]::new()
    foreach ($argument in $ArgumentList) {
        if ($null -eq $argument) {
            throw 'Native argument lists cannot contain null values.'
        }
        $value = [string]$argument
        if ($value -match "[`r`n`0]") {
            throw 'Native arguments cannot contain NUL, CR or LF characters.'
        }
        if ($value.Length -gt 32767) {
            throw 'A native argument exceeded the Windows command-line safety limit.'
        }
        $validated.Add($value)
    }
    $validated.ToArray()
}

function Invoke-HermesUpdateProcess {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $FilePath,
        [Parameter(Mandatory)][AllowEmptyCollection()][object[]] $ArgumentList,
        [string] $WorkingDirectory = (Get-HermesRoot),
        [string] $LogComponent = 'update',
        [hashtable] $Environment = @{}
    )

    $arguments = Assert-HermesUpdateNativeArguments `
        -FilePath $FilePath `
        -ArgumentList $ArgumentList
    Invoke-HermesProcess `
        -FilePath $FilePath `
        -ArgumentList $arguments `
        -WorkingDirectory $WorkingDirectory `
        -Environment $Environment `
        -LogComponent $LogComponent
}

function Invoke-HermesUpdateNativeText {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $FilePath,
        [Parameter(Mandatory)][AllowEmptyCollection()][object[]] $ArgumentList,
        [string] $WorkingDirectory = (Get-HermesRoot),
        [switch] $AllowFailure
    )

    $arguments = Assert-HermesUpdateNativeArguments `
        -FilePath $FilePath `
        -ArgumentList $ArgumentList
    Push-Location $WorkingDirectory
    try {
        $output = & $FilePath @arguments 2>&1
        $exitCode = $LASTEXITCODE
    } finally {
        Pop-Location
    }

    $text = (($output | ForEach-Object { [string]$_ }) -join [Environment]::NewLine).Trim()
    if ($exitCode -ne 0 -and -not $AllowFailure) {
        throw "$FilePath $($arguments -join ' ') failed with exit code $exitCode.`n$text"
    }
    $text
}

function Register-HermesUpdateAdapter {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [ValidatePattern('^[A-Za-z][A-Za-z0-9_-]{1,63}$')]
        [string] $Name,
        [Parameter(Mandatory)][object] $Adapter,
        [switch] $Force
    )

    if ($script:UpdateAdapters.Contains($Name) -and -not $Force) {
        throw "An update adapter named '$Name' is already registered."
    }
    foreach ($stage in $script:UpdateStages) {
        $property = $Adapter.PSObject.Properties[$stage]
        if ($property -and $null -ne $property.Value -and
            $property.Value -isnot [scriptblock]) {
            throw "Adapter '$Name' stage '$stage' must be a script block or null."
        }
    }
    $script:UpdateAdapters[$Name] = $Adapter
}

function Get-HermesUpdateAdapter {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string] $Name)

    if (-not $script:UpdateAdapters.Contains($Name)) {
        throw "No Hermes update adapter is registered for '$Name'."
    }
    $script:UpdateAdapters[$Name]
}

function New-HermesUpdateState {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $OperationId,
        [Parameter(Mandatory)][string] $Component,
        [Parameter(Mandatory)][string] $Mode,
        [Parameter(Mandatory)][string] $Caller,
        [Parameter(Mandatory)][string] $StatePath
    )

    $now = (Get-Date).ToUniversalTime().ToString('o')
    $stages = @(
        foreach ($stage in $script:UpdateStages) {
            [ordered]@{
                name = $stage
                status = 'pending'
                startedAt = $null
                completedAt = $null
                message = $null
                error = $null
            }
        }
    )

    [ordered]@{
        schemaVersion = 1
        operationId = $OperationId
        identity = [ordered]@{
            component = $Component
            mode = $Mode
            requestedAt = $now
        }
        caller = $Caller
        status = 'queued'
        currentStage = $null
        progress = [ordered]@{
            completed = 0
            total = $stages.Count
            percent = 0
        }
        resources = @(
            [ordered]@{ resource = 'update-orchestrator'; mode = 'exclusive' },
            [ordered]@{ resource = 'workstation'; mode = 'exclusive' }
        )
        stages = $stages
        logs = @()
        recovery = [ordered]@{
            staleLockRecovered = $false
            previousOperationId = $null
            recoveredLockPath = $null
        }
        result = $null
        failure = $null
        statePath = $StatePath
        reportPath = $null
        createdAt = $now
        updatedAt = $now
        completedAt = $null
    }
}

function Add-HermesUpdateLog {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][System.Collections.IDictionary] $State,
        [Parameter(Mandatory)][string] $Message,
        [ValidateSet('DEBUG', 'INFO', 'WARN', 'ERROR')][string] $Level = 'INFO'
    )

    $State.logs = @($State.logs) + @([ordered]@{
        at = (Get-Date).ToUniversalTime().ToString('o')
        level = $Level
        message = Protect-HermesLogText $Message
    })
    if ($State.logs.Count -gt 200) {
        $State.logs = @($State.logs | Select-Object -Last 200)
    }
}

function Set-HermesUpdateStageState {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][System.Collections.IDictionary] $State,
        [Parameter(Mandatory)][string] $Stage,
        [Parameter(Mandatory)]
        [ValidateSet('pending', 'running', 'succeeded', 'failed', 'skipped')]
        [string] $Status,
        [string] $Message,
        [string] $ErrorMessage
    )

    $record = @($State.stages | Where-Object { $_.name -eq $Stage })[0]
    if (-not $record) {
        throw "Unknown update stage '$Stage'."
    }

    $now = (Get-Date).ToUniversalTime().ToString('o')
    if ($Status -eq 'running' -and -not $record.startedAt) {
        $record.startedAt = $now
    }
    if ($Status -in @('succeeded', 'failed', 'skipped')) {
        if (-not $record.startedAt) {
            $record.startedAt = $now
        }
        $record.completedAt = $now
    }
    $record.status = $Status
    if ($Message) { $record.message = $Message }
    if ($ErrorMessage) { $record.error = $ErrorMessage }

    if ($Status -eq 'running') {
        $State.currentStage = $Stage
    }
    $completed = @(
        $State.stages | Where-Object { $_.status -in @('succeeded', 'failed', 'skipped') }
    ).Count
    $State.progress.completed = $completed
    $State.progress.percent = [math]::Round(
        ($completed / [math]::Max(1, $State.progress.total)) * 100,
        0
    )
    $State.updatedAt = $now
}

function Write-HermesUpdateState {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][System.Collections.IDictionary] $State,
        [Parameter(Mandatory)][pscustomobject] $Paths
    )

    Write-HermesUpdateJson -Path ([string]$State.statePath) -Value $State
    Write-HermesUpdateJson -Path (Join-Path $Paths.StateRoot 'LATEST.json') -Value $State
}

function Get-HermesUpdateOperation {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $OperationId,
        [string] $StoreRoot
    )

    $paths = Get-HermesUpdatePaths -StoreRoot $StoreRoot
    Read-HermesUpdateJson -Path (Join-Path $paths.StateRoot "$OperationId.json")
}

function Enter-HermesUpdateLock {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $OperationId,
        [Parameter(Mandatory)][pscustomobject] $Paths
    )

    [System.IO.Directory]::CreateDirectory($Paths.LockRoot) | Out-Null
    $recovered = $null

    for ($attempt = 0; $attempt -lt 3; $attempt += 1) {
        $record = [ordered]@{
            schemaVersion = 1
            operationId = $OperationId
            ownerPid = $PID
            acquiredAt = (Get-Date).ToUniversalTime().ToString('o')
            heartbeatAt = (Get-Date).ToUniversalTime().ToString('o')
            resources = @('update-orchestrator', 'workstation')
        }
        $bytes = [System.Text.UTF8Encoding]::new($false).GetBytes(
            (($record | ConvertTo-Json -Depth 8) + [Environment]::NewLine)
        )

        try {
            $stream = [System.IO.FileStream]::new(
                $Paths.LockPath,
                [System.IO.FileMode]::CreateNew,
                [System.IO.FileAccess]::Write,
                [System.IO.FileShare]::None
            )
            try {
                $stream.Write($bytes, 0, $bytes.Length)
                $stream.Flush($true)
            } finally {
                $stream.Dispose()
            }
            return [pscustomobject]@{ Record = $record; Recovered = $recovered }
        } catch [System.IO.IOException] {
            $existing = $null
            try {
                $existing = Read-HermesUpdateJson -Path $Paths.LockPath
            } catch {
                $existing = $null
            }
            if ($existing -and
                (Test-HermesUpdateProcessAlive -ProcessId $existing.ownerPid)) {
                throw "Update operation '$($existing.operationId)' already owns the update lock with process $($existing.ownerPid)."
            }

            $stamp = (Get-Date).ToUniversalTime().ToString('yyyyMMddTHHmmssfffZ')
            $recoveredPath = Join-Path $Paths.LockRoot "update-orchestrator.recovered-$stamp.json"
            try {
                Move-Item -LiteralPath $Paths.LockPath -Destination $recoveredPath -Force
            } catch {
                if (Test-Path -LiteralPath $Paths.LockPath) { throw }
            }
            $recovered = [pscustomobject]@{
                Previous = $existing
                Path = $recoveredPath
            }
        }
    }
    throw 'Could not acquire the Hermes update lock after recovering stale state.'
}

function Update-HermesUpdateLockHeartbeat {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $OperationId,
        [Parameter(Mandatory)][pscustomobject] $Paths
    )

    $record = Read-HermesUpdateJson -Path $Paths.LockPath
    if (-not $record -or [string]$record.operationId -ne $OperationId) {
        throw "Update operation '$OperationId' no longer owns the update lock."
    }
    Write-HermesUpdateJson -Path $Paths.LockPath -Value ([ordered]@{
        schemaVersion = 1
        operationId = [string]$record.operationId
        ownerPid = [int]$record.ownerPid
        acquiredAt = [string]$record.acquiredAt
        heartbeatAt = (Get-Date).ToUniversalTime().ToString('o')
        resources = @($record.resources)
    })
}

function Exit-HermesUpdateLock {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $OperationId,
        [Parameter(Mandatory)][pscustomobject] $Paths
    )

    if (-not (Test-Path -LiteralPath $Paths.LockPath -PathType Leaf)) { return }
    try {
        $record = Read-HermesUpdateJson -Path $Paths.LockPath
        if ($record -and [string]$record.operationId -eq $OperationId) {
            Remove-Item -LiteralPath $Paths.LockPath -Force
        }
    } catch {
        Write-Warning "Could not release Hermes update lock: $($_.Exception.Message)"
    }
}

function Invoke-GitUpdateText {
    param(
        [Parameter(Mandatory)][string] $Repository,
        [Parameter(Mandatory)][object[]] $Arguments,
        [switch] $AllowFailure
    )

    Invoke-HermesUpdateNativeText `
        -FilePath 'git' `
        -ArgumentList (@('-C', $Repository) + $Arguments) `
        -WorkingDirectory (Get-HermesRoot) `
        -AllowFailure:$AllowFailure
}

function Get-HermesRepositoryUpdate {
    param(
        [Parameter(Mandatory)][string] $Name,
        [Parameter(Mandatory)][string] $Repository,
        [Parameter(Mandatory)][string] $RemoteBranch,
        [Parameter(Mandatory)][string] $PinnedCommit
    )

    $current = Invoke-GitUpdateText -Repository $Repository -Arguments @('rev-parse', 'HEAD')
    $localBranch = Invoke-GitUpdateText -Repository $Repository -Arguments @('branch', '--show-current')
    $origin = Invoke-GitUpdateText -Repository $Repository -Arguments @('remote', 'get-url', 'origin')
    $candidateLine = Invoke-GitUpdateText -Repository $Repository -Arguments @(
        'ls-remote', '--heads', 'origin', "refs/heads/$RemoteBranch"
    )
    $candidate = if ($candidateLine) { ($candidateLine -split '\s+')[0] } else { $null }
    $dirtyText = Invoke-GitUpdateText `
        -Repository $Repository `
        -Arguments @('status', '--porcelain') `
        -AllowFailure
    $releaseNotes = if ($origin -match '^https://github\.com/(.+?)(?:\.git)?$' -and $candidate) {
        "https://github.com/$($Matches[1])/compare/$current...$candidate"
    } else {
        $null
    }

    [ordered]@{
        name = $Name
        current = $current
        pinned = $PinnedCommit
        candidate = $candidate
        updateAvailable = [bool]($candidate -and $candidate -ne $current)
        localBranch = $localBranch
        remoteBranch = $RemoteBranch
        origin = $origin
        dirty = [bool]$dirtyText
        releaseNotes = $releaseNotes
        policy = 'Source updates require a clean tree, staging build, smoke tests and explicit validation.'
    }
}

function Get-HermesUpdateFileHashRecord {
    param(
        [Parameter(Mandatory)][string] $Name,
        [Parameter(Mandatory)][string] $Path
    )

    [ordered]@{
        name = $Name
        path = $Path
        exists = Test-Path -LiteralPath $Path -PathType Leaf
        sha256 = if (Test-Path -LiteralPath $Path -PathType Leaf) {
            (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
        } else {
            $null
        }
    }
}

function Get-HermesUpdateRecordValue {
    param(
        [Parameter(Mandatory)][object] $Record,
        [Parameter(Mandatory)][string] $Name
    )

    if ($Record -is [System.Collections.IDictionary] -and $Record.Contains($Name)) {
        return $Record[$Name]
    }
    $property = $Record.PSObject.Properties[$Name]
    if ($property) { return $property.Value }
    $null
}

function Get-HermesUpdateInventory {
    [CmdletBinding()]
    param()

    $manifest = Get-HermesVersionManifest
    $configuration = Get-HermesConfiguration
    $selectedModel = $configuration.selectedModel
    $hermesRoot = Resolve-HermesPath 'source\hermes-agent'
    $llamaRoot = Resolve-HermesPath 'runtimes\llama.cpp\source'
    $desktopPackage = Get-Content -Raw -LiteralPath (
        Join-Path $hermesRoot 'apps\desktop\package.json'
    ) | ConvertFrom-Json
    $modelValid = Test-HermesSelectedModel `
        -Model $selectedModel `
        -Hash:([bool]$selectedModel.sha256)
    $python = Resolve-HermesPath 'runtimes\python\hermes\Scripts\python.exe'
    $pythonVersion = if (Test-Path -LiteralPath $python -PathType Leaf) {
        (Invoke-HermesUpdateNativeText -FilePath $python -ArgumentList @(
            '-c',
            'import sys,sqlite3; print(f"{sys.version_info.major}.{sys.version_info.minor}.{sys.version_info.micro}|{sqlite3.sqlite_version}")'
        )).Trim()
    } else {
        $null
    }
    $nodeVersion = try {
        (Invoke-HermesUpdateNativeText -FilePath 'node' -ArgumentList @('--version')).Trim()
    } catch {
        $null
    }
    $playwrightPath = Resolve-HermesPath 'runtimes\tools\playwright'

    [ordered]@{
        schemaVersion = 2
        checkedAt = (Get-Date).ToUniversalTime().ToString('o')
        orchestration = [ordered]@{
            api = 'Hermes-UpdateOrchestrator'
            stateStore = 'data/runtime/update-operations'
            lock = 'data/runtime/locks/update-orchestrator.json'
        }
        applyPolicy = 'Components are promoted only after compatibility, staging, backup and validation.'
        components = [ordered]@{
            HermesAgent = Get-HermesRepositoryUpdate `
                -Name 'Hermes Agent' `
                -Repository $hermesRoot `
                -RemoteBranch ([string]$manifest.sources.hermesAgent.branch) `
                -PinnedCommit ([string]$manifest.sources.hermesAgent.commit)
            Launcher = [ordered]@{
                name = 'Hermes Launcher custom code'
                current = [string]$desktopPackage.version
                candidate = [string]$desktopPackage.version
                updateAvailable = $false
                sourceBranch = [string]$manifest.sources.hermesAgent.integrationBranch
                releaseNotes = 'CHANGELOG-LOCAL.md'
            }
            LlamaCpp = Get-HermesRepositoryUpdate `
                -Name 'llama.cpp' `
                -Repository $llamaRoot `
                -RemoteBranch ([string]$manifest.sources.llamaCpp.branch) `
                -PinnedCommit ([string]$manifest.sources.llamaCpp.commit)
            Model = [ordered]@{
                name = [string]$selectedModel.displayName
                current = [string]$selectedModel.revision
                candidate = [string]$selectedModel.revision
                updateAvailable = $false
                integrityValid = $modelValid
                sha256 = [string]$selectedModel.sha256
            }
            PythonLock = Get-HermesUpdateFileHashRecord `
                -Name 'Python uv.lock' `
                -Path (Join-Path $hermesRoot 'uv.lock')
            NodeLock = Get-HermesUpdateFileHashRecord `
                -Name 'Node package-lock.json' `
                -Path (Join-Path $hermesRoot 'package-lock.json')
            BrowserBinaries = [ordered]@{
                name = 'Playwright browser binaries'
                packageVersion = [string]$desktopPackage.devDependencies.'@playwright/test'
                installed = Test-Path -LiteralPath $playwrightPath
                path = $playwrightPath
            }
            OptionalTools = [ordered]@{
                name = 'Optional local tools'
                pythonAndSqlite = $pythonVersion
                node = $nodeVersion
                inventory = Get-HermesToolSnapshot
            }
        }
    }
}

function Save-HermesUpdateInventory {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][System.Collections.IDictionary] $Inventory,
        [Parameter(Mandatory)][pscustomobject] $Paths
    )

    [System.IO.Directory]::CreateDirectory($Paths.ReportRoot) | Out-Null
    Write-HermesUpdateJson -Path (Join-Path $Paths.ReportRoot 'LATEST.json') -Value $Inventory

    $lines = @(
        '# Hermes Local update check',
        '',
        "Checked: $($Inventory.checkedAt)",
        '',
        '| Component | Current | Candidate | Update |',
        '|---|---|---|---|'
    )
    foreach ($entry in $Inventory.components.GetEnumerator()) {
        $currentValue = Get-HermesUpdateRecordValue -Record $entry.Value -Name current
        $hashValue = Get-HermesUpdateRecordValue -Record $entry.Value -Name sha256
        $candidateValue = Get-HermesUpdateRecordValue -Record $entry.Value -Name candidate
        $updateAvailable = Get-HermesUpdateRecordValue -Record $entry.Value -Name updateAvailable
        $current = if ($currentValue) {
            [string]$currentValue
        } elseif ($hashValue) {
            [string]$hashValue
        } else {
            'inventory'
        }
        $candidate = if ($candidateValue) { [string]$candidateValue } else { 'n/a' }
        $available = if ($updateAvailable) { 'yes' } else { 'no' }
        $currentShort = $current.Substring(0, [math]::Min(12, $current.Length))
        $candidateShort = $candidate.Substring(0, [math]::Min(12, $candidate.Length))
        $lines += "| $($entry.Key) | $currentShort | $candidateShort | $available |"
    }
    [System.IO.File]::WriteAllText(
        (Join-Path $Paths.ReportRoot 'LATEST.md'),
        (($lines -join [Environment]::NewLine) + [Environment]::NewLine),
        [System.Text.UTF8Encoding]::new($false)
    )
}

function New-HermesLauncherUpdateAdapter {
    [CmdletBinding()]
    param()

    [pscustomobject]@{
        AutoRollbackOnFailure = $true

        check = {
            param($Context)
            $packagePath = Resolve-HermesPath 'source\hermes-agent\apps\desktop\package.json'
            if (-not (Test-Path -LiteralPath $packagePath -PathType Leaf)) {
                throw "Hermes Launcher package metadata is missing: $packagePath"
            }
            $package = Get-Content -Raw -LiteralPath $packagePath | ConvertFrom-Json
            [ordered]@{
                name = 'Hermes Launcher custom code'
                current = [string]$package.version
                candidate = [string]$package.version
                updateAvailable = $false
            }
        }

        compatibility = {
            param($Context)
            foreach ($relativePath in @(
                'Backup-Hermes-Local.ps1',
                'Build-Hermes-Launcher.ps1',
                'Test-Hermes-Local.ps1'
            )) {
                $path = Resolve-HermesPath $relativePath
                if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
                    throw "Required launcher update command is missing: $path"
                }
            }
            [ordered]@{ compatible = $true }
        }

        prepare = {
            param($Context)
            $stamp = (Get-Date).ToUniversalTime().ToString('yyyyMMddTHHmmssZ')
            $Context.Working.Stamp = $stamp
            $Context.Working.HistoryRoot = Resolve-HermesPath 'build\updates\history'
            $Context.Working.Previous = Resolve-HermesPath "build\updates\known-good\launcher-$stamp"
            $Context.Working.Dist = Resolve-HermesPath 'dist'
            [System.IO.Directory]::CreateDirectory($Context.Working.HistoryRoot) | Out-Null
            [System.IO.Directory]::CreateDirectory($Context.Working.Previous) | Out-Null
            [ordered]@{ staging = $Context.Working.Previous }
        }

        verify = {
            param($Context)
            [ordered]@{
                currentBuildPresent = Test-Path -LiteralPath (
                    Join-Path $Context.Working.Dist 'Hermes Launcher.exe'
                ) -PathType Leaf
                dist = $Context.Working.Dist
            }
        }

        backup = {
            param($Context)
            Invoke-HermesUpdateProcess -FilePath 'pwsh.exe' -ArgumentList @(
                '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
                '-File', (Resolve-HermesPath 'Backup-Hermes-Local.ps1'),
                '-Name', "pre-update-$($Context.Working.Stamp)", '-NonInteractive'
            )
            if (Test-Path -LiteralPath $Context.Working.Dist -PathType Container) {
                Copy-Item `
                    -Path (Join-Path $Context.Working.Dist '*') `
                    -Destination $Context.Working.Previous `
                    -Recurse `
                    -Force
            }
            [ordered]@{ previousKnownGood = $Context.Working.Previous }
        }

        promote = {
            param($Context)
            Invoke-HermesUpdateProcess -FilePath 'pwsh.exe' -ArgumentList @(
                '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
                '-File', (Resolve-HermesPath 'Build-Hermes-Launcher.ps1'),
                '-NonInteractive'
            )
            [ordered]@{ promoted = $true }
        }

        validate = {
            param($Context)
            Invoke-HermesUpdateProcess -FilePath 'pwsh.exe' -ArgumentList @(
                '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
                '-File', (Resolve-HermesPath 'Test-Hermes-Local.ps1'),
                '-Quick', '-SkipAgentTool', '-NonInteractive'
            )
            if ($Context.Working.ContainsKey('RollbackMode')) {
                return [ordered]@{ rollbackValidated = $true }
            }

            $executable = Resolve-HermesPath 'dist\Hermes Launcher.exe'
            if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) {
                throw "Launcher validation completed without producing $executable."
            }
            $history = [ordered]@{
                schemaVersion = 2
                operationId = $Context.OperationId
                component = 'Launcher'
                appliedAt = (Get-Date).ToUniversalTime().ToString('o')
                status = 'succeeded'
                previousKnownGood = $Context.Working.Previous
                currentExecutable = $executable
                currentSha256 = (
                    Get-FileHash -LiteralPath $executable -Algorithm SHA256
                ).Hash.ToLowerInvariant()
            }
            Write-HermesUpdateJson `
                -Path (Join-Path $Context.Working.HistoryRoot "$($Context.Working.Stamp)-launcher.json") `
                -Value $history
            $history
        }

        rollback = {
            param($Context)
            $previous = if ($Context.Working.ContainsKey('Previous')) {
                [string]$Context.Working.Previous
            } else {
                $historyRoot = Resolve-HermesPath 'build\updates\history'
                $historyFile = Get-ChildItem `
                    -LiteralPath $historyRoot `
                    -Filter '*-launcher.json' `
                    -File `
                    -ErrorAction SilentlyContinue |
                    Sort-Object LastWriteTime -Descending |
                    Select-Object -First 1
                if (-not $historyFile) {
                    throw 'No successful launcher update history is available to roll back.'
                }
                $history = Get-Content -Raw -LiteralPath $historyFile.FullName | ConvertFrom-Json
                [System.IO.Path]::GetFullPath([string]$history.previousKnownGood)
            }

            $knownGoodRoot = (Resolve-HermesPath 'build\updates\known-good').TrimEnd('\') + '\'
            if (-not $previous.StartsWith(
                    $knownGoodRoot,
                    [System.StringComparison]::OrdinalIgnoreCase
                ) -or
                -not (Test-Path -LiteralPath (Join-Path $previous 'Hermes Launcher.exe') -PathType Leaf)) {
                throw 'The recorded known-good launcher snapshot is missing or outside the update store.'
            }

            $dist = Resolve-HermesPath 'dist'
            $quarantine = Resolve-HermesPath "build\updates\failed\launcher-$((Get-Date).ToUniversalTime().ToString('yyyyMMddTHHmmssZ'))"
            [System.IO.Directory]::CreateDirectory($quarantine) | Out-Null
            if (Test-Path -LiteralPath $dist -PathType Container) {
                Copy-Item -Path (Join-Path $dist '*') -Destination $quarantine -Recurse -Force
            }
            Copy-Item -Path (Join-Path $previous '*') -Destination $dist -Recurse -Force
            $Context.Working.RollbackMode = $true
            [ordered]@{
                rolledBackAt = (Get-Date).ToUniversalTime().ToString('o')
                restoredFrom = $previous
                displacedBuild = $quarantine
            }
        }
    }
}

function Invoke-HermesAgentUpdateDelegate {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][pscustomobject] $Context,
        [Parameter(Mandatory)]
        [ValidateSet('Check', 'Apply', 'Rollback')]
        [string] $Mode
    )

    $arguments = @(
        '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
        '-File', (Resolve-HermesPath 'Update-Hermes-Agent.ps1'),
        '-Mode', $Mode,
        '-NonInteractive'
    )
    if ($Mode -ne 'Rollback' -and
        $Context.Input.ContainsKey('TargetCommit') -and
        $Context.Input.TargetCommit) {
        $arguments += @('-TargetCommit', [string]$Context.Input.TargetCommit)
    }
    if ($Mode -ne 'Rollback' -and
        $Context.Input.ContainsKey('TargetBranch') -and
        $Context.Input.TargetBranch) {
        $arguments += @('-TargetBranch', [string]$Context.Input.TargetBranch)
    }
    Invoke-HermesUpdateProcess -FilePath 'pwsh.exe' -ArgumentList $arguments
    [ordered]@{ delegated = $true; mode = $Mode }
}

function New-HermesAgentUpdateAdapter {
    [CmdletBinding()]
    param()

    [pscustomobject]@{
        AutoRollbackOnFailure = $false
        check = {
            param($Context)
            Invoke-HermesAgentUpdateDelegate -Context $Context -Mode Check
        }
        compatibility = {
            param($Context)
            if (-not (Test-Path -LiteralPath (Resolve-HermesPath 'Update-Hermes-Agent.ps1') -PathType Leaf)) {
                throw 'Update-Hermes-Agent.ps1 is missing.'
            }
            [ordered]@{ compatible = $true; delegated = $true }
        }
        prepare = { param($Context) [ordered]@{ delegated = $true; stage = 'prepare' } }
        verify = { param($Context) [ordered]@{ delegated = $true; stage = 'verify' } }
        backup = { param($Context) [ordered]@{ delegated = $true; stage = 'backup' } }
        promote = {
            param($Context)
            Invoke-HermesAgentUpdateDelegate -Context $Context -Mode Apply
        }
        validate = { param($Context) [ordered]@{ delegated = $true; stage = 'validate' } }
        rollback = {
            param($Context)
            Invoke-HermesAgentUpdateDelegate -Context $Context -Mode Rollback
        }
    }
}

function New-HermesInventoryUpdateAdapter {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string] $Component)

    [pscustomobject]@{
        AutoRollbackOnFailure = $false
        check = {
            param($Context)
            $inventory = Get-HermesUpdateInventory
            $Context.Working.Inventory = $inventory
            if ($Context.Component -eq 'All') {
                return $inventory
            }
            $inventory.components.($Context.Component)
        }
        compatibility = {
            param($Context)
            if ($Context.Mode -ne 'Check') {
                throw "Automatic promotion for $($Context.Component) is blocked until its adapter implements transactional apply and rollback stages."
            }
            [ordered]@{ compatible = $true }
        }
        prepare = $null
        verify = $null
        backup = $null
        promote = $null
        validate = $null
        rollback = $null
    }
}

function Invoke-HermesUpdateAdapterStage {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][object] $Adapter,
        [Parameter(Mandatory)][string] $Stage,
        [Parameter(Mandatory)][pscustomobject] $Context
    )

    $property = $Adapter.PSObject.Properties[$Stage]
    if (-not $property -or $null -eq $property.Value) {
        return [pscustomobject]@{ Skipped = $true; Result = $null }
    }
    $handler = [scriptblock]$property.Value
    [pscustomobject]@{
        Skipped = $false
        Result = (& $handler $Context)
    }
}

function Write-HermesUpdateOperationReport {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][System.Collections.IDictionary] $State,
        [Parameter(Mandatory)][pscustomobject] $Paths
    )

    $projection = [ordered]@{
        schemaVersion = 1
        operationId = $State.operationId
        identity = $State.identity
        caller = $State.caller
        status = $State.status
        stages = @($State.stages)
        result = $State.result
        failure = $State.failure
        sourceStatePath = $State.statePath
        generatedAt = (Get-Date).ToUniversalTime().ToString('o')
    }
    $path = Join-Path $Paths.OperationReportRoot "$($State.operationId).json"
    Write-HermesUpdateJson -Path $path -Value $projection
    Write-HermesUpdateJson `
        -Path (Join-Path $Paths.ReportRoot 'LATEST-OPERATION.json') `
        -Value $projection
    $path
}

function Invoke-HermesUpdateOperation {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [ValidateSet('Check', 'Apply', 'Rollback')]
        [string] $Mode,
        [Parameter(Mandatory)][string] $Component,
        [ValidateSet('Cli', 'Desktop', 'Installer', 'Recovery', 'Test')]
        [string] $Caller = 'Cli',
        [Alias('Input')][hashtable] $Options = @{},
        [string] $StoreRoot
    )

    if ($Mode -ne 'Check' -and $Component -eq 'All') {
        $Component = 'Launcher'
    }
    $adapter = Get-HermesUpdateAdapter -Name $Component
    $paths = Get-HermesUpdatePaths -StoreRoot $StoreRoot
    [System.IO.Directory]::CreateDirectory($paths.StateRoot) | Out-Null
    [System.IO.Directory]::CreateDirectory($paths.OperationReportRoot) | Out-Null

    $operationId = [guid]::NewGuid().ToString('N')
    $statePath = Join-Path $paths.StateRoot "$operationId.json"
    $state = New-HermesUpdateState `
        -OperationId $operationId `
        -Component $Component `
        -Mode $Mode `
        -Caller $Caller `
        -StatePath $statePath
    Write-HermesUpdateState -State $state -Paths $paths

    $lock = $null
    $context = $null
    try {
        $lock = Enter-HermesUpdateLock -OperationId $operationId -Paths $paths
        if ($lock.Recovered) {
            $state.recovery.staleLockRecovered = $true
            $state.recovery.previousOperationId = if ($lock.Recovered.Previous) {
                [string]$lock.Recovered.Previous.operationId
            } else {
                $null
            }
            $state.recovery.recoveredLockPath = [string]$lock.Recovered.Path
            Add-HermesUpdateLog `
                -State $state `
                -Level WARN `
                -Message 'Recovered a stale update lock whose owner process was no longer running.'
        }

        $state.status = 'running'
        $state.updatedAt = (Get-Date).ToUniversalTime().ToString('o')
        Add-HermesUpdateLog -State $state -Message "Started $Mode for $Component from $Caller."
        Write-HermesUpdateState -State $state -Paths $paths

        $context = [pscustomobject]@{
            OperationId = $operationId
            Component = $Component
            Mode = $Mode
            Caller = $Caller
            Input = $Options
            Options = $Options
            StoreRoot = $paths.Root
            StatePath = $statePath
            Working = @{}
        }
        $sequence = if ($Mode -eq 'Check') {
            @('check')
        } elseif ($Mode -eq 'Apply') {
            @('check', 'compatibility', 'prepare', 'verify', 'backup', 'promote', 'validate')
        } else {
            @('check', 'compatibility', 'rollback', 'validate')
        }

        foreach ($stage in $sequence) {
            Set-HermesUpdateStageState `
                -State $state `
                -Stage $stage `
                -Status running `
                -Message "Running $stage."
            Add-HermesUpdateLog -State $state -Message "Stage '$stage' started."
            Write-HermesUpdateState -State $state -Paths $paths
            Update-HermesUpdateLockHeartbeat -OperationId $operationId -Paths $paths

            $stageResult = Invoke-HermesUpdateAdapterStage `
                -Adapter $adapter `
                -Stage $stage `
                -Context $context
            if ($stageResult.Skipped) {
                Set-HermesUpdateStageState `
                    -State $state `
                    -Stage $stage `
                    -Status skipped `
                    -Message 'Adapter does not require this stage.'
            } else {
                Set-HermesUpdateStageState `
                    -State $state `
                    -Stage $stage `
                    -Status succeeded `
                    -Message 'Stage completed.'
                $state.result = $stageResult.Result
            }
            Add-HermesUpdateLog -State $state -Message "Stage '$stage' completed."
            Write-HermesUpdateState -State $state -Paths $paths
        }

        foreach ($unused in $script:UpdateStages | Where-Object { $_ -notin $sequence }) {
            $record = @($state.stages | Where-Object { $_.name -eq $unused })[0]
            if ($record.status -eq 'pending') {
                Set-HermesUpdateStageState `
                    -State $state `
                    -Stage $unused `
                    -Status skipped `
                    -Message 'Stage is not part of this operation mode.'
            }
        }
        if ($Mode -eq 'Check' -and $context.Working.ContainsKey('Inventory')) {
            Save-HermesUpdateInventory -Inventory $context.Working.Inventory -Paths $paths
        }

        $state.status = 'succeeded'
        $state.currentStage = $null
        $state.completedAt = (Get-Date).ToUniversalTime().ToString('o')
        $state.updatedAt = $state.completedAt
        Add-HermesUpdateLog -State $state -Message "$Mode completed for $Component."
        Write-HermesUpdateState -State $state -Paths $paths
        $state.reportPath = Write-HermesUpdateOperationReport -State $state -Paths $paths
        Write-HermesUpdateState -State $state -Paths $paths
        return $state
    } catch {
        $failure = $_
        $failedStage = [string]$state.currentStage
        if ($failedStage) {
            Set-HermesUpdateStageState `
                -State $state `
                -Stage $failedStage `
                -Status failed `
                -Message 'Stage failed.' `
                -ErrorMessage $failure.Exception.Message
        }
        $state.failure = [ordered]@{
            code = 'update-operation-failed'
            message = $failure.Exception.Message
            type = $failure.Exception.GetType().FullName
        }
        Add-HermesUpdateLog -State $state -Level ERROR -Message $failure.Exception.Message

        $autoRollback = $false
        $property = $adapter.PSObject.Properties['AutoRollbackOnFailure']
        if ($property) { $autoRollback = [bool]$property.Value }

        if ($Mode -eq 'Apply' -and $autoRollback -and $context) {
            try {
                Set-HermesUpdateStageState `
                    -State $state `
                    -Stage rollback `
                    -Status running `
                    -Message 'Recovering the prior known-good state.'
                Write-HermesUpdateState -State $state -Paths $paths
                $rollbackResult = Invoke-HermesUpdateAdapterStage `
                    -Adapter $adapter `
                    -Stage rollback `
                    -Context $context
                if ($rollbackResult.Skipped) {
                    throw "Adapter '$Component' declared automatic rollback but does not implement it."
                }
                Set-HermesUpdateStageState `
                    -State $state `
                    -Stage rollback `
                    -Status succeeded `
                    -Message 'Rollback completed.'
                $state.result = [ordered]@{
                    failedStage = $failedStage
                    rollback = $rollbackResult.Result
                }
                $state.status = 'rolled-back'
                Add-HermesUpdateLog `
                    -State $state `
                    -Level WARN `
                    -Message 'The failed update was rolled back to its prior known-good state.'
            } catch {
                Set-HermesUpdateStageState `
                    -State $state `
                    -Stage rollback `
                    -Status failed `
                    -Message 'Rollback failed.' `
                    -ErrorMessage $_.Exception.Message
                $state.status = 'failed'
                $state.failure['rollback'] = $_.Exception.Message
                Add-HermesUpdateLog `
                    -State $state `
                    -Level ERROR `
                    -Message "Rollback failed: $($_.Exception.Message)"
            }
        } else {
            $state.status = 'failed'
        }

        foreach ($pending in @($state.stages | Where-Object { $_.status -eq 'pending' })) {
            Set-HermesUpdateStageState `
                -State $state `
                -Stage $pending.name `
                -Status skipped `
                -Message 'Not reached after failure.'
        }
        $state.currentStage = $null
        $state.completedAt = (Get-Date).ToUniversalTime().ToString('o')
        $state.updatedAt = $state.completedAt
        Write-HermesUpdateState -State $state -Paths $paths
        $state.reportPath = Write-HermesUpdateOperationReport -State $state -Paths $paths
        Write-HermesUpdateState -State $state -Paths $paths
        return $state
    } finally {
        if ($lock) {
            Exit-HermesUpdateLock -OperationId $operationId -Paths $paths
        }
    }
}

Register-HermesUpdateAdapter -Name All -Adapter (
    New-HermesInventoryUpdateAdapter -Component All
) -Force
Register-HermesUpdateAdapter -Name Launcher -Adapter (
    New-HermesLauncherUpdateAdapter
) -Force
Register-HermesUpdateAdapter -Name HermesAgent -Adapter (
    New-HermesAgentUpdateAdapter
) -Force
foreach ($component in @(
    'LlamaCpp', 'Model', 'PythonLock', 'NodeLock',
    'BrowserBinaries', 'OptionalTools'
)) {
    Register-HermesUpdateAdapter `
        -Name $component `
        -Adapter (New-HermesInventoryUpdateAdapter -Component $component) `
        -Force
}

Export-ModuleMember -Function @(
    'Assert-HermesUpdateNativeArguments',
    'Get-HermesUpdateAdapter',
    'Get-HermesUpdateOperation',
    'Invoke-HermesUpdateOperation',
    'Register-HermesUpdateAdapter'
)
