# Hermes Local

Hermes Local is a Windows-native, loopback-only AI workstation built from the
official NousResearch Hermes Agent. It combines the Hermes CLI, TUI, Desktop
and Web Dashboard with a CUDA-enabled llama.cpp server running
Laguna XS 2.1 Q4_K_M. The packaged control centre is **Hermes Launcher**.

> [!NOTE]
> This is an independent Windows integration, not an official Nous Research
> release. The upstream Hermes Agent remains the application core.

![Hermes Launcher home screen](reports/acceptance/launcher-home.png)

This installation is tuned for Windows 11 Pro, an Intel Core i5-14600K,
64 GiB RAM and an NVIDIA RTX 3060 12 GiB. All substantial source, runtimes,
model data, configuration, logs and build output live below
`D:\Hermes-Local`. Docker, WSL and paid model APIs are not required.

## Download

The [latest GitHub release](https://github.com/xdCloudy/Hermes-Local/releases/latest)
provides:

- [Windows installer](https://github.com/xdCloudy/Hermes-Local/releases/download/v0.17.0/Hermes-Launcher-0.17.0-windows-x64-setup.exe)
- [Portable launcher](https://github.com/xdCloudy/Hermes-Local/releases/download/v0.17.0/Hermes-Launcher-0.17.0-windows-x64-portable.exe)
- [Update blockmap](https://github.com/xdCloudy/Hermes-Local/releases/download/v0.17.0/Hermes-Launcher-0.17.0-windows-x64-setup.exe.blockmap)

The release binaries contain the control centre, not the 20 GB model or
project-managed runtimes. Provision the workstation from source first, then
use either launcher package. This keeps the download legal, inspectable and
resumable.

The locally built binaries are not Authenticode-signed. Windows may display a
SmartScreen warning. Verify the SHA-256 values in the release notes and the
[published packaging results](docs/ACCEPTANCE_RESULTS.md#packaging) before
running them.

## Install from source

Open a normal, non-elevated PowerShell 7 window. The project intentionally
uses the fixed `D:\Hermes-Local` root:

```powershell
Set-Location D:\
git clone https://github.com/xdCloudy/Hermes-Local.git Hermes-Local
Set-Location D:\Hermes-Local
& '.\Setup-Hermes-Local.ps1' -NonInteractive
```

Setup downloads and verifies the pinned model, reconstructs the official
Hermes integration from the committed patch series, prepares the local
runtimes and builds the CUDA-enabled backend. An interrupted model download
resumes when setup is rerun.

See the [installation guide](docs/INSTALLATION.md) for prerequisites,
storage requirements, installer behavior and first-run checks.

## Start here

Run the idempotent setup from PowerShell 7:

```powershell
& 'D:\Hermes-Local\Setup-Hermes-Local.ps1' -NonInteractive
```

Start the measured Daily profile:

```powershell
& 'D:\Hermes-Local\Start-Hermes-Local.ps1' -Profile Daily -NonInteractive
```

Open the control centre:

```powershell
& 'D:\Hermes-Local\dist\Hermes Launcher.exe'
```

Stop everything cleanly:

```powershell
& 'D:\Hermes-Local\Stop-Hermes-Local.ps1' -NonInteractive
```

The installer is
`D:\Hermes-Local\dist\Hermes-Launcher-0.17.0-windows-x64-setup.exe`; the
portable build is
`D:\Hermes-Local\dist\Hermes-Launcher-0.17.0-windows-x64-portable.exe`.

## What the launcher controls

Hermes Launcher extends the official Electron/React desktop application. It
keeps the normal Chat, Skills and Settings experiences and adds a local
workstation with Home, TUI, Web Dashboard, Services, Models, Profiles, Tasks,
Tools, Memory, Sessions, Projects, Logs, Benchmarks, Security and About
surfaces.

The Home and Services views use structured health and runtime state. The TUI
is the real Hermes TUI through a Windows pseudo-terminal and xterm.js. The
dashboard is the official Hermes dashboard on loopback. No second chatbot or
mock service replaces Hermes.

## Runtime layout

| Component | Location or endpoint |
|---|---|
| Project root | `D:\Hermes-Local` |
| Official Hermes checkout | `D:\Hermes-Local\source\hermes-agent` |
| Hermes user state | `D:\Hermes-Local\data\hermes` |
| Safe default working directory | `D:\Hermes-Local\data\user` |
| Laguna model | `D:\Hermes-Local\models\Laguna-XS-2.1\Laguna-XS-2.1-Q4_K_M.gguf` |
| llama.cpp model API | `http://127.0.0.1:8011/v1` |
| Hermes backend/dashboard | `http://127.0.0.1:9119` |
| Packaged launcher | `D:\Hermes-Local\dist\Hermes Launcher.exe` |

The local API token is randomly generated, protected per user with Windows
DPAPI and injected only into owned processes. It is not stored in Git,
renderer state or command-line arguments.

## Selected model profile

Daily is the quality-first default: 65,536 tokens, Q8_0 key/value cache,
Flash Attention, automatic CUDA fitting, 8 generation threads, 14 batch
threads, batch 1024, micro-batch 256 and 1,536 MiB VRAM reserve. Research uses
the same measured base; Deep Research provides 81,920 tokens with a 2,048 MiB
reserve. Maximum Context at 131,072 tokens is deliberately experimental.

Measured on this machine:

- 53.297 tok/s short-chat mean;
- 54.57 tok/s sustained 1,000-token decode;
- 284.582 prompt tok/s and 33.062 decode tok/s at 64K;
- 269.377 prompt tok/s and 33.347 decode tok/s at 80K;
- about 10,750 MiB peak VRAM and 18.1 GiB peak process RAM at 64K;
- no active page-file thrashing in the selected profile.

See [MODEL_TUNING.md](docs/MODEL_TUNING.md) and
[the latest benchmark report](benchmarks/reports/LATEST.md).

## Maintenance

| Action | Command |
|---|---|
| Health and real tool test | `& 'D:\Hermes-Local\Test-Hermes-Local.ps1' -NonInteractive` |
| Restart | `& 'D:\Hermes-Local\Restart-Hermes-Local.ps1' -Profile Daily -NonInteractive` |
| Repair | `& 'D:\Hermes-Local\Repair-Hermes-Local.ps1' -NonInteractive` |
| Backup | `& 'D:\Hermes-Local\Backup-Hermes-Local.ps1' -Name manual -NonInteractive` |
| Check updates | `& 'D:\Hermes-Local\Update-Hermes-Local.ps1' -Mode Check -NonInteractive` |
| Security scan | `& 'D:\Hermes-Local\Security-Scan-Hermes-Local.ps1' -NonInteractive` |
| Diagnostics | `& 'D:\Hermes-Local\Export-Hermes-Diagnostics.ps1' -NonInteractive` |

Updates are checked, staged, backed up and smoke-tested before switching.
Rollback restores the last known-good component without touching the model,
sessions, memory or skills. See
[UPDATE_AND_ROLLBACK.md](docs/UPDATE_AND_ROLLBACK.md).

## Security model

Both services bind to `127.0.0.1`; LAN mode is not implemented. Model
inference requires the DPAPI-backed bearer token. Electron uses context
isolation, sandboxing, no Node integration, web security, a strict CSP, exact
navigation controls and a narrow schema-validated preload bridge. Dangerous
terminal work, memory writes and skill writes require explicit approval.

The final repeatable scan passed with three triaged, non-reachable or optional
dependency advisories, zero installed Python vulnerabilities, zero production
secret findings and a clean Defender artifact scan. See
[SECURITY.md](docs/SECURITY.md) and
[SECURITY_REPORT.md](security/reports/SECURITY_REPORT.md).

## Documentation

- [Installation](docs/INSTALLATION.md)
- [User guide](docs/USER_GUIDE.md)
- [Architecture](docs/ARCHITECTURE.md)
- [Free/local feature matrix](docs/FREE_FEATURE_MATRIX.md)
- [Model tuning](docs/MODEL_TUNING.md)
- [Security](docs/SECURITY.md)
- [Update and rollback](docs/UPDATE_AND_ROLLBACK.md)
- [Troubleshooting](docs/TROUBLESHOOTING.md)
- [Development](docs/DEVELOPMENT.md)
- [Acceptance results](docs/ACCEPTANCE_RESULTS.md)
