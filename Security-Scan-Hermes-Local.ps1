[CmdletBinding()]
param(
    [switch] $Quick,
    [switch] $SkipDefender,
    [switch] $NonInteractive
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'scripts\Common-Hermes.psm1') -Force

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

        [int[]] $AcceptExitCode = @(0)
    )

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

    $stdoutTask = $process.StandardOutput.ReadToEndAsync()
    $stderrTask = $process.StandardError.ReadToEndAsync()
    $process.WaitForExit()
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
    $source = Resolve-HermesPath 'source\hermes-agent'
    $python = Resolve-HermesPath 'runtimes\python\hermes\Scripts\python.exe'
    $gitleaks = Resolve-HermesPath 'runtimes\tools\security\gitleaks-8.30.1\gitleaks.exe'
    $osv = Resolve-HermesPath 'runtimes\tools\security\osv-scanner-2.4.0\osv-scanner.exe'
    $gitleaksConfig = Resolve-HermesPath 'security\gitleaks-hermes.toml'
    $stamp = (Get-Date).ToUniversalTime().ToString('yyyyMMddTHHmmssZ')
    $script:RunDirectory = Resolve-HermesPath "security\scans\$stamp"
    [System.IO.Directory]::CreateDirectory($script:RunDirectory) | Out-Null
    $script:CommandRecords = [System.Collections.Generic.List[object]]::new()

    foreach ($required in @($python, $gitleaks, $osv, $gitleaksConfig)) {
        if (-not (Test-Path -LiteralPath $required -PathType Leaf)) {
            throw "Required security dependency is missing: $required"
        }
    }

    $npmProductionRun = Invoke-SecurityProcess -FilePath 'npm.cmd' -ArgumentList @(
        'audit', '--omit=dev', '--json'
    ) -WorkingDirectory $source -EvidenceName 'npm-audit-production' -AcceptExitCode @(0, 1)
    $npmProductionPath = Write-JsonEvidence -Name 'npm-audit-production.json' -Content $npmProductionRun.stdout
    $npmProduction = Get-Content -Raw -LiteralPath $npmProductionPath | ConvertFrom-Json -Depth 64

    $npmFullRun = Invoke-SecurityProcess -FilePath 'npm.cmd' -ArgumentList @(
        'audit', '--json'
    ) -WorkingDirectory $source -EvidenceName 'npm-audit-full' -AcceptExitCode @(0, 1)
    $npmFullPath = Write-JsonEvidence -Name 'npm-audit-full.json' -Content $npmFullRun.stdout
    $npmFull = Get-Content -Raw -LiteralPath $npmFullPath | ConvertFrom-Json -Depth 64

    $pipAuditPath = Join-Path $script:RunDirectory 'pip-audit.json'
    $null = Invoke-SecurityProcess -FilePath 'uvx' -ArgumentList @(
        '--from', 'pip-audit', 'pip-audit',
        '--path', (Resolve-HermesPath 'runtimes\python\hermes\Lib\site-packages'),
        '--format', 'json',
        '--output', $pipAuditPath
    ) -WorkingDirectory $root -EvidenceName 'pip-audit'
    $pipAudit = Get-Content -Raw -LiteralPath $pipAuditPath | ConvertFrom-Json -Depth 64
    $pipVulnerabilities = @(
        $pipAudit.dependencies |
            Where-Object { @($_.vulns).Count -gt 0 } |
            ForEach-Object { $_.vulns } |
            ForEach-Object { $_.id } |
            Sort-Object -Unique
    )

    $osvPath = Join-Path $script:RunDirectory 'osv-lockfiles.json'
    $null = Invoke-SecurityProcess -FilePath $osv -ArgumentList @(
        'scan', 'source',
        '--lockfile', (Join-Path $source 'package-lock.json'),
        '--lockfile', (Join-Path $source 'uv.lock'),
        '--format', 'json',
        '--output-file', $osvPath,
        '--all-packages'
    ) -WorkingDirectory $root -EvidenceName 'osv-lockfiles' -AcceptExitCode @(0, 1)
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

    $gitleaksPath = Join-Path $script:RunDirectory 'gitleaks-production-source.json'
    $null = Invoke-SecurityProcess -FilePath $gitleaks -ArgumentList @(
        'dir', $source,
        '--config', $gitleaksConfig,
        '--redact',
        '--no-banner',
        '--report-format', 'json',
        '--report-path', $gitleaksPath
    ) -WorkingDirectory $root -EvidenceName 'gitleaks-production-source'
    $gitleaksFindings = if ((Get-Item -LiteralPath $gitleaksPath).Length -gt 0) {
        @(Get-Content -Raw -LiteralPath $gitleaksPath | ConvertFrom-Json -Depth 64).Count
    } else {
        0
    }

    $null = Invoke-SecurityProcess -FilePath 'uvx' -ArgumentList @(
        'ruff', 'check', '.'
    ) -WorkingDirectory $source -EvidenceName 'ruff'
    $null = Invoke-SecurityProcess -FilePath 'npm.cmd' -ArgumentList @(
        'run', 'typecheck', '--workspace', 'apps/desktop'
    ) -WorkingDirectory $source -EvidenceName 'typescript'
    $eslintRun = Invoke-SecurityProcess -FilePath 'npm.cmd' -ArgumentList @(
        'run', 'lint', '--workspace', 'apps/desktop'
    ) -WorkingDirectory $source -EvidenceName 'eslint'

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
        ) -WorkingDirectory $source -EvidenceName 'semgrep'
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
        ) -WorkingDirectory $source -EvidenceName 'sbom-node'

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
        ) -WorkingDirectory $root -EvidenceName 'sbom-python'

        $nodeBomDocument = Get-Content -Raw -LiteralPath $nodeSbom | ConvertFrom-Json -Depth 100
        $pythonBomDocument = Get-Content -Raw -LiteralPath $pythonSbom | ConvertFrom-Json -Depth 100
        $sbomSummary = [ordered]@{
            node = [ordered]@{
                path = $nodeSbom
                specVersion = $nodeBomDocument.specVersion
                components = @($nodeBomDocument.components).Count
            }
            python = [ordered]@{
                path = $pythonSbom
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
        ) -WorkingDirectory $root -EvidenceName 'licenses-python'

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
            ) -WorkingDirectory $root -EvidenceName 'defender-dist'
            $defenderSummary = [ordered]@{
                clean = $defenderRun.stdout -match 'found no threats'
                engine = $defender
                exitCode = $defenderRun.exitCode
            }
        }
    }

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
        schemaVersion = 1
        generatedAt = (Get-Date).ToUniversalTime().ToString('o')
        status = if ($gateFailures.Count -eq 0) { 'pass-with-triaged-residuals' } else { 'failed' }
        quick = [bool]$Quick
        evidenceDirectory = $script:RunDirectory
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
                config = $gitleaksConfig
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
    }

    $summaryJson = ($summary | ConvertTo-Json -Depth 100) + [Environment]::NewLine
    $summaryPath = Join-Path $script:RunDirectory 'summary.json'
    Write-HermesAtomicText -Path $summaryPath -Content $summaryJson
    Write-HermesAtomicText -Path (Resolve-HermesPath 'security\scans\latest.json') -Content $summaryJson
    Write-HermesAtomicText -Path (Resolve-HermesPath 'security\reports\latest-scan.json') -Content $summaryJson

    if ($gateFailures.Count -gt 0) {
        throw "Security gate failed: $($gateFailures -join ' ')"
    }

    Write-HermesLog -Component security -Message "Security scan completed with triaged residuals. Evidence: $script:RunDirectory"
    Write-Host "Hermes Local security scan passed with triaged residuals. Evidence: $script:RunDirectory"
    exit 0
} catch {
    $failure = $_.Exception
    try {
        Write-HermesLog -Component security -Level WARN -Message $failure.ToString()
    } catch {
    }
    Write-Host "Hermes Local security scan failed: $($failure.Message)" -ForegroundColor Red
    exit 1
}
