[CmdletBinding()]
param(
    [switch] $SkipModel,
    [switch] $SkipLlamaBuild,
    [switch] $SkipHermesDependencies,
    [switch] $SkipLauncherBuild,
    [switch] $NonInteractive
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'scripts\Common-Hermes.psm1') -Force

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
        [string] $IntegrationBranch
    )

    if (-not (Test-Path -LiteralPath (Join-Path $Path '.git'))) {
        [System.IO.Directory]::CreateDirectory([System.IO.Path]::GetDirectoryName($Path)) | Out-Null
        Invoke-HermesProcess -FilePath git -ArgumentList @(
            'clone', '--filter=blob:none', '--branch', $Branch, $Repository, $Path
        ) -LogComponent setup
    }

    $status = (& git -C $Path status --porcelain) -join [Environment]::NewLine
    if ($LASTEXITCODE -ne 0) {
        throw "Unable to inspect Git checkout: $Path"
    }
    if ($status) {
        $currentCommit = (& git -C $Path rev-parse HEAD).Trim()
        if ($currentCommit -ne $Commit) {
            throw "Checkout has local changes and is not at the pinned commit: $Path"
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
}

function Build-LlamaCpp {
    param(
        [Parameter(Mandatory)]
        [string] $Source,
        [Parameter(Mandatory)]
        [string] $Build
    )

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
        throw 'nvcc was not found after CUDA Toolkit installation.'
    }
    $cudaRoot = [System.IO.Directory]::GetParent(
        [System.IO.Directory]::GetParent($nvccPath).FullName
    ).FullName
    $cudaEnvironment = @{
        CUDAToolkit_ROOT = $cudaRoot
        CudaToolkitDir = "$cudaRoot\"
        CUDA_PATH = $cudaRoot
        CUDA_PATH_V13_3 = $cudaRoot
        PATH = "$cudaRoot\bin;$env:PATH"
    }

    [System.IO.Directory]::CreateDirectory($Build) | Out-Null
    Invoke-HermesProcess -FilePath cmake -ArgumentList @(
        '--fresh',
        '-S', $Source,
        '-B', $Build,
        '-G', 'Visual Studio 17 2022',
        '-A', 'x64',
        '-DGGML_CUDA=ON',
        "-DCMAKE_CUDA_COMPILER=$nvccPath",
        '-DCMAKE_CUDA_ARCHITECTURES=86',
        "-DCMAKE_VS_GLOBALS=CudaToolkitDir=$cudaRoot",
        '-DGGML_NATIVE=ON',
        '-DGGML_CCACHE=OFF',
        '-DLLAMA_BUILD_TESTS=OFF',
        '-DLLAMA_BUILD_EXAMPLES=ON'
    ) -Environment $cudaEnvironment -LogComponent setup
    Invoke-HermesProcess -FilePath cmake -ArgumentList @(
        '--build', $Build,
        '--config', 'Release',
        '--target', 'llama-server', 'llama-cli', 'llama-bench',
        '--parallel', '14'
    ) -Environment $cudaEnvironment -LogComponent setup

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
        [string] $Runtime
    )

    $pythonExe = Join-Path $Runtime 'Scripts\python.exe'
    if (-not (Test-Path -LiteralPath $pythonExe)) {
        Invoke-HermesProcess -FilePath uv -ArgumentList @(
            'venv', $Runtime, '--python', '3.11', '--seed'
        ) -LogComponent setup
    }

    Invoke-HermesProcess -FilePath uv -ArgumentList @(
        'sync', '--project', $Source, '--locked', '--active',
        '--extra', 'all',
        '--extra', 'dev',
        '--extra', 'voice',
        '--extra', 'edge-tts'
    ) -Environment @{
        VIRTUAL_ENV = $Runtime
        UV_PROJECT_ENVIRONMENT = $Runtime
        UV_CACHE_DIR = (Resolve-HermesPath 'cache\uv')
    } -LogComponent setup

    Invoke-HermesProcess -FilePath npm.cmd -ArgumentList @(
        'ci', '--cache', (Resolve-HermesPath 'cache\npm'), '--no-audit'
    ) -WorkingDirectory $Source -LogComponent setup
}

function Install-LagunaModel {
    param(
        [Parameter(Mandatory)]
        [pscustomobject] $Manifest
    )

    $model = $Manifest.model
    $destination = Resolve-HermesPath "models\Laguna-XS-2.1\$($model.filename)"
    if (Test-HermesFile -Path $destination -ExpectedSize $model.sizeBytes -ExpectedSha256 $model.sha256) {
        Write-HermesLog -Component setup -Message 'Laguna model is already present and verified.'
        return
    }

    $url = "https://huggingface.co/$($model.repository)/resolve/$($model.revision)/$($model.filename)"
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

    if (-not (Test-HermesFile -Path $destination -ExpectedSize $model.sizeBytes -ExpectedSha256 $model.sha256)) {
        throw 'Laguna model verification failed after download.'
    }

    $file = Get-Item -LiteralPath $destination
    $modelManifest = [ordered]@{
        schemaVersion = 1
        repository = $model.repository
        revision = $model.revision
        filename = $model.filename
        localPath = $destination
        downloadedAt = (Get-Date).ToUniversalTime().ToString('o')
        sizeBytes = $file.Length
        sha256 = (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash.ToLowerInvariant()
        license = $model.license
        source = $url
        runtime = [ordered]@{
            implementation = 'llama.cpp'
            llamaCommit = $Manifest.sources.llamaCpp.commit
            cudaArchitecture = $Manifest.runtime.cudaArchitecture
        }
        benchmarkProfile = 'baseline-pending-benchmark'
        context = [ordered]@{
            selectedTokens = 65536
            modelMaximumTokens = 262144
            kvKeyType = 'q8_0'
            kvValueType = 'q8_0'
        }
    }
    $manifestJson = $modelManifest | ConvertTo-Json -Depth 12
    Write-HermesAtomicText -Path (Resolve-HermesPath 'models\manifests\Laguna-XS-2.1-Q4_K_M.json') -Content ($manifestJson + [Environment]::NewLine)
}

function Initialize-HermesConfiguration {
    $template = Resolve-HermesPath 'config\templates\hermes.local.yaml'
    $runtimeConfig = Resolve-HermesPath 'data\hermes\config.yaml'
    if (-not (Test-Path -LiteralPath $runtimeConfig)) {
        Write-HermesAtomicText -Path $runtimeConfig -Content (Get-Content -Raw -LiteralPath $template)
    }
    [void](Get-OrCreateHermesApiToken)
}

$started = Get-Date
try {
    Assert-HermesRoot
    Initialize-HermesLayout
    Set-HermesProcessEnvironment
    $hardware = Assert-HermesMachine
    $manifest = Get-HermesVersionManifest
    Write-HermesLog -Component setup -Message "Hardware validated: $($hardware.Cpu), $($hardware.Nvidia.Name), $([math]::Round($hardware.MemoryBytes / 1GB, 1)) GiB RAM."

    Require-Command -Name git -WingetId Git.Git
    Require-Command -Name uv -WingetId astral-sh.uv
    Require-Command -Name node -WingetId OpenJS.NodeJS.LTS
    Require-Command -Name cmake -WingetId Kitware.CMake
    Require-Command -Name nvcc -WingetId Nvidia.CUDA

    $hermesSource = Resolve-HermesPath 'source\hermes-agent'
    Initialize-SourceCheckout `
        -Path $hermesSource `
        -Repository $manifest.sources.hermesAgent.repository `
        -Branch $manifest.sources.hermesAgent.branch `
        -Commit $manifest.sources.hermesAgent.commit `
        -IntegrationBranch $manifest.sources.hermesAgent.integrationBranch

    $llamaSource = Resolve-HermesPath 'runtimes\llama.cpp\source'
    Initialize-SourceCheckout `
        -Path $llamaSource `
        -Repository $manifest.sources.llamaCpp.repository `
        -Branch $manifest.sources.llamaCpp.branch `
        -Commit $manifest.sources.llamaCpp.commit `
        -IntegrationBranch 'hermes-local-laguna'

    if (-not $SkipLlamaBuild) {
        Build-LlamaCpp -Source $llamaSource -Build (Resolve-HermesPath 'runtimes\llama.cpp\build')
    }
    if (-not $SkipHermesDependencies) {
        Install-HermesDependencies -Source $hermesSource -Runtime (Resolve-HermesPath 'runtimes\python\hermes')
    }
    if (-not $SkipModel) {
        Install-LagunaModel -Manifest $manifest
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
    Write-Host 'Hermes Local setup failed. See D:\Hermes-Local\logs\setup\setup.log.' -ForegroundColor Red
    exit 1
}
