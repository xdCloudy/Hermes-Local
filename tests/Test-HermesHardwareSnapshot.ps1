[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$root = [IO.Path]::GetFullPath((Split-Path $PSScriptRoot -Parent))
$common = Join-Path $root 'scripts\Common-Hermes.psm1'

if (-not (Test-Path -LiteralPath $common -PathType Leaf)) {
    throw "Required module is missing: $common"
}

Import-Module $common -Force

function Assert-Equal {
    param(
        [AllowNull()][object] $Actual,
        [AllowNull()][object] $Expected,
        [Parameter(Mandatory)][string] $Message
    )

    if ($Actual -ne $Expected) {
        throw "$Message Expected '$Expected'; detected '$Actual'."
    }
}

$hardwareByClass = @{
    Win32_OperatingSystem = [pscustomobject]@{
        Caption = 'Microsoft Windows Server 2025 Datacenter'
        Version = '10.0.26100'
    }
    Win32_Processor = [pscustomobject]@{
        Name = 'Synthetic CPU'
    }
    Win32_ComputerSystem = [pscustomobject]@{}
    Win32_VideoController = @(
        [pscustomobject]@{ Name = 'Microsoft Basic Display Adapter' },
        [pscustomobject]@{}
    )
}

$cimInstanceProvider = {
    param([string] $ClassName)
    return $hardwareByClass[$ClassName]
}.GetNewClosure()
$noNvidiaSmiProvider = { return $null }

$snapshot = Get-HermesHardwareSnapshot `
    -CimInstanceProvider $cimInstanceProvider `
    -NvidiaSmiProvider $noNvidiaSmiProvider

Assert-Equal $snapshot.OperatingSystem 'Microsoft Windows Server 2025 Datacenter' 'The OS caption was not retained.'
Assert-Equal $snapshot.Version '10.0.26100' 'The OS version was not retained.'
Assert-Equal $snapshot.Build $null 'A missing OS build must remain null.'
Assert-Equal $snapshot.Architecture $null 'A missing OS architecture must remain null.'
Assert-Equal $snapshot.Cpu 'Synthetic CPU' 'The CPU name was not retained.'
Assert-Equal $snapshot.PhysicalCores $null 'Missing physical-core data must remain null.'
Assert-Equal $snapshot.LogicalProcessors $null 'Missing logical-processor data must remain null.'
Assert-Equal $snapshot.MemoryBytes $null 'Missing memory data must remain null.'
Assert-Equal $snapshot.DisplayGpu $null 'A host without an NVIDIA display adapter must report a null DisplayGpu.'
Assert-Equal $snapshot.Nvidia $null 'A failed or unavailable nvidia-smi probe must report a null Nvidia device.'

$hardwareByClass.Win32_OperatingSystem = [pscustomobject]@{
    Caption = 'Microsoft Windows 11 Pro'
    Version = '10.0.26100'
    BuildNumber = '26100'
    OSArchitecture = '64-bit'
}
$hardwareByClass.Win32_Processor = [pscustomobject]@{
    Name = 'Synthetic Workstation CPU'
    NumberOfCores = 16
    NumberOfLogicalProcessors = 24
}
$hardwareByClass.Win32_ComputerSystem = [pscustomobject]@{
    TotalPhysicalMemory = [int64] 68719476736
}
$hardwareByClass.Win32_VideoController = @(
    [pscustomobject]@{ Name = 'NVIDIA GeForce RTX 4090' },
    [pscustomobject]@{ Name = 'Microsoft Basic Display Adapter' }
)
$nvidiaSmiProvider = {
    return 'NVIDIA GeForce RTX 4090, 555.99, 24564, 8.9'
}

$workstation = Get-HermesHardwareSnapshot `
    -CimInstanceProvider $cimInstanceProvider `
    -NvidiaSmiProvider $nvidiaSmiProvider

Assert-Equal $workstation.DisplayGpu 'NVIDIA GeForce RTX 4090' 'The existing NVIDIA display-adapter behaviour changed.'
Assert-Equal $workstation.Nvidia.Name 'NVIDIA GeForce RTX 4090' 'The nvidia-smi device name was not parsed.'
Assert-Equal $workstation.Nvidia.DriverVersion '555.99' 'The nvidia-smi driver version was not parsed.'
Assert-Equal $workstation.Nvidia.MemoryMiB 24564 'The nvidia-smi memory value was not parsed.'
Assert-Equal $workstation.Nvidia.ComputeCapability '8.9' 'The nvidia-smi compute capability was not parsed.'

$gpuLessSnapshotProvider = {
    return [pscustomobject]@{
        OperatingSystem = 'Microsoft Windows 11 Pro'
        Architecture = '64-bit'
        Nvidia = $null
    }
}
$cudaFailure = $null
try {
    Assert-HermesMachine `
        -Acceleration cuda `
        -RequiredFreeBytes 0 `
        -HardwareSnapshotProvider $gpuLessSnapshotProvider | Out-Null
} catch {
    $cudaFailure = $_
}

if ($null -eq $cudaFailure) {
    throw 'CUDA validation accepted a hardware snapshot without a usable nvidia-smi device.'
}
Assert-Equal `
    $cudaFailure.Exception.Message `
    'CUDA acceleration was requested, but an NVIDIA GPU was not detected by nvidia-smi.' `
    'CUDA validation returned the wrong failure.'

Write-Host 'Hermes hardware snapshot tests passed.'
