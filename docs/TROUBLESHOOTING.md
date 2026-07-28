# Troubleshooting

## Begin with the health test

From the project directory:

```powershell
& '.\Test-Hermes-Local.ps1' -Quick -NonInteractive
& '.\Export-Hermes-Diagnostics.ps1' -NonInteractive
```

Inspect `logs\diagnostics\latest-test.json` and the launcher Logs surface.
Never publish the DPAPI token store or a raw session database.

## Stack will not start

1. Check `data\runtime\status.json`.
2. Inspect `logs\supervisor`, `logs\model-server` and `logs\hermes`.
3. Open Models and confirm the selected GGUF says **On disk**.
4. Confirm the configured ports are not owned by unrelated processes:

```powershell
$settings = Get-Content '.\config\defaults\workstation.json' | ConvertFrom-Json
$user = if (Test-Path '.\config\launcher\user-settings.json') {
  Get-Content '.\config\launcher\user-settings.json' | ConvertFrom-Json
}
$ports = @(
  $(if ($user.network.modelPort) { $user.network.modelPort } else { $settings.network.modelPort }),
  $(if ($user.network.hermesPort) { $user.network.hermesPort } else { $settings.network.hermesPort })
)
Get-NetTCPConnection -State Listen |
  Where-Object LocalPort -In $ports |
  Select-Object LocalAddress, LocalPort, OwningProcess
```

Only loopback addresses are supported. Resolve an unrelated port owner before
stopping it, or choose different ports in the launcher.

Use the conservative CPU profile to separate accelerator/context problems:

```powershell
& '.\Restart-Hermes-Local.ps1' -Profile 'Safe Recovery' -NonInteractive
```

## Accelerator out of memory

Reduce context, logical/micro-batch size or GPU layers; increase the VRAM
reserve; close accelerator-heavy applications; or switch acceleration to CPU.
Do not assume settings measured for another model or GPU will fit yours.

After editing a profile, restart and run the quick test. Rebuild llama.cpp for
a changed acceleration mode with:

```powershell
& '.\Setup-Hermes-Local.ps1' `
  -SkipHermesDependencies -SkipModel -SkipLauncherBuild -NonInteractive
```

## Model integrity failure

The selected registration may declare `sizeBytes` and `sha256`. If either
check fails, do not launch the file. Quarantine the suspect file and rerun
setup when the model has a trusted `source`, or register a known-good GGUF.

A custom model without integrity metadata is checked for existence but cannot
receive cryptographic verification until you add its SHA-256.

## CPU/CUDA mismatch

Open Models → Runtime and network:

- **Auto** selects CUDA only when the NVIDIA driver and compiler are present.
- **CUDA** fails clearly when the toolchain or GPU is unavailable.
- **CPU** forces zero GPU layers.

Changing this setting requires rebuilding llama.cpp with setup and restarting
the stack.

## Missing or damaged dependencies

```powershell
& '.\Repair-Hermes-Local.ps1' -NonInteractive
```

Repair creates a safety backup, reinstalls locked dependencies and restarts
the previously active profile.

## Launcher cannot find the project

The portable launcher walks upward for `VERSION.json` and
`scripts\Common-Hermes.psm1`. If the executable lives elsewhere:

```powershell
$env:HERMES_LOCAL_ROOT = (Resolve-Path '.').Path
& 'C:\path\to\Hermes Launcher.exe'
```

The same root can be passed as
`--hermes-local-root=C:\path\to\Hermes-Local`.

## TUI disconnected

Use its restart control, then inspect launcher and Hermes logs. Launch the
managed CLI directly:

```powershell
$env:HERMES_HOME = (Resolve-Path '.\data\hermes').Path
& '.\runtimes\python\hermes\Scripts\hermes.exe'
```

## Backup or restore failure

Do not edit a backup ZIP in place. Verify its `.sha256` sidecar and keep the
archive under the current clone's `backups` directory. Restore rejects
absolute and parent-traversal entries and creates a pre-restore backup.
