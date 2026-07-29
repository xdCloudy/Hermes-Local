[CmdletBinding()]
param(
    [ValidateSet('Fast', 'Full', 'Package')]
    [string] $Scope = 'Fast',
    [string] $OutputDirectory
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$nested = Join-Path $root 'source\hermes-agent'
$desktop = Join-Path $nested 'apps\desktop'
$runId = (Get-Date).ToUniversalTime().ToString('yyyyMMddTHHmmssZ')

if (-not $OutputDirectory) {
    $OutputDirectory = Join-Path $root "temp\qa-runs\$runId"
}

$output = [IO.Path]::GetFullPath($OutputDirectory, $root)
$null = New-Item -ItemType Directory -Path $output -Force
$results = [System.Collections.Generic.List[object]]::new()

function Resolve-Executable {
    param([Parameter(Mandatory)][string] $Name)

    $command = Get-Command $Name -CommandType Application -ErrorAction Stop |
        Select-Object -First 1

    return [string] $command.Source
}

function Invoke-QaStep {
    param(
        [Parameter(Mandatory)][string] $Id,
        [Parameter(Mandatory)][string] $FilePath,
        [Parameter(Mandatory)][string[]] $ArgumentList,
        [Parameter(Mandatory)][string] $WorkingDirectory,
        [bool] $Required = $true
    )

    $stdoutPath = Join-Path $output "$Id.stdout.txt"
    $stderrPath = Join-Path $output "$Id.stderr.txt"
    $startedAt = (Get-Date).ToUniversalTime()
    Write-Host "[$Id] starting"

    $startInfo = [Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $FilePath
    $startInfo.WorkingDirectory = $WorkingDirectory
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true

    foreach ($argument in $ArgumentList) {
        $startInfo.ArgumentList.Add($argument)
    }

    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $startInfo
    $null = $process.Start()
    $stdoutTask = $process.StandardOutput.ReadToEndAsync()
    $stderrTask = $process.StandardError.ReadToEndAsync()
    $process.WaitForExit()
    $stdout = $stdoutTask.GetAwaiter().GetResult()
    $stderr = $stderrTask.GetAwaiter().GetResult()
    [IO.File]::WriteAllText($stdoutPath, $stdout, [Text.UTF8Encoding]::new($false))
    [IO.File]::WriteAllText($stderrPath, $stderr, [Text.UTF8Encoding]::new($false))

    $completedAt = (Get-Date).ToUniversalTime()
    $result = [pscustomobject]@{
        id = $Id
        required = $Required
        command = [ordered]@{
            file = $FilePath
            arguments = @($ArgumentList)
            workingDirectory = $WorkingDirectory
        }
        startedAt = $startedAt.ToString('o')
        completedAt = $completedAt.ToString('o')
        durationMilliseconds = [Math]::Round(($completedAt - $startedAt).TotalMilliseconds)
        exitCode = $process.ExitCode
        passed = $process.ExitCode -eq 0
        stdout = [IO.Path]::GetRelativePath($root, $stdoutPath).Replace('\', '/')
        stderr = [IO.Path]::GetRelativePath($root, $stderrPath).Replace('\', '/')
    }

    $results.Add($result)
    Write-Host "[$Id] exit $($result.exitCode) in $($result.durationMilliseconds) ms"
}

$node = Resolve-Executable 'node.exe'
$npm = Resolve-Executable 'npm.cmd'
$npx = Resolve-Executable 'npx.cmd'
$pwsh = Resolve-Executable 'pwsh.exe'
$python = Join-Path $root 'runtimes\python\hermes\Scripts\python.exe'

if (-not (Test-Path -LiteralPath $python -PathType Leaf)) {
    throw "The managed Hermes Python interpreter is missing: $python"
}

Invoke-QaStep -Id inventories -FilePath $node -ArgumentList @('scripts/qa/build-qa-inventories.mjs') -WorkingDirectory $root
Invoke-QaStep -Id powershell-syntax -FilePath $pwsh -ArgumentList @(
    '-NoLogo',
    '-NoProfile',
    '-NonInteractive',
    '-File',
    (Join-Path $PSScriptRoot 'Test-PowerShellSyntax.ps1'),
    '-OutputPath',
    (Join-Path $output 'powershell-syntax.json')
) -WorkingDirectory $root
Invoke-QaStep -Id desktop-typecheck -FilePath $npm -ArgumentList @('run', 'typecheck') -WorkingDirectory $desktop
Invoke-QaStep -Id desktop-lint -FilePath $npm -ArgumentList @('run', 'lint') -WorkingDirectory $desktop
Invoke-QaStep -Id local-electron-tests -FilePath $npx -ArgumentList @(
    'vitest',
    'run',
    '--project',
    'electron',
    'electron/hermes-local-control.test.ts',
    'electron/hermes-local-settings.test.ts'
) -WorkingDirectory $desktop
Invoke-QaStep -Id billing-regressions -FilePath $npx -ArgumentList @(
    'vitest',
    'run',
    '--project',
    'ui',
    'src/app/settings/billing/index.test.tsx',
    'src/app/settings/billing/billing-amounts.test.ts'
) -WorkingDirectory $desktop
Invoke-QaStep -Id python-functional -FilePath $python -ArgumentList @(
    '-m',
    'pytest',
    'tests/test_windows_subprocess_no_window_flags.py',
    'tests/agent/test_skill_commands.py',
    'tests/run_agent/test_compression_persistence.py',
    'tests/tools/test_read_extract.py',
    '--junitxml',
    (Join-Path $output 'python-functional.junit.xml'),
    '-q'
) -WorkingDirectory $nested

if ($Scope -In @('Full', 'Package')) {
    Invoke-QaStep -Id recovery-fixtures -FilePath $pwsh -ArgumentList @(
        '-NoLogo',
        '-NoProfile',
        '-NonInteractive',
        '-File',
        (Join-Path $PSScriptRoot 'Test-RecoveryFixtures.ps1'),
        '-OutputPath',
        (Join-Path $output 'recovery-fixtures.json')
    ) -WorkingDirectory $root
    Invoke-QaStep -Id electron-full -FilePath $npm -ArgumentList @('run', 'test:desktop:platforms') -WorkingDirectory $desktop
    Invoke-QaStep -Id ui-full -FilePath $npm -ArgumentList @('run', 'test:ui') -WorkingDirectory $desktop
    Invoke-QaStep -Id desktop-build -FilePath $npm -ArgumentList @('run', 'build') -WorkingDirectory $desktop
    Invoke-QaStep -Id operational-diagnostics -FilePath $pwsh -ArgumentList @(
        '-NoLogo',
        '-NoProfile',
        '-NonInteractive',
        '-File',
        (Join-Path $root 'Test-Hermes-Local.ps1'),
        '-NonInteractive'
    ) -WorkingDirectory $root
}

if ($Scope -EQ 'Package') {
    Invoke-QaStep -Id launcher-package -FilePath $pwsh -ArgumentList @(
        '-NoLogo',
        '-NoProfile',
        '-NonInteractive',
        '-File',
        (Join-Path $root 'Package-Hermes-Launcher.ps1'),
        '-NonInteractive'
    ) -WorkingDirectory $root
}

Invoke-QaStep -Id finalize-inventories -FilePath $node -ArgumentList @(
    'scripts/qa/finalize-qa-inventories.mjs',
    $output
) -WorkingDirectory $root

$requiredFailures = @($results | Where-Object { $_.required -and -not $_.passed })
$summary = [ordered]@{
    schemaVersion = 1
    runId = $runId
    scope = $Scope
    generatedAt = (Get-Date).ToUniversalTime().ToString('o')
    rootCommit = (& git.exe -C $root rev-parse HEAD).Trim()
    nestedCommit = (& git.exe -C $nested rev-parse HEAD).Trim()
    passed = $requiredFailures.Count -eq 0
    steps = $results.Count
    passedSteps = @($results | Where-Object passed).Count
    failedSteps = @($results | Where-Object { -not $_.passed }).Count
    requiredFailures = @($requiredFailures | ForEach-Object id)
    results = @($results)
}

$jsonPath = Join-Path $output 'qa-run.json'
[IO.File]::WriteAllText(
    $jsonPath,
    (($summary | ConvertTo-Json -Depth 10) + [Environment]::NewLine),
    [Text.UTF8Encoding]::new($false)
)

$markdown = @(
    '# Hermes Local functional QA run'
    ''
    "- Run: ``$runId``"
    "- Scope: ``$Scope``"
    "- Result: **$(if ($summary.passed) { 'PASS' } else { 'FAIL' })**"
    "- Steps: $($summary.passedSteps) passed, $($summary.failedSteps) failed"
    ''
    '| Step | Required | Result | Duration (ms) |'
    '|---|---:|---:|---:|'
    foreach ($result in $results) {
        "| $($result.id) | $($result.required) | $(if ($result.passed) { 'PASS' } else { 'FAIL' }) | $($result.durationMilliseconds) |"
    }
    ''
    'Security auditing and vulnerability assessment were excluded from this QA engagement.'
)

[IO.File]::WriteAllText(
    (Join-Path $output 'qa-run.md'),
    (($markdown -join [Environment]::NewLine) + [Environment]::NewLine),
    [Text.UTF8Encoding]::new($false)
)

Write-Host "QA evidence: $output"

if (-not $summary.passed) {
    exit 1
}

exit 0
