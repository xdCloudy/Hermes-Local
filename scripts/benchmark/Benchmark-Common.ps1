function Get-BenchmarkValue {
    param(
        [AllowNull()]
        [object] $Record,
        [Parameter(Mandatory)]
        [string] $Name,
        [AllowNull()]
        [object] $Default = $null
    )

    if ($null -eq $Record) {
        return $Default
    }
    if ($Record -is [System.Collections.IDictionary]) {
        if ($Record.Contains($Name)) {
            return $Record[$Name]
        }
        return $Default
    }
    $property = $Record.PSObject.Properties[$Name]
    if ($property) {
        return $property.Value
    }
    return $Default
}

function Get-BenchmarkPathValue {
    param(
        [AllowNull()]
        [object] $Record,
        [Parameter(Mandatory)]
        [string[]] $Path,
        [AllowNull()]
        [object] $Default = $null
    )

    $current = $Record
    foreach ($segment in $Path) {
        $missing = [guid]::NewGuid().ToString('N')
        $value = Get-BenchmarkValue -Record $current -Name $segment -Default $missing
        if ($value -eq $missing) {
            return $Default
        }
        $current = $value
    }
    if ($null -eq $current) {
        return $Default
    }
    return $current
}

