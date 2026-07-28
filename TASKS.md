# Hermes Local Task Ledger

> Historical ledger for the original reference build. Hardware and model
> entries below are acceptance evidence, not current installation requirements.

Last updated: 2026-07-28

## Evidence captured

- [x] Windows 11 Pro x64 detected.
- [x] Intel Core i5-14600K detected: 14 cores / 20 logical processors.
- [x] NVIDIA GeForce RTX 3060 detected: 12,288 MiB, compute capability 8.6.
- [x] 64 GiB physical memory detected.
- [x] `D:` verified with approximately 4.1 TB free.
- [x] PowerShell 7, Git, Python 3.11, uv, Node, npm, CMake, Ninja, VS Build
  Tools, ripgrep, and ffmpeg detected.
- [x] Official Hermes cloned and pinned at
  `3be565fbdee3115ab5b9338551768b8e5e655c56`.
- [x] Hermes `upstream` remote preserved and local
  `hermes-local-integration` branch created.
- [x] Official model metadata captured, including repository revision, exact
  file size, OpenMDW-1.1 licence, and upstream SHA-256.
- [x] llama.cpp Laguna support status checked: PR 25165 merged 2026-07-22.
- [x] CUDA Toolkit 13.3.1 installed from NVIDIA and installer SHA-256
  verified against the Microsoft WinGet manifest.
- [x] CUDA-enabled llama.cpp build `10154` (`0e4a0362`) produced
  `llama-server.exe`, `llama-cli.exe`, and `llama-bench.exe`.
- [x] Runtime probe sees the RTX 3060 as `CUDA0` with 12,287 MiB VRAM.
- [x] Laguna GGUF downloaded at 20,274,300,032 bytes and local SHA-256
  verified as `1ac7079101fca5a6df8c5a7523a3c30ea7d1c0e4b1258090e7d6d4039287f6cb`.
- [x] Benchmark-profile server load passed at 32K context in about 5.4 seconds.
- [x] Deterministic generation returned exactly `LAGUNA_OK`.
- [x] Native OpenAI-format function call returned schema-valid JSON.
- [x] Hermes made a real terminal tool call through the protected local model
  endpoint and returned `HERMES_TOOL_OK`.
- [x] Model and Hermes services bind to `127.0.0.1` and use a per-user
  DPAPI-protected local API token.
- [x] Persistent supervisor uses a Windows Job Object, ordered structured
  health checks, PID tracking, restart backoff, and bounded tree cleanup.

## Current phase

- [x] Finish diagnostics, update, backup/restore, benchmark, security, build,
  and package scripts.
- [x] Clone and build pinned CUDA-enabled llama.cpp.
- [x] Download, resume, and verify Laguna XS 2.1 Q4_K_M.
- [x] Install Hermes Python and Node dependencies into project-managed runtimes.
- [x] Configure encrypted local authentication and Hermes custom endpoint.
- [x] Run model-load, generation, and real Hermes tool-call smoke tests.

## Product, tuning, security, and release gates

- [x] Produce a complete dark/light launcher design concept and design spec.
- [x] Extend official Hermes Desktop into Hermes Launcher.
- [x] Implement Windows ConPTY TUI and real process supervisor.
- [x] Complete all required launcher navigation and operational surfaces.
- [x] Run context, KV, offload, thread, batch, cache, and speculation
  applicability sweeps.
- [x] Select and publish measured named profiles.
- [x] Complete threat model and repository security scan phases.
- [x] Validate and remediate reachable findings with regression tests.
- [x] Generate dependency audits, licence inventory, Defender scan, and SBOM.
- [x] Complete critical unit, integration, agent, UI, packaging, update,
  rollback, repair, backup, restore, offline and uninstall acceptance tests.
- [x] Produce portable build, installer, Start Menu shortcut, optional desktop
  shortcut, clean uninstaller, documentation, benchmark reports, and security
  reports.

## Recorded exceptions

- [ ] Perform one physical Windows reboot and confirm unattended launch-at-login.
  The current-user login item was toggled/restored and cold/offline restarts
  passed, but rebooting would terminate the active Codex build task.
- [ ] Upstream full Desktop Vitest is not wholly portable: 3,332 tests pass,
  21 fail and 3 skip, plus two POSIX package-test import errors. Nineteen
  failures are POSIX-only SSH/path/mode/symlink fixtures; two are disabled
  cloud-billing UI assertions. No failures are hidden. The Windows-critical
  Hermes suite passes 463/463 applicable tests.
- [ ] Authenticode-sign the locally built launcher if it will be distributed
  beyond this workstation.
