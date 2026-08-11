# Hermes Local Dioxus Migration Journal

Last updated: 2026-08-11 (Europe/London)

## Recovery checkpoint

- Starting `main`: `def1f22aabc36f1e03b9fb72edbf33da71b27cf7`
- Starting commit subject: `refactor: make the native Hermes Local client first-class`
- Remote: `https://github.com/xdCloudy/Hermes-Local.git`
- Migration branch: `refactor/dioxus-rust-client`
- Working directory: `work/Hermes-Local` inside the Codex workspace (the brief's
  expected `D:\Hermes-Local` path was not the supplied checkout)
- Last validated checkpoint: clean clone of `origin/main`; branch created; root
  and Desktop `AGENTS.md` read in full.
- Exact next action: inventory the repository architecture, renderer routes,
  preload/IPC surface, native responsibilities, tests, packaging, release and
  update contracts; then run and record the accepted baseline.

## Objective

Replace the owned Electron/React/TypeScript production Desktop client with a
Rust application using supported Dioxus Desktop and the operating-system
WebView. Preserve product behavior and visual identity, retain Hermes Agent as
the pinned Python harness and llama.cpp behind the OpenAI-compatible inference
boundary, and remove Electron/React/Node from the production runtime and
packaging path.

## Non-negotiables

- One canonical Hermes Local-owned production client.
- Dioxus components call typed application services; they do not hold arbitrary
  filesystem, process, Windows, terminal, Git, update, or secret-store authority.
- Desktop is production-first while shared UI remains able to compile for a
  future Web/WASM transport.
- No LAN listener or future remote/mobile access is implemented in this change.
- No Dioxus wrapper around the React application, hidden Electron process,
  iframe, generic native command escape hatch, or runtime Node requirement.
- Hermes Agent and llama.cpp remain separate, pinned components.
- Harness patches stay harness-only and may not contain client UI source.
- Preserve secrets outside the DOM and store them with per-user DPAPI or Windows
  Credential Manager.
- Preserve user data across repair, update, rollback, and uninstall.

## Verified starting architecture

- `apps/desktop`: Hermes Local-owned Electron/React production client.
- `packages/hermes-agent-client`: TypeScript Agent wire/client package.
- `source/hermes-agent`: ignored, separately reconstructed pinned Agent harness.
- `source/hermes-launcher/patches`: ordered harness-only patch series.
- Root PowerShell scripts own setup, build, package, security, update, rollback,
  restore, benchmark, diagnostics, and uninstall workflows.
- `VERSION.json` records product/runtime provenance.
- Trust boundaries described by repository guidance:
  - Electron: machine/process/native authority.
  - React renderer: navigation, presentation, ephemeral interaction state.
  - Agent backend: sessions, tools, model calls, streaming.

## Current UI and native responsibilities

Current UI technology: React 19/TypeScript rendered by Electron, with Vite and
Electron Builder in the build/package path. Detailed routes, stores, components,
secondary surfaces, IPC handlers and native owners are pending exhaustive audit.

Known native responsibility categories from the initial scan include backend
lifecycle/environment resolution, filesystem and Git/worktrees, terminal/PTY,
window state and secondary windows, quick entry/global shortcuts, OAuth and
encrypted token storage, dashboard embedding/navigation guards, updates and
relaunch, power-save behavior, find-in-page, clipboard/media, SSH, crash/update
recovery, and native packaging.

## Baselines

### Tests and builds

Pending. Existing failures on untouched `main` will be recorded separately from
migration regressions.

### Packaging and overhead

Pending. Measure only where repeatable on this Windows host; do not fabricate
unavailable installer, launch, process, startup, RAM or CPU values.

| Metric | Electron baseline | Dioxus result |
| --- | ---: | ---: |
| Packaged size | pending | pending |
| Unpacked size | pending | pending |
| Process count | pending | pending |
| Cold start | pending | pending |
| First usable window | pending | pending |
| Idle working set | pending | pending |
| Idle CPU | pending | pending |
| Background processes | pending | pending |

## Migration matrix

The exhaustive matrix will be populated during Phase 1. No existing capability
may be marked removed merely because it is difficult to port.

| Old capability | New Rust owner | New Dioxus surface | Test coverage | Status |
| --- | --- | --- | --- | --- |
| Desktop shell/routing/theme | pending design | pending | pending | audit pending |
| Agent gateway/session streaming | pending design | pending | pending | audit pending |
| Native machine authority | pending design | n/a via typed services | pending | audit pending |
| Build/package/update | pending design | status/update surfaces | pending | audit pending |

## Dependency decisions

Pending official Dioxus and upstream crate documentation review. Dependencies
will be selected for mature support, minimal enabled features, Windows Desktop
support and Web/WASM isolation, then pinned in `Cargo.lock`.

## Security decisions

- Renderer/WebView content is untrusted relative to native authority.
- No generic command, shell, filesystem or process endpoint will be exposed.
- Privileged operations require typed DTOs, validation and least-authority
  services.
- No token will be placed in DOM state, URLs, command lines or logs.
- WebView navigation, external links, Markdown/HTML, dashboard auth, path
  canonicalisation, Git/PTY argument handling, OAuth/deep links, update staging,
  plugin/MCP/skill trust and secret storage require explicit review.

## Completed components

- Clean clone and exact starting revision recorded.
- Dedicated migration branch created.
- Repository and Desktop engineering rules read.

## Outstanding components

- Entire Phase 1 capability and IPC audit.
- Baseline validation and performance capture.
- Rust workspace, protocol, core/native services and Dioxus UI.
- Native feature, packaging, updater, CI, guard, documentation and test migration.
- Security review, visual parity, clean-build/package validation, commits, push
  and draft pull request.

## Results and unresolved failures

None yet. This journal must be reconciled with Git state at every resumed
session and updated before destructive removal of the legacy stack.
