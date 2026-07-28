[CmdletBinding(SupportsShouldProcess, ConfirmImpact = 'High')]
param(
    [ValidateSet('Check', 'Apply', 'Rollback')]
    [string] $Mode = 'Check',

    [ValidateSet(
        'All', 'HermesAgent', 'Launcher', 'LlamaCpp', 'Model',
        'PythonLock', 'NodeLock', 'BrowserBinaries', 'OptionalTools'
    )]
    [string] $Component = 'All',

    [switch] $NonInteractive
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'scripts\Common-Hermes.psm1') -Force

function Invoke-GitText {
    param(
        [Parameter(Mandatory)]
        [string] $Repository,
        [Parameter(Mandatory)]
        [string[]] $Arguments,
        [switch] $AllowFailure
    )

    $output = & git -C $Repository @Arguments 2>&1
    if ($LASTEXITCODE -ne 0 -and -not $AllowFailure) {
        throw "git $($Arguments -join ' ') failed in $Repository."
    }
    return (($output | ForEach-Object { [string]$_ }) -join [Environment]::NewLine).Trim()
}

function Get-RepositoryUpdate {
    param(
        [Parameter(Mandatory)]
        [string] $Name,
        [Parameter(Mandatory)]
        [string] $Repository,
        [Parameter(Mandatory)]
        [string] $RemoteBranch,
        [Parameter(Mandatory)]
        [string] $PinnedCommit
    )

    $current = Invoke-GitText -Repository $Repository -Arguments @('rev-parse', 'HEAD')
    $localBranch = Invoke-GitText -Repository $Repository -Arguments @('branch', '--show-current')
    $origin = Invoke-GitText -Repository $Repository -Arguments @('remote', 'get-url', 'origin')
    $candidateLine = Invoke-GitText -Repository $Repository -Arguments @(
        'ls-remote', '--heads', 'origin', "refs/heads/$RemoteBranch"
    )
    $candidate = if ($candidateLine) { ($candidateLine -split '\s+')[0] } else { $null }
    $dirty = @(Invoke-GitText -Repository $Repository -Arguments @('status', '--porcelain') -AllowFailure)
    $compareOrigin = if ($origin -match '^https://github\.com/(.+?)(?:\.git)?$') {
        "https://github.com/$($Matches[1])/compare/$current...$candidate"
    } else {
        $null
    }

    return [ordered]@{
        name = $Name
        current = $current
        pinned = $PinnedCommit
        candidate = $candidate
        updateAvailable = [bool]($candidate -and $candidate -ne $current)
        localBranch = $localBranch
        remoteBranch = $RemoteBranch
        origin = $origin
        dirty = [bool]($dirty -and $dirty[0])
        releaseNotes = $compareOrigin
        policy = 'Source updates require a clean tree, staging build, smoke tests and explicit validation.'
    }
}

function Get-FileHashRecord {
    param(
        [Parameter(Mandatory)]
        [string] $Name,
        [Parameter(Mandatory)]
        [string] $Path
    )

    return [ordered]@{
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

function Get-RecordValue {
    param(
        [Parameter(Mandatory)]
        [object] $Record,
        [Parameter(Mandatory)]
        [string] $Name
    )

    if ($Record -is [System.Collections.IDictionary] -and $Record.Contains($Name)) {
        return $Record[$Name]
    }
    $property = $Record.PSObject.Properties[$Name]
    if ($property) {
        return $property.Value
    }
    return $null
}

function Get-UpdateInventory {
    $manifest = Get-HermesVersionManifest
    Import-Module (Join-Path $PSScriptRoot 'scripts\Hermes-Configuration.psm1') -Force
    $configuration = Get-HermesConfiguration
    $selectedModel = $configuration.selectedModel
    $hermesRoot = Resolve-HermesPath 'source\hermes-agent'
    $llamaRoot = Resolve-HermesPath 'runtimes\llama.cpp\source'
    $desktopPackage = Get-Content -Raw -LiteralPath (Join-Path $hermesRoot 'apps\desktop\package.json') | ConvertFrom-Json
    $modelValid = Test-HermesSelectedModel -Model $selectedModel -Hash:([bool]$selectedModel.sha256)
    $python = Resolve-HermesPath 'runtimes\python\hermes\Scripts\python.exe'
    $pythonVersion = if (Test-Path -LiteralPath $python) {
        (& $python -c 'import sys,sqlite3; print(f"{sys.version_info.major}.{sys.version_info.minor}.{sys.version_info.micro}|{sqlite3.sqlite_version}")').Trim()
    } else {
        $null
    }
    $nodeVersion = (& node --version 2>$null).Trim()
    $playwrightPath = Resolve-HermesPath 'runtimes\tools\playwright'

    return [ordered]@{
        schemaVersion = 1
        checkedAt = (Get-Date).ToUniversalTime().ToString('o')
        applyPolicy = 'No automatic application. Each component is checked separately and promoted only after backup, staging and smoke tests.'
        components = [ordered]@{
            HermesAgent = Get-RepositoryUpdate `
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
            LlamaCpp = Get-RepositoryUpdate `
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
                policy = 'Model files are staged beside the active model and promoted only after size, SHA-256 and smoke validation.'
            }
            PythonLock = Get-FileHashRecord -Name 'Python uv.lock' -Path (Join-Path $hermesRoot 'uv.lock')
            NodeLock = Get-FileHashRecord -Name 'Node package-lock.json' -Path (Join-Path $hermesRoot 'package-lock.json')
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

function Save-UpdateReport {
    param(
        [Parameter(Mandatory)]
        [System.Collections.IDictionary] $Inventory
    )

    $reportRoot = Resolve-HermesPath 'build\updates'
    [System.IO.Directory]::CreateDirectory($reportRoot) | Out-Null
    Write-HermesAtomicText -Path (Join-Path $reportRoot 'LATEST.json') -Content (
        ($Inventory | ConvertTo-Json -Depth 32) + [Environment]::NewLine
    )
    $lines = @(
        '# Hermes Local update check',
        '',
        "Checked: $($Inventory.checkedAt)",
        '',
        '| Component | Current | Candidate | Update |',
        '|---|---|---|---|'
    )
    foreach ($entry in $Inventory.components.GetEnumerator()) {
        $currentValue = Get-RecordValue -Record $entry.Value -Name 'current'
        $hashValue = Get-RecordValue -Record $entry.Value -Name 'sha256'
        $candidateValue = Get-RecordValue -Record $entry.Value -Name 'candidate'
        $updateAvailable = Get-RecordValue -Record $entry.Value -Name 'updateAvailable'
        $current = if ($currentValue) { [string]$currentValue } elseif ($hashValue) { [string]$hashValue } else { 'inventory' }
        $candidate = if ($candidateValue) { [string]$candidateValue } else { 'n/a' }
        $available = if ($updateAvailable) { 'yes' } else { 'no' }
        $lines += "| $($entry.Key) | $($current.Substring(0, [math]::Min(12, $current.Length))) | $($candidate.Substring(0, [math]::Min(12, $candidate.Length))) | $available |"
    }
    Write-HermesAtomicText -Path (Join-Path $reportRoot 'LATEST.md') -Content (
        ($lines -join [Environment]::NewLine) + [Environment]::NewLine
    )
    return $reportRoot
}

function Apply-LauncherUpdate {
    $stamp = (Get-Date).ToUniversalTime().ToString('yyyyMMddTHHmmssZ')
    $historyRoot = Resolve-HermesPath 'build\updates\history'
    $previous = Resolve-HermesPath "build\updates\known-good\launcher-$stamp"
    $dist = Resolve-HermesPath 'dist'
    [System.IO.Directory]::CreateDirectory($historyRoot) | Out-Null
    [System.IO.Directory]::CreateDirectory($previous) | Out-Null

    $null = Invoke-HermesProcess -FilePath 'pwsh.exe' -ArgumentList @(
        '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
        '-File', (Resolve-HermesPath 'Backup-Hermes-Local.ps1'),
        '-Name', "pre-update-$stamp", '-NonInteractive'
    ) -LogComponent update
    if (Test-Path -LiteralPath $dist) {
        Copy-Item -Path (Join-Path $dist '*') -Destination $previous -Recurse -Force
    }

    try {
        $null = Invoke-HermesProcess -FilePath 'pwsh.exe' -ArgumentList @(
            '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
            '-File', (Resolve-HermesPath 'Build-Hermes-Launcher.ps1'), '-NonInteractive'
        ) -LogComponent update
        $null = Invoke-HermesProcess -FilePath 'pwsh.exe' -ArgumentList @(
            '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
            '-File', (Resolve-HermesPath 'Test-Hermes-Local.ps1'),
            '-Quick', '-SkipAgentTool', '-NonInteractive'
        ) -LogComponent update
    } catch {
        if (Test-Path -LiteralPath (Join-Path $previous 'Hermes Launcher.exe')) {
            Copy-Item -Path (Join-Path $previous '*') -Destination $dist -Recurse -Force
        }
        throw
    }

    $history = [ordered]@{
        schemaVersion = 1
        component = 'Launcher'
        appliedAt = (Get-Date).ToUniversalTime().ToString('o')
        status = 'succeeded'
        previousKnownGood = $previous
        currentExecutable = Resolve-HermesPath 'dist\Hermes Launcher.exe'
        currentSha256 = (Get-FileHash -LiteralPath (Resolve-HermesPath 'dist\Hermes Launcher.exe') -Algorithm SHA256).Hash.ToLowerInvariant()
    }
    Write-HermesAtomicText -Path (Join-Path $historyRoot "$stamp-launcher.json") -Content (
        ($history | ConvertTo-Json -Depth 8) + [Environment]::NewLine
    )
    return $history
}

function Rollback-Launcher {
    $historyRoot = Resolve-HermesPath 'build\updates\history'
    $historyFile = Get-ChildItem -LiteralPath $historyRoot -Filter '*-launcher.json' -File -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
    if (-not $historyFile) {
        throw 'No successful launcher update history is available to roll back.'
    }
    $history = Get-Content -Raw -LiteralPath $historyFile.FullName | ConvertFrom-Json
    $previous = [System.IO.Path]::GetFullPath([string]$history.previousKnownGood)
    $knownGoodRoot = (Resolve-HermesPath 'build\updates\known-good').TrimEnd('\') + '\'
    if (-not $previous.StartsWith($knownGoodRoot, [System.StringComparison]::OrdinalIgnoreCase) -or
        -not (Test-Path -LiteralPath (Join-Path $previous 'Hermes Launcher.exe'))) {
        throw 'The recorded known-good launcher snapshot is missing or outside the update store.'
    }

    $dist = Resolve-HermesPath 'dist'
    $quarantine = Resolve-HermesPath "build\updates\failed\launcher-$((Get-Date).ToUniversalTime().ToString('yyyyMMddTHHmmssZ'))"
    [System.IO.Directory]::CreateDirectory($quarantine) | Out-Null
    Copy-Item -Path (Join-Path $dist '*') -Destination $quarantine -Recurse -Force
    Copy-Item -Path (Join-Path $previous '*') -Destination $dist -Recurse -Force
    return [ordered]@{
        rolledBackAt = (Get-Date).ToUniversalTime().ToString('o')
        restoredFrom = $previous
        displacedBuild = $quarantine
    }
}

try {
    Assert-HermesRoot
    Initialize-HermesLayout
    Set-HermesProcessEnvironment

    if ($Mode -eq 'Check') {
        $inventory = Get-UpdateInventory
        $reportRoot = Save-UpdateReport -Inventory $inventory
        Write-HermesLog -Component update -Message "Checked all update components; report $reportRoot."
        $inventory | ConvertTo-Json -Depth 10
        Write-Host "Update report: $reportRoot"
        exit 0
    }

    if ($Component -notin @('Launcher', 'All')) {
        throw "Automatic promotion for $Component is intentionally blocked. Run Check, review its release notes, and validate a component-specific staging build before changing the pinned manifest."
    }
    if (-not $NonInteractive -and -not $PSCmdlet.ShouldProcess("Hermes Local $Component", $Mode)) {
        Write-Host "$Mode cancelled."
        exit 2
    }

    $result = if ($Mode -eq 'Apply') {
        Apply-LauncherUpdate
    } else {
        Rollback-Launcher
    }
    Write-HermesLog -Component update -Message "$Mode completed for $Component."
    $result | ConvertTo-Json -Depth 8
    exit 0
} catch {
    Write-HermesLog -Component update -Level ERROR -Message $_.Exception.ToString()
    Write-Host "Hermes Local update $Mode failed: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
}
