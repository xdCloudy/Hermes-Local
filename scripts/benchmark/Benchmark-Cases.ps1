function New-BenchmarkCase {
    param(
        [Parameter(Mandatory)][string] $Name,
        [Parameter(Mandatory)]
        [AllowEmptyCollection()]
        [string[]] $Arguments
    )
    return [ordered]@{ name = $Name; args = $Arguments }
}

function Add-BenchmarkCase {
    param(
        [Parameter(Mandatory)]
        [AllowEmptyCollection()]
        [System.Collections.Generic.List[object]] $Cases,
        [Parameter(Mandatory)][string] $Name,
        [Parameter(Mandatory)]
        [AllowEmptyCollection()]
        [string[]] $Arguments
    )
    $Cases.Add((New-BenchmarkCase -Name $Name -Arguments $Arguments))
}

function Get-RelativeSweep {
    param(
        [Parameter(Mandatory)][int] $Current,
        [int] $Minimum = 1,
        [int] $Maximum = [int]::MaxValue,
        [double] $Factor = 2
    )
    return @(
        [math]::Max($Minimum, [int][math]::Floor($Current / $Factor)),
        [math]::Max($Minimum, [math]::Min($Maximum, $Current)),
        [math]::Max($Minimum, [math]::Min($Maximum, [int][math]::Ceiling($Current * $Factor)))
    ) | Sort-Object -Unique
}

function New-BenchmarkCases {
    param(
        [Parameter(Mandatory)][object] $Profile,
        [Parameter(Mandatory)][string] $ModelPath,
        [Parameter(Mandatory)][string] $Acceleration,
        [Parameter(Mandatory)][string] $HelpText,
        [Parameter(Mandatory)][int] $MaximumContext,
        [switch] $QuickMode
    )

    $base = New-BaseBenchmarkArguments -Profile $Profile -ModelPath $ModelPath -Acceleration $Acceleration -HelpText $HelpText
    $withoutThreads = New-BaseBenchmarkArguments -Profile $Profile -ModelPath $ModelPath -Acceleration $Acceleration -HelpText $HelpText -WithoutThreads
    $withoutBatch = New-BaseBenchmarkArguments -Profile $Profile -ModelPath $ModelPath -Acceleration $Acceleration -HelpText $HelpText -WithoutBatch
    $withoutKv = New-BaseBenchmarkArguments -Profile $Profile -ModelPath $ModelPath -Acceleration $Acceleration -HelpText $HelpText -WithoutKv
    $withoutReserve = New-BaseBenchmarkArguments -Profile $Profile -ModelPath $ModelPath -Acceleration $Acceleration -HelpText $HelpText -WithoutReserve
    $cases = [System.Collections.Generic.List[object]]::new()

    Add-BenchmarkCase -Cases $cases -Name 'short-chat-2k' -Arguments ($base + @('-p', '2048', '-n', '128', '-r', $(if ($QuickMode) { '2' } else { '3' })))
    Add-BenchmarkCase -Cases $cases -Name 'sustained-decode-1000' -Arguments ($base + @('-p', '0', '-n', '1000', '-r', '1'))
    if ($QuickMode) {
        return $cases.ToArray()
    }

    $ordered = [System.Collections.Generic.List[object]]::new()
    $coldArguments = $base
    if (Test-BenchmarkOption -HelpText $HelpText -Option '--no-warmup') {
        $coldArguments += '--no-warmup'
    }
    Add-BenchmarkCase -Cases $ordered -Name 'cold-start' -Arguments ($coldArguments + @('-p', '32', '-n', '0', '-r', '1'))
    Add-BenchmarkCase -Cases $ordered -Name 'short-chat-2k' -Arguments ($base + @('-p', '2048', '-n', '128', '-r', '3'))

    $selectedContext = [int](Get-BenchmarkValue -Record $Profile -Name contextTokens -Default 32768)
    $contextTargets = @(16384, 32768, $selectedContext) |
        Where-Object { $_ -gt 0 -and $_ -le $MaximumContext } |
        Sort-Object -Unique
    foreach ($context in $contextTargets) {
        Add-BenchmarkCase -Cases $ordered -Name "context-$([math]::Round($context / 1024))k" -Arguments (
            $base + @('-p', [string]$context, '-n', '32', '-r', $(if ($context -le 32768) { '2' } else { '1' }))
        )
    }

    Add-BenchmarkCase -Cases $ordered -Name 'long-prefill' -Arguments ($base + @('-p', [string][math]::Min($selectedContext, $MaximumContext), '-n', '0', '-r', '1'))
    Add-BenchmarkCase -Cases $ordered -Name 'sustained-decode-1000' -Arguments ($base + @('-p', '0', '-n', '1000', '-r', '1'))
    Add-BenchmarkCase -Cases $ordered -Name 'warm-standard' -Arguments ($base + @('-p', '2048', '-n', '256', '-r', '3'))

    $threads = [int](Get-BenchmarkPathValue -Record $Profile -Path @('threads', 'generation') -Default ([math]::Max(1, [int][math]::Floor([Environment]::ProcessorCount / 2))))
    if (Test-BenchmarkOption -HelpText $HelpText -Option '-t') {
        $threadSweep = @(Get-RelativeSweep -Current $threads -Minimum 1 -Maximum ([Environment]::ProcessorCount) -Factor 1.5)
        Add-BenchmarkCase -Cases $ordered -Name 'thread-sweep' -Arguments ($withoutThreads + @('-t', ($threadSweep -join ','), '-p', '0', '-n', '256', '-r', '2'))
    }

    $cpuMoe = [int](Get-BenchmarkPathValue -Record $Profile -Path @('cpu', 'moeLayers') -Default 0)
    if ($cpuMoe -gt 0 -and (Test-BenchmarkOption -HelpText $HelpText -Option '-ncmoe')) {
        $moeSweep = @(0, [math]::Max(1, [int][math]::Floor($cpuMoe / 2)), $cpuMoe) | Sort-Object -Unique
        Add-BenchmarkCase -Cases $ordered -Name 'cpu-moe-sweep' -Arguments ($base + @('-ncmoe', ($moeSweep -join ','), '-p', '0', '-n', '256', '-r', '2'))
    }

    $batch = [int](Get-BenchmarkPathValue -Record $Profile -Path @('batch', 'logical') -Default 512)
    $ubatch = [int](Get-BenchmarkPathValue -Record $Profile -Path @('batch', 'physical') -Default ([math]::Min(256, $batch)))
    if ((Test-BenchmarkOption -HelpText $HelpText -Option '-b') -and (Test-BenchmarkOption -HelpText $HelpText -Option '-ub')) {
        foreach ($candidate in @(Get-RelativeSweep -Current $batch -Minimum 32 -Factor 2)) {
            $candidateUbatch = [math]::Max(1, [math]::Min($candidate, [int][math]::Round($candidate * ($ubatch / [double][math]::Max(1, $batch)))))
            Add-BenchmarkCase -Cases $ordered -Name "batch-sweep-$candidate" -Arguments (
                $withoutBatch + @('-b', [string]$candidate, '-ub', [string]$candidateUbatch, '-p', '4096', '-n', '0', '-r', '2')
            )
        }
    }

    if ((Test-BenchmarkOption -HelpText $HelpText -Option '-ctk') -and (Test-BenchmarkOption -HelpText $HelpText -Option '-ctv')) {
        $selectedKey = [string](Get-BenchmarkPathValue -Record $Profile -Path @('kvCache', 'keyType') -Default '')
        $selectedValue = [string](Get-BenchmarkPathValue -Record $Profile -Path @('kvCache', 'valueType') -Default '')
        $kvPairs = [System.Collections.Generic.List[object]]::new()
        if ($selectedKey -and $selectedValue) {
            $kvPairs.Add(@($selectedKey, $selectedValue))
        }
        foreach ($candidate in @('f16', 'q8_0', 'q4_0')) {
            if ($HelpText -match [regex]::Escape($candidate)) {
                $kvPairs.Add(@($candidate, $candidate))
            }
        }
        $seenKv = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
        foreach ($pair in $kvPairs) {
            $key = "$($pair[0])/$($pair[1])"
            if ($seenKv.Add($key)) {
                $slug = ($key -replace '[^A-Za-z0-9]+', '-').Trim('-').ToLowerInvariant()
                Add-BenchmarkCase -Cases $ordered -Name "kv-sweep-$slug" -Arguments (
                    $withoutKv + @('-ctk', [string]$pair[0], '-ctv', [string]$pair[1], '-p', '4096', '-n', '128', '-r', '2')
                )
            }
        }
    }

    $reserve = [int](Get-BenchmarkPathValue -Record $Profile -Path @('gpu', 'vramReserveMiB') -Default 0)
    if ($Acceleration -eq 'cuda' -and $reserve -gt 0 -and (Test-BenchmarkOption -HelpText $HelpText -Option '-fitt')) {
        $reserveSweep = @(
            [math]::Max(0, $reserve - [math]::Max(256, [int]($reserve / 3))),
            $reserve,
            $reserve + [math]::Max(256, [int]($reserve / 3))
        ) | Sort-Object -Unique
        Add-BenchmarkCase -Cases $ordered -Name 'reserve-sweep' -Arguments (
            $withoutReserve + @('-fitt', ($reserveSweep -join ','), '-p', '4096', '-n', '128', '-r', '2')
        )
    }

    return $ordered.ToArray()
}
