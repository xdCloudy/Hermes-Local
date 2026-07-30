# Model and profile configuration

[← Documentation home](README.md) · [Project home](../README.md) ·
[User guide](USER_GUIDE.md)

## Configuration layers

Hermes Local resolves settings in this order:

1. tracked portable defaults in `config\defaults\workstation.json`;
2. tracked starter profiles in `config\profiles\profiles.json`;
3. tracked model manifests in `models\manifests`;
4. ignored current-user overrides in
   `config\launcher\user-settings.json`.

The last layer owns model/profile selection and any launcher edits. Updating a
clone therefore does not overwrite the user's choices or dirty the repository.

## Adding models

Use **Models → Register GGUF**. Hermes Launcher stores the selected absolute
path and basic metadata without copying the file. A user registration can
override a catalog model with the same ID.

A manifest or registration supports:

- `id`, `displayName`, `alias`, `filename` and `localPath`;
- optional source URL, repository, revision, licence, size and SHA-256;
- optional architecture, quantization, context limit, reasoning and native
  tool-call metadata;
- Jinja on/off, a custom chat template and safe additional llama-server
  arguments.

The model path, host, port, API-key file and log path remain owned by the
supervisor. Custom arguments cannot override those security boundaries.

## Auto tuning

Portable starter profiles use `auto` for:

- generation threads: up to eight and no more than half the logical CPUs;
- batch threads: about three quarters of logical CPUs;
- VRAM reserve: about 15% of detected NVIDIA VRAM, bounded to a practical
  starter range.

Build parallelism defaults to the logical CPU count. CUDA architecture is
derived from `nvidia-smi`. These are starting points, not universal optima.
Saving a profile records explicit values so later hardware changes do not
silently retune an established setup.

## Profile fields

| Field | Effect |
|---|---|
| `contextTokens` | Maximum live context requested from llama.cpp |
| `kvCache.keyType/valueType` | KV memory/quality trade-off |
| `threads.generation/batch` | CPU scheduling for decode and batch work |
| `batch.logical/physical` | Prompt-processing batch and micro-batch |
| `gpu.layers` | `auto`, zero for CPU, or an explicit offload count |
| `gpu.vramReserveMiB` | Memory left free when accelerator fitting is active |
| `flashAttention` | llama.cpp Flash Attention policy |
| `promptCache` | Reuse identical prompt prefixes |
| `speculativeDecoding` | Profile intent; a compatible draft model is still required |

The supervisor forces zero GPU layers in CPU mode. In CUDA mode it enables
fit-to-memory and uses the profile reserve.

## Benchmarking

```powershell
& '.\Benchmark-Hermes-Local.ps1' -Quick -NonInteractive
& '.\Benchmark-Hermes-Local.ps1' -NonInteractive
```

The harness takes its model, selected profile, paths, ports, context, threads,
batches, cache types and accelerator reserve from current configuration. It
writes results and a report but never changes tracked profiles or manifests.

Run the full gate after changing a model, context, cache type, llama.cpp
revision, driver, acceleration mode or thread/batch settings.

## Original reference measurements

The committed reports under `benchmarks\reports` document the original
Laguna/RTX 3060 validation run. They are reproducibility evidence, not defaults
for other machines. New results are generated from the current selection and
should be interpreted for that host and model only.
