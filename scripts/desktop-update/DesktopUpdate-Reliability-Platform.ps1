function Invoke-HermesDesktopGit {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string[]] $Arguments,
        [switch] $AllowFailure,
        [ValidateRange(0, 5)][int] $MaxAttempts = 0
    )

    $networkCommand = $Arguments.Count -gt 0 -and
        $Arguments[0] -in @('fetch', 'ls-remote')
    if ($MaxAttempts -le 0) {
        $MaxAttempts = if ($networkCommand) { 3 } else { 1 }
    }

    $lastExitCode = -1
    $lastText = ''
    for ($attempt = 1; $attempt -le $MaxAttempts; $attempt += 1) {
        Push-Location $root
        try {
            $previousNativePreference = $PSNativeCommandUseErrorActionPreference
            $PSNativeCommandUseErrorActionPreference = $false
            try {
                $output = @(& git @Arguments 2>&1 | ForEach-Object { [string]$_ })
                $lastExitCode = $LASTEXITCODE
            } finally {
                $PSNativeCommandUseErrorActionPreference = $previousNativePreference
            }
        } finally {
            Pop-Location
        }
        $lastText = ($output -join [Environment]::NewLine).Trim()
        $null = Add-HermesDesktopUpdateLog -Plan $null -Message (
            "git $($Arguments -join ' ') [attempt $attempt/$MaxAttempts, exit $lastExitCode]" +
            $(if ($lastText) { "`n$lastText" } else { '' })
        )

        if ($lastExitCode -eq 0) {
            break
        }
        if ($attempt -lt $MaxAttempts) {
            Start-Sleep -Seconds ([math]::Min(12, [math]::Pow(2, $attempt)))
        }
    }

    if ($lastExitCode -ne 0 -and -not $AllowFailure) {
        throw (
            "git $($Arguments -join ' ') failed with exit code $lastExitCode after " +
            "$MaxAttempts attempt(s).`n$lastText`nFull update log: " +
            (Get-HermesDesktopUpdateLogPath -Plan $null)
        )
    }

    [pscustomobject]@{
        ExitCode = $lastExitCode
        Text = $lastText
        Attempts = $MaxAttempts
    }
}

function Invoke-HermesDesktopNestedSourceGit {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $Repository,
        [Parameter(Mandatory)][string[]] $Arguments,
        [switch] $AllowFailure,
        [ValidateRange(0, 5)][int] $MaxAttempts = 0
    )

    $networkCommand = $Arguments.Count -gt 0 -and
        $Arguments[0] -in @('fetch', 'ls-remote')
    if ($MaxAttempts -le 0) {
        $MaxAttempts = if ($networkCommand) { 3 } else { 1 }
    }

    $lastExitCode = -1
    $lastText = ''
    for ($attempt = 1; $attempt -le $MaxAttempts; $attempt += 1) {
        $previousNativePreference = $PSNativeCommandUseErrorActionPreference
        $PSNativeCommandUseErrorActionPreference = $false
        try {
            $output = @(
                & git -C $Repository @Arguments 2>&1 |
                    ForEach-Object { [string]$_ }
            )
            $lastExitCode = $LASTEXITCODE
        } finally {
            $PSNativeCommandUseErrorActionPreference = $previousNativePreference
        }
        $lastText = ($output -join [Environment]::NewLine).Trim()
        $null = Add-HermesDesktopUpdateLog -Plan $null -Message (
            "git -C $Repository $($Arguments -join ' ') " +
            "[attempt $attempt/$MaxAttempts, exit $lastExitCode]" +
            $(if ($lastText) { "`n$lastText" } else { '' })
        )
        if ($lastExitCode -eq 0) {
            break
        }
        if ($attempt -lt $MaxAttempts) {
            Start-Sleep -Seconds ([math]::Min(12, [math]::Pow(2, $attempt)))
        }
    }

    if ($lastExitCode -ne 0 -and -not $AllowFailure) {
        throw (
            "git -C $Repository $($Arguments -join ' ') failed with exit code " +
            "$lastExitCode after $MaxAttempts attempt(s).`n$lastText`nFull update log: " +
            (Get-HermesDesktopUpdateLogPath -Plan $null)
        )
    }

    [pscustomobject]@{
        ExitCode = $lastExitCode
        Text = $lastText
        Attempts = $MaxAttempts
    }
}

function Get-HermesDesktopGitDirectory {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string] $Repository)

    if (-not (Test-Path -LiteralPath $Repository -PathType Container)) {
        return $null
    }

    $repositoryPath = [IO.Path]::GetFullPath($Repository)
    $dotGit = Join-Path $repositoryPath '.git'
    if (Test-Path -LiteralPath $dotGit -PathType Container) {
        return [IO.Path]::GetFullPath($dotGit)
    }

    if (Test-Path -LiteralPath $dotGit -PathType Leaf) {
        $pointer = (Get-Content -Raw -LiteralPath $dotGit).Trim()
        if ($pointer -match '^gitdir:\s*(.+)$') {
            $target = $Matches[1].Trim()
            if (-not [IO.Path]::IsPathFullyQualified($target)) {
                $target = Join-Path $repositoryPath $target
            }
            $resolved = [IO.Path]::GetFullPath($target)
            if (Test-Path -LiteralPath $resolved -PathType Container) {
                return $resolved
            }
        }
    }

    $resolvedGitDirectory = Invoke-HermesDesktopNestedSourceGit `
        -Repository $repositoryPath `
        -Arguments @('rev-parse', '--absolute-git-dir') `
        -AllowFailure
    if (
        $resolvedGitDirectory.ExitCode -eq 0 -and
        -not [string]::IsNullOrWhiteSpace([string]$resolvedGitDirectory.Text)
    ) {
        $resolved = [IO.Path]::GetFullPath(([string]$resolvedGitDirectory.Text).Trim())
        if (Test-Path -LiteralPath $resolved -PathType Container) {
            return $resolved
        }
    }

    $null
}
