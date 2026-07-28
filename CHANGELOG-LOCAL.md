# Hermes Local changelog

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
