# Installation

[← Documentation home](README.md) · [Project home](../README.md) ·
[Quick start](../README.md#quick-start)

## Supported systems

Hermes Local supports 64-bit Windows 10 and Windows 11. It runs natively and
should not be installed inside WSL or a Docker filesystem.

Required:

- PowerShell 7
- Git, Node.js, uv and CMake (setup can install missing official packages with
  `winget`)
- Visual Studio 2022 C++ build tools
- at least 16 GiB free before adding model weights

Optional:

- an NVIDIA GPU, current driver and CUDA Toolkit for CUDA acceleration

CPU-only inference is supported. Performance and memory requirements depend
mostly on the selected GGUF, context length and cache type.

### Default model capacity

The tracked starter configuration uses Qwen3.6 35B-A3B APEX-MTP I-Quality.
Its main GGUF is approximately 23.5 GB and its vision projector is approximately
903 MB. Allow at least 30 GB of free disk space for those two artifacts alone;
40 GB or more is recommended once build products, dependency caches and logs are
included.

The starter profile is aimed at capable enthusiast hardware. A 12 GB NVIDIA GPU
with 32 GB system RAM is a practical minimum for mixed CPU/GPU execution;
64 GB system RAM provides substantially more headroom for 64K context, vision,
background tools and recovery operations. Lower-spec systems should register a
smaller GGUF before running full setup.

## Clone anywhere

Use a normal, non-elevated PowerShell 7 window:

```powershell
git clone https://github.com/xdCloudy/Hermes-Local.git
Set-Location .\Hermes-Local
& '.\Setup-Hermes-Local.ps1' -NonInteractive
```

The project root is derived from the scripts themselves. A drive root or a
directory missing the Hermes Local markers is rejected as a safety boundary.

## Acceleration selection

The tracked default is `auto`:

- CUDA is used when both `nvidia-smi` and `nvcc` are available;
- otherwise llama.cpp is built for CPU inference.

To force a mode before setup, create
`config\launcher\user-settings.json` from this minimal example:

```json
{
  "schemaVersion": 1,
  "runtime": {
    "acceleration": "cpu"
  }
}
```

Use `"cuda"` to require CUDA. CUDA architecture is discovered from
`nvidia-smi`; build parallelism is based on the logical CPU count. Both can be
overridden in the same runtime object using `cudaArchitecture` and
`buildParallelism`.

After the launcher is built, these choices are available under **Models →
Runtime and network**.

## Models

The starter manifest downloads Qwen3.6 35B-A3B APEX-MTP I-Quality and its
matching Qwen3.6 vision projector with resumable `curl`. Setup verifies the
published size and SHA-256 of each artifact before completing. llama-server is
then given the provisioned local projector path, so vision does not depend on
llama.cpp being compiled with HTTPS support.

The model enables its bundled MTP self-speculative decoding and uses a
1,024-token minimum for image inputs. The projector is stored beside the model:

```text
models\Qwen3.6-35B-A3B-APEX-MTP\
├── Qwen3.6-35B-A3B-APEX-MTP-I-Quality.gguf
└── mmproj.gguf
```

`-SkipModel` skips both the selected GGUF and any required sidecar artifacts,
including the vision projector. Use that switch only when every required file
is already installed at the paths recorded by the selected manifest.

To use an existing model before setup, add it to the ignored user settings:

```json
{
  "schemaVersion": 1,
  "selectedModelId": "my-model",
  "models": [
    {
      "id": "my-model",
      "displayName": "My model",
      "alias": "my-model",
      "filename": "my-model.gguf",
      "localPath": "E:\\Models\\my-model.gguf",
      "metadata": {
        "modelMaximumContextTokens": 32768
      },
      "server": {
        "jinja": true,
        "extraArguments": []
      }
    }
  ]
}
```

Only `localPath`, `id`, `displayName`, `alias` and `filename` are required.
Add `sizeBytes` and `sha256` when integrity metadata is available. Relative
paths resolve under the clone; absolute paths allow a shared model library.

After first launch, use **Models → Register GGUF** instead of editing JSON.
Registration does not copy or delete the selected weights.

## What setup does

Setup is idempotent. It:

1. validates the project root, Windows architecture and available storage;
2. reads the per-user model, profile, port and acceleration selection;
3. reconstructs the pinned official Hermes integration;
4. builds llama.cpp for CPU or the detected CUDA architecture;
5. installs the configured Python runtime and locked Hermes dependencies;
6. downloads and verifies the selected model plus required sidecar artifacts
   only when they are missing or invalid;
7. merges the selected provider/model/context into the user's Hermes YAML
   while preserving unrelated settings;
8. builds the launcher and runs schema/bootstrap checks.

### Rebuilding an existing installation

Stop Hermes Local before running setup without `-SkipLlamaBuild`:

```powershell
& '.\Stop-Hermes-Local.ps1' -NonInteractive
& '.\Setup-Hermes-Local.ps1' -SkipModel -NonInteractive
```

A running `llama-server`, `llama-cli` or `llama-bench` process can keep native
DLLs locked and prevent MSBuild from replacing them. Setup now detects those
processes before configuration and exits with a direct instruction instead of
allowing an opaque `LNK1104` linker failure.

`-SkipLlamaBuild` retains the existing native binaries. Use it only when that
build already matches the pinned llama.cpp revision and supports every argument
required by the selected model manifest.

## First start

```powershell
& '.\Start-Hermes-Local.ps1' -NonInteractive
& '.\Test-Hermes-Local.ps1' -NonInteractive
& '.\dist\Hermes Launcher.exe'
```

The start script uses the selected model and profile from the ignored user
settings. Service URLs in the launcher and tests come from the same network
configuration.

## Installer and portable launcher

The release installer and portable executable contain only the control
centre. They do not bundle model weights, the vision projector, the source
checkout or the native runtime. Provision the clone first.

When the root cannot be discovered by walking upward from the portable
executable, set it for that launch:

```powershell
$env:HERMES_LOCAL_ROOT = (Get-Location).Path
& '.\path\to\Hermes Launcher.exe'
```

You can also pass `--hermes-local-root=C:\path\to\Hermes-Local`.

## Repair and removal

```powershell
& '.\Repair-Hermes-Local.ps1' -NonInteractive
& '.\Uninstall-Hermes-Local.ps1' -NonInteractive
```

Repair creates a backup, reinstalls locked dependencies and restarts the prior
profile. Uninstall preserves user data and models unless a broader,
explicitly documented removal mode is chosen.
