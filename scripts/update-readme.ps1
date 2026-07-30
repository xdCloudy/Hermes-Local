<#
.SYNOPSIS
Refreshes the generated project dashboard and roadmap in README.md.

.DESCRIPTION
Reads repository, release, milestone, issue and commit data from the GitHub API.
Only content between the generated markers is replaced. Pass -DryRun to print
the result without writing, or -FixturePath to test with a local JSON payload.

.EXAMPLE
.\scripts\update-readme.ps1 -Verbose

.EXAMPLE
.\scripts\update-readme.ps1 -DryRun -FixturePath .\temp\readme-fixture.json
#>
[CmdletBinding()]
param(
    [Parameter()]
    [string]$Repository = 'xdCloudy/Hermes-Local',

    [Parameter()]
    [string]$ReadmePath = (Join-Path $PSScriptRoot '..\README.md'),

    [Parameter()]
    [string]$RoadmapPath = (Join-Path $PSScriptRoot '..\docs\roadmap.json'),

    [Parameter()]
    [string]$FixturePath,

    [Parameter()]
    [string]$Token = $env:GITHUB_TOKEN,

    [Parameter()]
    [switch]$DryRun
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$statusStart = '<!-- BEGIN GENERATED STATUS -->'
$statusEnd = '<!-- END GENERATED STATUS -->'
$roadmapStart = '<!-- BEGIN GENERATED ROADMAP -->'
$roadmapEnd = '<!-- END GENERATED ROADMAP -->'

function Write-GeneratorLog {
    param([Parameter(Mandatory)][string]$Message)
    Write-Verbose "[update-readme] $Message"
}

function Get-GitHubHeaders {
    $headers = @{
        Accept = 'application/vnd.github+json'
        'X-GitHub-Api-Version' = '2022-11-28'
        'User-Agent' = 'Hermes-Local-README-Generator'
    }
    if (-not [string]::IsNullOrWhiteSpace($Token)) {
        $headers.Authorization = "Bearer $Token"
    }
    return $headers
}

function Invoke-GitHubPagedRequest {
    param([Parameter(Mandatory)][string]$Uri)

    $items = [System.Collections.Generic.List[object]]::new()
    $nextUri = $Uri
    while ($nextUri) {
        Write-GeneratorLog "GET $nextUri"
        $responseHeaders = $null
        $page = Invoke-RestMethod -Uri $nextUri -Headers (Get-GitHubHeaders) `
            -ResponseHeadersVariable responseHeaders
        foreach ($item in @($page)) {
            $items.Add($item)
        }

        $nextUri = $null
        $link = if ($responseHeaders -and $responseHeaders.ContainsKey('Link')) {
            $responseHeaders['Link']
        } else { $null }
        if ($link -and $link -match '<([^>]+)>;\s*rel="next"') {
            $nextUri = $Matches[1]
        }
    }
    return @($items)
}

function Get-ProjectData {
    if ($FixturePath) {
        $resolvedFixture = (Resolve-Path -LiteralPath $FixturePath).Path
        Write-GeneratorLog "Loading local fixture $resolvedFixture"
        return Get-Content -Raw -LiteralPath $resolvedFixture | ConvertFrom-Json
    }

    $api = "https://api.github.com/repos/$Repository"
    $repositoryData = Invoke-RestMethod -Uri $api -Headers (Get-GitHubHeaders)
    $releases = Invoke-GitHubPagedRequest "$api/releases?per_page=100"
    $milestones = Invoke-GitHubPagedRequest "$api/milestones?state=all&per_page=100"
    $allIssues = Invoke-GitHubPagedRequest "$api/issues?state=all&per_page=100"
    $issues = @($allIssues | Where-Object { -not $_.PSObject.Properties['pull_request'] })
    Write-GeneratorLog "GET $api/commits?per_page=1"
    $commits = @(
        Invoke-RestMethod -Uri "$api/commits?per_page=1" -Headers (Get-GitHubHeaders)
    )

    return [pscustomobject]@{
        repository = $repositoryData
        releases = $releases
        milestones = $milestones
        issues = $issues
        commits = $commits
    }
}

function ConvertTo-MarkdownText {
    param([AllowNull()][object]$Value)
    if ($null -eq $Value) { return '—' }
    return ([string]$Value).Replace('|', '\|').Replace("`r", ' ').Replace("`n", ' ')
}

function Get-ReleaseVersion {
    $versionPath = Join-Path $PSScriptRoot '..\VERSION.json'
    if (-not (Test-Path -LiteralPath $versionPath)) { return 'Unknown' }
    $manifest = Get-Content -Raw -LiteralPath $versionPath | ConvertFrom-Json
    return "v$($manifest.product.version)"
}

function Get-MilestoneProgress {
    param([Parameter(Mandatory)][object[]]$Milestones)
    $open = [int](($Milestones | Measure-Object -Property open_issues -Sum).Sum)
    $closed = [int](($Milestones | Measure-Object -Property closed_issues -Sum).Sum)
    $total = $open + $closed
    $percent = if ($total -gt 0) { [math]::Round(($closed / $total) * 100) } else { 0 }
    return [pscustomobject]@{ Open = $open; Closed = $closed; Total = $total; Percent = $percent }
}

function New-StatusMarkdown {
    param(
        [Parameter(Mandatory)][object]$Data,
        [Parameter(Mandatory)][object]$Roadmap
    )

    $issues = @($Data.issues)
    $openIssues = @($issues | Where-Object state -EQ 'open').Count
    $closedIssues = @($issues | Where-Object state -EQ 'closed').Count
    $totalIssues = $openIssues + $closedIssues
    $completion = if ($totalIssues -gt 0) {
        [math]::Round(($closedIssues / $totalIssues) * 100)
    } else { 0 }

    $latestRelease = @($Data.releases | Sort-Object published_at -Descending | Select-Object -First 1)
    $latestCommit = @($Data.commits | Select-Object -First 1)
    $currentStage = @($Roadmap.stages | Where-Object id -EQ $Roadmap.currentStage | Select-Object -First 1)
    $currentMilestones = @(
        $Data.milestones | Where-Object {
            [int]$_.number -in @($currentStage.milestones | ForEach-Object { [int]$_ })
        }
    )
    $currentMilestone = @($currentMilestones | Sort-Object number | Select-Object -First 1)
    $stageIndex = -1
    for ($index = 0; $index -lt @($Roadmap.stages).Count; $index++) {
        if ($Roadmap.stages[$index].id -eq $Roadmap.currentStage) {
            $stageIndex = $index
            break
        }
    }
    $nextStage = if ($stageIndex -ge 0 -and $stageIndex + 1 -lt @($Roadmap.stages).Count) {
        $Roadmap.stages[$stageIndex + 1]
    } else { $null }

    $latestReleaseCell = if ($latestRelease.Count) {
        "[$(ConvertTo-MarkdownText $latestRelease[0].tag_name)]($($latestRelease[0].html_url))"
    } else { 'No release published' }
    $recentReleaseCell = if ($latestRelease.Count) {
        "$(ConvertTo-MarkdownText $latestRelease[0].name) · $([datetime]$latestRelease[0].published_at | Get-Date -Format 'yyyy-MM-dd')"
    } else { 'No release published' }
    $commitCell = if ($latestCommit.Count) {
        $shortSha = ([string]$latestCommit[0].sha).Substring(0, [math]::Min(7, ([string]$latestCommit[0].sha).Length))
        $message = ([string]$latestCommit[0].commit.message -split "`n")[0]
        "[``$shortSha``]($($latestCommit[0].html_url)) $(ConvertTo-MarkdownText $message)"
    } else { 'No commit found' }
    $milestoneCell = if ($currentMilestone.Count) {
        "[$(ConvertTo-MarkdownText $currentMilestone[0].title)]($($currentMilestone[0].html_url))"
    } else { ConvertTo-MarkdownText $currentStage.title }
    $nextCell = if ($nextStage) { ConvertTo-MarkdownText $nextStage.title } else { 'Version 1.0' }

    return @"
| Release | Delivery | Repository |
|---|---|---|
| **Current build:** $(Get-ReleaseVersion)<br>**Latest release:** $latestReleaseCell<br>**Recent release:** $recentReleaseCell | **Current milestone:** $milestoneCell<br>**Current focus:** $(ConvertTo-MarkdownText $currentStage.purpose)<br>**Next:** $nextCell | **Issues:** $openIssues open · $closedIssues closed<br>**Overall completion:** $completion%<br>**Recent commit:** $commitCell |

> Status is generated from GitHub issues, milestones, releases and commits.
"@
}

function New-RoadmapMarkdown {
    param(
        [Parameter(Mandatory)][object]$Data,
        [Parameter(Mandatory)][object]$Roadmap
    )

    $rows = [System.Collections.Generic.List[string]]::new()
    $rows.Add('| Stage | Purpose and success criteria | GitHub milestones | Progress |')
    $rows.Add('|---|---|---|---:|')

    foreach ($stage in $Roadmap.stages) {
        $numbers = @($stage.milestones | ForEach-Object { [int]$_ })
        $milestones = @($Data.milestones | Where-Object { [int]$_.number -in $numbers })
        $progress = Get-MilestoneProgress -Milestones $milestones
        $icon = [string]$stage.icon
        $stateOverride = if ($stage.PSObject.Properties['stateOverride']) {
            [string]$stage.stateOverride
        } else { '' }
        if ($stateOverride -eq 'complete') {
            $icon = '✅'
        } elseif ($stage.id -eq $Roadmap.currentStage) {
            $icon = '🚧'
        } elseif ($progress.Total -gt 0 -and $progress.Open -eq 0) {
            $icon = '✅'
        } elseif ($progress.Closed -gt 0) {
            $icon = '🚧'
        }

        $milestoneLinks = if ($milestones.Count) {
            (@($milestones | Sort-Object number | ForEach-Object {
                "[$(ConvertTo-MarkdownText $_.title)]($($_.html_url))"
            }) -join '<br>')
        } else {
            'Programme milestone not yet populated'
        }
        $progressText = if ($stateOverride -eq 'complete' -and $progress.Total -eq 0) {
            '**Complete**'
        } elseif ($progress.Total -gt 0) {
            "**$($progress.Percent)%**<br>$($progress.Closed)/$($progress.Total) issues"
        } else {
            '**Planned**'
        }
        $purpose = "**Purpose:** $(ConvertTo-MarkdownText $stage.purpose)<br>**Success:** $(ConvertTo-MarkdownText $stage.successCriteria)"
        $rows.Add("| $icon **$(ConvertTo-MarkdownText $stage.title)** | $purpose | $milestoneLinks | $progressText |")
    }

    $rows.Add('')
    $rows.Add('Progress is derived from issue counts on the linked milestones. The sequence is intentional; dates are added only when maintainers have a credible delivery window.')
    return $rows -join "`n"
}

function Replace-GeneratedBlock {
    param(
        [Parameter(Mandatory)][string]$Content,
        [Parameter(Mandatory)][string]$StartMarker,
        [Parameter(Mandatory)][string]$EndMarker,
        [Parameter(Mandatory)][string]$Generated
    )
    $pattern = "(?s)$([regex]::Escape($StartMarker)).*?$([regex]::Escape($EndMarker))"
    if ($Content -notmatch $pattern) {
        throw "README is missing generated block markers: $StartMarker / $EndMarker"
    }
    return [regex]::Replace(
        $Content,
        $pattern,
        "$StartMarker`n$($Generated.Trim())`n$EndMarker",
        1
    )
}

$resolvedReadme = (Resolve-Path -LiteralPath $ReadmePath).Path
$resolvedRoadmap = (Resolve-Path -LiteralPath $RoadmapPath).Path
Write-GeneratorLog "Repository: $Repository"
Write-GeneratorLog "README: $resolvedReadme"

$data = Get-ProjectData
$roadmap = Get-Content -Raw -LiteralPath $resolvedRoadmap | ConvertFrom-Json
$original = Get-Content -Raw -LiteralPath $resolvedReadme
$updated = Replace-GeneratedBlock -Content $original -StartMarker $statusStart `
    -EndMarker $statusEnd -Generated (New-StatusMarkdown -Data $data -Roadmap $roadmap)
$updated = Replace-GeneratedBlock -Content $updated -StartMarker $roadmapStart `
    -EndMarker $roadmapEnd -Generated (New-RoadmapMarkdown -Data $data -Roadmap $roadmap)
$updated = $updated.TrimEnd() + [Environment]::NewLine

if ($updated -ceq $original) {
    Write-GeneratorLog 'README is already current.'
    return
}

if ($DryRun) {
    Write-Output $updated
    Write-GeneratorLog 'Dry run complete; README was not written.'
    return
}

Set-Content -LiteralPath $resolvedReadme -Value $updated -Encoding utf8NoBOM -NoNewline
Write-GeneratorLog 'README generated sections updated.'
