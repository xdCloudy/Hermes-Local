# Hermes Local changelog

## Unreleased

- Added a transactional Hermes Agent updater that stages upstream source away
  from the active installation and applies the ordered Hermes Local patch series
  with three-way merge support.
- Added automatic source, Python environment, launcher and source-pin rollback
  when dependency rebuilds or runtime health checks fail.
- Added a machine-local source pin override consumed by setup, repair,
  diagnostics and future update checks without modifying tracked defaults.
- Added a double-click Windows updater and documented why the upstream in-chat
  `/update` command must not be used for a patched Hermes Local checkout.

## 0.18.1 — 2026-07-28

- Fixed fresh launcher startup so the Electron connection gate starts and
  awaits the configured Hermes Local workstation instead of deadlocking on an
  offline port 9119.
- Made concurrent launcher/start requests join an already-booting supervisor
  rather than treating normal startup overlap as a failure.
- Fixed strict-mode source reconstruction and patch argument handling for
  genuinely fresh public clones, and removed leaked native exit-code output.

## 0.18.0 — 2026-07-28

- Replaced the fixed drive, model, profile, port, CUDA architecture and build
  parallelism assumptions with versioned portable defaults and ignored
  current-user settings.
- Added arbitrary GGUF registration/selection, profile create/edit/delete,
  loopback port selection and Auto/CUDA/CPU controls to Hermes Launcher.
- Added hardware-resolved starter tuning, CPU fallback, detected CUDA
  architecture, dynamic Hermes provider generation and configuration schemas.
- Updated setup, supervisor, test, benchmark, update, backup, diagnostics and
  recovery workflows to use one selected configuration.
- Generalised installation, operation, model-tuning, architecture, security
  and troubleshooting documentation while preserving reference reports as
  explicitly historical evidence.

## 0.17.0-local.1 — 2026-07-28

- Added the packaged **Hermes Launcher** workstation with live Home, Services,
  Models, Profiles, Tasks, Tools, Memory, Benchmarks, Security, Logs and About
  surfaces.
- Added DPAPI-backed automatic authentication to the loopback-only Hermes
  dashboard and inference server.
- Added a real ConPTY/xterm.js Hermes TUI with PID, resize, ANSI, keyboard and
  scrollback support.
- Added supervised model and Hermes lifecycle scripts with restart-loop
  protection, structured health and data-preserving backup/restore.
- Added project-managed Python 3.13.14 with SQLite 3.53.1 and retained the
  previous Python 3.11 venv as a rollback runtime.
- Disabled the upstream desktop update notifier in Hermes Local mode; updates
  use the local snapshot, staging and rollback workflow.
- Added packaged-launcher Playwright acceptance coverage and redacted
  diagnostic export.
- Added strict renderer CSP, exact navigation controls, fail-closed media
  permissions, main-process profile grammar validation and defused XML parsing.
- Removed network font dependencies from built-in themes so installed startup
  remains offline-capable.
- Added optional current-user launch at login through Electron, with packaged
  toggle-and-restore coverage.
- Added native Windows skill preprocessing that selects Git Bash, rejects the
  WSL launcher and emits portable supporting-file references.
- Made compression-persistence regression fixtures portable on Windows and
  closed test-owned SQLite handles.
- Added measured Daily/Research 64K and Deep Research 80K profiles, a
  51.2-minute benchmark report, repeatable security workflow, CycloneDX SBOM,
  packaged installer/portable artifacts and complete operator documentation.
