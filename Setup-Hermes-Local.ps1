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

function Require-Command {
    param(
        [Parameter(Mandatory)]
        [string] $Name,
        [Parameter(Mandatory)]
        [string] $WingetId
    )

    if (Get-Command $Name -ErrorAction SilentlyContinue) {
        return
    }
    if ($Name -eq 'nvcc') {
        $cudaDirectory = Join-Path $env:ProgramFiles 'NVIDIA GPU Computing Toolkit\CUDA'
        $installedNvcc = if (Test-Path -LiteralPath $cudaDirectory) {
            Get-ChildItem -LiteralPath $cudaDirectory -Recurse -Filter nvcc.exe -File -ErrorAction SilentlyContinue |
                Sort-Object FullName -Descending |
                Select-Object -First 1
        }
        if ($installedNvcc) {
            Write-HermesLog -Component setup -Message "Detected CUDA compiler at $($installedNvcc.FullName)."
            return
        }
    }
    if ($NonInteractive -and -not (Get-Command winget -ErrorAction SilentlyContinue)) {
        throw "$Name is missing and winget is unavailable in noninteractive mode."
    }
    if (-not (Get-Command winget -ErrorAction SilentlyContinue)) {
        throw "$Name is missing and winget is unavailable."
    }
    if ($NonInteractive) {
        Write-HermesLog -Component setup -Level WARN -Message "$Name is missing; installing official package $WingetId noninteractively."
    } else {
        Write-HermesLog -Component setup -Level WARN -Message "$Name is missing; installing official package $WingetId."
    }
    Invoke-HermesProcess -FilePath 'winget' -ArgumentList @(
        'install', '--id', $WingetId, '--exact', '--source', 'winget',
        '--accept-package-agreements', '--accept-source-agreements',
        '--silent', '--disable-interactivity'
    ) -LogComponent setup
}

function Initialize-SourceCheckout {
    param(
        [Parameter(Mandatory)]
        [string] $Path,
        [Parameter(Mandatory)]
        [string] $Repository,
        [Parameter(Mandatory)]
        [string] $Branch,
        [Parameter(Mandatory)]
        [string] $Commit,
        [Parameter(Mandatory)]
        [string] $IntegrationBranch,
        [string] $IntegrationCommit,
        [string] $IntegrationTree,
        [string] $PatchDirectory
    )

    if (-not (Test-Path -LiteralPath (Join-Path $Path '.git'))) {
        [System.IO.Directory]::CreateDirectory([System.IO.Path]::GetDirectoryName($Path)) | Out-Null
        Invoke-HermesProcess -FilePath git -ArgumentList @(
            'clone', '--filter=blob:none', '--branch', $Branch, $Repository, $Path
        ) -LogComponent setup
    }

    $currentCommit = (& git -C $Path rev-parse HEAD).Trim()
    $currentTree = (& git -C $Path rev-parse 'HEAD^{tree}').Trim()
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
        Invoke-HermesProcess -FilePath git -ArgumentList @(
            '-C', $Path, 'remote', 'add', 'upstream', $Repository
        ) -LogComponent setup
    }

    if ($IntegrationTree -and $currentTree -eq $IntegrationTree) {
        $branch = (& git -C $Path branch --show-current).Trim()
        if ($branch -ne $IntegrationBranch) {
            Invoke-HermesProcess -FilePath git -ArgumentList @(
                '-C', $Path, 'switch', '-C', $IntegrationBranch
            ) -LogComponent setup
        }
        Write-HermesLog -Component setup -Message (
            "Preserved verified Hermes Local integration at $currentCommit (tree $currentTree)."
        )
        return
    }

    Invoke-HermesProcess -FilePath git -ArgumentList @(
        '-C', $Path, 'fetch', 'upstream', $Branch, '--tags'
    ) -LogComponent setup
    Invoke-HermesProcess -FilePath git -ArgumentList @(
        '-C', $Path, 'checkout', '--detach', $Commit
    ) -LogComponent setup

    $branchExists = (& git -C $Path branch --list $IntegrationBranch).Trim()
    if ($branchExists) {
        Invoke-HermesProcess -FilePath git -ArgumentList @(
            '-C', $Path, 'branch', '-f', $IntegrationBranch, $Commit
        ) -LogComponent setup
    } else {
        Invoke-HermesProcess -FilePath git -ArgumentList @(
            '-C', $Path, 'branch', $IntegrationBranch, $Commit
        ) -LogComponent setup
    }
    Invoke-HermesProcess -FilePath git -ArgumentList @(
        '-C', $Path, 'switch', $IntegrationBranch
    ) -LogComponent setup

    if ($IntegrationTree) {
        if (-not $PatchDirectory -or -not (Test-Path -LiteralPath $PatchDirectory -PathType Container)) {
            throw 'The Hermes Local integration patch series is missing.'
        }
        $patches = @(
            Get-ChildItem -LiteralPath $PatchDirectory -Filter '*.patch' -File |
                Sort-Object Name
        )
        if ($patches.Count -eq 0) {
            throw 'The Hermes Local integration patch series is empty.'
        }

        try {
            Invoke-HermesProcess -FilePath git -ArgumentList @(
                '-C', $Path, 'am', '--committer-date-is-author-date',
                @($patches | ForEach-Object FullName)
            ) -LogComponent setup
        } catch {
            & git -C $Path am --abort 2>$null
            throw
        }

        $appliedCommit = (& git -C $Path rev-parse HEAD).Trim()
        $appliedTree = (& git -C $Path rev-parse 'HEAD^{tree}').Trim()
        if ($appliedTree -ne $IntegrationTree) {
            throw "Hermes Local patch series produced tree $appliedTree; expected $IntegrationTree."
        }
        if ($IntegrationCommit -and $appliedCommit -ne $IntegrationCommit) {
            Write-HermesLog -Component setup -Level WARN -Message (
                "Patch tree is exact, but local Git committer identity produced commit $appliedCommit instead of $IntegrationCommit."
            )
        }
    }
}

function Build-LlamaCpp {
    param(
        [Parameter(Mandatory)]
        [string] $Source,
        [Parameter(Mandatory)]
        [string] $Build,
        [Parameter(Mandatory)]
        [pscustomobject] $Configuration
    )

    $acceleration = Get-HermesEffectiveAcceleration -Configuration $Configuration
    $buildEnvironment = @{}
    $configureArguments = [System.Collections.Generic.List[string]]::new()
    foreach ($argument in @(
        '--fresh',
        '-S', $Source,
        '-B', $Build,
        '-G', 'Visual Studio 17 2022',
        '-A', 'x64',
        '-DGGML_NATIVE=ON',
        '-DGGML_CCACHE=OFF',
        '-DLLAMA_BUILD_TESTS=OFF',
        '-DLLAMA_BUILD_EXAMPLES=ON'
    )) {
        $configureArguments.Add([string]$argument)
    }
    if ($acceleration -eq 'cuda') {
        $nvccCommand = Get-Command nvcc -ErrorAction SilentlyContinue | Select-Object -First 1
        $nvccPath = if ($nvccCommand) {
            $nvccCommand.Source
        } else {
            Get-ChildItem -LiteralPath (Join-Path $env:ProgramFiles 'NVIDIA GPU Computing Toolkit\CUDA') `
                -Recurse -Filter nvcc.exe -File -ErrorAction SilentlyContinue |
                Sort-Object FullName -Descending |
                Select-Object -First 1 -ExpandProperty FullName
        }
        if (-not $nvccPath) {
            throw 'CUDA acceleration was selected, but nvcc was not found.'
        }
        $cudaRoot = [System.IO.Directory]::GetParent(
            [System.IO.Directory]::GetParent($nvccPath).FullName
        ).FullName
        $cudaArchitecture = Get-HermesCudaArchitecture -Configuration $Configuration
        $buildEnvironment = @{
            CUDAToolkit_ROOT = $cudaRoot
            CudaToolkitDir = "$cudaRoot\"
            CUDA_PATH = $cudaRoot
            PATH = "$cudaRoot\bin;$env:PATH"
        }
        foreach ($argument in @(
            '-DGGML_CUDA=ON',
            "-DCMAKE_CUDA_COMPILER=$nvccPath",
            "-DCMAKE_CUDA_ARCHITECTURES=$cudaArchitecture",
            "-DCMAKE_VS_GLOBALS=CudaToolkitDir=$cudaRoot"
        )) {
            $configureArguments.Add($argument)
        }
    } else {
        $configureArguments.Add('-DGGML_CUDA=OFF')
    }
    [System.IO.Directory]::CreateDirectory($Build) | Out-Null
    Invoke-HermesProcess -FilePath cmake -ArgumentList $configureArguments.ToArray() `
        -Environment $buildEnvironment -LogComponent setup
    Invoke-HermesProcess -FilePath cmake -ArgumentList @(
        '--build', $Build,
        '--config', 'Release',
        '--target', 'llama-server', 'llama-cli', 'llama-bench',
        '--parallel', [string](Get-HermesBuildParallelism -Configuration $Configuration)
    ) -Environment $buildEnvironment -LogComponent setup

    $expected = @('llama-server.exe', 'llama-cli.exe', 'llama-bench.exe')
    foreach ($name in $expected) {
        $matches = @(Get-ChildItem -LiteralPath $Build -Recurse -Filter $name -File)
        if ($matches.Count -ne 1) {
            throw "Expected one $name under $Build; found $($matches.Count)."
        }
    }
}

function Install-HermesDependencies {
    param(
        [Parameter(Mandatory)]
        [string] $Source,
        [Parameter(Mandatory)]
        [string] $Runtime,
        [switch] $Reinstall,
        [Parameter(Mandatory)]
        [pscustomobject] $Configuration
    )

    $pythonVersion = [string]$Configuration.runtime.pythonVersion
    $managedRoot = Resolve-HermesPath 'runtimes\python\managed'
    Invoke-HermesProcess -FilePath uv -ArgumentList @(
        'python', 'install', $pythonVersion,
        '--install-dir', $managedRoot,
        '--no-registry',
        '--compile-bytecode'
    ) -LogComponent setup

    $managedPython = Get-ChildItem -LiteralPath $managedRoot -Recurse -Filter python.exe -File |
        Where-Object FullName -Match "cpython-$([regex]::Escape($pythonVersion))\." |
        Sort-Object FullName -Descending |
        Select-Object -First 1
    if (-not $managedPython) {
        throw "The project-managed Python $pythonVersion interpreter was not installed."
    }

    $pythonExe = Join-Path $Runtime 'Scripts\python.exe'
    if (-not (Test-Path -LiteralPath $pythonExe)) {
        Invoke-HermesProcess -FilePath uv -ArgumentList @(
            'venv', $Runtime, '--python', $managedPython.FullName, '--seed'
        ) -LogComponent setup
    } else {
        $runtimeVersion = (& $pythonExe -c 'import sys; print(f"{sys.version_info.major}.{sys.version_info.minor}")').Trim()
        if ($runtimeVersion -ne $pythonVersion) {
            throw "The existing Hermes runtime uses Python $runtimeVersion. Preserve it as a rollback copy and rebuild the venv with project-managed Python $pythonVersion."
        }
    }

    $syncArguments = @(
        'sync', '--project', $Source, '--locked', '--active',
        '--extra', 'all',
        '--extra', 'dev',
        '--extra', 'voice',
        '--extra', 'edge-tts'
    )
    if ($Reinstall) {
        $syncArguments += '--reinstall'
    }
    Invoke-HermesProcess -FilePath uv -ArgumentList $syncArguments -Environment @{
        VIRTUAL_ENV = $Runtime
        UV_PROJECT_ENVIRONMENT = $Runtime
        UV_CACHE_DIR = (Resolve-HermesPath 'cache\uv')
    } -LogComponent setup

    Invoke-HermesProcess -FilePath npm.cmd -ArgumentList @(
        'ci', '--cache', (Resolve-HermesPath 'cache\npm'), '--no-audit'
    ) -WorkingDirectory $Source -LogComponent setup

    $playwright = Join-Path $Source 'apps\desktop\node_modules\.bin\playwright.cmd'
    if (Test-Path -LiteralPath $playwright -PathType Leaf) {
        Invoke-HermesProcess -FilePath $playwright -ArgumentList @(
            'install', 'chromium'
        ) -WorkingDirectory (Join-Path $Source 'apps\desktop') -Environment @{
            PLAYWRIGHT_BROWSERS_PATH = (Resolve-HermesPath 'runtimes\tools\playwright')
        } -LogComponent setup
    }
}

function Install-SelectedModel {
    param(
        [Parameter(Mandatory)]
        [pscustomobject] $Configuration
    )

    $model = $Configuration.selectedModel
    $destination = [string]$model.resolvedPath
    $verifyHash = [bool]$Configuration.runtime.verifyModelOnStart
    if (Test-HermesSelectedModel -Model $model -Hash:$verifyHash) {
        Write-HermesLog -Component setup -Message "Model '$($model.displayName)' is already present and valid."
        return
    }

    $url = [string]$model.source
    if (-not $url) {
        throw "Model '$($model.displayName)' is not installed and has no download source. Register an existing GGUF path or add a source URL."
    }
    [System.IO.Directory]::CreateDirectory([System.IO.Path]::GetDirectoryName($destination)) | Out-Null
    Invoke-HermesProcess -FilePath curl.exe -ArgumentList @(
        '--location',
        '--fail',
        '--show-error',
        '--retry', '8',
        '--retry-all-errors',
        '--continue-at', '-',
        '--output', $destination,
        $url
    ) -LogComponent setup

    $refreshed = Get-HermesConfiguration
    if (-not (Test-HermesSelectedModel -Model $refreshed.selectedModel -Hash:$verifyHash)) {
        throw "Model '$($model.displayName)' verification failed after download."
    }
}

function Initialize-HermesConfiguration {
    Sync-HermesRuntimeConfiguration -Configuration (Get-HermesConfiguration)
    [void](Get-OrCreateHermesApiToken)
}

$started = Get-Date
try {
    Assert-HermesRoot
    Initialize-HermesLayout
    Set-HermesProcessEnvironment
    $configuration = Get-HermesConfiguration
    $effectiveAcceleration = Get-HermesEffectiveAcceleration -Configuration $configuration
    $hardware = Assert-HermesMachine -Acceleration $effectiveAcceleration
    $manifest = Get-HermesVersionManifest
    $acceleratorName = if ($hardware.Nvidia) { $hardware.Nvidia.Name } else { 'CPU inference' }
    Write-HermesLog -Component setup -Message "Hardware validated: $($hardware.Cpu), $acceleratorName, $([math]::Round($hardware.MemoryBytes / 1GB, 1)) GiB RAM; acceleration=$effectiveAcceleration."

    Require-Command -Name git -WingetId Git.Git
    Require-Command -Name uv -WingetId astral-sh.uv
    Require-Command -Name node -WingetId OpenJS.NodeJS.LTS
    Require-Command -Name cmake -WingetId Kitware.CMake
    if ($effectiveAcceleration -eq 'cuda') {
        Require-Command -Name nvcc -WingetId Nvidia.CUDA
    }

    $hermesSource = Resolve-HermesPath 'source\hermes-agent'
    Initialize-SourceCheckout `
        -Path $hermesSource `
        -Repository $manifest.sources.hermesAgent.repository `
        -Branch $manifest.sources.hermesAgent.branch `
        -Commit $manifest.sources.hermesAgent.commit `
        -IntegrationBranch $manifest.sources.hermesAgent.integrationBranch `
        -IntegrationCommit $manifest.sources.hermesAgent.integrationCommit `
        -IntegrationTree $manifest.sources.hermesAgent.integrationTree `
        -PatchDirectory (Resolve-HermesPath 'source\hermes-launcher\patches')

    $llamaSource = Resolve-HermesPath 'runtimes\llama.cpp\source'
    Initialize-SourceCheckout `
        -Path $llamaSource `
        -Repository $manifest.sources.llamaCpp.repository `
        -Branch $manifest.sources.llamaCpp.branch `
        -Commit $manifest.sources.llamaCpp.commit `
        -IntegrationBranch 'hermes-local-runtime'

    if (-not $SkipLlamaBuild) {
        Build-LlamaCpp -Source $llamaSource -Build (Resolve-HermesPath 'runtimes\llama.cpp\build') -Configuration $configuration
    }
    if (-not $SkipHermesDependencies) {
        Install-HermesDependencies `
            -Source $hermesSource `
            -Runtime (Resolve-HermesPath 'runtimes\python\hermes') `
            -Configuration $configuration `
            -Reinstall:$ReinstallDependencies
    }
    if (-not $SkipModel) {
        Install-SelectedModel -Configuration $configuration
    }
    Initialize-HermesConfiguration

    if (-not $SkipLauncherBuild) {
        $launcherBuild = Resolve-HermesPath 'Build-Hermes-Launcher.ps1'
        if (Test-Path -LiteralPath $launcherBuild) {
            & $launcherBuild -NonInteractive:$NonInteractive
            if ($LASTEXITCODE -ne 0) {
                throw "Launcher build failed with exit code $LASTEXITCODE."
            }
        }
    }

    $diagnosticScript = Resolve-HermesPath 'Test-Hermes-Local.ps1'
    if (Test-Path -LiteralPath $diagnosticScript) {
        & $diagnosticScript -BootstrapOnly
        if ($LASTEXITCODE -ne 0) {
            throw "Bootstrap diagnostics failed with exit code $LASTEXITCODE."
        }
    }

    $elapsed = (Get-Date) - $started
    Write-HermesLog -Component setup -Message "Setup completed in $([math]::Round($elapsed.TotalMinutes, 1)) minutes."
    Write-Host "Hermes Local setup completed successfully in $([math]::Round($elapsed.TotalMinutes, 1)) minutes."
    exit 0
} catch {
    Write-HermesLog -Component setup -Level ERROR -Message $_.Exception.ToString()
    Write-Host "Hermes Local setup failed. See $(Resolve-HermesPath 'logs\setup\setup.log')." -ForegroundColor Red
    exit 1
}
