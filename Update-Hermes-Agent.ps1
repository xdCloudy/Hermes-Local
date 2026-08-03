[CmdletBinding(SupportsShouldProcess, ConfirmImpact = 'High')]
param(
    [ValidateSet('Check', 'Compatibility', 'Apply', 'Rollback')]
    [string] $Mode = 'Apply',

    [ValidatePattern('^[0-9a-fA-F]{40}$')]
    [string] $TargetCommit,

    [ValidatePattern('^[A-Za-z0-9._/-]+$')]
    [string] $TargetBranch,

    [switch] $NonInteractive,

    [switch] $SkipCompatibility
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'scripts\Common-Hermes.psm1') -Force
Import-Module (Join-Path $PSScriptRoot 'scripts\Hermes-Configuration.psm1') -Force

$HermesAgentNpmVersion = '12.0.0'

$script:AgentUpdateStage = 'check'
$script:AgentUpdateRollbackStatus = 'not-required'

function Write-HermesAgentUpdateStage {
    param(
        [Parameter(Mandatory)]
        [ValidateSet(
            'check', 'compatibility', 'prepare', 'patch', 'dependency',
            'schema', 'backup', 'promote', 'build', 'test', 'validate',
            'rollback', 'complete'
        )]
        [string] $Stage,
        [Parameter(Mandatory)]
        [string] $Message
    )

    $script:AgentUpdateStage = $Stage
    Write-Output "::hermes-update-stage::$Stage::$Message"
}

function Get-HermesAgentUpdateFailureCode {
    param([Parameter(Mandatory)][string] $Stage)

    switch ($Stage) {
        'check' { 'update-check-failed' }
        'compatibility' { 'compatibility-failed' }
        'prepare' { 'prepare-failed' }
        'patch' { 'patch-conflict' }
        'dependency' { 'dependency-failed' }
        'schema' { 'schema-failed' }
        'backup' { 'backup-failed' }
        'promote' { 'promotion-failed' }
        'build' { 'build-failed' }
        'test' { 'test-failed' }
        'validate' { 'validation-failed' }
        'rollback' { 'rollback-failed' }
        default { 'update-operation-failed' }
    }
}

function Write-HermesAgentUpdateResult {
    param([Parameter(Mandatory)][object] $Record)

    $json = $Record | ConvertTo-Json -Depth 24 -Compress
    Write-Output "::hermes-update-result::$json"
}

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

function Get-UnmergedAgentPaths {
    param(
        [Parameter(Mandatory)]
        [string] $Source
    )

    $text = Invoke-NativeText `
        -FilePath 'git' `
        -WorkingDirectory $Source `
        -ArgumentList @('diff', '--name-only', '--diff-filter=U') `
        -AllowFailure
    if (-not $text) {
        return @()
    }
    return @(
        $text -split '\r?\n' |
            ForEach-Object { $_.Trim().Replace('\\', '/') } |
            Where-Object { $_ }
    )
}

function Resolve-AgentNpmLockConflict {
    param(
        [Parameter(Mandatory)]
        [string] $Source,
        [Parameter(Mandatory)]
        [string[]] $Conflicts
    )

    $normalized = @(
        $Conflicts |
            ForEach-Object { $_.Trim().Replace('\\', '/') } |
            Where-Object { $_ }
    )
    if ($normalized.Count -ne 1 -or $normalized[0] -ne 'package-lock.json') {
        return $false
    }
    if (-not (Test-Path -LiteralPath (Join-Path $Source 'package.json') -PathType Leaf) -or
        -not (Test-Path -LiteralPath (Join-Path $Source 'package-lock.json') -PathType Leaf)) {
        return $false
    }

    Write-HermesLog `
        -Component update `
        -Level WARN `
        -Message 'Regenerating package-lock.json after an upstream-only lockfile conflict.'

    Invoke-HermesProcess -FilePath 'git' -ArgumentList @(
        '-C', $Source, 'checkout', '--ours', '--', 'package-lock.json'
    ) -LogComponent update
    Invoke-HermesProcess -FilePath 'npx.cmd' -ArgumentList @(
        '--yes', "npm@$HermesAgentNpmVersion",
        '--prefix', $Source,
        'install',
        '--package-lock-only',
        '--ignore-scripts',
        '--no-audit',
        '--fund=false'
    ) -LogComponent update

    $unstagedText = Invoke-NativeText `
        -FilePath 'git' `
        -WorkingDirectory $Source `
        -ArgumentList @('diff', '--name-only')
    $unexpected = @(
        $unstagedText -split '\r?\n' |
            ForEach-Object { $_.Trim().Replace('\\', '/') } |
            Where-Object { $_ -and $_ -ne 'package-lock.json' }
    )
    if ($unexpected.Count -gt 0) {
        throw "npm lockfile regeneration modified unexpected files: $($unexpected -join ', ')"
    }

    Invoke-HermesProcess -FilePath 'git' -ArgumentList @(
        '-C', $Source, 'add', '--', 'package-lock.json'
    ) -LogComponent update
    $remaining = @(Get-UnmergedAgentPaths -Source $Source)
    if ($remaining.Count -gt 0) {
        throw "Lockfile regeneration left unresolved paths: $($remaining -join ', ')"
    }

    Invoke-HermesProcess -FilePath 'git' -ArgumentList @(
        '-C', $Source, 'am', '--continue'
    ) -Environment @{
        GIT_COMMITTER_NAME = 'Hermes Local Updater'
        GIT_COMMITTER_EMAIL = 'hermes-local@localhost'
    } -LogComponent update
    return $true
}

function Invoke-AgentPatchSeries {
    param(
        [Parameter(Mandatory)]
        [string] $Source,
        [Parameter(Mandatory)]
        [System.IO.FileInfo[]] $Patches,
        [Parameter(Mandatory)]
        [string] $Candidate
    )

    foreach ($patch in $Patches) {
        try {
            Invoke-HermesProcess -FilePath 'git' -ArgumentList @(
                '-C', $Source,
                'am', '--3way', '--committer-date-is-author-date',
                $patch.FullName
            ) -Environment @{
                GIT_COMMITTER_NAME = 'Hermes Local Updater'
                GIT_COMMITTER_EMAIL = 'hermes-local@localhost'
            } -LogComponent update
        } catch {
            $patchError = $_
            $conflicts = @(Get-UnmergedAgentPaths -Source $Source)
            try {
                if (Resolve-AgentNpmLockConflict -Source $Source -Conflicts $conflicts) {
                    continue
                }
            } catch {
                throw "Patch $($patch.Name) hit a package-lock.json conflict and deterministic regeneration failed. The active installation was not changed. $($_.Exception.Message)"
            }
            throw "Patch $($patch.Name) did not apply cleanly to $Candidate. Conflicted files: $($conflicts -join ', '). The active installation was not changed. $($patchError.Exception.Message)"
        }
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

    Write-HermesAgentUpdateStage -Stage prepare -Message "Preparing isolated candidate $Candidate."
    [System.IO.Directory]::CreateDirectory($stageRoot) | Out-Null
    try {
        # The mail patches contain abbreviated preimage blob IDs. A blob-filtered
        # clone cannot resolve those IDs while git am --3way constructs its fake
        # ancestor, so staging must retain the complete upstream object database.
        Invoke-HermesProcess -FilePath 'git' -ArgumentList @(
            'clone', '--no-checkout', $repository, $source
        ) -LogComponent update
        Invoke-HermesProcess -FilePath 'git' -ArgumentList @(
            '-C', $source, 'fetch', 'origin', $currentBase
        ) -LogComponent update
        Invoke-HermesProcess -FilePath 'git' -ArgumentList @(
            '-C', $source, 'fetch', 'origin', $Candidate
        ) -LogComponent update
        Invoke-HermesProcess -FilePath 'git' -ArgumentList @(
            '-C', $source, 'checkout', '--detach', $Candidate
        ) -LogComponent update
        Invoke-HermesProcess -FilePath 'git' -ArgumentList @(
            '-C', $source, 'switch', '-c', $integrationBranch
        ) -LogComponent update

        Write-HermesAgentUpdateStage -Stage patch -Message "Replaying $($patches.Count) Hermes Local integration patches."
        try {
            Invoke-AgentPatchSeries `
                -Source $source `
                -Patches $patches `
                -Candidate $Candidate
        } catch {
            # Keep the failed am session intact. The outer catch quarantines this
            # staging tree under build\updates\failed for direct conflict review.
            throw "The Hermes Local patch series did not apply cleanly to $Candidate. The active installation was not changed. $($_.Exception.Message)"
        }

        Write-HermesAgentUpdateStage -Stage compatibility -Message 'Verifying the reconstructed integration tree.'
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

function New-CompatibleAgentCandidate {
    param(
        [Parameter(Mandatory)][pscustomobject] $Manifest,
        [Parameter(Mandatory)][string] $Candidate,
        [Parameter(Mandatory)][string] $Stamp
    )

    $staged = New-StagedAgentCandidate -Manifest $Manifest -Candidate $Candidate -Stamp $Stamp
    try {
        Write-HermesAgentUpdateStage -Stage dependency -Message 'Installing candidate Node and Python dependencies in isolation.'
        Invoke-HermesProcess -FilePath 'npx.cmd' -ArgumentList @(
            '--yes', "npm@$HermesAgentNpmVersion",
            '--prefix', $staged.Source,
            'ci', '--no-audit', '--fund=false'
        ) -LogComponent update

        $uvCommand = Get-Command 'uv.exe' -ErrorAction SilentlyContinue
        if (-not $uvCommand) {
            $uvCommand = Get-Command 'uv' -ErrorAction SilentlyContinue
        }
        if (-not $uvCommand) {
            throw 'uv is required to validate Hermes Agent Python dependencies.'
        }
        $uvArguments = @('sync', '--extra', 'all', '--extra', 'dev')
        if (Test-Path -LiteralPath (Join-Path $staged.Source 'uv.lock') -PathType Leaf) {
            $uvArguments += '--frozen'
        }
        Invoke-HermesProcess `
            -FilePath $uvCommand.Source `
            -ArgumentList $uvArguments `
            -WorkingDirectory $staged.Source `
            -LogComponent update

        Write-HermesAgentUpdateStage -Stage schema -Message 'Validating manifests, TypeScript contracts and Python modules.'
        foreach ($required in @(
            'package.json', 'apps\desktop\package.json', 'pyproject.toml',
            'apps\desktop\electron\hermes-local-control.ts'
        )) {
            if (-not (Test-Path -LiteralPath (Join-Path $staged.Source $required) -PathType Leaf)) {
                throw "Candidate schema is missing required file '$required'."
            }
        }
        Invoke-HermesProcess -FilePath 'npx.cmd' -ArgumentList @(
            '--yes', "npm@$HermesAgentNpmVersion",
            '--prefix', $staged.Source,
            'run', 'typecheck', '--workspace', 'apps/desktop'
        ) -LogComponent update
        $candidatePython = Join-Path $staged.Source '.venv\Scripts\python.exe'
        if (-not (Test-Path -LiteralPath $candidatePython -PathType Leaf)) {
            throw "Candidate Python environment was not created: $candidatePython"
        }
        Invoke-HermesProcess `
            -FilePath $candidatePython `
            -ArgumentList @('-m', 'compileall', '-q', 'hermes_cli', 'tui_gateway') `
            -WorkingDirectory $staged.Source `
            -LogComponent update

        Write-HermesAgentUpdateStage -Stage test -Message 'Running focused packaged Desktop and backend regression tests.'
        Invoke-HermesProcess -FilePath 'npx.cmd' -ArgumentList @(
            '--yes', "npm@$HermesAgentNpmVersion",
            '--prefix', $staged.Source,
            'exec', '--', 'vitest', 'run', '--project', 'electron',
            'electron/hermes-local-control.test.ts',
            'electron/hermes-local-update.test.ts'
        ) -WorkingDirectory (Join-Path $staged.Source 'apps\desktop') -LogComponent update
        Invoke-HermesProcess `
            -FilePath $candidatePython `
            -ArgumentList @(
                '-m', 'pytest',
                'tests/hermes_state/test_session_md_export.py',
                'tests/tui_gateway/test_projects_rpc.py',
                '-q'
            ) `
            -WorkingDirectory $staged.Source `
            -LogComponent update

        Write-HermesAgentUpdateStage -Stage build -Message 'Building the candidate Desktop workspace.'
        Invoke-HermesProcess -FilePath 'npx.cmd' -ArgumentList @(
            '--yes', "npm@$HermesAgentNpmVersion",
            '--prefix', $staged.Source,
            'run', 'build', '--workspace', 'apps/desktop'
        ) -LogComponent update
        return $staged
    } catch {
        $failedRoot = Resolve-HermesPath "build\updates\failed\hermes-agent-compatibility-$Stamp"
        if (Test-Path -LiteralPath $staged.Root) {
            [System.IO.Directory]::CreateDirectory([System.IO.Path]::GetDirectoryName($failedRoot)) | Out-Null
            if (Test-Path -LiteralPath $failedRoot) {
                Remove-Item -LiteralPath $failedRoot -Recurse -Force
            }
            Move-Item -LiteralPath $staged.Root -Destination $failedRoot
        }
        throw
    }
}

function Invoke-AgentCompatibility {
    $manifest = Get-HermesVersionManifest
    Write-HermesAgentUpdateStage -Stage compatibility -Message 'Validating the active checkout before candidate testing.'
    Assert-ActiveAgentIsReplaceable -Manifest $manifest
    Write-HermesAgentUpdateStage -Stage check -Message 'Resolving the requested upstream candidate.'
    $candidate = Get-AgentCandidate -Manifest $manifest
    $currentBase = ([string]$manifest.sources.hermesAgent.commit).ToLowerInvariant()
    $patchDirectory = Resolve-HermesPath ([string]$manifest.sources.hermesAgent.patchSeries)
    $patchCount = @(Get-ChildItem -LiteralPath $patchDirectory -Filter '*.patch' -File).Count

    if ($candidate -eq $currentBase) {
        return [ordered]@{
            component = 'HermesAgent'
            status = 'already-current'
            compatible = $true
            current = $currentBase
            candidate = $candidate
            patchCount = $patchCount
        }
    }

    $stamp = (Get-Date).ToUniversalTime().ToString('yyyyMMddTHHmmssZ')
    $staged = New-CompatibleAgentCandidate -Manifest $manifest -Candidate $candidate -Stamp $stamp
    $result = [ordered]@{
        component = 'HermesAgent'
        status = 'compatible'
        compatible = $true
        current = $currentBase
        candidate = $candidate
        patchCount = $patchCount
        integrationCommit = $staged.IntegrationCommit
        integrationTree = $staged.IntegrationTree
    }
    if (Test-Path -LiteralPath $staged.Root) {
        Remove-Item -LiteralPath $staged.Root -Recurse -Force
    }
    return $result
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
    Write-HermesAgentUpdateStage -Stage compatibility -Message 'Validating the active checkout.'
    Assert-ActiveAgentIsReplaceable -Manifest $manifest
    Write-HermesAgentUpdateStage -Stage check -Message 'Resolving the requested upstream candidate.'
    $candidate = Get-AgentCandidate -Manifest $manifest
    $currentBase = ([string]$manifest.sources.hermesAgent.commit).ToLowerInvariant()
    if ($candidate -eq $currentBase) {
        Write-HermesAgentUpdateStage -Stage complete -Message 'Hermes Agent is already current.'
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
    $staged = if ($SkipCompatibility) {
        New-StagedAgentCandidate -Manifest $manifest -Candidate $candidate -Stamp $stamp
    } else {
        New-CompatibleAgentCandidate -Manifest $manifest -Candidate $candidate -Stamp $stamp
    }
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
        Write-HermesAgentUpdateStage -Stage backup -Message 'Stopping services and capturing the known-good installation.'
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

        Write-HermesAgentUpdateStage -Stage promote -Message 'Promoting the verified candidate into the active installation.'
        Move-Item -LiteralPath $staged.Source -Destination $activeSource
        Set-HermesAgentSourceOverride `
            -BaseCommit $staged.BaseCommit `
            -IntegrationCommit $staged.IntegrationCommit `
            -IntegrationTree $staged.IntegrationTree
        $promoted = $true

        Write-HermesAgentUpdateStage -Stage dependency -Message 'Reinstalling active Hermes Agent dependencies.'
        Invoke-HermesPowerShellScript -RelativePath 'Setup-Hermes-Local.ps1' -Arguments @(
            '-SkipModel', '-SkipLlamaBuild', '-SkipLauncherBuild',
            '-ReinstallDependencies', '-NonInteractive'
        )
        Write-HermesAgentUpdateStage -Stage build -Message 'Rebuilding the packaged Hermes Launcher.'
        Invoke-HermesPowerShellScript -RelativePath 'Build-Hermes-Launcher.ps1' -Arguments @('-NonInteractive')
        Write-HermesAgentUpdateStage -Stage test -Message 'Starting and health-checking the promoted backend.'
        Start-And-TestAgent -Profile $runState.Profile
        if (-not $runState.WasRunning) {
            Stop-AgentStack
        }

        Write-HermesAgentUpdateStage -Stage validate -Message 'Recording the verified integration and recovery history.'
        $history = [ordered]@{
            schemaVersion = 2
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
        $history.reportPath = $historyPath
        Write-HermesAtomicText -Path $historyPath -Content (
            ($history | ConvertTo-Json -Depth 12) + [Environment]::NewLine
        )
        if (Test-Path -LiteralPath $staged.Root) {
            Remove-Item -LiteralPath $staged.Root -Recurse -Force
        }
        Write-HermesAgentUpdateStage -Stage complete -Message 'Hermes Agent update completed and passed health checks.'
        return $history
    } catch {
        $failure = $_
        if ($promoted -or $capturedPrevious) {
            $script:AgentUpdateRollbackStatus = 'running'
            Write-HermesAgentUpdateStage -Stage rollback -Message 'Restoring the previous known-good Hermes Agent installation.'
            try {
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
                    Start-And-TestAgent -Profile $runState.Profile
                }
                $script:AgentUpdateRollbackStatus = 'succeeded'
            } catch {
                $script:AgentUpdateRollbackStatus = 'failed'
                throw "Hermes Agent update failed and rollback also failed. Original failure: $($failure.Exception.Message). Rollback failure: $($_.Exception.Message)"
            }
        }
        throw "Hermes Agent update failed. Rollback: $script:AgentUpdateRollbackStatus. $($failure.Exception.Message)"
    }
}

function Invoke-AgentRollback {
    Write-HermesAgentUpdateStage -Stage check -Message 'Locating the latest known-good Hermes Agent snapshot.'
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

    Write-HermesAgentUpdateStage -Stage backup -Message 'Backing up the current installation before rollback.'
    Stop-AgentStack
    Invoke-HermesPowerShellScript -RelativePath 'Backup-Hermes-Local.ps1' -Arguments @(
        '-Name', "pre-hermes-agent-rollback-$Stamp", '-NonInteractive'
    )
    Write-HermesAgentUpdateStage -Stage rollback -Message 'Restoring the previous source, environment and launcher build.'
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
    Write-HermesAgentUpdateStage -Stage validate -Message 'Health-checking the restored backend.'
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
    Write-HermesAgentUpdateStage -Stage complete -Message 'Hermes Agent rollback completed.'
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
            Write-HermesAgentUpdateStage -Stage check -Message 'Checking the configured Hermes Agent upstream target.'
            $manifest = Get-HermesVersionManifest
            $candidate = Get-AgentCandidate -Manifest $manifest
            $patchDirectory = Resolve-HermesPath ([string]$manifest.sources.hermesAgent.patchSeries)
            [ordered]@{
                component = 'HermesAgent'
                status = 'checked'
                current = [string]$manifest.sources.hermesAgent.commit
                currentIntegrationCommit = [string]$manifest.sources.hermesAgent.integrationCommit
                currentIntegrationTree = [string]$manifest.sources.hermesAgent.integrationTree
                candidate = $candidate
                targetBranch = if ($TargetBranch) { $TargetBranch } else { [string]$manifest.sources.hermesAgent.branch }
                patchCount = @(Get-ChildItem -LiteralPath $patchDirectory -Filter '*.patch' -File).Count
                updateAvailable = $candidate -ne [string]$manifest.sources.hermesAgent.commit
            }
        }
        'Compatibility' { Invoke-AgentCompatibility }
        'Apply' { Invoke-AgentApply }
        'Rollback' { Invoke-AgentRollback }
    }
    $result | ConvertTo-Json -Depth 24
    Write-HermesAgentUpdateResult -Record $result
    exit 0
} catch {
    $failure = [ordered]@{
        component = 'HermesAgent'
        status = 'failed'
        mode = $Mode
        stage = $script:AgentUpdateStage
        activePreserved = $script:AgentUpdateStage -notin @('promote', 'dependency', 'build', 'test', 'validate', 'rollback') -or
            $script:AgentUpdateRollbackStatus -eq 'succeeded'
        rollback = [ordered]@{
            status = $script:AgentUpdateRollbackStatus
        }
        failure = [ordered]@{
            code = Get-HermesAgentUpdateFailureCode -Stage $script:AgentUpdateStage
            message = $_.Exception.Message
            type = $_.Exception.GetType().FullName
        }
    }
    try {
        Write-HermesLog -Component update -Level ERROR -Message $_.Exception.ToString()
    } catch { }
    $failure | ConvertTo-Json -Depth 24
    Write-HermesAgentUpdateResult -Record $failure
    exit 1
}
