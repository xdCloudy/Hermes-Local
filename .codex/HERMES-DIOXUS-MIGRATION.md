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

## Project scope checkpoint (2026-08-11)

Project selection is now shared across the application shell, the new-chat
composer and resumed-session composer. The compact OG status row opens an
upward project menu, supports clearing or changing the active scope, and feeds
the selected project ID and primary working folder into typed session creation.
Optimistic activation rolls back on a gateway failure.

The Project Centre now uses the OG source copy, search and active/archive filter
geometry, compact project rows, active state, creation overlay and explicit
registration-only removal confirmation. Creation and removal refresh or update
the same shared snapshot used by the composer, so scope state cannot diverge
between surfaces. Folder attachment currently accepts an explicit local path;
the native folder picker and repair/delete-files flows remain required before
final Project Centre parity.

The Rust gateway adapter was corrected to the official source contracts:
`projects.create` sends `primary_path` and `use` and decodes the nested
`project`; activation and deletion send `id`, not `project_id`. An in-process
WebSocket compatibility test proves all three real JSON-RPC frames and the
nested create response. Native dark-mode captures at 1296x809 cover the empty
Project Centre and creation dialog. `cargo test --workspace` passes 21 unit
tests plus doc-tests.

Exact next action: add folder picking and the remaining Project Centre RPCs,
then extend the deterministic Agent harness through resume/event/interrupt and
REST error mapping before moving to settings/theme parity.

The deterministic Agent harness now also proves stored-to-runtime session
resume identity, interleaved `message.delta` delivery through the application
event stream, and `session.interrupt` targeting the resumed runtime. Separate
HTTP peers prove that Agent 403 and 404 responses remain distinct typed
permission and not-found failures. The workspace now passes 23 unit tests plus
doc-tests without relying on a configured user Agent or external network.

Exact next action: finish folder picking and the remaining Project Centre RPCs,
then port settings and immediate light/dark/system theme application against the
official source.

Project Centre parity now includes the source's Empty, Attach folder and Clone
Git creation modes, pinned filtering and project pin/unpin and archive/restore
actions. The adapter uses `projects.centre`, `projects.clone`, `projects.pin`,
`projects.archive` and registration-only `projects.remove`; the latter replaces
the broader legacy delete call for the Project Centre confirmation. A new
compatibility fixture proves all method names, `restore` polarity, clone fields
and pinned snapshot decoding. The workspace passes 24 unit tests plus doc-tests,
and the clone dialog has a native 1296x809 dark-mode regression capture.

Exact next action: add the typed native folder picker and repair/delete-files
Project Centre flows, then port settings and immediate light/dark/system theme
application against the official source.

## Settings and theme checkpoint (2026-08-11)

Settings now opens in the OG overlay geometry instead of a generic routed card:
an inset full-height panel, compact 208px navigation rail, close affordance,
footer actions and a scrollable content pane. The complete official settings
navigation taxonomy is represented, while Appearance is the first connected
feature slice. Its mode control, installed-theme grid, UI-scale row and compact
settings dividers follow the source hierarchy and density. The Codicon sprite
was regenerated from the pinned official `@vscode/codicons` package so every
new control uses source SVG paths rather than missing glyphs or text symbols.

App settings load once into shared Dioxus state. Light, dark and system modes
apply immediately at the application root, persist through an atomic native
JSON replace, survive restart, and roll back on a failed save. The selected
skin identity is persisted in the same typed settings object; full per-skin
palette projection and the non-Appearance settings services remain outstanding.
Native 1296x809 captures cover both dark and light Appearance surfaces; the
light capture caught and fixed inherited dark text before this checkpoint.
`cargo test --workspace` passes 25 unit tests plus doc-tests, including an
atomic theme mode/skin persistence round trip.

Exact next action: connect the Model and Chat settings sections and OG config
contracts, then return to the typed folder picker and remaining destructive
Project Centre confirmations.

The Project Centre native folder-picker gap is closed. `PlatformService` owns a
typed, cancellable `pick_folder` operation and the Windows implementation uses
pinned `rfd` 0.17.2; shared Dioxus code receives only the selected path. Attach
folder and Clone Git now reproduce the source's disabled path field plus
`Choose…` affordance, seed the dialog from the configured default project
directory when available, and preserve cancellation without mutating form
state. A native Windows dialog capture confirms the real OS picker and title;
the dialog was cancelled after QA without selecting or changing any folder.

Exact next action: connect the Model and Chat settings sections and OG config
contracts, then implement repair and separately confirmed delete-files Project
Centre flows.

## Agent config settings checkpoint (2026-08-11)

Model and Chat are no longer placeholder routes. A scoped `AgentConfigService`
implements the OG desktop's official profile-aware REST sequence exactly:
`GET /api/config`, `GET /api/config/defaults`, `GET /api/config/schema`, and a
whole-record `PUT /api/config` body shaped as `{ config }`. Config, defaults and
forward-compatible schema metadata have typed protocol owners; Dioxus receives
no generic HTTP or RPC authority. Whole-record saves optimistically update the
surface, preserve unrelated keys, and roll back visibly when the Agent rejects
an edit.

The Chat pane now renders and persists the source-curated Personality,
Timezone, Reasoning Blocks and Image Attachments rows. The Model pane renders
Context Window plus the structured ordered fallback-provider/model editor;
incomplete fallback rows remain local and only complete pairs are persisted,
matching the OG save rule. Loading, disconnected/retry, saving and error states
use the same compact settings geometry rather than a generic routed page.

An in-process HTTP compatibility test proves all four endpoint paths, profile
encoding, legacy token header, methods, schema decoding and exact replacement
body. Native 1296x809 light-mode captures cover populated Model and Chat panes.
Rendered QA caught and fixed a native select-state bug, then an end-to-end
Reasoning toggle proved `false → true` through the real WebView and REST
fixture without clobbering context or fallback keys. The workspace passes 26
unit tests plus doc-tests.

This checkpoint does not claim the complete OG Model page: the main provider
and model chooser, reasoning/service-tier defaults, auxiliary assignments and
Mixture-of-Agents editor still require their official typed model services.

Exact next action: port those OG Model settings services and controls into this
pane, then implement Project Centre repair and separately confirmed file
deletion.

## Main and auxiliary model checkpoint (2026-08-11)

The upper OG Model settings hierarchy is restored. A typed `ModelService`
loads `/api/model/info`, `/api/model/options?explicit_only=1` and
`/api/model/auxiliary`, and posts source-shaped assignments to
`/api/model/set`. Profile routing, authentication, provider capabilities,
custom provider base URLs and unknown response fields remain behind the Rust
boundary. Model and provider identifiers permit the slash-rich model names the
real catalog uses while rejecting empty, oversized or control-character input.

The populated pane now renders the main provider/model selectors and Apply
action, capability-gated Reasoning and Fast profile defaults, the Auxiliary
models heading and reset action, all eight source task rows, persisted override
copy, and inline provider/model Change, Apply and Cancel controls. Reset and
Set-to-main use the OG `scope: auxiliary` plus `task: __reset__`/task-name
contract instead of inventing a second endpoint.

An in-process compatibility peer proves all three reads and the assignment
write, including `explicit_only=1`, profile encoding, the session-token header
and exact auxiliary JSON body. Native 1296x809 QA covers the expanded model
pane, the inline Vision editor, and a successful applied assignment followed
by a service refresh. The workspace passes 27 unit tests plus doc-tests.

The Mixture-of-Agents preset editor remains the one large OG Model-settings
block not yet ported.

Exact next action: connect `/api/model/moa` load/save and reproduce the OG MoA
preset/slot editor, then return to Project Centre repair and separately
confirmed file deletion.

## Mixture-of-Agents settings checkpoint (2026-08-11)

The final large OG Model-settings block is now connected. `ModelService` treats
`GET /api/model/moa` as an optional capability exactly like the source and
round-trips the complete forward-compatible config through
`PUT /api/model/moa`. Typed presets retain aggregator/reference slots,
temperatures, token and timeout limits, degraded policy, per-slot reasoning,
enabled state, fanout, and unknown future fields even where this first UI does
not edit them.

The pane reproduces the source hierarchy and controls: named preset selection,
Enabled, Set default, Delete, clone-as-new preset, default identity, ordered
reference rows and toggles, provider/model editors, Remove, Add reference, and
the acting Aggregator row. The `moa` virtual provider is excluded from slot
selectors to prevent recursive agent trees. Provider changes clear their stale
model locally, and the completeness guard refuses to persist half-filled or
reference-free presets; a focused unit test proves that hold behavior.

The compatibility peer now also proves the profile-scoped MoA GET and exact
whole-config PUT. Native 1296x809 QA covers the complete block at the bottom of
the scrollable Model pane. A real WebView toggle persisted the balanced preset
`true → false → true` while the fixture confirmed both reference slots and the
`openai/gpt-5` aggregator were preserved. The workspace passes 28 unit tests
plus doc-tests.

Exact next action: implement the OG Project Centre repair operation and the
separately confirmed delete-files flow, then move through the remaining
settings sections in official source order.

## Project repair and file deletion checkpoint (2026-08-11)

Broken project registrations now retain the source's path-state and repository
identity metadata and render an amber `Path needs repair` badge. The repair
action selects the same recovery candidate order as the OG client (first broken
folder, then primary, then first registered folder), opens the typed native
folder picker, and sends `projects.recover_path` with `id`, `old_path`,
`new_path`, and `repository_id`. The returned project replaces only its stable
registration row.

Registration removal remains a separate, non-filesystem operation. Projects
with folders now expose `Delete files…`, which opens the source's dedicated
warning dialog, requires the exact `DELETE {project name}` phrase, and only then
sends `projects.delete_files` with the confirmation. The authoritative Project
Centre snapshot replaces local state after success; returned deleted paths are
decoded without exposing deletion authority to Dioxus.

An in-process WebSocket compatibility peer proves both exact RPC method names,
parameter objects, repaired-project decoding, deleted-path decoding, and the
authoritative empty snapshot. Maximized native QA at 1296x809 covers the broken
row, warning badge, repair/delete actions, disabled confirmation, exact-match
enabled state, and successful fixture-only removal. No real folder was selected
or deleted. `cargo test --workspace` passes 29 unit tests plus doc-tests;
workspace Clippy still reports only the existing pedantic documentation/style
backlog.

Exact next action: audit and port the OG Workspace settings section in official
source order, beginning with default directory and repository discovery fields,
then continue into Safety.

## Workspace settings checkpoint (2026-08-11)

The first remaining official config section is connected in source order.
Workspace renders Working Directory, Automatic Repository Discovery,
Repository Discovery Roots, Excluded Repository Paths, Code Execution Mode,
Persistent Shell, Environment Passthrough, and File Read Limit. Fields appear
only when the Agent schema or current record declares them, preserving the OG
client's backend-capability behavior rather than fabricating unsupported knobs.

Boolean, numeric, ordered option, list, and free-text controls are selected from
the typed schema. List values retain the source comma-separated editing shape;
select writes preserve the schema's underlying JSON option; nested edits use
whole-record replacement while retaining unrelated current and future keys.
Focused unit coverage proves both nested preservation and list presentation.

Maximized native QA at 1296x809 covers all eight populated rows and their OG
compact divider rhythm. A real WebView interaction saved Automatic Repository
Discovery `true → false`, navigated away, remounted the pane, reloaded config
from the fixture, and retained `false`. The workspace passes 31 unit tests plus
doc-tests; Clippy output remains the existing pedantic documentation/style
backlog only.

Exact next action: port the OG Safety config section with the same schema-aware
whole-record guarantees, including approval modes, URL policy, command allowlist,
and checkpoints.

## Safety settings checkpoint (2026-08-11)

The official Safety whitelist is connected: Approval Mode, Approval Timeout,
Confirm MCP Reloads, Command Allowlist, Redact Secrets, both general and browser
private-URL policies, automatic local-browser routing for private URLs, and File
Checkpoints. Unsupported keys remain absent even if a future schema grows,
keeping security-surface expansion explicit and reviewable.

Approval Mode uses the OG `manual / smart / off` override even when the backend
declares only a string. The same enum path now correctly preserves an unknown
active legacy option without turning ordinary numeric or list values into
one-option selects. Maximized QA caught that generic-control defect in Approval
Timeout and Command Allowlist; a focused regression test now guards it.

The corrected 1296x809 native pass covers the full nine-row surface. A real
WebView Redact Secrets toggle persisted `true → false`, remounted the pane, and
reloaded as `false` while the fixture retained unrelated config. The workspace
passes 32 unit tests plus doc-tests.

Exact next action: port the OG Memory & Context section, preserving dynamic
provider schema options while adding the built-in memory, profile, context, and
compression controls.

## Memory and context settings checkpoint (2026-08-11)

Memory & Context now follows the official ten-field order: Persistent Memory,
User Profile, both character budgets, Memory Provider, Context Engine,
Auto-Compression, threshold, target ratio, and protected recent messages.
Schema descriptions fill the fields where the curated OG copy intentionally
has no override.

Memory Provider consumes the Agent's live schema options unchanged, so
installed providers such as `honcho` and `hindsight` remain visible without a
stale Rust catalog shadowing backend discovery. Context Engine keeps the
source's explicit `compressor / default / custom` choices and safely retains an
unknown active legacy value. Focused coverage proves both behaviors.

Maximized 1296x809 QA covers the complete scrollable surface with a dynamically
supplied `hindsight` provider. A real WebView Persistent Memory toggle persisted
`true → false`, remounted the pane, and reloaded as `false`. The workspace still
passes 32 unit tests plus doc-tests.

Exact next action: port the OG Voice section with provider-dependent field
visibility so users see only the active TTS/STT backend rather than the full
multi-provider field wall.

## Voice settings checkpoint (2026-08-11)

The complete curated Voice key inventory is represented, but the rendered
topology follows the source rather than dumping every backend at once. The five
top-level TTS/STT/auto-speech controls always render; only the selected TTS
provider's details render; STT details require both STT enabled and their
provider selected. Recording shortcut and maximum duration remain independent.

Provider, device, and local-model closed enums keep the official choices.
Voice/model identifiers that accept cloned voices, custom IDs, and newly
released model names stay free-input with suggestions through native datalists.
Focused tests prove provider switching, disabled-STT visibility, and the open
versus closed field classification.

Maximized native QA first showed OpenAI plus Local STT while inactive Edge,
ElevenLabs, and Groq fields were present in the fixture but absent from the UI.
A real provider change replaced OpenAI's rows with Edge Voice and persisted the
selection. Disabling STT then removed the Local model/language rows while
retaining the top-level provider control. The workspace passes 34 unit tests
plus doc-tests.

Exact next action: port the OG Advanced section, including the curated toolset,
terminal backend, output limits, agent/delegation controls, and update-local-
changes policy; keep device-only keep-awake and Quick Entry for their native
service slices.

## Advanced settings checkpoint (2026-08-11)

Advanced now renders the official 22-key Agent configuration inventory in
source order: toolsets; terminal backend, timeout, and backend images; terminal
and file-output limits; checkpoint retention; agent turn, retry, tier, and
tool-use controls; delegation model/provider/limits/reasoning; and the in-app
update local-changes policy. Schema support still gates every row, and edits
continue to replace the exact whole config record without dropping unknown
future keys.

Execution Backend keeps the OG `local / docker / singularity / modal / daytona
/ ssh` choices. Subagent Reasoning Effort includes inherited-empty plus the
complete official effort scale, while In-App Update Local Changes is limited to
`stash / discard`. Focused tests pin all three source-owned choice sets.

Maximized native QA at 1296x809 covers both halves of the long scrollable pane,
including delegation and update controls. A real WebView interaction saved
Execution Backend `docker → local`, remounted Advanced, and reloaded `local`;
the fixture also proved an unrelated future config key survived replacement.
The temporary server and app were stopped and removed. The workspace passes 35
unit tests plus doc-tests.

Device-only keep-awake and Quick Entry remain intentionally outside the generic
Agent record until their typed PowerService and ShortcutService/WindowService
slices exist. Exact next action: port Notifications through the typed native
PlatformService rather than simulating desktop notification authority in the
Dioxus layer.

## Notification preferences checkpoint (2026-08-11)

The Notifications section now preserves the OG per-device topology: a master
notification switch; Approval, Input, Response Ready, Turn Failed, Background
Task, and Credit switches; the complete 14-name completion-sound catalog with
selection and preview; a native test-notification action; and the original
background-only completion hint. Legacy settings records default every kind on,
matching the source.

Preferences are typed protocol state persisted atomically through
`SettingsService`; the UI has no storage or native authority. The preview is a
small generated WAV data URI rendered by the shared WebView, so it remains
web-ready without runtime Node or direct platform calls. Native test delivery
uses `PlatformService::notify` and reports unsupported honestly while the
Windows AppUserModelID/toast registration slice remains unfinished.

Focused tests prove legacy defaults, independent per-kind round trips, invalid
sound-ID clamping, and WAV generation. The workspace passes 37 unit tests plus
doc-tests, and Clippy has no new warnings beyond the recorded backlog. A native
visual pass could not be completed in this run because shell-launched Dioxus
processes stopped exposing a top-level window; the same condition reproduced
from detached checkpoint `64a3395`, ruling out this diff as the regression.

Exact next action: port Providers from the official source and its typed Agent
provider/model contracts, then return to native toast registration during the
Windows integration/package identity slice.

## Provider authority checkpoint (2026-08-11)

The Rust service boundary now owns the OG provider contracts before their UI is
ported. Accounts use profile-scoped OAuth list and disconnect calls. Credential
keys use the profile-scoped environment list, set, remove, and reveal calls.
Custom endpoints deliberately remain global and support list, save, validate,
activate, and delete through the exact official REST paths and payload shapes.

The boundary rejects unsafe provider/endpoint path segments, malformed
environment keys, empty or control-bearing credential values, credentialed
endpoint URLs, and invalid endpoint context lengths before transport. It does
not add a silent removal path for externally owned credentials; the upcoming UI
must preserve the OG visible-terminal disconnect flow.

An in-process Agent fixture covers all 11 calls, including profile query
encoding, HTTP methods, bodies, authentication propagation, response decoding,
and confirmation semantics. The workspace passes 39 unit tests plus doc-tests,
and Clippy adds no warning beyond the recorded backlog.

Exact next action: port the Providers nested Accounts, API Keys, and Custom
Endpoints Dioxus views 1:1 from the official source, then run maximized visual
and rendered interaction QA when a native top-level window is available.

## Provider accounts UI checkpoint (2026-08-11)

Providers now expands the same nested Accounts, API Keys, and Custom Endpoints
rail used by the official desktop settings. The Accounts view preserves the
shared onboarding picker order and titles, the featured Nous row, Fireworks and
OpenRouter API-key shortcuts, connected and collapsed-other group topology,
flow-specific descriptions, externally managed credential hints, best-effort
loading, and a confirmed profile-scoped sign-out path.

The view talks only to `ProviderService`; it has no REST, token, profile, shell,
or native-window authority. Unconnected account rows are present but OAuth
initiation remains deliberately unwired until its typed start/poll/submit/cancel
session slice lands, rather than simulating sign-in in Dioxus.

The workspace passes 40 unit tests plus doc-tests. Focused coverage pins the
official provider ordering and display-name overlays. Maximized native visual QA
was attempted with a populated local Agent fixture, but the Dioxus process again
settled with no top-level window handle. Captures briefly associated with other
foreground windows were rejected and deleted; this checkpoint therefore makes
no rendered-visual claim.

Exact next action: add the typed OAuth session lifecycle and connect the account
rows to the OG browser/device-code/external sign-in overlay, then port provider
API-key groups.
