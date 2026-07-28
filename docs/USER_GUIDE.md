# User guide

## Start and stop

From the project directory:

```powershell
& '.\Start-Hermes-Local.ps1' -NonInteractive
& '.\dist\Hermes Launcher.exe'
```

Use Home or Services to start, stop, restart, repair, test and diagnose the
stack. Startup validates the selected configuration and model, starts the
model API, waits for structured health, then starts Hermes and its dashboard.

## Models

The Models page is the authority for local inference:

- **Register GGUF** adds an existing file without copying it.
- **Select** makes a registered model active for the next stack start.
- A user registration can be removed without deleting its weights.
- Built-in catalog manifests remain read-only unless you edit your fork.

The selected model card shows its alias, file, size, optional context limit,
quantization and integrity metadata. Models without native tool support still
receive a basic completion test; tool-call validation runs only when the
manifest declares that capability.

## Runtime and network

On Models, choose:

- **Auto detect** to use CUDA when the toolchain is installed and CPU
  otherwise;
- **NVIDIA CUDA** to require CUDA;
- **CPU only** to disable GPU offload;
- IPv4 or IPv6 loopback;
- separate model API and Hermes/dashboard ports from 1024 to 65535;
- automatic or explicit build workers and CUDA architecture;
- the project-managed Python major/minor line;
- whether startup re-hashes the selected model.

LAN and wildcard binding are intentionally unsupported. Restart the stack
after changing a model, profile, port or acceleration mode.

## Profiles

Profiles control context, KV cache, generation/batch threads, logical and
micro-batch sizes, GPU layers, VRAM reserve, Flash Attention, prompt caching
and speculative decoding.

Tracked starter profiles use machine-resolved values for thread counts and
VRAM reserve. Use **New profile** to copy the current settings, edit it, then
save. At least one profile is always retained.

Launcher changes are stored in ignored
`config\launcher\user-settings.json`; tracked
`config\profiles\profiles.json` remains a portable starter catalog.

CLI starts use the selected profile by default:

```powershell
& '.\Restart-Hermes-Local.ps1' -NonInteractive
```

For a one-off choice:

```powershell
& '.\Start-Hermes-Local.ps1' -Profile 'Safe Recovery' -NonInteractive
```

## Chat, TUI and dashboard

Chat is the official Hermes Desktop conversation surface. TUI opens the real
Hermes terminal UI in a Windows pseudo-terminal. The Web Dashboard button
opens the configured loopback URL.

A standalone TUI can be started with:

```powershell
$env:HERMES_HOME = (Resolve-Path '.\data\hermes').Path
& '.\runtimes\python\hermes\Scripts\hermes.exe'
```

The default working directory is `data\user`, resolved below the current clone.

## Settings file

The user file is versioned JSON and supports:

```json
{
  "schemaVersion": 1,
  "selectedModelId": "model-id",
  "selectedProfile": "Profile name",
  "network": {
    "host": "127.0.0.1",
    "modelPort": 8011,
    "hermesPort": 9119
  },
  "runtime": {
    "acceleration": "auto",
    "buildParallelism": "auto",
    "cudaArchitecture": "auto",
    "pythonVersion": "3.13",
    "verifyModelOnStart": true
  },
  "models": [],
  "profiles": []
}
```

All fields except `schemaVersion` are optional and layer over tracked defaults.
The launcher validates IDs, ranges, GGUF paths, loopback binding and reserved
llama-server arguments before writing atomically.

## Logs and diagnostics

Logs are under `logs` and the Hermes data directory. Export a redacted bundle:

```powershell
& '.\Export-Hermes-Diagnostics.ps1' -NonInteractive
```

Diagnostics report the configured model/profile/ports without including API
tokens, passwords, cookies, conversations or private file contents.

## Backup and restore

```powershell
& '.\Backup-Hermes-Local.ps1' -Name before-change -NonInteractive
& '.\Restore-Hermes-Local.ps1' -BackupPath '.\backups\<archive>.zip' -NonInteractive
```

Backups include per-user settings and local Hermes data. Restore creates a
pre-restore safety backup and restarts the prior profile.

## Security

Both services are restricted to loopback. The model API requires the
DPAPI-protected bearer token. Electron uses a sandboxed renderer, context
isolation and a narrow typed IPC bridge. File and terminal operations keep the
normal Hermes approval controls.

Do not expose the configured ports through router forwarding, a reverse proxy
or a permissive firewall rule.
