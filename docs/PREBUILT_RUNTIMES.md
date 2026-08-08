# Verified prebuilt inference runtimes

Hermes Local installs a verified prebuilt `llama.cpp` runtime by default. A normal supported Windows installation no longer requires Visual Studio, CMake, Ninja, `nvcc`, or a locally installed CUDA Toolkit.

## Default installation

```powershell
.\Setup-Hermes-Local.ps1
```

Setup detects Windows and CPU capabilities, NVIDIA compute capability, driver version, RAM, VRAM, and free storage. It selects the highest-priority compatible package from `config/runtime/llama-runtime-catalog.json`. When an automatically selected CUDA package is unavailable, setup may select the verified CPU fallback and records the reason in diagnostics.

The selected model format is part of compatibility resolution. Current official packages explicitly support `gguf`; a runtime cannot be selected for a model format it does not declare.

## Canonical package identity

Runtime identity is data, not a filename convention. Every catalog package declares and Hermes Local fingerprints:

- runtime family (`llama.cpp`) and distribution;
- package ID and package version;
- exact upstream repository and source commit;
- Windows platform and CPU/CUDA backend;
- build flags and CUDA architecture set;
- supported model formats;
- exact release repository, tag and asset set;
- integrity and dependency-inventory policy;
- licence declarations and provenance.

`Get-HermesLlamaRuntimePackageIdentity` serializes those fields in a deterministic order and records a SHA-256 identity fingerprint. Setup, the runtime CLI and the Update Centre all use the same resolver and fingerprint. A decision whose package data no longer matches the authoritative catalog is rejected before any asset is downloaded or active runtime location is changed.

The package filename is therefore evidence only. It is never sufficient on its own to establish runtime identity.

## Managed lifecycle

The catalog declares the locations used by the runtime manager:

| Role | Location |
|---|---|
| Candidate staging | `runtimes/llama.cpp/managed/staging` |
| Active runtime | `runtimes/llama.cpp/build` |
| Retained known-good runtimes | `runtimes/llama.cpp/managed/rollback` |
| Current state | `runtimes/llama.cpp/managed/current.json` |
| Lifecycle history | `runtimes/llama.cpp/managed/history.json` |
| Diagnostic projection | `data/runtime/llama-runtime.json` |

Lifecycle paths must be project-relative, remain beneath the Hermes Local root, and keep staging, active and retained locations distinct. The runtime manager consumes these declarations directly rather than maintaining an independent path contract.

## Verification and promotion

For every release asset, Hermes Local:

1. Revalidates the selected package identity against the current catalog, hardware and selected model format.
2. Resolves the exact repository, release tag and asset name.
3. Requires a release-published SHA-256 digest and checks any digest pinned in the catalog.
4. Downloads into a transaction-specific staging directory.
5. Rejects ZIP entries that escape the staging directory.
6. Checks archive size and SHA-256.
7. Runs `llama-server.exe --version`, `llama-cli.exe --version`, and `llama-bench.exe --version`.
8. Records a file-level SHA-256 inventory plus an executable/DLL dependency inventory.
9. Records canonical identity, source commit, build flags, compatibility range, licences, provenance, release identity and smoke-test evidence.
10. Promotes the validated directory into the declared active location only after all checks pass.

The previous working runtime is retained at the declared retained location. A failed compatibility check, download, hash check, extraction or smoke test does not replace the active runtime.

Rollback performs the same fail-closed checks: every retained file is hash-verified and a managed package is checked against the current catalog, hardware and model format **before** the active directory is displaced.

Runtime identity is recorded in:

- `runtimes\llama.cpp\build\runtime-manifest.json`
- `runtimes\llama.cpp\managed\current.json`
- `runtimes\llama.cpp\managed\history.json`
- `data\runtime\llama-runtime.json`

## Update Centre integration

The `LlamaCpp` Update Centre component is a transactional adapter over the same runtime manager used by setup. Check results expose installed and target identities, compatibility state and lifecycle paths; apply and rollback use the same canonical package identity.

```powershell
.\Update-Hermes-Local.ps1 -Mode Check -Component LlamaCpp
.\Update-Hermes-Local.ps1 -Mode Apply -Component LlamaCpp -NonInteractive
.\Update-Hermes-Local.ps1 -Mode Rollback -Component LlamaCpp -NonInteractive
```

The dedicated recovery entry points delegate to that same adapter:

```powershell
.\Update-Hermes-Runtime.ps1
.\Test-Hermes-Runtime.ps1 -SmokeTest
.\Rollback-Hermes-Runtime.ps1
```

Stop Hermes Local before changing the runtime. Runtime operations remain independent of model downloads and Hermes Agent or Desktop updates.

## Developer/custom source build

The previous pinned source-build path remains available explicitly:

```powershell
.\Setup-Hermes-Local.ps1 -LlamaRuntimeMode source
```

Use source mode for unsupported hardware, custom upstream revisions, experimental compiler flags, custom CUDA architectures, or other developer builds. Source mode retains the existing Visual Studio, CMake, and CUDA Toolkit requirements.

## Catalog policy

Catalog entries are data, not shell input. Package IDs, repositories, tags, asset names, source commits, architectures, model formats, lifecycle paths and digests are validated before use. Downloads are resolved through the GitHub Releases API rather than by concatenating an untrusted command line.
