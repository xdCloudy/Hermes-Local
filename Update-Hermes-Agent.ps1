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
    Write-Host "::hermes-update-stage::$Stage::$Message"
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

function Invoke-NativeResult {
    param(
        [Parameter(Mandatory)]
        [string] $FilePath,
        [Parameter(Mandatory)]
        [AllowEmptyCollection()]
        [string[]] $ArgumentList,
        [string] $WorkingDirectory = (Get-HermesRoot),
        [hashtable] $Environment = @{}
    )

    $previousEnvironment = @{}
    foreach ($name in $Environment.Keys) {
        $previousEnvironment[$name] = [Environment]::GetEnvironmentVariable(
            [string]$name,
            [EnvironmentVariableTarget]::Process
        )
        [Environment]::SetEnvironmentVariable(
            [string]$name,
            [string]$Environment[$name],
            [EnvironmentVariableTarget]::Process
        )
    }

    $nativePreference = Get-Variable `
        -Name PSNativeCommandUseErrorActionPreference `
        -Scope Local `
        -ErrorAction SilentlyContinue
    $previousNativePreference = if ($nativePreference) {
        [bool]$nativePreference.Value
    } else {
        $null
    }

    $output = @()
    $exitCode = -1
    Push-Location $WorkingDirectory
    try {
        $PSNativeCommandUseErrorActionPreference = $false
        $output = @(& $FilePath @ArgumentList 2>&1)
        $exitCode = [int]$LASTEXITCODE
    } finally {
        Pop-Location
        if ($null -eq $previousNativePreference) {
            Remove-Variable `
                -Name PSNativeCommandUseErrorActionPreference `
                -Scope Local `
                -ErrorAction SilentlyContinue
        } else {
            $PSNativeCommandUseErrorActionPreference = $previousNativePreference
        }
        foreach ($name in $Environment.Keys) {
            [Environment]::SetEnvironmentVariable(
                [string]$name,
                $previousEnvironment[$name],
                [EnvironmentVariableTarget]::Process
            )
        }
    }

    $lines = @($output | ForEach-Object { [string]$_ })
    return [pscustomobject]@{
        ExitCode = $exitCode
        Lines = $lines
        Text = (($lines -join [Environment]::NewLine).Trim())
    }
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

function Get-AgentPatchGeneratedLockPaths {
    param(
        [Parameter(Mandatory)]
        [System.IO.FileInfo] $Patch
    )

    return @(
        Get-Content -LiteralPath $Patch.FullName |
            ForEach-Object {
                if ($_ -match '^diff --git a/(package-lock\.json|uv\.lock) b/') {
                    $Matches[1]
                }
            } |
            Where-Object { $_ } |
            Sort-Object -Unique
    )
}

function Get-AgentPatchFailurePaths {
    param(
        [Parameter(Mandatory)]
        [AllowEmptyCollection()]
        [string[]] $Lines
    )

    return @(
        $Lines |
            ForEach-Object {
                if ($_ -match '^error: patch failed: (.+):\d+$') {
                    $Matches[1]
                } elseif ($_ -match '^error: (.+): patch does not apply$') {
                    $Matches[1]
                }
            } |
            ForEach-Object { $_.Trim().Replace('\\', '/') } |
            Where-Object { $_ } |
            Sort-Object -Unique
    )
}

function Test-AgentPathsAreGeneratedLocks {
    param(
        [Parameter(Mandatory)]
        [AllowEmptyCollection()]
        [string[]] $Paths
    )

    $normalized = @(
        $Paths |
            ForEach-Object { $_.Trim().Replace('\\', '/') } |
            Where-Object { $_ } |
            Sort-Object -Unique
    )
    if ($normalized.Count -eq 0) {
        return $false
    }

    return @(
        $normalized |
            Where-Object { $_ -notin @('package-lock.json', 'uv.lock') }
    ).Count -eq 0
}

function Invoke-AgentGitAmAttempt {
    param(
        [Parameter(Mandatory)]
        [string] $Source,
        [Parameter(Mandatory)]
        [System.IO.FileInfo] $Patch,
        [switch] $ThreeWay,
        [AllowEmptyCollection()]
        [string[]] $ExcludePaths = @()
    )

    $arguments = @('-C', $Source, 'am')
    if ($ThreeWay) {
        $arguments += '--3way'
    }
    $arguments += '--committer-date-is-author-date'
    foreach ($path in $ExcludePaths) {
        $arguments += "--exclude=$path"
    }
    $arguments += $Patch.FullName

    return Invoke-NativeResult `
        -FilePath 'git' `
        -ArgumentList $arguments `
        -Environment @{
            GIT_COMMITTER_NAME = 'Hermes Local Updater'
            GIT_COMMITTER_EMAIL = 'hermes-local@localhost'
        }
}

function Stop-AgentPatchApplication {
    param(
        [Parameter(Mandatory)]
        [string] $Source
    )

    $result = Invoke-NativeResult `
        -FilePath 'git' `
        -ArgumentList @('-C', $Source, 'am', '--abort')
    if ($result.ExitCode -ne 0) {
        throw "Could not abort the failed git am session.`n$($result.Text)"
    }
}

function Invoke-AgentGeneratedLockFallback {
    param(
        [Parameter(Mandatory)]
        [string] $Source,
        [Parameter(Mandatory)]
        [System.IO.FileInfo] $Patch
    )

    $lockPaths = @(Get-AgentPatchGeneratedLockPaths -Patch $Patch)
    if ($lockPaths.Count -eq 0) {
        return $false
    }

    Stop-AgentPatchApplication -Source $Source
    Write-HermesLog `
        -Component update `
        -Level WARN `
        -Message "Reapplying $($Patch.Name) without generated lockfiles, then regenerating: $($lockPaths -join ', ')."

    $reducedAttempt = Invoke-AgentGitAmAttempt `
        -Source $Source `
        -Patch $Patch `
        -ExcludePaths $lockPaths
    if ($reducedAttempt.ExitCode -ne 0) {
        throw "Patch source changes still failed after generated lockfiles were excluded.`n$($reducedAttempt.Text)"
    }

    if ($lockPaths -contains 'package-lock.json') {
        foreach ($required in @('package.json', 'package-lock.json')) {
            if (-not (Test-Path -LiteralPath (Join-Path $Source $required) -PathType Leaf)) {
                throw "Cannot regenerate package-lock.json because '$required' is missing."
            }
        }
        Invoke-HermesProcess -FilePath 'npx.cmd' -ArgumentList @(
            '--yes', "npm@$HermesAgentNpmVersion",
            '--prefix', $Source,
            'install',
            '--package-lock-only',
            '--ignore-scripts',
            '--no-audit',
            '--fund=false'
        ) -LogComponent update
    }

    if ($lockPaths -contains 'uv.lock') {
        foreach ($required in @('pyproject.toml', 'uv.lock')) {
            if (-not (Test-Path -LiteralPath (Join-Path $Source $required) -PathType Leaf)) {
                throw "Cannot regenerate uv.lock because '$required' is missing."
            }
        }
        $uvCommand = Get-Command 'uv.exe' -ErrorAction SilentlyContinue
        if (-not $uvCommand) {
            $uvCommand = Get-Command 'uv' -ErrorAction SilentlyContinue
        }
        if (-not $uvCommand) {
            throw 'uv is required to regenerate uv.lock during patch replay.'
        }
        Invoke-HermesProcess `
            -FilePath $uvCommand.Source `
            -ArgumentList @('lock') `
            -WorkingDirectory $Source `
            -LogComponent update
    }

    $statusText = Invoke-NativeText `
        -FilePath 'git' `
        -WorkingDirectory $Source `
        -ArgumentList @('status', '--porcelain', '--untracked-files=all')
    $changedPaths = @(
        $statusText -split '\r?\n' |
            Where-Object { $_ } |
            ForEach-Object {
                if ($_.Length -lt 4) {
                    throw "Unexpected git status entry: $_"
                }
                $_.Substring(3).Trim('"').Replace('\\', '/')
            } |
            Sort-Object -Unique
    )
    $unexpected = @(
        $changedPaths |
            Where-Object { $_ -notin $lockPaths }
    )
    if ($unexpected.Count -gt 0) {
        throw "Generated lockfile regeneration modified unexpected files: $($unexpected -join ', ')"
    }

    if ($changedPaths.Count -gt 0) {
        $addArguments = @('-C', $Source, 'add', '--') + $changedPaths
        Invoke-HermesProcess `
            -FilePath 'git' `
            -ArgumentList $addArguments `
            -LogComponent update

        $authorDate = Invoke-NativeText `
            -FilePath 'git' `
            -WorkingDirectory $Source `
            -ArgumentList @('show', '-s', '--format=%aI', 'HEAD')
        Invoke-HermesProcess -FilePath 'git' -ArgumentList @(
            '-C', $Source,
            'commit', '--amend', '--no-edit', '--no-verify'
        ) -Environment @{
            GIT_COMMITTER_NAME = 'Hermes Local Updater'
            GIT_COMMITTER_EMAIL = 'hermes-local@localhost'
            GIT_COMMITTER_DATE = $authorDate
        } -LogComponent update
    }

    $remainingStatus = Invoke-NativeText `
        -FilePath 'git' `
        -WorkingDirectory $Source `
        -ArgumentList @('status', '--porcelain', '--untracked-files=all')
    if ($remainingStatus) {
        throw "Generated lockfile fallback left the candidate dirty.`n$remainingStatus"
    }
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
        $threeWay = Invoke-AgentGitAmAttempt `
            -Source $Source `
            -Patch $patch `
            -ThreeWay
        if ($threeWay.ExitCode -eq 0) {
            continue
        }

        $conflicts = @(Get-UnmergedAgentPaths -Source $Source)
        if (Test-AgentPathsAreGeneratedLocks -Paths $conflicts) {
            try {
                if (Invoke-AgentGeneratedLockFallback -Source $Source -Patch $patch) {
                    continue
                }
            } catch {
                throw "Patch $($patch.Name) hit a generated-lock conflict and deterministic regeneration failed. The active installation was not changed. $($_.Exception.Message)"
            }
        }

        $fakeAncestorFailure = $threeWay.Text -match (
            'could not build fake ancestor|' +
            'sha1 information is lacking or useless'
        )
        if ($conflicts.Count -eq 0 -and $fakeAncestorFailure) {
            Stop-AgentPatchApplication -Source $Source
            Write-HermesLog `
                -Component update `
                -Level WARN `
                -Message "Retrying $($patch.Name) without --3way because Git could not construct its fake ancestor."

            $direct = Invoke-AgentGitAmAttempt `
                -Source $Source `
                -Patch $patch
            if ($direct.ExitCode -eq 0) {
                continue
            }

            $directConflicts = @(Get-UnmergedAgentPaths -Source $Source)
            $failedPaths = @(Get-AgentPatchFailurePaths -Lines $direct.Lines)
            $generatedLockFailure =
                (Test-AgentPathsAreGeneratedLocks -Paths $directConflicts) -or
                (Test-AgentPathsAreGeneratedLocks -Paths $failedPaths)
            if ($generatedLockFailure) {
                try {
                    if (Invoke-AgentGeneratedLockFallback -Source $Source -Patch $patch) {
                        continue
                    }
                } catch {
                    throw "Patch $($patch.Name) required generated-lock regeneration after direct replay, but regeneration failed. The active installation was not changed. $($_.Exception.Message)"
                }
            }

            $reportedPaths = @($directConflicts + $failedPaths | Sort-Object -Unique)
            throw "Patch $($patch.Name) failed both three-way and direct replay against $Candidate. Conflicted or rejected files: $($reportedPaths -join ', '). The active installation was not changed.`n$($direct.Text)"
        }

        throw "Patch $($patch.Name) did not apply cleanly to $Candidate. Conflicted files: $($conflicts -join ', '). The active installation was not changed.`n$($threeWay.Text)"
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
        # The staging path may exceed the legacy Windows MAX_PATH limit once
        # upstream documentation paths are appended. Enable Git for Windows'
        # long-path handling before the first checkout writes the worktree.
        Invoke-HermesProcess -FilePath 'git' -ArgumentList @(
            '-C', $source, 'config', 'core.longpaths', 'true'
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

    # An explicit target is also a request to revalidate that exact revision.
    # This provides a deterministic way to exercise patch replay after updater
    # changes even when the manifest already points at the requested commit.
    if ($candidate -eq $currentBase -and -not $TargetCommit) {
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
