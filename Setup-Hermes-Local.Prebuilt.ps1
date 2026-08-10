[CmdletBinding()]
param(
    [switch] $SkipModel,
    [switch] $SkipLlamaBuild,
    [switch] $SkipHermesDependencies,
    [switch] $SkipLauncherBuild,
    [switch] $ReinstallDependencies,
    [switch] $NonInteractive
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'scripts\Common-Hermes.psm1') -Force
Import-Module (Join-Path $PSScriptRoot 'scripts\Hermes-Configuration.psm1') -Force
Import-Module (Join-Path $PSScriptRoot 'scripts\Hermes-RuntimeManager.psm1') -Force

function Invoke-IsolatedPowerShellScript {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $ScriptPath,
        [string[]] $ArgumentList = @(),
        [Parameter(Mandatory)][string] $Description
    )

    $hostExecutable = (Get-Process -Id $PID -ErrorAction Stop).Path
    if ([string]::IsNullOrWhiteSpace($hostExecutable)) {
        throw 'Unable to resolve the current PowerShell host executable.'
    }
    & $hostExecutable @(
        '-NoLogo', '-NoProfile', '-NonInteractive',
        '-ExecutionPolicy', 'Bypass', '-File', $ScriptPath
    ) @ArgumentList
    if ($LASTEXITCODE -ne 0) {
        throw "$Description failed with exit code $LASTEXITCODE."
    }
}

function Require-Command {
    param(
        [Parameter(Mandatory)][string] $Name,
        [Parameter(Mandatory)][string] $WingetId
    )

    if (Get-Command $Name -ErrorAction SilentlyContinue) {
        return
    }
    if (-not (Get-Command winget -ErrorAction SilentlyContinue)) {
        throw "$Name is missing and winget is unavailable."
    }
    Write-HermesLog -Component setup -Level WARN -Message "Installing required package $WingetId because $Name is missing."
    Invoke-HermesProcess -FilePath 'winget' -ArgumentList @(
        'install', '--id', $WingetId, '--exact', '--source', 'winget',
        '--accept-package-agreements', '--accept-source-agreements',
        '--silent', '--disable-interactivity'
    ) -LogComponent setup
}

function Initialize-SourceCheckout {
    param(
        [Parameter(Mandatory)][string] $Path,
        [Parameter(Mandatory)][string] $Repository,
        [Parameter(Mandatory)][string] $Branch,
        [Parameter(Mandatory)][string] $Commit,
        [Parameter(Mandatory)][string] $IntegrationBranch,
        [string] $IntegrationCommit,
        [string] $IntegrationTree,
        [string] $PatchDirectory
    )

    if (-not (Test-Path -LiteralPath (Join-Path $Path '.git'))) {
        [System.IO.Directory]::CreateDirectory([System.IO.Path]::GetDirectoryName($Path)) | Out-Null
        Invoke-HermesProcess -FilePath git -ArgumentList @(
            '-c', 'core.longpaths=true',
            'clone', '--filter=blob:none', '--branch', $Branch, $Repository, $Path
        ) -LogComponent setup
    }

    # Candidate checkouts live below the Desktop updater's staging directory.
    # Hermes Agent contains tracked paths that exceed the legacy Windows
    # MAX_PATH limit at that depth, so both clone checkout and every subsequent
    # Git operation must opt into Git for Windows long-path handling.
    Invoke-HermesProcess -FilePath git -ArgumentList @(
        '-C', $Path, 'config', 'core.longpaths', 'true'
    ) -LogComponent setup

    $currentCommit = (@(& git -C $Path rev-parse HEAD) -join [Environment]::NewLine).Trim()
    $currentTree = (@(& git -C $Path rev-parse 'HEAD^{tree}') -join [Environment]::NewLine).Trim()
    $status = (& git -C $Path status --porcelain) -join [Environment]::NewLine
    if ($LASTEXITCODE -ne 0) {
        throw "Unable to inspect Git checkout: $Path"
    }
    if ($status) {
        if ($IntegrationTree -and $currentTree -ne $IntegrationTree) {
            throw "Checkout has local changes outside the recorded Hermes Local integration tree: $Path"
        }
        Write-HermesLog -Component setup -Level WARN -Message "Preserving local changes in $Path."
        return
    }

    $remotes = @(& git -C $Path remote)
    if ($remotes -notcontains 'upstream') {
        Invoke-HermesProcess -FilePath git -ArgumentList @('-C', $Path, 'remote', 'add', 'upstream', $Repository) -LogComponent setup
    }
    if ($IntegrationTree -and $currentTree -eq $IntegrationTree) {
        $currentBranch = (@(& git -C $Path branch --show-current) -join [Environment]::NewLine).Trim()
        if ($currentBranch -ne $IntegrationBranch) {
            Invoke-HermesProcess -FilePath git -ArgumentList @('-C', $Path, 'switch', '-C', $IntegrationBranch) -LogComponent setup
        }
        Write-HermesLog -Component setup -Message "Preserved verified Hermes Local integration at $currentCommit (tree $currentTree)."
        return
    }

    Invoke-HermesProcess -FilePath git -ArgumentList @('-C', $Path, 'fetch', 'upstream', $Branch, '--tags') -LogComponent setup
    Invoke-HermesProcess -FilePath git -ArgumentList @('-C', $Path, 'checkout', '--detach', $Commit) -LogComponent setup
    $branchExists = (@(& git -C $Path branch --list $IntegrationBranch) -join [Environment]::NewLine).Trim()
    if ($branchExists) {
        Invoke-HermesProcess -FilePath git -ArgumentList @('-C', $Path, 'branch', '-f', $IntegrationBranch, $Commit) -LogComponent setup
    } else {
        Invoke-HermesProcess -FilePath git -ArgumentList @('-C', $Path, 'branch', $IntegrationBranch, $Commit) -LogComponent setup
    }
    Invoke-HermesProcess -FilePath git -ArgumentList @('-C', $Path, 'switch', $IntegrationBranch) -LogComponent setup

    if ($IntegrationTree) {
        if (-not $PatchDirectory -or -not (Test-Path -LiteralPath $PatchDirectory -PathType Container)) {
            throw 'The Hermes Local integration patch series is missing.'
        }
        $patches = @(Get-ChildItem -LiteralPath $PatchDirectory -Filter '*.patch' -File | Sort-Object Name)
        if ($patches.Count -eq 0) {
            throw 'The Hermes Local integration patch series is empty.'
        }
        $patchArguments = @('-C', $Path, 'am', '--committer-date-is-author-date') + @($patches.FullName)
        try {
            Invoke-HermesProcess -FilePath git -ArgumentList $patchArguments -LogComponent setup
        } catch {
            & git -C $Path am --abort 2>$null
            throw
        }
        $appliedCommit = (@(& git -C $Path rev-parse HEAD) -join [Environment]::NewLine).Trim()
        $appliedTree = (@(& git -C $Path rev-parse 'HEAD^{tree}') -join [Environment]::NewLine).Trim()
        if ($appliedTree -ne $IntegrationTree) {
            throw "Hermes Local patch series produced tree $appliedTree; expected $IntegrationTree."
        }
        if ($IntegrationCommit -and $appliedCommit -ne $IntegrationCommit) {
            Write-HermesLog -Component setup -Level WARN -Message "Patch tree is exact, but local Git identity produced commit $appliedCommit instead of $IntegrationCommit."
        }
    }
}

function Install-HermesDependencies {
    param(
        [Parameter(Mandatory)][string] $Source,
        [Parameter(Mandatory)][string] $Runtime,
        [switch] $Reinstall,
        [Parameter(Mandatory)][pscustomobject] $Configuration
    )

    $pythonVersion = [string]$Configuration.runtime.pythonVersion
    $managedRoot = Resolve-HermesPath 'runtimes\python\managed'
    Invoke-HermesProcess -FilePath uv -ArgumentList @(
        'python', 'install', $pythonVersion, '--install-dir', $managedRoot,
        '--no-registry', '--compile-bytecode'
    ) -LogComponent setup
    $managedPython = Get-ChildItem -LiteralPath $managedRoot -Recurse -Filter python.exe -File |
        Where-Object FullName -Match "cpython-$([regex]::Escape($pythonVersion))\." |
        Sort-Object FullName -Descending | Select-Object -First 1
    if (-not $managedPython) {
        throw "The project-managed Python $pythonVersion interpreter was not installed."
    }

    $pythonExe = Join-Path $Runtime 'Scripts\python.exe'
    if (-not (Test-Path -LiteralPath $pythonExe)) {
        Invoke-HermesProcess -FilePath uv -ArgumentList @('venv', $Runtime, '--python', $managedPython.FullName, '--seed') -LogComponent setup
    } else {
        $runtimeVersion = (@(& $pythonExe -c 'import sys; print(f"{sys.version_info.major}.{sys.version_info.minor}")') -join [Environment]::NewLine).Trim()
        if ($runtimeVersion -ne $pythonVersion) {
            throw "The existing Hermes runtime uses Python $runtimeVersion; expected $pythonVersion."
        }
    }

    $syncArguments = @(
        'sync', '--project', $Source, '--locked', '--active',
        '--extra', 'all', '--extra', 'dev', '--extra', 'voice', '--extra', 'edge-tts'
    )
    if ($Reinstall) { $syncArguments += '--reinstall' }
    Invoke-HermesProcess -FilePath uv -ArgumentList $syncArguments -Environment @{
        VIRTUAL_ENV = $Runtime
        UV_PROJECT_ENVIRONMENT = $Runtime
        UV_CACHE_DIR = (Resolve-HermesPath 'cache\uv')
    } -LogComponent setup

    Invoke-HermesProcess -FilePath npm.cmd -ArgumentList @('ci', '--cache', (Resolve-HermesPath 'cache\npm'), '--no-audit') `
        -WorkingDirectory $Source -LogComponent setup
    $playwright = Join-Path $Source 'apps\desktop\node_modules\.bin\playwright.cmd'
    if (Test-Path -LiteralPath $playwright -PathType Leaf) {
        Invoke-HermesProcess -FilePath $playwright -ArgumentList @('install', 'chromium') `
            -WorkingDirectory (Join-Path $Source 'apps\desktop') -Environment @{
                PLAYWRIGHT_BROWSERS_PATH = (Resolve-HermesPath 'runtimes\tools\playwright')
            } -LogComponent setup
    }
}

function Test-HermesModelArtifact {
    param(
        [Parameter(Mandatory)][string] $Destination,
        [object] $SizeBytes,
        [string] $Sha256,
        [switch] $Hash
    )

    if (-not (Test-Path -LiteralPath $Destination -PathType Leaf)) { return $false }
    $item = Get-Item -LiteralPath $Destination
    if ($SizeBytes -and $item.Length -ne [int64]$SizeBytes) { return $false }
    if ($Hash -and -not [string]::IsNullOrWhiteSpace($Sha256)) {
        return (Get-FileHash -LiteralPath $Destination -Algorithm SHA256).Hash.ToLowerInvariant() -eq $Sha256.ToLowerInvariant()
    }
    return $true
}

function Install-HermesModelArtifact {
    param(
        [Parameter(Mandatory)][string] $Name,
        [Parameter(Mandatory)][string] $Destination,
        [Parameter(Mandatory)][string] $Source,
        [object] $SizeBytes,
        [string] $Sha256,
        [switch] $VerifyExistingHash
    )

    if (Test-HermesModelArtifact -Destination $Destination -SizeBytes $SizeBytes -Sha256 $Sha256 -Hash:$VerifyExistingHash) {
        Write-HermesLog -Component setup -Message "$Name is already present and valid."
        return
    }
    if ([string]::IsNullOrWhiteSpace($Source)) {
        throw "$Name is not installed and has no download source."
    }
    if (Test-Path -LiteralPath $Destination -PathType Leaf) {
        $item = Get-Item -LiteralPath $Destination
        if ((-not $SizeBytes) -or $item.Length -ge [int64]$SizeBytes) {
            Remove-Item -LiteralPath $Destination -Force
        }
    }
    [System.IO.Directory]::CreateDirectory([System.IO.Path]::GetDirectoryName($Destination)) | Out-Null
    Invoke-HermesProcess -FilePath curl.exe -ArgumentList @(
        '--location', '--fail', '--show-error', '--retry', '8', '--retry-all-errors',
        '--continue-at', '-', '--output', $Destination, $Source
    ) -LogComponent setup
    if (-not (Test-HermesModelArtifact -Destination $Destination -SizeBytes $SizeBytes -Sha256 $Sha256 -Hash:(-not [string]::IsNullOrWhiteSpace($Sha256)))) {
        throw "$Name verification failed after download."
    }
}

function Install-SelectedModel {
    param([Parameter(Mandatory)][pscustomobject] $Configuration)

    $model = $Configuration.selectedModel
    $verifyHash = [bool]$Configuration.runtime.verifyModelOnStart
    Install-HermesModelArtifact -Name "Model '$($model.displayName)'" -Destination ([string]$model.resolvedPath) `
        -Source ([string]$model.source) -SizeBytes $model.sizeBytes -Sha256 ([string]$model.sha256) -VerifyExistingHash:$verifyHash

    $metadata = $model.metadata
    $projectorSource = if ($metadata) { $metadata.PSObject.Properties['visionProjectorSource'] } else { $null }
    if ($projectorSource -and -not [string]::IsNullOrWhiteSpace([string]$projectorSource.Value)) {
        $localPath = $metadata.PSObject.Properties['visionProjectorLocalPath']
        $size = $metadata.PSObject.Properties['visionProjectorSizeBytes']
        $hash = $metadata.PSObject.Properties['visionProjectorSha256']
        if (-not $localPath -or -not $size -or -not $hash -or [string]::IsNullOrWhiteSpace([string]$hash.Value)) {
            throw "Model '$($model.displayName)' has incomplete vision-projector metadata."
        }
        Install-HermesModelArtifact -Name "Vision projector for '$($model.displayName)'" `
            -Destination (Resolve-HermesModelPath ([string]$localPath.Value)) `
            -Source ([string]$projectorSource.Value) -SizeBytes $size.Value -Sha256 ([string]$hash.Value) `
            -VerifyExistingHash:$verifyHash
    }
    $refreshed = Get-HermesConfiguration
    if (-not (Test-HermesSelectedModel -Model $refreshed.selectedModel -Hash:$verifyHash)) {
        throw "Model '$($model.displayName)' verification failed after provisioning."
    }
}

$started = Get-Date
try {
    Assert-HermesRoot
    Initialize-HermesLayout
    Set-HermesProcessEnvironment
    $configuration = Get-HermesConfiguration
    $requestedAcceleration = Get-HermesRequestedAcceleration -Configuration $configuration
    $hardware = Assert-HermesMachine -Acceleration $(if ($requestedAcceleration -eq 'cuda') { 'cuda' } else { 'auto' })
    $manifest = Get-HermesVersionManifest

    if (-not $SkipLlamaBuild) {
        $decision = Resolve-HermesLlamaRuntimePackage -Configuration $configuration -Hardware $hardware
        Write-HermesLog -Component setup -Message "Runtime selection: $($decision.SelectionState). $($decision.Reason)"
        if (-not $decision.Package) {
            throw "$($decision.SelectionState): $($decision.Reason) Use -LlamaRuntimeMode source for a developer/custom build."
        }
        [void](Install-HermesLlamaRuntime -Decision $decision)
        $configuration = Get-HermesConfiguration
    }

    $acceleratorName = if ($hardware.Nvidia) { $hardware.Nvidia.Name } else { 'CPU inference' }
    Write-HermesLog -Component setup -Message "Hardware validated: $($hardware.Cpu), $acceleratorName, $([math]::Round($hardware.MemoryBytes / 1GB, 1)) GiB RAM."

    Require-Command -Name git -WingetId Git.Git
    Require-Command -Name uv -WingetId astral-sh.uv
    Require-Command -Name node -WingetId OpenJS.NodeJS.LTS

    $hermesSource = Resolve-HermesPath 'source\hermes-agent'
    Initialize-SourceCheckout -Path $hermesSource -Repository $manifest.sources.hermesAgent.repository `
        -Branch $manifest.sources.hermesAgent.branch -Commit $manifest.sources.hermesAgent.commit `
        -IntegrationBranch $manifest.sources.hermesAgent.integrationBranch `
        -IntegrationCommit $manifest.sources.hermesAgent.integrationCommit `
        -IntegrationTree $manifest.sources.hermesAgent.integrationTree `
        -PatchDirectory (Resolve-HermesPath 'source\hermes-launcher\patches')

    if (-not $SkipHermesDependencies) {
        Install-HermesDependencies -Source $hermesSource -Runtime (Resolve-HermesPath 'runtimes\python\hermes') `
            -Configuration $configuration -Reinstall:$ReinstallDependencies
    }
    if (-not $SkipModel) {
        Install-SelectedModel -Configuration $configuration
    }
    Sync-HermesRuntimeConfiguration -Configuration (Get-HermesConfiguration)
    [void](Get-OrCreateHermesApiToken)

    if (-not $SkipLauncherBuild) {
        $launcherBuild = Resolve-HermesPath 'Build-Hermes-Launcher.ps1'
        if (Test-Path -LiteralPath $launcherBuild -PathType Leaf) {
            $launcherArguments = @()
            if ($NonInteractive) { $launcherArguments += '-NonInteractive' }
            Invoke-IsolatedPowerShellScript -ScriptPath $launcherBuild -ArgumentList $launcherArguments -Description 'Launcher build'
        }
    }
    $diagnosticScript = Resolve-HermesPath 'Test-Hermes-Local.ps1'
    if (Test-Path -LiteralPath $diagnosticScript -PathType Leaf) {
        Invoke-IsolatedPowerShellScript -ScriptPath $diagnosticScript -ArgumentList @('-BootstrapOnly') -Description 'Bootstrap diagnostics'
    }

    $elapsed = (Get-Date) - $started
    Write-HermesLog -Component setup -Message "Prebuilt-runtime setup completed in $([math]::Round($elapsed.TotalMinutes, 1)) minutes."
    Write-Host "Hermes Local setup completed successfully in $([math]::Round($elapsed.TotalMinutes, 1)) minutes."
    exit 0
} catch {
    Write-HermesLog -Component setup -Level ERROR -Message $_.Exception.ToString()
    Write-Host "Hermes Local setup failed. See $(Resolve-HermesPath 'logs\setup\setup.log')." -ForegroundColor Red
    exit 1
}
