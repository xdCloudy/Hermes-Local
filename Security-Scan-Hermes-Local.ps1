[CmdletBinding()]
param(
    [switch] $Quick,
    [switch] $SkipDefender,
    [switch] $NonInteractive
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'scripts\Common-Hermes.psm1') -Force

$script:securityRoot = $null
$script:securityProgressPath = $null
$script:securityCancelPath = $null
$script:securityTaskLogPath = $null
$script:securityTaskId = $null
$script:securityProgressStartedAt = $null
$script:securityProgressTerminalStatus = $null
$script:securityCancellationObserved = $false
$script:securityCompletedChecks = 0
$script:securityTotalChecks = 0
$script:securityTargetCount = 0
$script:securityFindingCount = 0
$script:securityCurrentStage = 'scope-validation'
$script:securityCurrentTool = $null

. (Join-Path $PSScriptRoot 'scripts\security\Security-Progress.ps1')

function Invoke-SecurityProcess {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string] $FilePath,

        [string[]] $ArgumentList = @(),

        [Parameter(Mandatory)]
        [string] $WorkingDirectory,

        [Parameter(Mandatory)]
        [string] $EvidenceName,

        [Parameter(Mandatory)]
        [ValidateSet('tool-preparation', 'discovery', 'crawling', 'passive-checks', 'active-checks')]
        [string] $Stage,

        [Parameter(Mandatory)]
        [string] $Message,

        [int[]] $AcceptExitCode = @(0)
    )

    Assert-SecurityNotCancelled
    Start-SecurityCheck -Stage $Stage -Tool $EvidenceName -Message $Message -WorkerPid $null

    $command = Get-Command $FilePath -ErrorAction SilentlyContinue | Select-Object -First 1
    $resolvedFile = if ($command) {
        $command.Source
    } elseif (Test-Path -LiteralPath $FilePath -PathType Leaf) {
        [System.IO.Path]::GetFullPath($FilePath)
    } else {
        throw "Security tool not found: $FilePath"
    }

    $resolvedWorkingDirectory = [System.IO.Path]::GetFullPath($WorkingDirectory)
    if (-not (Test-Path -LiteralPath $resolvedWorkingDirectory -PathType Container)) {
        throw "Security scan working directory does not exist: $resolvedWorkingDirectory"
    }

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

    # Security tools need normal runtime variables, not the user's unrelated
    # provider credentials. Remove credential-shaped inherited variables from
    # every child before it can inspect or upload its environment.
    foreach ($key in @($startInfo.Environment.Keys)) {
        if ($key -match '(?i)(API[_-]?KEY|TOKEN|SECRET|PASSWORD|CREDENTIAL|AUTHORIZATION)') {
            $startInfo.Environment.Remove($key)
        }
    }

    $startedAt = Get-Date
    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $startInfo
    if (-not $process.Start()) {
        throw "Failed to start security tool: $resolvedFile"
    }
    Start-SecurityCheck -Stage $Stage -Tool $EvidenceName -Message $Message -WorkerPid $process.Id

    $stdoutTask = $process.StandardOutput.ReadToEndAsync()
    $stderrTask = $process.StandardError.ReadToEndAsync()
    while (-not $process.WaitForExit(250)) {
        if (-not (Test-SecurityCancellationRequested)) {
            continue
        }
        $script:securityCancellationObserved = $true
        Write-SecurityScanProgress `
            -Stage $Stage `
            -Message 'Cancellation accepted; stopping the owned scanner process.' `
            -CurrentTool $EvidenceName `
            -WorkerPid $process.Id `
            -Status 'cancelling' `
            -Indeterminate
        try {
            $process.Kill($true)
        } catch {
            try { $process.Kill() } catch { }
        }
        $process.WaitForExit()
        throw [System.OperationCanceledException]::new("Security scan cancelled while $EvidenceName was running.")
    }
    $stdout = $stdoutTask.GetAwaiter().GetResult()
    $stderr = $stderrTask.GetAwaiter().GetResult()
    $elapsed = ((Get-Date) - $startedAt).TotalSeconds

    $safeStdout = Protect-HermesLogText $stdout
    $safeStderr = Protect-HermesLogText $stderr
    Write-HermesAtomicText -Path (Join-Path $script:RunDirectory "$EvidenceName.stdout.txt") -Content (
        $safeStdout + $(if ($safeStdout.EndsWith([Environment]::NewLine) -or -not $safeStdout) {
            ''
        } else {
            [Environment]::NewLine
        })
    )
    Write-HermesAtomicText -Path (Join-Path $script:RunDirectory "$EvidenceName.stderr.txt") -Content (
        $safeStderr + $(if ($safeStderr.EndsWith([Environment]::NewLine) -or -not $safeStderr) {
            ''
        } else {
            [Environment]::NewLine
        })
    )

    $record = [pscustomobject]@{
        name = $EvidenceName
        executable = $resolvedFile
        exitCode = $process.ExitCode
        durationSeconds = [math]::Round($elapsed, 3)
        stdout = $stdout
        stderr = $stderr
    }
    $script:CommandRecords.Add([pscustomobject]@{
        name = $EvidenceName
        exitCode = $process.ExitCode
        durationSeconds = $record.durationSeconds
    })

    if ($AcceptExitCode -notcontains $process.ExitCode) {
        throw "$EvidenceName exited with code $($process.ExitCode). Evidence: $script:RunDirectory"
    }
    return $record
}

function Write-JsonEvidence {
    param(
        [Parameter(Mandatory)]
        [string] $Name,
        [Parameter(Mandatory)]
        [AllowEmptyString()]
        [string] $Content
    )

    $path = Join-Path $script:RunDirectory $Name
    Write-HermesAtomicText -Path $path -Content (
        $Content + $(if ($Content.EndsWith([Environment]::NewLine) -or -not $Content) {
            ''
        } else {
            [Environment]::NewLine
        })
    )
    return $path
}

function Get-NpmVulnerabilitySummary {
    param([Parameter(Mandatory)] $Audit)

    return [ordered]@{
        info = [int]$Audit.metadata.vulnerabilities.info
        low = [int]$Audit.metadata.vulnerabilities.low
        moderate = [int]$Audit.metadata.vulnerabilities.moderate
        high = [int]$Audit.metadata.vulnerabilities.high
        critical = [int]$Audit.metadata.vulnerabilities.critical
        total = [int]$Audit.metadata.vulnerabilities.total
        packages = @($Audit.vulnerabilities.PSObject.Properties.Name | Sort-Object)
    }
}

try {
    Assert-HermesRoot
    Initialize-HermesLayout
    Set-HermesProcessEnvironment

    $root = Get-HermesRoot
    $script:securityRoot = $root
    $script:securityProgressPath = Resolve-HermesPath 'data\runtime\security-scan-progress.json'
    $script:securityCancelPath = Resolve-HermesPath 'data\runtime\security-scan-cancel.json'
    $source = Resolve-HermesPath 'source\hermes-agent'
    $python = Resolve-HermesPath 'runtimes\python\hermes\Scripts\python.exe'
    $gitleaks = Resolve-HermesPath 'runtimes\tools\security\gitleaks-8.30.1\gitleaks.exe'
    $osv = Resolve-HermesPath 'runtimes\tools\security\osv-scanner-2.4.0\osv-scanner.exe'
    $gitleaksConfig = Resolve-HermesPath 'security\gitleaks-hermes.toml'
    $stamp = (Get-Date).ToUniversalTime().ToString('yyyyMMddTHHmmssZ')
    $script:RunDirectory = Resolve-HermesPath "security\scans\$stamp"
    [System.IO.Directory]::CreateDirectory($script:RunDirectory) | Out-Null
    $script:securityTaskLogPath = Join-Path $script:RunDirectory 'task.log'
    $script:CommandRecords = [System.Collections.Generic.List[object]]::new()
    $totalChecks = if ($Quick) { 8 } elseif ($SkipDefender) { 12 } else { 13 }
    $targetCount = if (-not $Quick -and -not $SkipDefender) { 2 } else { 1 }
    Initialize-SecurityScanProgress -TotalChecks $totalChecks -TargetCount $targetCount -Quick:$Quick -SkipDefender:$SkipDefender

    Write-SecurityScanProgress -Stage 'tool-preparation' -Message 'Verifying scanner executables and local configuration.' -Indeterminate
    foreach ($required in @($python, $gitleaks, $osv, $gitleaksConfig)) {
        if (-not (Test-Path -LiteralPath $required -PathType Leaf)) {
            throw "Required security dependency is missing: $required"
        }
    }

    $npmProductionRun = Invoke-SecurityProcess -FilePath 'npm.cmd' -ArgumentList @(
        'audit', '--omit=dev', '--json'
    ) -WorkingDirectory $source -EvidenceName 'npm-audit-production' -Stage discovery -Message 'Auditing production Node dependencies.' -AcceptExitCode @(0, 1)
    $npmProductionPath = Write-JsonEvidence -Name 'npm-audit-production.json' -Content $npmProductionRun.stdout
    $npmProduction = Get-Content -Raw -LiteralPath $npmProductionPath | ConvertFrom-Json -Depth 64
    $npmProductionFindingCount = [int]$npmProduction.metadata.vulnerabilities.total
    Complete-SecurityCheck -Stage discovery -Tool 'npm-audit-production' -Message 'Production Node dependency audit completed.' -FindingsAdded $npmProductionFindingCount

    $npmFullRun = Invoke-SecurityProcess -FilePath 'npm.cmd' -ArgumentList @(
        'audit', '--json'
    ) -WorkingDirectory $source -EvidenceName 'npm-audit-full' -Stage discovery -Message 'Auditing the full Node dependency graph.' -AcceptExitCode @(0, 1)
    $npmFullPath = Write-JsonEvidence -Name 'npm-audit-full.json' -Content $npmFullRun.stdout
    $npmFull = Get-Content -Raw -LiteralPath $npmFullPath | ConvertFrom-Json -Depth 64
    $npmFullFindingCount = [int]$npmFull.metadata.vulnerabilities.total
    Complete-SecurityCheck -Stage discovery -Tool 'npm-audit-full' -Message 'Full Node dependency audit completed.' -FindingsAdded $npmFullFindingCount

    $pipAuditPath = Join-Path $script:RunDirectory 'pip-audit.json'
    $null = Invoke-SecurityProcess -FilePath 'uvx' -ArgumentList @(
        '--from', 'pip-audit', 'pip-audit',
        '--path', (Resolve-HermesPath 'runtimes\python\hermes\Lib\site-packages'),
        '--format', 'json',
        '--output', $pipAuditPath
    ) -WorkingDirectory $root -EvidenceName 'pip-audit' -Stage discovery -Message 'Auditing installed Python dependencies.'
    $pipAudit = Get-Content -Raw -LiteralPath $pipAuditPath | ConvertFrom-Json -Depth 64
    $pipVulnerabilities = @(
        $pipAudit.dependencies |
            Where-Object { @($_.vulns).Count -gt 0 } |
            ForEach-Object { $_.vulns } |
            ForEach-Object { $_.id } |
            Sort-Object -Unique
    )
    Complete-SecurityCheck -Stage discovery -Tool 'pip-audit' -Message 'Python dependency audit completed.' -FindingsAdded $pipVulnerabilities.Count

    $osvPath = Join-Path $script:RunDirectory 'osv-lockfiles.json'
    $null = Invoke-SecurityProcess -FilePath $osv -ArgumentList @(
        'scan', 'source',
        '--lockfile', (Join-Path $source 'package-lock.json'),
        '--lockfile', (Join-Path $source 'uv.lock'),
        '--format', 'json',
        '--output-file', $osvPath,
        '--all-packages'
    ) -WorkingDirectory $root -EvidenceName 'osv-lockfiles' -Stage discovery -Message 'Scanning lockfiles against the OSV database.' -AcceptExitCode @(0, 1)
    $osvDocument = Get-Content -Raw -LiteralPath $osvPath | ConvertFrom-Json -Depth 100
    $osvPackages = @(
        foreach ($result in $osvDocument.results) {
            foreach ($package in $result.packages) {
                $vulnerabilityProperty = $package.PSObject.Properties['vulnerabilities']
                $vulnerabilities = @(
                    if ($vulnerabilityProperty) {
                        $vulnerabilityProperty.Value
                    }
                )
                if ($vulnerabilities.Count -gt 0) {
                    [pscustomobject]@{
                        source = [System.IO.Path]::GetFileName([string]$result.source.path)
                        name = [string]$package.package.name
                        version = [string]$package.package.version
                        ids = @($vulnerabilities.id | Sort-Object -Unique)
                    }
                }
            }
        }
    )
    Complete-SecurityCheck -Stage discovery -Tool 'osv-lockfiles' -Message 'OSV lockfile scan completed.' -FindingsAdded $osvPackages.Count

    $gitleaksPath = Join-Path $script:RunDirectory 'gitleaks-production-source.json'
    $null = Invoke-SecurityProcess -FilePath $gitleaks -ArgumentList @(
        'dir', $source,
        '--config', $gitleaksConfig,
        '--redact',
        '--no-banner',
        '--report-format', 'json',
        '--report-path', $gitleaksPath
    ) -WorkingDirectory $root -EvidenceName 'gitleaks-production-source' -Stage crawling -Message 'Crawling production source for credential patterns.'
    $gitleaksFindings = if ((Get-Item -LiteralPath $gitleaksPath).Length -gt 0) {
        @(Get-Content -Raw -LiteralPath $gitleaksPath | ConvertFrom-Json -Depth 64).Count
    } else {
        0
    }
    Complete-SecurityCheck -Stage crawling -Tool 'gitleaks-production-source' -Message 'Credential-pattern crawl completed.' -FindingsAdded $gitleaksFindings

    $null = Invoke-SecurityProcess -FilePath 'uvx' -ArgumentList @(
        'ruff', 'check', '.'
    ) -WorkingDirectory $source -EvidenceName 'ruff' -Stage passive-checks -Message 'Running Python static checks.'
    Complete-SecurityCheck -Stage passive-checks -Tool 'ruff' -Message 'Python static checks completed.'
    $null = Invoke-SecurityProcess -FilePath 'npm.cmd' -ArgumentList @(
        'run', 'typecheck', '--workspace', 'apps/desktop'
    ) -WorkingDirectory $source -EvidenceName 'typescript' -Stage passive-checks -Message 'Validating Desktop TypeScript contracts.'
    Complete-SecurityCheck -Stage passive-checks -Tool 'typescript' -Message 'Desktop TypeScript validation completed.'
    $eslintRun = Invoke-SecurityProcess -FilePath 'npm.cmd' -ArgumentList @(
        'run', 'lint', '--workspace', 'apps/desktop'
    ) -WorkingDirectory $source -EvidenceName 'eslint' -Stage passive-checks -Message 'Running Desktop lint and unsafe-pattern checks.'
    Complete-SecurityCheck -Stage passive-checks -Tool 'eslint' -Message 'Desktop lint checks completed.'

    $semgrepSummary = $null
    $defenderSummary = $null
    $sbomSummary = $null
    if (-not $Quick) {
        $semgrepPath = Join-Path $script:RunDirectory 'semgrep-hermes-agent.json'
        $null = Invoke-SecurityProcess -FilePath 'uvx' -ArgumentList @(
            '--from', 'semgrep', 'semgrep', 'scan',
            '--config', 'p/security-audit',
            '--config', 'p/secrets',
            '--json',
            '--output', $semgrepPath,
            '--exclude', 'node_modules',
            '--exclude', 'dist',
            '--exclude', 'release',
            '--exclude', 'build',
            '--exclude', '.venv',
            '.'
        ) -WorkingDirectory $source -EvidenceName 'semgrep' -Stage active-checks -Message 'Running Semgrep security and secret rules.'
        $semgrep = Get-Content -Raw -LiteralPath $semgrepPath | ConvertFrom-Json -Depth 100
        $semgrepSummary = [ordered]@{
            candidates = @($semgrep.results).Count
            errors = @($semgrep.results | Where-Object { $_.extra.severity -eq 'ERROR' }).Count
            warnings = @($semgrep.results | Where-Object { $_.extra.severity -eq 'WARNING' }).Count
            secretRuleCandidates = @(
                $semgrep.results |
                    Where-Object { $_.check_id -match '(?i)(secret|credential|api.key|private.key|password)' }
            ).Count
        }
        Complete-SecurityCheck -Stage active-checks -Tool 'semgrep' -Message 'Semgrep rule evaluation completed.' -FindingsAdded $semgrepSummary.candidates

        $nodeSbom = Resolve-HermesPath 'security\sbom\node-launcher.cdx.json'
        $null = Invoke-SecurityProcess -FilePath 'npm.cmd' -ArgumentList @(
            'exec', '--yes', '--package', '@cyclonedx/cyclonedx-npm', '--',
            'cyclonedx-npm',
            '--workspace', 'apps/desktop',
            '--include-workspace-root',
            '--omit', 'dev',
            '--package-lock-only',
            '--ignore-npm-errors',
            '--spec-version', '1.6',
            '--output-format', 'JSON',
            '--output-file', $nodeSbom,
            '--validate'
        ) -WorkingDirectory $source -EvidenceName 'sbom-node' -Stage active-checks -Message 'Generating and validating the Node SBOM.'
        Complete-SecurityCheck -Stage active-checks -Tool 'sbom-node' -Message 'Node SBOM generation completed.'

        $pythonSbom = Resolve-HermesPath 'security\sbom\python-runtime.cdx.json'
        $null = Invoke-SecurityProcess -FilePath 'uvx' -ArgumentList @(
            '--from', 'cyclonedx-bom', 'cyclonedx-py', 'environment',
            $python,
            '--pyproject', (Join-Path $source 'pyproject.toml'),
            '--spec-version', '1.6',
            '--output-format', 'JSON',
            '--output-reproducible',
            '--output-file', $pythonSbom,
            '--validate'
        ) -WorkingDirectory $root -EvidenceName 'sbom-python' -Stage active-checks -Message 'Generating and validating the Python SBOM.'
        Complete-SecurityCheck -Stage active-checks -Tool 'sbom-python' -Message 'Python SBOM generation completed.'

        $nodeBomDocument = Get-Content -Raw -LiteralPath $nodeSbom | ConvertFrom-Json -Depth 100
        $pythonBomDocument = Get-Content -Raw -LiteralPath $pythonSbom | ConvertFrom-Json -Depth 100
        $sbomSummary = [ordered]@{
            node = [ordered]@{
                path = ConvertTo-SecurityRelativePath $nodeSbom
                specVersion = $nodeBomDocument.specVersion
                components = @($nodeBomDocument.components).Count
            }
            python = [ordered]@{
                path = ConvertTo-SecurityRelativePath $pythonSbom
                specVersion = $pythonBomDocument.specVersion
                components = @($pythonBomDocument.components).Count
            }
        }

        $null = Invoke-SecurityProcess -FilePath 'uvx' -ArgumentList @(
            '--from', 'pip-licenses', 'pip-licenses',
            '--python', $python,
            '--from', 'mixed',
            '--format', 'json',
            '--with-urls',
            '--output-file', (Resolve-HermesPath 'security\sbom\python-licenses.json')
        ) -WorkingDirectory $root -EvidenceName 'licenses-python' -Stage active-checks -Message 'Generating the Python licence inventory.'
        Complete-SecurityCheck -Stage active-checks -Tool 'licenses-python' -Message 'Python licence inventory completed.'

        if (-not $SkipDefender) {
            $defenderPlatform = Get-ChildItem -LiteralPath 'C:\ProgramData\Microsoft\Windows Defender\Platform' -Directory |
                Sort-Object Name -Descending |
                Select-Object -First 1
            $defender = if ($defenderPlatform) {
                Join-Path $defenderPlatform.FullName 'MpCmdRun.exe'
            } else {
                $null
            }
            if (-not $defender -or -not (Test-Path -LiteralPath $defender -PathType Leaf)) {
                throw 'Windows Defender command-line scanner was not found.'
            }
            $defenderRun = Invoke-SecurityProcess -FilePath $defender -ArgumentList @(
                '-Scan', '-ScanType', '3',
                '-File', (Resolve-HermesPath 'dist'),
                '-DisableRemediation',
                '-CpuThrottling'
            ) -WorkingDirectory $root -EvidenceName 'defender-dist' -Stage active-checks -Message 'Scanning the packaged distribution with Windows Defender.'
            $defenderSummary = [ordered]@{
                clean = $defenderRun.stdout -match 'found no threats'
                engine = [System.IO.Path]::GetFileName($defender)
                exitCode = $defenderRun.exitCode
            }
            Complete-SecurityCheck -Stage active-checks -Tool 'defender-dist' -Message 'Windows Defender distribution scan completed.' -FindingsAdded $(if ($defenderSummary.clean) { 0 } else { 1 })
        }
    }

    Assert-SecurityNotCancelled
    Write-SecurityScanProgress -Stage validation -Message 'Validating findings against the release security gate.' -CompletedChecks $script:securityCompletedChecks -TotalChecks $script:securityTotalChecks
    $productionSummary = Get-NpmVulnerabilitySummary $npmProduction
    $fullSummary = Get-NpmVulnerabilitySummary $npmFull
    $unexpectedProductionPackages = @(
        $productionSummary.packages |
            Where-Object { $_ -notin @('react-router', 'react-router-dom') }
    )
    $unexpectedOsvPackages = @(
        $osvPackages.name |
            Where-Object { $_ -notin @('brace-expansion', 'react-router', 'pynacl') } |
            Sort-Object -Unique
    )

    $gateFailures = [System.Collections.Generic.List[string]]::new()
    if ($productionSummary.critical -gt 0 -or $fullSummary.critical -gt 0) {
        $gateFailures.Add('Critical npm dependency advisory detected.')
    }
    if ($unexpectedProductionPackages.Count -gt 0) {
        $gateFailures.Add("Unexpected production npm advisory package(s): $($unexpectedProductionPackages -join ', ')")
    }
    if ($pipVulnerabilities.Count -gt 0) {
        $gateFailures.Add("Installed Python environment has advisory IDs: $($pipVulnerabilities -join ', ')")
    }
    if ($unexpectedOsvPackages.Count -gt 0) {
        $gateFailures.Add("Unexpected OSV lockfile package(s): $($unexpectedOsvPackages -join ', ')")
    }
    if ($gitleaksFindings -ne 0) {
        $gateFailures.Add("Production secret scan found $gitleaksFindings candidate(s).")
    }
    if ($semgrepSummary -and $semgrepSummary.secretRuleCandidates -ne 0) {
        $gateFailures.Add("Semgrep secret rules found $($semgrepSummary.secretRuleCandidates) candidate(s).")
    }
    if ($defenderSummary -and -not $defenderSummary.clean) {
        $gateFailures.Add('Windows Defender did not report the distribution directory clean.')
    }

    $summary = [ordered]@{
        schemaVersion = 2
        taskId = $script:securityTaskId
        generatedAt = (Get-Date).ToUniversalTime().ToString('o')
        status = if ($gateFailures.Count -eq 0) { 'pass-with-triaged-residuals' } else { 'failed' }
        quick = [bool]$Quick
        evidenceDirectory = [System.IO.Path]::GetRelativePath($root, $script:RunDirectory).Replace('\', '/')
        sourceCommit = (& git -C $source rev-parse HEAD).Trim()
        scanners = [ordered]@{
            npmProduction = $productionSummary
            npmFull = $fullSummary
            pipAudit = [ordered]@{
                vulnerabilities = $pipVulnerabilities
                installedDependencies = @($pipAudit.dependencies).Count
            }
            osv = [ordered]@{
                vulnerablePackages = $osvPackages
            }
            gitleaks = [ordered]@{
                findings = $gitleaksFindings
                config = ConvertTo-SecurityRelativePath $gitleaksConfig
            }
            semgrep = $semgrepSummary
            eslintWarnings = ([regex]::Matches($eslintRun.stdout, '(?m)\bwarning\b')).Count
            defender = $defenderSummary
        }
        sbom = $sbomSummary
        commands = $script:CommandRecords
        acceptedResiduals = @(
            'React Router RSC-mode advisory: renderer is a client-only Electron SPA and does not enable RSC actions.',
            'brace-expansion advisory: transitive build/lint tooling only; no fixed release is available in the locked major lines.',
            'PyNaCl 1.5.0 advisory: optional Discord voice lock entry, excluded from the installed workstation environment; discord.py 2.7.1 caps PyNaCl below 1.6.'
        )
        failures = $gateFailures
        progress = [ordered]@{
            completedChecks = $script:securityCompletedChecks
            totalChecks = $script:securityTotalChecks
            targets = $script:securityTargetCount
            findings = $script:securityFindingCount
        }
    }

    Assert-SecurityNotCancelled
    Write-SecurityScanProgress -Stage report-generation -Message 'Writing the scan summary, findings index and durable result links.' -CompletedChecks $script:securityCompletedChecks -TotalChecks $script:securityTotalChecks
    $findings = [ordered]@{
        schemaVersion = 1
        taskId = $script:securityTaskId
        generatedAt = (Get-Date).ToUniversalTime().ToString('o')
        status = $summary.status
        counters = $summary.progress
        gateFailures = @($gateFailures)
        npmProduction = $productionSummary
        npmFull = $fullSummary
        pipVulnerabilities = $pipVulnerabilities
        osvPackages = $osvPackages
        gitleaksFindings = $gitleaksFindings
        semgrep = $semgrepSummary
        defender = $defenderSummary
    }
    $findingsPath = Join-Path $script:RunDirectory 'findings.json'
    Write-HermesAtomicText -Path $findingsPath -Content (($findings | ConvertTo-Json -Depth 100) + [Environment]::NewLine)

    $summaryJson = ($summary | ConvertTo-Json -Depth 100) + [Environment]::NewLine
    $summaryPath = Join-Path $script:RunDirectory 'summary.json'
    Write-HermesAtomicText -Path $summaryPath -Content $summaryJson
    Write-HermesAtomicText -Path (Resolve-HermesPath 'security\scans\latest.json') -Content $summaryJson
    Write-HermesAtomicText -Path (Resolve-HermesPath 'security\reports\latest-scan.json') -Content $summaryJson

    if ($gateFailures.Count -gt 0) {
        throw "Security gate failed: $($gateFailures -join ' ')"
    }

    Complete-SecurityScanProgress `
        -Status succeeded `
        -Message 'Security scan completed with triaged residuals.' `
        -ResultDirectory $script:RunDirectory `
        -ReportPath $summaryPath `
        -FindingsPath $findingsPath `
        -LogPath $script:securityTaskLogPath
    Write-HermesLog -Component security -Message "Security scan completed with triaged residuals. Evidence: $script:RunDirectory"
    Write-Host "Hermes Local security scan passed with triaged residuals. Evidence: $script:RunDirectory"
    exit 0
} catch {
    $failure = $_.Exception
    $cancelled = $failure -is [System.OperationCanceledException] -or $script:securityCancellationObserved
    $status = if ($cancelled) { 'cancelled' } else { 'failed' }
    $failureCode = if ($cancelled) {
        'security-scan-cancelled'
    } elseif ($failure.Message -match 'exited with code') {
        'security-tool-exit'
    } elseif ($failure.Message -match 'dependency is missing|tool not found') {
        'security-tool-missing'
    } elseif ($script:securityCurrentStage) {
        "security-$($script:securityCurrentStage)-failed"
    } else {
        'security-scan-failed'
    }
    try {
        if ($script:securityTaskId) {
            Complete-SecurityScanProgress `
                -Status $status `
                -Message $(if ($cancelled) { 'Security scan cancelled safely.' } else { 'Security scan failed.' }) `
                -ResultDirectory $script:RunDirectory `
                -ReportPath $(if ($script:RunDirectory) { Join-Path $script:RunDirectory 'summary.json' } else { $null }) `
                -FindingsPath $(if ($script:RunDirectory) { Join-Path $script:RunDirectory 'findings.json' } else { $null }) `
                -LogPath $script:securityTaskLogPath `
                -FailureCode $failureCode `
                -ErrorMessage $failure.Message
        }
        Write-HermesLog -Component security -Level WARN -Message $failure.ToString()
    } catch { }
    if ($cancelled) {
        Write-Host "Hermes Local security scan cancelled. Evidence: $script:RunDirectory" -ForegroundColor Yellow
        exit 130
    }
    Write-Host "Hermes Local security scan failed: $(Protect-SecurityTaskText $failure.Message)" -ForegroundColor Red
    exit 1
}
