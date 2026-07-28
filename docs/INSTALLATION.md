# Installation

## Supported machine

This release is built and measured for Windows 11 Pro x64, PowerShell 7,
Intel Core i5-14600K, 64 GiB RAM and NVIDIA RTX 3060 12 GiB. It requires a
CUDA-capable NVIDIA driver and approximately 30 GiB of fast local storage for
the model, source, runtimes, browser and build products.

The implementation is Windows-native. Do not install it inside WSL and do not
substitute Docker paths.

## Full setup

Open a normal, non-elevated PowerShell 7 window and run:

```powershell
& 'D:\Hermes-Local\Setup-Hermes-Local.ps1' -NonInteractive
```

Setup validates the OS, architecture, CPU, memory, free space, GPU and CUDA
driver. It installs only missing official prerequisites, preserves the pinned
source integration, restores locked Python/Node dependencies, builds
llama.cpp when requested, verifies the 20,274,300,032-byte model, builds the
launcher and runs bootstrap diagnostics.

Large model downloads use `curl.exe --continue-at -`; rerunning setup resumes
an interrupted partial file. The final file must match SHA-256
`1ac7079101fca5a6df8c5a7523a3c30ea7d1c0e4b1258090e7d6d4039287f6cb`.

Setup is idempotent. It does not reset a verified Hermes Local integration
tree, replace user configuration or re-download a verified model.

## First start

```powershell
& 'D:\Hermes-Local\Start-Hermes-Local.ps1' -Profile Daily -NonInteractive
& 'D:\Hermes-Local\Test-Hermes-Local.ps1' -NonInteractive
& 'D:\Hermes-Local\dist\Hermes Launcher.exe'
```

Model startup normally takes about six seconds, with additional time on the
first Windows launch. The test must report nine passed checks, including
authenticated inventory, loopback binding, a native tool-call schema and a
real Hermes terminal tool call.

## Installer and portable application

The assisted per-user installer is:

```text
D:\Hermes-Local\dist\Hermes-Launcher-0.17.0-windows-x64-setup.exe
```

It supports changing the installation directory, creates Start Menu and
optional Desktop shortcuts, does not require machine-wide installation and
provides a clean uninstaller. The uninstaller removes application files and
shortcuts; it does not remove `D:\Hermes-Local` user data, model or runtime.

The portable build is:

```text
D:\Hermes-Local\dist\Hermes-Launcher-0.17.0-windows-x64-portable.exe
```

`D:\Hermes-Local\dist\Hermes Launcher.exe` is an identical convenience copy
of that portable artifact.

## Launch at login

Open About in Hermes Launcher and enable **Launch at login**. This uses the
current-user Electron login item; it does not create a scheduled task or
request elevation. Disable the same switch to remove the entry. The setting
was acceptance-tested by toggling and restoring it.

## Offline use

Once installed, Daily startup does not need the internet. The launcher uses
bundled/system fonts; the model and browser binaries are local. Browser
research and network-backed optional tools naturally require connectivity.
An offline-equivalent restart through a dead outbound proxy passed the local
health suite.

## Repair

If a dependency is missing or damaged:

```powershell
& 'D:\Hermes-Local\Repair-Hermes-Local.ps1' -NonInteractive
```

Repair stops a running stack, creates a pre-repair backup, force-reinstalls
locked dependencies, preserves the exact integration tree, runs bootstrap
diagnostics and restarts the prior profile. It does not overwrite user data.

## Removal

To remove only packaged application files, use **Uninstall Hermes Launcher**
from Windows Settings or the installation folder.

To stop services and remove generated machine-local launcher state while
preserving user data by default:

```powershell
& 'D:\Hermes-Local\Uninstall-Hermes-Local.ps1' -NonInteractive
```

Read the script help before requesting any broader data removal. The model,
sessions, memory and skills are valuable data and should be backed up first.
