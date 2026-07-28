# Hermes Local hardening record

Reviewed: 2026-07-28

## Platform and process

- Native Windows only; no Docker/WSL runtime dependency.
- Runs as the current user; no hidden elevation.
- Supervisor uses bounded restart/backoff and native process-tree cleanup.
- Services bind to `127.0.0.1`; acceptance tests reject non-loopback listeners.
- Terminal, updater, and launcher process calls use validated executables and argument arrays.
- Model size and SHA-256 are pinned and rechecked.
- Windows Defender remains enabled; the built distribution scanned clean.

## Authentication and secrets

- The local API token is generated with a cryptographic RNG and stored as current-user DPAPI ciphertext.
- The token is not placed in Git, screenshots, renderer snapshots, process arguments, or diagnostics.
- Model inference rejects unauthenticated calls.
- Hermes REST and WebSocket paths use generated session tokens, constant-time comparison, loopback peer checks, and Host/Origin controls.
- Logs and diagnostics redact credential-shaped values.

## Electron

- `contextIsolation: true`
- `nodeIntegration: false`
- `sandbox: true`
- `webSecurity: true`
- Electron remote module unavailable/unused
- strict CSP with same-origin scripts and no inline/eval execution
- exact renderer navigation allowlist
- new-window denial by default
- permission requests denied by default; only audio-only capture from an owned trusted renderer is allowed
- narrow preload surface; no generic Node/Electron escape hatch
- schema/range/path validation on Hermes Local IPC
- renderer profile values validated before process creation
- no untrusted remote content in a privileged BrowserWindow
- DOMPurify SVG sanitisation and bounded file/data previews

## Files, archives, and updates

- Paths are canonicalised and checked against intended roots before sensitive reads/writes.
- Archive extraction uses safe staging and traversal checks.
- Config writes are atomic and optionally backed up.
- Update modes separate check/apply; rollback restores a known-good snapshot and reruns health checks.
- Repair/update/uninstall preserve user data by default.
- Uninstaller verification confirmed model/runtime preservation.

## Agent and terminal

- Safe default cwd is `D:\Hermes-Local\data\user`.
- Exact command and cwd are visible at approval time.
- Dangerous operations require the configured approval policy.
- No hidden elevation.
- Cancellation, timeout, output limits, and process-tree cleanup are available.
- Installation directories are treated as protected operational state.

## Supply chain

- Official Hermes and llama.cpp revisions are recorded in `VERSION.json`.
- Python and Node dependencies are locked.
- npm audit, pip-audit, OSV-Scanner, Semgrep, Ruff, TypeScript, ESLint, Gitleaks, and Defender are repeatable through `Security-Scan-Hermes-Local.ps1`.
- CycloneDX 1.6 SBOMs cover 616 Node components and 127 Python dependency components.
- Dependency license inventories are stored beside the SBOMs.
- Gitleaks production source and recent integration history are clean under the documented false-positive policy.

## Remaining improvements

- Authenticode-sign the installer and launcher before wider distribution.
- Re-run the optional Discord voice review once `discord.py` allows fixed PyNaCl.
- Upgrade React Router when a fixed client-compatible release is available.
- Replace vulnerable build-tool transitive chains when upstream electron-builder/ESLint releases permit it without regressions.
- Add a reproducible Windows VM test for reboot/login-start behavior.
