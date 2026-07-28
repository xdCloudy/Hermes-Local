[CmdletBinding(SupportsShouldProcess, ConfirmImpact = 'High')]
param(
    [ValidateSet('Check', 'Apply', 'Rollback')]
    [string] $Mode = 'Apply',

    [ValidatePattern('^[0-9a-fA-F]{40}$')]
    [string] $TargetCommit,

    [ValidatePattern('^[A-Za-z0-9._/-]+$')]
    [string] $TargetBranch,

    [switch] $NonInteractive
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'scripts\Common-Hermes.psm1') -Force
Import-Module (Join-Path $PSScriptRoot 'scripts\Hermes-Configuration.psm1') -Force

function Invoke-NativeText {
    param(
        [Parameter(Mandatory)]
        [string] $FilePath,
        [Parameter(Mandatory)]
        [string[]] $ArgumentList,
        [string] $WorkingDirectory = (Get-HermesRoot),
        [switch] $AllowFailure
    )

    Push-Location $WorkingDirectory
    try {
        $output = & $FilePath @ArgumentList 2>&1
        $exitCode = $LASTEXITCODE
    } finally {
        Pop-Location
    }
    $text = (($output | ForEach-Object { [string]$_ }) -join [Environment]::NewLine).Trim()
    if ($exitCode -ne 0 -and -not $AllowFailure) {
        throw "$FilePath $($ArgumentList -join ' ') failed with exit code $exitCode.`n$text"
    }
    return $text
}

function Invoke-HermesPowerShellScript {
    param(
        [Parameter(Mandatory)]
        [string] $RelativePath,
        [string[]] $Arguments = @(),
        [string] $LogComponent = 'update'
    )

    $allArguments = @(
        '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
        '-File', (Resolve-HermesPath $RelativePath)
    ) + $Arguments
    Invoke-HermesProcess `
        -FilePath 'pwsh.exe' `
        -ArgumentList $allArguments `
        -LogComponent $LogComponent
}

function Get-AgentCandidate {
    param(
        [Parameter(Mandatory)]
        [pscustomobject] $Manifest
    )

    if ($TargetCommit) {
        return $TargetCommit.ToLowerInvariant()
    }
    $branch = if ($TargetBranch) { $TargetBranch } else { [string]$Manifest.sources.hermesAgent.branch }
    $repository = [string]$Manifest.sources.hermesAgent.repository
    $line = Invoke-NativeText -FilePath 'git' -ArgumentList @(
        'ls-remote', '--heads', $repository, "refs/heads/$branch"
    )
    if (-not $line) {
        throw "No upstream commit was found for branch '$branch'."
    }
    return (($line -split '\s+')[0]).ToLowerInvariant()
}

function Get-AgentRunState {
    $configuration = Get-HermesConfiguration
    $profile = [string]$configuration.selectedProfile
    $statusPath = Resolve-HermesPath 'data\runtime\status.json'
    $running = $false
    if (Test-Path -LiteralPath $statusPath -PathType Leaf) {
        try {
            $status = Get-Content -Raw -LiteralPath $statusPath | ConvertFrom-Json
            $running = $status.phase -eq 'running'
            if ($status.profile) {
                $profile = [string]$status.profile
            }
        } catch {
            Write-HermesLog -Component update -Level WARN -Message "Could not read runtime status: $($_.Exception.Message)"
        }
    }
    return [pscustomobject]@{
        WasRunning = $running
        Profile = $profile
    }
}

function Get-SourceOverrideContent {
    $path = Resolve-HermesPath 'config\launcher\source-overrides.json'
    if (Test-Path -LiteralPath $path -PathType Leaf) {
        return Get-Content -Raw -LiteralPath $path
    }
    return $null
}

function Set-HermesAgentSourceOverride {
    param(
        [Parameter(Mandatory)]
        [string] $BaseCommit,
        [Parameter(Mandatory)]
        [string] $IntegrationCommit,
        [Parameter(Mandatory)]
        [string] $IntegrationTree
    )

    $record = [ordered]@{
        schemaVersion = 1
        sources = [ordered]@{
            hermesAgent = [ordered]@{
                commit = $BaseCommit.ToLowerInvariant()
                integrationCommit = $IntegrationCommit.ToLowerInvariant()
                integrationTree = $IntegrationTree.ToLowerInvariant()
                updatedAt = (Get-Date).ToUniversalTime().ToString('o')
            }
        }
    }
    Write-HermesAtomicText `
        -Path (Resolve-HermesPath 'config\launcher\source-overrides.json') `
        -Content (($record | ConvertTo-Json -Depth 8) + [Environment]::NewLine) `
        -Backup
}

function Restore-SourceOverride {
    param(
        [AllowNull()]
        [string] $Content
    )

    $path = Resolve-HermesPath 'config\launcher\source-overrides.json'
    if ($null -eq $Content) {
        if (Test-Path -LiteralPath $path) {
            Remove-Item -LiteralPath $path -Force
        }
        return
    }
    Write-HermesAtomicText -Path $path -Content $Content
}

function Assert-ActiveAgentIsReplaceable {
    param(
        [Parameter(Mandatory)]
        [pscustomobject] $Manifest
    )

    $source = Resolve-HermesPath 'source\hermes-agent'
    if (-not (Test-Path -LiteralPath (Join-Path $source '.git') -PathType Container)) {
        throw "Hermes Agent checkout is missing: $source. Run Repair-Hermes-Local.ps1 first."
    }
    $status = Invoke-NativeText -FilePath 'git' -WorkingDirectory $source -ArgumentList @('status', '--porcelain')
    if ($status) {
        throw "Hermes Agent checkout has local changes. Run Repair-Hermes-Local.ps1 before updating, or preserve the changes manually.`n$status"
    }
    $tree = Invoke-NativeText -FilePath 'git' -WorkingDirectory $source -ArgumentList @('rev-parse', 'HEAD^{tree}')
    $expectedTree = [string]$Manifest.sources.hermesAgent.integrationTree
    if ($expectedTree -and $tree -ne $expectedTree) {
        throw "Hermes Agent tree $tree does not match the recorded integration tree $expectedTree. Run Repair-Hermes-Local.ps1 before updating."
    }
}

function New-StagedAgentCandidate {
    param(
        [Parameter(Mandatory)]
        [pscustomobject] $Manifest,
        [Parameter(Mandatory)]
        [string] $Candidate,
        [Parameter(Mandatory)]
        [string] $Stamp
    )

    $stageRoot = Resolve-HermesPath "build\updates\staging\hermes-agent-$Stamp"
    $source = Join-Path $stageRoot 'source'
    $repository = [string]$Manifest.sources.hermesAgent.repository
    $currentBase = [string]$Manifest.sources.hermesAgent.commit
    $integrationBranch = [string]$Manifest.sources.hermesAgent.integrationBranch
    $patchDirectory = Resolve-HermesPath ([string]$Manifest.sources.hermesAgent.patchSeries)
    $patches = @(Get-ChildItem -LiteralPath $patchDirectory -Filter '*.patch' -File | Sort-Object Name)
    if ($patches.Count -eq 0) {
        throw "Hermes Local integration patches are missing from $patchDirectory."
    }

    [System.IO.Directory]::CreateDirectory($stageRoot) | Out-Null
    try {
        Invoke-HermesProcess -FilePath 'git' -ArgumentList @(
            'clone', '--filter=blob:none', '--no-checkout', $repository, $source
        ) -LogComponent update
        Invoke-HermesProcess -FilePath 'git' -ArgumentList @(
            '-C', $source, 'fetch', 'origin', $currentBase, '--depth', '1'
        ) -LogComponent update
        Invoke-HermesProcess -FilePath 'git' -ArgumentList @(
            '-C', $source, 'fetch', 'origin', $Candidate, '--depth', '1'
        ) -LogComponent update
        Invoke-HermesProcess -FilePath 'git' -ArgumentList @(
            '-C', $source, 'checkout', '--detach', $Candidate
        ) -LogComponent update
        Invoke-HermesProcess -FilePath 'git' -ArgumentList @(
            '-C', $source, 'switch', '-c', $integrationBranch
        ) -LogComponent update

        $patchArguments = @(
            '-C', $source, 'am', '--3way', '--committer-date-is-author-date'
        ) + @($patches | ForEach-Object { $_.FullName })
        try {
            Invoke-HermesProcess -FilePath 'git' -ArgumentList $patchArguments -Environment @{
                GIT_COMMITTER_NAME = 'Hermes Local Updater'
                GIT_COMMITTER_EMAIL = 'hermes-local@localhost'
            } -LogComponent update
        } catch {
            Invoke-NativeText -FilePath 'git' -WorkingDirectory $source -ArgumentList @('am', '--abort') -AllowFailure | Out-Null
            throw "The Hermes Local patch series did not apply cleanly to $Candidate. The active installation was not changed. $($_.Exception.Message)"
        }

        $integrationCommit = Invoke-NativeText -FilePath 'git' -WorkingDirectory $source -ArgumentList @('rev-parse', 'HEAD')
        $integrationTree = Invoke-NativeText -FilePath 'git' -WorkingDirectory $source -ArgumentList @('rev-parse', 'HEAD^{tree}')
        $status = Invoke-NativeText -FilePath 'git' -WorkingDirectory $source -ArgumentList @('status', '--porcelain')
        if ($status) {
            throw "The staged Hermes Agent checkout is unexpectedly dirty.`n$status"
        }
        return [pscustomobject]@{
            Root = $stageRoot
            Source = $source
            BaseCommit = $Candidate
            IntegrationCommit = $integrationCommit
            IntegrationTree = $integrationTree
        }
    } catch {
        $failedRoot = Resolve-HermesPath "build\updates\failed\hermes-agent-$Stamp"
        if (Test-Path -LiteralPath $stageRoot) {
            [System.IO.Directory]::CreateDirectory([System.IO.Path]::GetDirectoryName($failedRoot)) | Out-Null
            if (Test-Path -LiteralPath $failedRoot) {
                Remove-Item -LiteralPath $failedRoot -Recurse -Force
            }
            Move-Item -LiteralPath $stageRoot -Destination $failedRoot
        }
        throw
    }
}

function Start-And-TestAgent {
    param(
        [Parameter(Mandatory)]
        [string] $Profile
    )

    Invoke-HermesPowerShellScript -RelativePath 'Start-Hermes-Local.ps1' -Arguments @(
        '-Profile', $Profile, '-NonInteractive'
    )
    Invoke-HermesPowerShellScript -RelativePath 'Test-Hermes-Local.ps1' -Arguments @(
        '-Quick', '-NonInteractive'
    )
}

function Stop-AgentStack {
    Invoke-HermesPowerShellScript -RelativePath 'Stop-Hermes-Local.ps1' -Arguments @('-NonInteractive')
}

function Invoke-AgentApply {
    $manifest = Get-HermesVersionManifest
    Assert-ActiveAgentIsReplaceable -Manifest $manifest
    $candidate = Get-AgentCandidate -Manifest $manifest
    $currentBase = ([string]$manifest.sources.hermesAgent.commit).ToLowerInvariant()
    if ($candidate -eq $currentBase) {
        return [ordered]@{
            component = 'HermesAgent'
            status = 'already-current'
            commit = $currentBase
        }
    }

    if (-not $NonInteractive -and -not $PSCmdlet.ShouldProcess(
        "Hermes Agent $currentBase -> $candidate",
        'Stage, rebuild, promote and health-check the updated agent'
    )) {
        return [ordered]@{ component = 'HermesAgent'; status = 'cancelled' }
    }

    $stamp = (Get-Date).ToUniversalTime().ToString('yyyyMMddTHHmmssZ')
    $staged = New-StagedAgentCandidate -Manifest $manifest -Candidate $candidate -Stamp $stamp
    $runState = Get-AgentRunState
    $activeSource = Resolve-HermesPath 'source\hermes-agent'
    $activeVenv = Resolve-HermesPath 'runtimes\python\hermes'
    $dist = Resolve-HermesPath 'dist'
    $knownGood = Resolve-HermesPath "build\updates\known-good\hermes-agent-$Stamp"
    $knownSource = Join-Path $knownGood 'source'
    $knownVenv = Join-Path $knownGood 'venv'
    $knownDist = Join-Path $knownGood 'dist'
    $historyRoot = Resolve-HermesPath 'build\updates\history'
    $previousOverride = Get-SourceOverrideContent
    $capturedPrevious = $false
    $promoted = $false

    [System.IO.Directory]::CreateDirectory($knownGood) | Out-Null
    [System.IO.Directory]::CreateDirectory($historyRoot) | Out-Null

    try {
        Stop-AgentStack
        Invoke-HermesPowerShellScript -RelativePath 'Backup-Hermes-Local.ps1' -Arguments @(
            '-Name', "pre-hermes-agent-update-$Stamp", '-NonInteractive'
        )

        Move-Item -LiteralPath $activeSource -Destination $knownSource
        $capturedPrevious = $true
        if (Test-Path -LiteralPath $activeVenv) {
            Move-Item -LiteralPath $activeVenv -Destination $knownVenv
        }
        if (Test-Path -LiteralPath $dist) {
            [System.IO.Directory]::CreateDirectory($knownDist) | Out-Null
            Copy-Item -Path (Join-Path $dist '*') -Destination $knownDist -Recurse -Force
        }

        Move-Item -LiteralPath $staged.Source -Destination $activeSource
        Set-HermesAgentSourceOverride `
            -BaseCommit $staged.BaseCommit `
            -IntegrationCommit $staged.IntegrationCommit `
            -IntegrationTree $staged.IntegrationTree
        $promoted = $true

        Invoke-HermesPowerShellScript -RelativePath 'Setup-Hermes-Local.ps1' -Arguments @(
            '-SkipModel', '-SkipLlamaBuild', '-SkipLauncherBuild',
            '-ReinstallDependencies', '-NonInteractive'
        )
        Invoke-HermesPowerShellScript -RelativePath 'Build-Hermes-Launcher.ps1' -Arguments @('-NonInteractive')
        Start-And-TestAgent -Profile $runState.Profile
        if (-not $runState.WasRunning) {
            Stop-AgentStack
        }

        $history = [ordered]@{
            schemaVersion = 1
            component = 'HermesAgent'
            status = 'succeeded'
            appliedAt = (Get-Date).ToUniversalTime().ToString('o')
            previous = [ordered]@{
                baseCommit = $currentBase
                integrationCommit = [string]$manifest.sources.hermesAgent.integrationCommit
                integrationTree = [string]$manifest.sources.hermesAgent.integrationTree
                source = $knownSource
                venv = if (Test-Path -LiteralPath $knownVenv) { $knownVenv } else { $null }
                dist = if (Test-Path -LiteralPath $knownDist) { $knownDist } else { $null }
                sourceOverrideContent = $previousOverride
            }
            current = [ordered]@{
                baseCommit = $staged.BaseCommit
                integrationCommit = $staged.IntegrationCommit
                integrationTree = $staged.IntegrationTree
            }
        }
        $historyPath = Join-Path $historyRoot "$Stamp-hermes-agent.json"
        Write-HermesAtomicText -Path $historyPath -Content (
            ($history | ConvertTo-Json -Depth 12) + [Environment]::NewLine
        )
        if (Test-Path -LiteralPath $staged.Root) {
            Remove-Item -LiteralPath $staged.Root -Recurse -Force
        }
        return $history
    } catch {
        $failure = $_
        if ($promoted -or $capturedPrevious) {
            try { Stop-AgentStack } catch { }
            $failedRoot = Resolve-HermesPath "build\updates\failed\hermes-agent-promoted-$Stamp"
            [System.IO.Directory]::CreateDirectory($failedRoot) | Out-Null
            if (Test-Path -LiteralPath $activeSource) {
                Move-Item -LiteralPath $activeSource -Destination (Join-Path $failedRoot 'source')
            }
            if (Test-Path -LiteralPath $activeVenv) {
                Move-Item -LiteralPath $activeVenv -Destination (Join-Path $failedRoot 'venv')
            }
            if (Test-Path -LiteralPath $knownSource) {
                Move-Item -LiteralPath $knownSource -Destination $activeSource
            }
            if (Test-Path -LiteralPath $knownVenv) {
                Move-Item -LiteralPath $knownVenv -Destination $activeVenv
            }
            if (Test-Path -LiteralPath $knownDist) {
                Get-ChildItem -LiteralPath $dist -Force -ErrorAction SilentlyContinue | Remove-Item -Recurse -Force
                Copy-Item -Path (Join-Path $knownDist '*') -Destination $dist -Recurse -Force
            }
            Restore-SourceOverride -Content $previousOverride
            if ($runState.WasRunning) {
                try { Start-And-TestAgent -Profile $runState.Profile } catch { }
            }
        }
        throw "Hermes Agent update failed and the previous installation was restored. $($failure.Exception.Message)"
    }
}

function Invoke-AgentRollback {
    $historyRoot = Resolve-HermesPath 'build\updates\history'
    $historyFile = Get-ChildItem -LiteralPath $historyRoot -Filter '*-hermes-agent.json' -File -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime -Descending |
        Where-Object {
            try {
                $record = Get-Content -Raw -LiteralPath $_.FullName | ConvertFrom-Json
                $record.status -eq 'succeeded' -and (Test-Path -LiteralPath ([string]$record.previous.source))
            } catch {
                $false
            }
        } |
        Select-Object -First 1
    if (-not $historyFile) {
        throw 'No successful Hermes Agent update with an available known-good snapshot was found.'
    }
    $history = Get-Content -Raw -LiteralPath $historyFile.FullName | ConvertFrom-Json
    if (-not $NonInteractive -and -not $PSCmdlet.ShouldProcess(
        "Hermes Agent $($history.current.baseCommit) -> $($history.previous.baseCommit)",
        'Restore the previous source, environment and launcher build'
    )) {
        return [ordered]@{ component = 'HermesAgent'; status = 'cancelled' }
    }

    $stamp = (Get-Date).ToUniversalTime().ToString('yyyyMMddTHHmmssZ')
    $runState = Get-AgentRunState
    $activeSource = Resolve-HermesPath 'source\hermes-agent'
    $activeVenv = Resolve-HermesPath 'runtimes\python\hermes'
    $dist = Resolve-HermesPath 'dist'
    $quarantine = Resolve-HermesPath "build\updates\failed\hermes-agent-rollback-$Stamp"
    [System.IO.Directory]::CreateDirectory($quarantine) | Out-Null

    Stop-AgentStack
    Invoke-HermesPowerShellScript -RelativePath 'Backup-Hermes-Local.ps1' -Arguments @(
        '-Name', "pre-hermes-agent-rollback-$Stamp", '-NonInteractive'
    )
    if (Test-Path -LiteralPath $activeSource) {
        Move-Item -LiteralPath $activeSource -Destination (Join-Path $quarantine 'source')
    }
    if (Test-Path -LiteralPath $activeVenv) {
        Move-Item -LiteralPath $activeVenv -Destination (Join-Path $quarantine 'venv')
    }
    Move-Item -LiteralPath ([string]$history.previous.source) -Destination $activeSource
    if ($history.previous.venv -and (Test-Path -LiteralPath ([string]$history.previous.venv))) {
        Move-Item -LiteralPath ([string]$history.previous.venv) -Destination $activeVenv
    }
    if ($history.previous.dist -and (Test-Path -LiteralPath ([string]$history.previous.dist))) {
        Get-ChildItem -LiteralPath $dist -Force -ErrorAction SilentlyContinue | Remove-Item -Recurse -Force
        Copy-Item -Path (Join-Path ([string]$history.previous.dist) '*') -Destination $dist -Recurse -Force
    }
    $previousOverride = if ($null -eq $history.previous.sourceOverrideContent) {
        $null
    } else {
        [string]$history.previous.sourceOverrideContent
    }
    Restore-SourceOverride -Content $previousOverride
    Start-And-TestAgent -Profile $runState.Profile
    if (-not $runState.WasRunning) {
        Stop-AgentStack
    }

    $result = [ordered]@{
        component = 'HermesAgent'
        status = 'rolled-back'
        rolledBackAt = (Get-Date).ToUniversalTime().ToString('o')
        restoredBaseCommit = [string]$history.previous.baseCommit
        displacedInstallation = $quarantine
        sourceHistory = $historyFile.FullName
    }
    Write-HermesAtomicText -Path (Join-Path $historyRoot "$Stamp-hermes-agent-rollback.json") -Content (
        ($result | ConvertTo-Json -Depth 8) + [Environment]::NewLine
    )
    return $result
}

try {
    Assert-HermesRoot
    Initialize-HermesLayout
    Set-HermesProcessEnvironment
    if ($PSVersionTable.PSVersion.Major -lt 7) {
        throw 'PowerShell 7 or newer is required. Run this script with pwsh.exe.'
    }

    $result = switch ($Mode) {
        'Check' {
            $manifest = Get-HermesVersionManifest
            $candidate = Get-AgentCandidate -Manifest $manifest
            [ordered]@{
                component = 'HermesAgent'
                current = [string]$manifest.sources.hermesAgent.commit
                candidate = $candidate
                updateAvailable = $candidate -ne [string]$manifest.sources.hermesAgent.commit
            }
        }
        'Apply' { Invoke-AgentApply }
        'Rollback' { Invoke-AgentRollback }
    }
    $result | ConvertTo-Json -Depth 12
    exit 0
} catch {
    Write-HermesLog -Component update -Level ERROR -Message $_.Exception.ToString()
    Write-Host "Hermes Agent update $Mode failed: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
}
