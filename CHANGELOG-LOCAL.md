# Hermes Local changelog

## Unreleased

- Reworked Desktop self-update to build and validate an immutable detached
  worktree before source promotion, preserve staged, unstaged, untracked and
  nested-source edits in place, and abort conflicting fast-forwards without
  stashing or hard-resetting the installed checkout.
- Added a versioned 49-scenario Windows install, upgrade, repair, rollback,
  uninstall and adverse-condition matrix with deterministic preservation
  fixtures, retained machine-readable evidence, disposable and trusted physical
  runner lanes, and a Stable gate that fails closed on missing CPU or NVIDIA
  lifecycle proof.
- Replaced the Desktop implementation-ledger placeholder with an authoritative
  Task Centre for active, queued, completed, failed, interrupted and cancelled
  work, including capability-gated controls, bounded redacted output, result
  and recovery links, keyboard filters, narrow-window layout and a sidebar
  active-task indicator.
- Persisted Desktop background-task records in a bounded, atomic store and
  reconciled live owners and action-specific completion evidence after renderer
  reloads, Desktop restarts and unexpected process exits.
- Replaced the desktop's action-name exception with a versioned task schema,
  explicit state machine and deterministic shared/exclusive resource policy;
  automatic readiness now queues behind maintenance without blocking benchmark
  recovery, health reads or reconnects.
- Rebased the complete Hermes Local integration patch series onto Hermes Agent
  `85148f79` while preserving the newer upstream Desktop architecture and
  dependency locks.
- Made upstream compatibility validation reproducible with an exact supported
  npm CLI and the pinned Jinja dependency required by llama.cpp's Python-backed
  CTest coverage.
- Provisioned and SHA-256 verified the starter model's vision projector during setup, then passed its portable local path to llama.cpp instead of depending on an HTTPS-enabled native build.
- Blocked llama.cpp rebuilds while `llama-server`, `llama-cli` or `llama-bench` is running, replacing opaque locked-DLL linker failures with a direct stop instruction.
- Corrected messaging-gateway health to treat Hermes Agent's persisted `updated_at` value as a state-change timestamp rather than a periodic heartbeat, preventing a live connected gateway from being restarted merely because it has been idle or the model is leased to a benchmark.
- Checkpointed completed native benchmark cases before model restoration and added replacement-supervisor recovery so valid measurements survive an unrelated restoration failure.
- Fixed benchmark startup on PowerShell by allowing newly created empty argument and case accumulator lists to bind before they are populated, with a real construction-path self-test.
- Made the benchmark harness hardware- and settings-agnostic by probing the installed `llama-bench` options, translating abstract GPU-layer settings without mutating profiles, deriving sweeps from the selected profile and hardware, and flattening adaptive context cases.
- Preserved a complete telemetry schema for failed native benchmark cases so report generation records the original failure instead of throwing on missing performance counters.
- Made shutdown verify the complete captured supervisor process tree and detect replacement supervisors instead of reporting a false clean stop when descendants survive.
- Kept the Desktop backend and messaging gateway online during benchmarks by leasing only the model process, with automatic model restoration and stale-request recovery.
- Prevented nested gateway-module imports from unloading shared logging and redaction commands from parent PowerShell scripts.
- Isolated nested launcher-build and bootstrap-diagnostic scripts so forced module reloads cannot remove setup logging commands.
- Managed enabled Hermes messaging gateways with the local stack, added authoritative gateway readiness and ownership state, and made diagnostics fail when a required gateway is offline.
- Removed the redirect-only Tools page from primary navigation, preserved `/tools` as a redirect to Skills, and retained direct Chat and TUI access.
- Replaced the remaining native launcher selects with the shared themed,
  accessible dropdown across model, runtime, network, log, profile and quick
  entry controls.
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
