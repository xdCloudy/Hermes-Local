function Test-BenchmarkOption {
    param(
        [Parameter(Mandatory)][string] $HelpText,
        [Parameter(Mandatory)][string] $Option
    )
    $escaped = [regex]::Escape($Option)
    return $HelpText -match "(?m)(^|[\s,])$escaped(?=([\s,=<]|$))"
}

function Get-LlamaBenchHelp {
    param([Parameter(Mandatory)][string] $Binary)
    $output = & $Binary '--help' 2>&1 | Out-String
    if (-not $output.Trim()) {
        throw 'llama-bench did not return help text.'
    }
    return $output
}

function Resolve-BenchmarkGpuLayerArguments {
    param(
        [Parameter(Mandatory)][string] $Acceleration,
        [AllowNull()][object] $ConfiguredLayers,
        [Parameter(Mandatory)][string] $HelpText
    )

    if (-not (Test-BenchmarkOption -HelpText $HelpText -Option '-ngl')) {
        return @()
    }
    if ($Acceleration -ne 'cuda') {
        return @('-ngl', '0')
    }

    $value = [string]$ConfiguredLayers
    $normalised = $value.Trim().ToLowerInvariant()
    if ([string]::IsNullOrWhiteSpace($value) -or $normalised -eq 'auto') {
        # llama-bench has its own numeric parser and does not accept the
        # llama-server keyword "auto". Leaving -ngl unset preserves the installed
        # binary's native automatic/default policy without mutating the profile.
        return @()
    }
    if ($normalised -eq 'all') {
        # A high numeric ceiling is clamped by the native loader to the model's
        # actual layer count. The fitter still decides what the detected devices can hold.
        return @('-ngl', '999')
    }

    if ($value.Trim() -match '^\d+(?:-\d+)?(?:,\d+(?:-\d+)?)*$') {
        return @('-ngl', $value.Trim())
    }

    Write-HermesLog -Component benchmarks -Level WARN -Message (
        "Ignoring unsupported GPU-layer setting '$value' for llama-bench; using its native default."
    )
    return @()
}

function Get-BenchmarkBooleanValue {
    param(
        [Parameter(Mandatory)][string] $HelpText,
        [Parameter(Mandatory)][string] $Option,
        [Parameter(Mandatory)][bool] $Enabled
    )

    $escaped = [regex]::Escape($Option)
    $line = $HelpText -split '\r?\n' |
        Where-Object { $_ -match "(^|[\s,])$escaped(?=([\s,=<]|$))" } |
        Select-Object -First 1
    if ($line -match '(?i)(on\s*\|\s*off|off\s*\|\s*on)') {
        return $(if ($Enabled) { 'on' } else { 'off' })
    }
    # Current upstream llama-bench uses 0/1. This is also the safest fallback
    # for older builds whose help does not spell out the accepted boolean tokens.
    return $(if ($Enabled) { '1' } else { '0' })
}

function Add-OptionValue {
    param(
        [Parameter(Mandatory)][System.Collections.Generic.List[string]] $Arguments,
        [Parameter(Mandatory)][string] $HelpText,
        [Parameter(Mandatory)][string] $Option,
        [AllowNull()][object] $Value,
        [switch] $AllowEmpty
    )

    if (-not (Test-BenchmarkOption -HelpText $HelpText -Option $Option)) {
        return
    }
    $text = [string]$Value
    if (-not $AllowEmpty -and [string]::IsNullOrWhiteSpace($text)) {
        return
    }
    $Arguments.Add($Option)
    $Arguments.Add($text)
}

function New-BaseBenchmarkArguments {
    param(
        [Parameter(Mandatory)][object] $Profile,
        [Parameter(Mandatory)][string] $ModelPath,
        [Parameter(Mandatory)][string] $Acceleration,
        [Parameter(Mandatory)][string] $HelpText,
        [switch] $WithoutThreads,
        [switch] $WithoutBatch,
        [switch] $WithoutKv,
        [switch] $WithoutReserve
    )

    $arguments = [System.Collections.Generic.List[string]]::new()
    Add-OptionValue -Arguments $arguments -HelpText $HelpText -Option '-m' -Value $ModelPath

    if (Test-BenchmarkOption -HelpText $HelpText -Option '-fa') {
        Add-OptionValue -Arguments $arguments -HelpText $HelpText -Option '-fa' -Value (
            Get-BenchmarkBooleanValue -HelpText $HelpText -Option '-fa' -Enabled ([bool](Get-BenchmarkValue -Record $Profile -Name flashAttention -Default $false))
        )
    }
    if (Test-BenchmarkOption -HelpText $HelpText -Option '--prio') {
        Add-OptionValue -Arguments $arguments -HelpText $HelpText -Option '--prio' -Value '1'
    }

    foreach ($item in @(Resolve-BenchmarkGpuLayerArguments `
        -Acceleration $Acceleration `
        -ConfiguredLayers (Get-BenchmarkPathValue -Record $Profile -Path @('gpu', 'layers')) `
        -HelpText $HelpText)) {
        $arguments.Add([string]$item)
    }

    if ($Acceleration -eq 'cuda' -and (Test-BenchmarkOption -HelpText $HelpText -Option '-fit')) {
        Add-OptionValue -Arguments $arguments -HelpText $HelpText -Option '-fit' -Value (Get-BenchmarkBooleanValue -HelpText $HelpText -Option '-fit' -Enabled $true)
    }
    if (-not $WithoutReserve -and $Acceleration -eq 'cuda') {
        $reserve = Get-BenchmarkPathValue -Record $Profile -Path @('gpu', 'vramReserveMiB')
        if ($null -ne $reserve -and [string]$reserve -match '^\d+$') {
            Add-OptionValue -Arguments $arguments -HelpText $HelpText -Option '-fitt' -Value $reserve
        }
    }
    if (-not $WithoutKv) {
        Add-OptionValue -Arguments $arguments -HelpText $HelpText -Option '-ctk' -Value (
            Get-BenchmarkPathValue -Record $Profile -Path @('kvCache', 'keyType')
        )
        Add-OptionValue -Arguments $arguments -HelpText $HelpText -Option '-ctv' -Value (
            Get-BenchmarkPathValue -Record $Profile -Path @('kvCache', 'valueType')
        )
    }
    if (-not $WithoutThreads) {
        Add-OptionValue -Arguments $arguments -HelpText $HelpText -Option '-t' -Value (
            Get-BenchmarkPathValue -Record $Profile -Path @('threads', 'generation')
        )
    }
    if (-not $WithoutBatch) {
        Add-OptionValue -Arguments $arguments -HelpText $HelpText -Option '-b' -Value (
            Get-BenchmarkPathValue -Record $Profile -Path @('batch', 'logical')
        )
        Add-OptionValue -Arguments $arguments -HelpText $HelpText -Option '-ub' -Value (
            Get-BenchmarkPathValue -Record $Profile -Path @('batch', 'physical')
        )
    }
    return $arguments.ToArray()
}

