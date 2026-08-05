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
