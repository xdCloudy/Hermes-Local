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
- Exact next action: finish focused PowerShell/release/harness baseline checks,
  then establish the pinned Rust workspace and protocol/service boundaries.

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

Current UI technology: React 19/TypeScript rendered by Electron 40.10.2, with
Vite 8.2 and Electron Builder 26.15.3 in the build/package path. The exhaustive
living ledger is `docs/DIOXUS_MIGRATION_MATRIX.md`.

Audit inventory:

- 28 reserved route IDs plus contributed one-segment routes.
- 125 literal `ipcMain.handle`/`ipcMain.on` channels in `main.ts`.
- 174 literal preload-observed invoke/send/event channels, including controller
  channels registered through helper modules.
- 1,546 Desktop files at initial scan: 703 `.ts`, 507 `.tsx`, 510 test/spec
  files.
- 26 harness-only patches; architecture guard reports harness tree
  `557925025d999637c616ee39defe8eacfe135a0a`.

Known native responsibility categories from the initial scan include backend
lifecycle/environment resolution, filesystem and Git/worktrees, terminal/PTY,
window state and secondary windows, quick entry/global shortcuts, OAuth and
encrypted token storage, dashboard embedding/navigation guards, updates and
relaunch, power-save behavior, find-in-page, clipboard/media, SSH, crash/update
recovery, and native packaging.

## Baselines

### Tests and builds

Electron baseline from the clean clone plus the journal-only checkpoint:

- locked install: PASS, 1,080 packages installed with npm 11.19.0;
- TypeScript: PASS in 39.3 s;
- lint: PASS in 65.9 s with 311 existing warnings and zero errors;
- Vitest: PASS in 152.9 s; 490 files passed, 1 skipped; 4,442 tests passed,
  2 skipped;
- native-client architecture guard: PASS; 1,547 tracked client files, 14 Agent
  client files, 26 harness patches;
- renderer/Electron production build: PASS outside the Codex filesystem sandbox;
  the first in-sandbox attempt failed because esbuild was denied a root-directory
  read while resolving `electron/main.ts`, not because of repository code;
- unpacked Electron Windows package: PASS using Electron Builder `--dir --win
  --x64`.
- repository Python contract suite: PASS, 113 tests, after reproducing CI's
  `jsonschema` and `PyYAML` dependencies in an isolated workspace virtualenv;
  the first two attempts documented missing host dependencies rather than code
  failures;
- native Desktop update contract: PASS;
- Desktop updater reliability: PASS;
- updater failure-path fixtures: PASS, 18 scenarios.

### Packaging and overhead

Pending. Measure only where repeatable on this Windows host; do not fabricate
unavailable installer, launch, process, startup, RAM or CPU values.

| Metric | Electron baseline | Dioxus result |
| --- | ---: | ---: |
| Packaged size | installer/portable pending | pending |
| Unpacked size | 399,302,609 B (380.80 MiB), 486 files | pending |
| Process count | pending | one native client before Agent/runtime children; packaged measurement pending |
| Cold start | median spawn-to-CDP 321 ms; DOM interactive 150 ms; FCP 588 ms | pending |
| First usable window | median spawn-to-driver 1,397 ms | pending |
| Idle working set | pending | pending |
| Idle CPU | pending | pending |
| Background processes | pending | pending |

Additional size evidence: the Electron launcher executable is 214,007,808 B
(204.09 MiB) and the initial built client payload was 43,997,060 B (41.96 MiB).
The performance harness completed its three measured runs, then exited non-zero
because `.codex/electron-cold-start-baseline.json` was resolved under
`apps/desktop`, where that directory did not exist. The metrics printed above
are valid; the JSON-output path failure is recorded rather than concealed.

## Migration matrix

The exhaustive matrix is maintained in `docs/DIOXUS_MIGRATION_MATRIX.md`. No
existing capability may be marked removed merely because it is difficult to
port.

| Old capability | New Rust owner | New Dioxus surface | Test coverage | Status |
| --- | --- | --- | --- | --- |
| Desktop shell/routing/theme | `hermes-ui` | Dioxus shell | Rust + visual/E2E | designed |
| Agent gateway/session streaming | `hermes-agent-client` + `SessionService` | chat | protocol/harness/perf | designed |
| Native machine authority | cohesive services in `hermes-desktop` | n/a via typed services | unit/integration/E2E | designed |
| Build/package/update | Rust/Dioxus + `UpdateService` | status/update surfaces | clean package/lifecycle | designed |

## Dependency decisions

Official documentation review and the resolved crate graph selected stable Dioxus 0.7.10, not the 0.8 alpha
or experimental native renderer. Dioxus Desktop uses the system WebView (WebView2
on Windows) through Wry. Desktop and web will be isolated with Cargo features;
the platform-neutral UI will compile with `dioxus/web` while Desktop composition
uses `dioxus/desktop`. Rust 1.97.1 and the `wasm32-unknown-unknown` target are
installed. An initial local Dioxus 0.7.9 CLI binary was verified against release
SHA-256 `0423b94dd36372d09936a9a288c4a6e7903a9f1bddd193b60bba9659890c87c4`;
the production dependency graph and Cargo lock use 0.7.10, and the CLI must be
updated to the matching version before package validation.

Pinned direct Rust dependencies now include Dioxus/Dioxus Router 0.7.10,
Tokio 1.53.1, tokio-tungstenite 0.30.0, reqwest 0.13.4 with Rustls,
portable-pty 0.9.0, serde 1.0.229,
serde_json 1.0.151, thiserror 2.0.20, url 2.5.8, uuid 1.24.0, trash 5.2.6,
open 5.4.1, and async-stream 0.3.6. `Cargo.lock` is generated.

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
- Exhaustive capability ledger created.
- Locked TypeScript/lint/Vitest/build/architecture baseline captured.
- Unpacked Electron Windows package and initial size/startup evidence captured.
- Stable Rust, WASM target and hash-verified Dioxus CLI prepared.
- Root Cargo workspace created with five cohesive crates and the Desktop
  composition root.
- Forward-compatible JSON-RPC DTOs and an actor-based WebSocket client are
  implemented with bounded queues, request/connect timeouts, cancellation,
  state/event subscriptions, ping/pong, and graceful close.
- Cohesive typed service boundaries exist for sessions, projects, settings,
  runtime/tasks, Trust, files, Git, ConPTY terminals, updates and platform
  operations. The Dioxus crate contains no direct native authority calls.
- Native filesystem operations canonicalise the selected root and target or
  parent; Git uses argument arrays and `--`; external URL schemes are
  allowlisted; deletion uses the Windows recycle bin.
- Dioxus Desktop shell, route model, visual tokens, responsive/reduced-motion
  CSS, primary navigation and all audited top-level feature destinations exist.
- `cargo check --workspace --all-targets`: PASS.
- `cargo test --workspace`: PASS, 12 unit tests plus doc-tests.

## Outstanding components

- Remaining installer/portable, process/RAM/CPU and visual baseline capture.
- Complete DTO coverage, Agent bootstrap/auth/reconnect, state models,
  route-specific behavior, secondary windows and specialty surfaces.
- Native feature, packaging, updater, CI, guard, documentation and test migration.
- Security review, visual parity, clean-build/package validation, commits, push
  and draft pull request.

## Results and unresolved failures

The initial Dioxus compile required explicit `macro`, `asset`, `document`,
`router`, and `launch` features because workspace dependencies disable defaults;
that mismatch is repaired. No unresolved Rust compiler or unit-test failure
exists at this checkpoint. Updater apply and native notifications deliberately
report unavailable until their signed Windows implementations are completed;
neither is marked migrated.

Exact next action: connect native startup to local/remote Agent resolution and
wire live settings/session/project state into Dioxus, then add protocol fixtures
before the first implementation checkpoint commit.

## Visual parity correction (2026-08-11)

The product owner rejected the initial Dioxus visual direction because it did
not resemble the OG Hermes Launcher. That shell was an exploratory migration
scaffold, not an acceptable parity result, and is now superseded.

The implementation oracle is the official OG launcher source retained under
`apps/desktop/src`, especially `app/chat/sidebar/index.tsx`,
`app/local-workstation/index.tsx`, `styles.css`, and the theme definitions.
Repository screenshots under `docs/assets/screenshots` and
`reports/qa/screenshots` are the visual regression baselines. The port must
preserve the original compact 0.8125rem sidebar rows, full launcher navigation,
light/dark theme tokens, workstation header, bordered cards, action geometry,
session/search regions, and bottom status bar. No visual redesign is authorised.

Parity is a four-part gate: visual parity, interaction parity, state parity and
behavioural parity. A matching screenshot alone is insufficient. Shell parity
must be validated before the same component language is propagated through
large feature areas. The already validated Rust workspace, typed Agent client,
async gateway actor, cohesive application service traits and security
boundaries remain the migration foundation unless concrete evidence proves an
implementation incorrect.

The corrected shell now uses the OG source geometry and tokens, a 34px custom
titlebar behind typed composition-root window actions, the 237px compact
sidebar and original route order, source Codicon SVG paths, workstation cards,
session/search regions and a 20px status bar. Native captures exist at standard
and maximised dimensions; the first unstyled capture was rejected because a
plain Cargo build had skipped the external asset transform. CSS is now embedded
by the shared Dioxus crate so both Cargo and Dioxus builds render identically.

Exact next action: audit and port the OG sidebar interaction/state model
(selection, search, pinned/recent sessions, row controls and context actions),
then verify its interaction and visual parity before beginning the chat slice.

## Durable sidebar checkpoint (2026-08-11)

The sidebar now reads persisted sessions through the OG `/api/sessions` REST
contract rather than the gateway's active-runtime-only RPC. The Rust adapter
derives an origin-confined REST base from the connected WebSocket URL, retains
base-path deployments, extracts the legacy session token without exposing it
to Dioxus state, and sends it only as `X-Hermes-Session-Token`. HTTP status
classes map to typed service errors. Session identifiers reject URL path/query
delimiters before they can reach a REST path.

Pinned and recent sections, case-insensitive loaded-session search, active
route selection, running state, hover and context actions, inline rename,
archive, destructive delete confirmation and durable lineage-root pin identity
are implemented in the OG compact row geometry. Mutations are optimistic; a
monotonic mutation epoch prevents an older failed request from rolling back a
newer user action. Runtime rename first uses the OG `session.title` RPC and
falls back to persisted REST for non-live rows and title clearing.

Native visual QA at 1296x809 passes the corrected shell geometry. This build
used about 34.4 MiB working set and 6.6 MiB private memory. `cargo check
--workspace`, `cargo test --workspace` (13 unit tests plus doc-tests), and
`cargo clippy --workspace --all-targets` pass; remaining Clippy output is the
documented pedantic-warning backlog rather than denied correctness lints.

Exact next action: port the OG chat/session transcript state machine and
streaming event reconciliation, then exercise sidebar actions against a live
Agent fixture before marking the session slice validated.

## Chat and transcript checkpoint (2026-08-11)

The new-chat route now reproduces the OG empty-chat composition rather than a
generic application page: the centred Hermes Agent wordmark and source copy sit
above the compact two-tier bottom composer, with project scope, attach, model,
voice and send affordances in the original hierarchy. A standard-size native
capture at 1296x809 is retained in the migration work area. This checkpoint
used about 35.0 MiB working set and 6.9 MiB private memory.

Session resume now returns a typed response containing the stored/runtime
identity, messages, running state and forward-compatible metadata. Persisted
messages tolerate numeric row IDs and structured content arrays without losing
text. `SessionTranscript` owns foreground reconciliation outside Dioxus: it
isolates runtime events, coalesces message and reasoning deltas, settles interim
responses without duplication, upserts tool progress, clears stranded streams
on `running=false`, flags blocking input requests and records terminal errors.

The Dioxus session surface loads that state, subscribes to the typed event
stream, renders user/assistant/reasoning/tool states, optimistically submits,
restores both transcript and draft on failure, and exposes interruption from
the header and composer. Gateway submission now matches the official source
contract (`text`, not `prompt`) and uses the source's 30-minute acknowledgement
window while normal RPCs keep the short default timeout.

`cargo test --workspace` passes 20 unit tests plus doc-tests, including stream
isolation, tool upsert, interim settlement and structured stored-message
fixtures. `cargo clippy --workspace --all-targets` passes with only the existing
pedantic documentation/style warning backlog.

Exact next action: add a deterministic live Agent HTTP/WebSocket harness for
resume, streaming, submit, interrupt and sidebar mutations, then port project
scope and the remaining OG chat composer controls.

The first compatibility-harness slice is complete: an in-process HTTP peer
proves base-path-preserving session routes, the legacy token header and JSON
mutation body, while an in-process WebSocket peer decodes the actual JSON-RPC
frame and proves `prompt.submit` carries the official `session_id` + `text`
shape. Neither test requires a configured user Agent or external network.

Exact next action: extend the harness through resume/event/interrupt and REST
error mapping, then port project scope into the sidebar and composer.
