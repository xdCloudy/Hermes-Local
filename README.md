# Hermes Local

Hermes Local is a configurable, Windows-native local AI workstation built on
the official [NousResearch Hermes Agent](https://github.com/NousResearch/hermes-agent)
and llama.cpp. Hermes Launcher combines chat, the TUI, dashboard, model
management, inference profiles, health, logs, backups, benchmarks and security
controls in one desktop application.

> [!NOTE]
> This is an independent Windows integration, not an official Nous Research
> release. The upstream Hermes Agent remains the application core.

![Hermes Launcher home screen](reports/acceptance/launcher-home.png)

## Portable by design

The repository has no required drive letter, model, GPU, CUDA architecture,
port pair or fixed CPU tuning:

- clone it to any non-system folder;
- use CPU inference or NVIDIA CUDA, with auto-detection as the default;
- register any local GGUF model and switch models from Hermes Launcher;
- create, clone, edit, select and delete inference profiles;
- change the loopback ports and acceleration policy in the launcher;
- keep selections in ignored `config\launcher\user-settings.json`, never in
  tracked defaults;
- resolve default thread counts, build parallelism and VRAM reserve from the
  current machine.

The included Laguna XS 2.1 Q4_K_M manifest is a ready-to-download starter, not
a runtime requirement. Model weights are never committed or bundled.

## Requirements

- Windows 10 or Windows 11 x64
- PowerShell 7
- about 16 GiB free before model weights; allow space for the GGUFs you choose
- Visual Studio 2022 C++ build tools for the bundled llama.cpp build
- optional NVIDIA GPU, driver and CUDA Toolkit for CUDA acceleration

Docker, WSL and paid inference APIs are not required.

## Install

Open a normal, non-elevated PowerShell 7 window:

```powershell
git clone https://github.com/xdCloudy/Hermes-Local.git
Set-Location .\Hermes-Local
& '.\Setup-Hermes-Local.ps1' -NonInteractive
```

Setup reconstructs the pinned Hermes integration, installs project-managed
dependencies, builds llama.cpp for the selected acceleration mode, downloads
the selected model when it has a source URL, generates the runtime provider
configuration and runs bootstrap diagnostics. Downloads resume when setup is
rerun.

See [Installation](docs/INSTALLATION.md) for CPU/CUDA selection, custom models
and release packages.

## Run

Commands are relative to the clone, so the examples work on any drive:

```powershell
& '.\Start-Hermes-Local.ps1' -NonInteractive
& '.\dist\Hermes Launcher.exe'
```

Stop and test the stack:

```powershell
& '.\Stop-Hermes-Local.ps1' -NonInteractive
& '.\Test-Hermes-Local.ps1' -NonInteractive
```

Omit `-Profile` to use the current launcher selection, or select one for a
single start:

```powershell
& '.\Start-Hermes-Local.ps1' -Profile 'Coding' -NonInteractive
```

## Models, profiles and settings

Open **Models** in Hermes Launcher to:

- register a `.gguf` file without copying it;
- select any registered model;
- choose `Auto`, `CUDA` or `CPU` acceleration;
- change the loopback-only model and Hermes ports;
- set build workers, CUDA architecture, Python line and startup verification
  when an automatic choice is not appropriate.

Open **Profiles** to create and tune context, KV cache, threads, batching,
offload, Flash Attention and prompt caching. Tracked starter profiles use
`auto` for machine-dependent values. Once edited, the resolved values become
explicit per-user settings.

Model registrations and profile edits are stored in
`config\launcher\user-settings.json`. That file is Git-ignored and included in
local backups. Tracked manifests under `models\manifests` remain portable by
using relative paths.

## Runtime layout

| Component | Portable location |
|---|---|
| Project root | the directory containing this README |
| Hermes checkout | `source\hermes-agent` |
| Hermes source pin override | `config\launcher\source-overrides.json` |
| Hermes user state | `data\hermes` |
| Default workspace | `data\user` |
| User settings | `config\launcher\user-settings.json` |
| Model catalog | `models\manifests\*.json` plus user registrations |
| llama.cpp build | `runtimes\llama.cpp\build` |
| Packaged launcher | `dist\Hermes Launcher.exe` |

The default endpoints are `http://127.0.0.1:8011/v1` and
`http://127.0.0.1:9119`, but both ports are configurable. The host is
deliberately restricted to IPv4 or IPv6 loopback.

The API token is random, protected for the current Windows user with DPAPI and
passed to owned processes without command-line exposure.

## Download

The [latest GitHub release](https://github.com/xdCloudy/Hermes-Local/releases/latest)
provides the Windows installer and portable launcher. Release binaries contain
the control centre, not model weights or project-managed runtimes: clone and
provision the workstation first.

The binaries are not Authenticode-signed. Verify release SHA-256 values before
running them.

## Maintenance

| Action | Command |
|---|---|
| Test | `& '.\Test-Hermes-Local.ps1' -NonInteractive` |
| Restart selected configuration | `& '.\Restart-Hermes-Local.ps1' -NonInteractive` |
| Repair | `& '.\Repair-Hermes-Local.ps1' -NonInteractive` |
| Backup | `& '.\Backup-Hermes-Local.ps1' -Name manual -NonInteractive` |
| Check all updates | `& '.\Update-Hermes-Local.ps1' -Mode Check -NonInteractive` |
| Check Hermes Agent | `& '.\Update-Hermes-Agent.ps1' -Mode Check` |
| Update Hermes Agent | `& '.\Update-Hermes-Agent.ps1' -Mode Apply` |
| Roll back Hermes Agent | `& '.\Update-Hermes-Agent.ps1' -Mode Rollback` |
| Security scan | `& '.\Security-Scan-Hermes-Local.ps1' -NonInteractive` |
| Diagnostics | `& '.\Export-Hermes-Diagnostics.ps1' -NonInteractive` |

Windows users can also double-click `Update-Hermes-Agent.cmd`. The updater
stages upstream code away from the active installation, applies the Hermes
Local patch series, backs up state, rebuilds dependencies and the launcher,
runs health checks, and restores the previous installation automatically if
promotion fails. Do not use Hermes Agent's in-chat `/update` command for a
Hermes Local checkout because it does not understand the integration patch
layer.

Historical benchmark and acceptance reports describe the original validation
machine and are labelled as evidence, not current runtime requirements.

## Documentation

- [Installation](docs/INSTALLATION.md)
- [User guide](docs/USER_GUIDE.md)
- [Architecture](docs/ARCHITECTURE.md)
- [Model and profile configuration](docs/MODEL_TUNING.md)
- [Free/local feature matrix](docs/FREE_FEATURE_MATRIX.md)
- [Security](docs/SECURITY.md)
- [Update and rollback](docs/UPDATE_AND_ROLLBACK.md)
- [Troubleshooting](docs/TROUBLESHOOTING.md)
- [Development](docs/DEVELOPMENT.md)
- [Historical acceptance results](docs/ACCEPTANCE_RESULTS.md)
