# Verified prebuilt inference runtimes

Hermes Local installs a verified prebuilt `llama.cpp` runtime by default. A normal supported Windows installation no longer requires Visual Studio, CMake, Ninja, `nvcc`, or a locally installed CUDA Toolkit.

## Default installation

```powershell
.\Setup-Hermes-Local.ps1
```

Setup detects Windows and CPU capabilities, NVIDIA compute capability, driver version, RAM, VRAM, and free storage. It selects the highest-priority compatible package from `config/runtime/llama-runtime-catalog.json`. When an automatically selected CUDA package is unavailable, setup may select the verified CPU fallback and records the reason in diagnostics.

## Verification and promotion

For every release asset, Hermes Local:

1. Resolves the exact repository, release tag, and asset name.
2. Requires a release-published SHA-256 digest and checks any digest pinned in the catalog.
3. Downloads into a transaction-specific staging directory.
4. Rejects ZIP entries that escape the staging directory.
5. Checks archive size and SHA-256.
6. Runs `llama-server.exe --version`, `llama-cli.exe --version`, and `llama-bench.exe --version`.
7. Records a file-level SHA-256 inventory, source commit, build flags, compatibility range, release identity, and smoke-test evidence.
8. Promotes the validated directory into `runtimes\llama.cpp\build` only after all checks pass.

The previous working runtime is retained under `runtimes\llama.cpp\managed\rollback`. A failed download, hash check, extraction, or smoke test does not replace the active runtime.

Runtime identity is recorded in:

- `runtimes\llama.cpp\build\runtime-manifest.json`
- `runtimes\llama.cpp\managed\current.json`
- `runtimes\llama.cpp\managed\history.json`
- `data\runtime\llama-runtime.json`

## Independent update, verification, and rollback

```powershell
.\Update-Hermes-Runtime.ps1
.\Test-Hermes-Runtime.ps1 -SmokeTest
.\Rollback-Hermes-Runtime.ps1
```

Stop Hermes Local before changing the runtime. Runtime operations are independent of model downloads and Hermes Agent or Desktop updates.

## Developer/custom source build

The previous pinned source-build path remains available explicitly:

```powershell
.\Setup-Hermes-Local.ps1 -LlamaRuntimeMode source
```

Use source mode for unsupported hardware, custom upstream revisions, experimental compiler flags, custom CUDA architectures, or other developer builds. Source mode retains the existing Visual Studio, CMake, and CUDA Toolkit requirements.

## Catalog policy

Catalog entries are data, not shell input. Package IDs, repositories, tags, asset names, source commits, architectures, and digests are validated before use. Downloads are resolved through the GitHub Releases API rather than by concatenating an untrusted command line.
