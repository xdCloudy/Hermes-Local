function Invoke-HermesDesktopNestedSourceGit {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string] $Repository,
        [Parameter(Mandatory)][string[]] $Arguments,
        [switch] $AllowFailure
    )

    $output = @(
        & git -C $Repository @Arguments 2>&1 |
            ForEach-Object { [string]$_ }
    )
    $exitCode = $LASTEXITCODE
    $text = ($output -join [Environment]::NewLine).Trim()

    if ($exitCode -ne 0 -and -not $AllowFailure) {
        throw "git -C $Repository $($Arguments -join ' ') failed with exit code $exitCode.`n$text"
    }

    [pscustomobject]@{
        ExitCode = $exitCode
        Text = $text
    }
}

function Get-HermesDesktopNestedSourceChanges {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string] $Repository)

    if (-not (Test-Path -LiteralPath (Join-Path $Repository '.git'))) {
        return ''
    }

    (Invoke-HermesDesktopNestedSourceGit `
        -Repository $Repository `
        -Arguments @('status', '--porcelain=v1', '--untracked-files=all') `
        -AllowFailure).Text
}

$coreVariable = Get-Variable `
    -Name HermesDesktopUpdateStageCore `
    -Scope Script `
    -ErrorAction SilentlyContinue
if (-not $coreVariable) {
    Set-Variable `
        -Name HermesDesktopUpdateStageCore `
        -Scope Script `
        -Value ${function:Invoke-HermesDesktopUpdateStage}
}

function Invoke-HermesDesktopUpdateStage {
    [CmdletBinding()]
    param([Parameter(Mandatory)][object] $Plan)

    $repository = Join-Path ([string]$Plan.root) 'source\hermes-agent'
    $nestedChanges = Get-HermesDesktopNestedSourceChanges -Repository $repository
    $items = [System.Collections.Generic.List[object]]::new()
    $coreError = $null

    if ($nestedChanges) {
        try {
            Write-HermesDesktopUpdateProgress `
                -Plan $Plan `
                -Stage preserving-local-changes `
                -Status succeeded `
                -Message 'Local Hermes Agent source changes will remain in place.' `
                -Percent 14 `
                -Failure $null `
                -Result $null | Out-Null
        } catch {
        }
    }

    try {
        & $script:HermesDesktopUpdateStageCore -Plan $Plan |
            ForEach-Object { $items.Add($_) }
    } catch {
        $coreError = $_
    }

    $structured = @(
        $items |
            Where-Object {
                $null -ne $_ -and
                $null -ne (Get-HermesDesktopObjectValue `
                    -InputObject $_ `
                    -Name status `
                    -Default $null)
            }
    ) | Select-Object -Last 1

    if ($structured) {
        Set-HermesDesktopObjectValue `
            -InputObject $structured `
            -Name nestedSourceChangesPreserved `
            -Value ([bool]$nestedChanges)
        Set-HermesDesktopObjectValue `
            -InputObject $structured `
            -Name nestedSourceChangesRestored `
            -Value ([bool]$nestedChanges)
        Set-HermesDesktopObjectValue `
            -InputObject $structured `
            -Name retainedNestedSourceStashCommit `
            -Value $null
        Set-HermesDesktopObjectValue `
            -InputObject $structured `
            -Name nestedSourceRestoreWarning `
            -Value $(if ($nestedChanges) {
                'The locally modified Hermes Agent checkout was left unchanged; run setup after reconciling those changes.'
            } else {
                $null
            })
    }

    foreach ($item in $items) {
        Write-Output $item
    }

    if ($coreError) {
        throw $coreError
    }
}
