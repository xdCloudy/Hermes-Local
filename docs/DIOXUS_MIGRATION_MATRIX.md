# Dioxus Migration Roadmap and Validation Matrix

> **Branch source of truth:** `refactor/dioxus-rust-client`  
> **Implementation audit checkpoint:** `452a44f1a9136a2df69e03b984d9b7cdcd4cfd9f` (2026-08-14)
> **Migration base:** `def1f22aabc36f1e03b9fb72edbf33da71b27cf7`  
> **Final acceptance authority:** human product-owner review. Automation and AI may prepare a capability for review, but may **never** self-award final validation.

This document is both the implementation roadmap and the acceptance ledger for the
Hermes Local Desktop migration from Electron/React/TypeScript to Rust + Dioxus.
The existing Hermes Local client under `apps/desktop` remains the visual,
interaction, state and behavioral oracle until a capability has passed the full
human gate.

The detailed manual review procedure lives in
[`docs/DIOXUS_HUMAN_VALIDATION.md`](DIOXUS_HUMAN_VALIDATION.md).

## How to read this roadmap

A capability is deliberately tracked through more than a binary "done/not done"
state. This prevents compiled code, a green unit test, or a visually similar
screenshot from being mistaken for a finished product port.

| Stage | Meaning | Who may set it | Exit requirement |
| --- | --- | --- | --- |
| **A0 Audited** | OG capability, owner, contracts and important edge cases have been identified. | AI/engineer | Rust ownership and acceptance plan are defined. |
| **A1 Designed** | Target Rust owner, Dioxus surface and safety boundary are defined, but production implementation is incomplete. | AI/engineer | Cohesive implementation exists behind typed boundaries. |
| **A2 Service** | Native/core service implementation exists and has meaningful automated coverage, but user-facing parity is incomplete or absent. | AI/engineer | Dioxus/UX slice is wired and usable. |
| **A3 Ported** | End-user implementation exists and is usable, but automated parity/integration evidence is incomplete. | AI/engineer | Relevant automated, contract, security and integration gates pass. |
| **A4 Auto-verified** | The implemented slice has passed the applicable automated/contract/security gates available in the branch. **This is not final acceptance.** | AI/CI | Live integration and review evidence are complete enough for a human to judge it. |
| **A5 Human-ready** | Implementation, automated evidence, live integration and comparison evidence are complete. The only remaining gate is manual product-owner review. | AI/engineer | Human executes the row-specific acceptance checklist and records PASS. |
| **A6 Human-validated** | Final product-owner acceptance has been explicitly recorded for this exact capability/build. | **Human only** | Final state. A later material regression returns the row to the appropriate earlier stage. |
| **BX Blocked** | Progress or validation is prevented by a named external/environment dependency. | AI/engineer | Blocker is removed and the row returns to its previous stage. |

### Non-negotiable validation rule

**No automated process, coding agent, CI run or model may set `A6`.** A row reaches
`A6` only after a human has manually compared the Rust port against the OG
implementation using the procedure in `DIOXUS_HUMAN_VALIDATION.md`. The `Human`
cell records that outcome.

`A4` means "the machine-verifiable portion is in good shape." It does **not**
mean "the user has approved it."

## Current branch snapshot

The initial Desktop audit found 1,546 files: 703 `.ts`, 507 `.tsx`, and 510
test/spec files, with 125 literal main-process IPC channels and 174 preload-
observed channels. The migration intentionally consolidates many of those
capabilities into typed Rust services instead of mirroring the old file count.

At the audit checkpoint above:

| Stage | Capabilities | Interpretation |
| --- | ---: | --- |
| A0 Audited | 1 | Inventoried but target migration decision is incomplete. |
| A1 Designed | 18 | Largest remaining implementation backlog. |
| A2 Service | 7 | Native/core foundations exist; UI/parity work remains. |
| A3 Ported | 2 | User-facing implementation exists; more evidence required. |
| A4 Auto-verified | 99 | Automated slice is green; human/live acceptance still required. |
| A5 Human-ready | 0 | Ready for final manual review. |
| A6 Human-validated | 0 | Human-approved capabilities. |
| BX Blocked | 0 | No named external validation blockers remain. |
| **Total** | **127** | Every row must end at A6 or be explicitly retired with product-owner approval. |

At least-service coverage is **108/127 capabilities (85.0%)**. This is service-foundation coverage, not end-user parity or human acceptance.

The PR #179 exact-head validation set was green for Documentation
validation, the Hermes Agent harness/native-client boundary workflow, Dioxus
Rust validation, Windows installer/portable packaging and artifact-footprint
regression before merge as `2413ce42`. The Rust validation gate includes architecture enforcement, rustfmt,
`cargo check --workspace --all-targets`, shared-UI WASM compilation, workspace
tests, Clippy, optimized Windows release build, resolved Cargo CycloneDX SBOM,
release-manifest/SHA-256 generation and artifact upload. Windows distribution CI
also exercises silent install/uninstall, package identity and data-preserving
repair. DI-07 remains A2 because its OS global-hotkey, secondary-window lifecycle and
submission surface are not wired. AG-01 is now A4 after PR #185 wired the typed
Dioxus Skills/Hub surface and passed its applicable exact-head release gates.

PR #187 exact head `a71ea7d3` adds the Desktop-owned Shell interaction controller,
native WebView zoom/find behavior, command palette and Command Centre, the major
OG global shell chords, a typed live runtime/gateway/task status bar, right-rail
composition and shell accessibility/reduced-motion handling. Dioxus Rust
validation run `31664757126` passed architecture, rustfmt, all-target compile,
shared-UI WASM compile, workspace tests, Clippy and SBOM tooling for that exact
head.

PR #188 exact implementation head `0d72892f` completes the machine-verifiable
Shell slice with a serializable pane/split/tab/floating layout model, persistent
tool-pane composition, focus-aware shortcut routing and capture guard, typed
window/single-instance lifecycle contracts, accessibility audit contracts and a
five-locale Shell/static-label layer (`en`, `zh`, `zh-hant`, `ja`, `ar`) with RTL
and persistence. Documentation validation, the Hermes Agent harness/native-client
boundary, Rust validation, Windows packaging, install lifecycle and footprint all
passed on that exact head. The Rust gate passed architecture, rustfmt, all-target
and WASM compile, workspace tests, Clippy, SBOM tooling and optimized Windows
artifact production. All `SH-01`…`SH-15` rows are therefore A4; A5 still requires
the row-specific live, visual, keyboard and screen-reader evidence.

PR #192 exact head `655995db` and PR #195 exact head `df9920cf` close the
machine-verifiable prompt-queue and composer-draft/directive slices. Their Rust
validation, Windows packaging, install-lifecycle and footprint workflows all
passed; SSH interoperability was not applicable. PR #196 exact head `e90cc709`
then ports the native attachment pipeline, including opaque picker capability
IDs, local/remote staging, attachment-aware background queues and cleanup. Rust
validation `31714415036`, Windows packaging `31714415069`, install lifecycle
`31714414994` and footprint `31714415039` all passed for that exact head; the
SSH interoperability workflow was the expected skip. `CH-09`…`CH-11` are
therefore A4; A5 still requires the row-specific live composer/session review.

PR #199 exact head `bb48f4d` advances the remaining machine-verifiable chat
slice: bounded large-history hydration/windowing, session-scoped live model/tool/
approval controls, canonical reaction writes with rollback/reconciliation, and
bounded rich content for Markdown, code, math, ANSI, tables, diffs, images,
Mermaid-like graphs and allowlisted social references. Rust validation
`31747821028`, Windows packaging `31747821031`, install lifecycle `31747821027`
and footprint `31747821023` all passed for that exact head; SSH interoperability
was not applicable. `CH-07` and `CH-13`…`CH-21` are therefore A4. `DI-04` is A3:
the safe Dioxus link surface is wired, but plain HTTP is still rejected by the
native external opener and must be aligned before automated verification.

PR #200 exact head `9e04dd9` replaces the Runtime and Tasks placeholders with
typed live status/task surfaces, 1.5-second reconciliation, explicit loading and
error states, and validated start/cancel controls. Rust validation `31748327602`,
Windows packaging `31748327608`, install lifecycle `31748327612` and footprint
`31748327598` all passed for that exact head. `RT-01` and `RT-02` are therefore
A4. The same tranche makes the bounded benchmark and security task launch/
progress/cancel slices usable at A3; persisted reports, result detail/export and
broader OG workflow parity still need implementation and automated coverage.

The merged-PR reconciliation also confirms that PR #190 exact head `3bd7832e`
already closed the `CH-03` machine-verifiable reconnect slice. It implements
bounded exponential backoff, deterministic `Error → Connecting → Open` state,
explicit reconnect-time request failure, pending-request cleanup and responsive
shutdown, with loopback recovery tests. Rust validation `31694802116`, Windows
packaging `31694802071`, install lifecycle `31694802069` and footprint
`31694802203` all passed for that exact head, so `CH-03` is A4.

PR #201 exact head `ab8381f7` adds the typed About surface for product, Agent and
runtime versions, sanitized update status, build provenance, SBOM, checksums and
attestation availability. Rust validation `31750226260`, Windows packaging
`31750226289`, install lifecycle `31750226295` and footprint `31750226317` all
passed before merge as `097f31b2`, so `AG-14` is A4. Packaged visual and
manifest-by-manifest comparison remain A5 evidence.

PR #202 exact composed head `ffda6b8a` aligns the native external opener with
the existing safe Dioxus policy: bounded HTTP, HTTPS and `mailto` targets are
accepted, while empty, oversized, control-character, hostless, credentialed and
privileged-scheme targets are rejected. Rust validation `31752025260`, Windows
packaging `31752025345`, install lifecycle `31752025322` and footprint
`31752025377` all passed before merge as `39ed1047`, so `DI-04` is A4.

PR #203 exact composed head `ea6660e4` wires the Logs surface to a typed,
sanitized diagnostics snapshot and native folder-picker export. Log tails are
allowlisted and bounded to 200 lines/2 MiB, exports include crash state and a
SHA-256 sidecar, and privacy-negative tests cover credentials, tokens, private
paths and private addresses. Rust validation `31753324310`, Windows packaging
`31753324266`, install lifecycle `31753324257`, footprint `31753324291` and the
native-client boundary `31753324245` passed before merge as `7cf5cefe`, so
`AG-13` is A4.

PR #204 exact composed head `832360e1` wires Memory and Curator into Dioxus
through the existing profile-bound native REST boundary. The surface reports
memory/provider/curator state, supports provider configuration and OAuth status,
offers pause/run controls, and requires a two-step confirmation before reset.
Rust validation `31754564185`, Windows packaging `31754564160`, install lifecycle
`31754564163`, footprint `31754564133` and the native-client boundary
`31754564091` passed before merge as `5b2869be`, so `AG-04` is A4.

PR #205 exact composed head `1a96e16f` replaces the Automations and Integrations
placeholders with typed Dioxus surfaces. Cron supports list/run history,
create/update, pause/resume, trigger and confirmed deletion. Messaging supports
state/test/configuration with write-only secret mutations plus pairing approval/
revocation. Webhooks support gateway enablement, one-time create secrets,
enable/disable and confirmed deletion. Authenticated REST remains Desktop-owned
with encoded segments, 30/60-second timeouts, 4 MiB response bounds and redacted
read DTOs. Rust validation `31756212001`, Windows packaging `31756211983`, install
lifecycle `31756212034` and footprint `31756211990` passed before merge as
`9b5f2b58`, so `AG-05`, `AG-06` and `AG-07` are A4.

PR #207 exact head `77840bd7` wires the General settings power and login-item
controls through typed protocol/core/Desktop boundaries. The UI reports bounded
Windows power state, enables/disables the native keep-awake lease and per-user
login item, persists the settings atomically, restores keep-awake on startup and
rolls the UI back on native or persistence failure. Rust validation
`31757593207`, Windows packaging `31757593161`, install lifecycle `31757593172`,
artifact footprint `31757593235` and native-client boundary `31757593199` all
passed for that exact head; SSH interoperability was the expected skip. It
merged as `7e8250b5`, so `DI-10` and `DI-11` are A4.

PR #208 exact head `1677f6bf` completes the machine-verifiable benchmark and
security-task result slice. The typed task DTO now preserves stage, bounded
output/truncation, timestamps, structured failure and result metadata; the
Dioxus detail panel exposes those fields and exports a sanitized JSON report
through a user-selected folder and `FileService` rather than trusting returned
paths. The native boundary caps the list at 512 tasks, text fields at 4 KiB and
output at 256 KiB, with invalid identifiers and traversal-safe export names
covered by focused tests. Rust validation `31758991026`, Windows packaging
`31758991098`, install lifecycle `31758991090` and footprint `31758991028` all
passed for that exact head; SSH interoperability was the expected skip. It
merged as `f5fd2793`, so `RT-05` and `RT-06` are A4. Live benchmark and scan
quality, redaction and report-content comparison remain A5 evidence.

PR #209 exact head `368f9f8e` completes the machine-verifiable diagnostics
recovery slice. The typed snapshot exposes only environment-variable presence
and aggregate counts, never values. The Dioxus Logs surface can clear only the
bounded latest crash record and open the fixed Windows environment settings
surface. Native recovery rejects symlinks and non-files, is idempotent when no
record exists, and launches `SystemPropertiesAdvanced.exe` through explicit
process arguments without shell interpolation. Rust validation `31760519540`,
Windows packaging `31760519412`, install lifecycle `31760519440`, footprint
`31760519408` and native-client boundary `31760519411` all passed for that exact
head; SSH interoperability was the expected skip. It merged as `51395de4`, so
`DI-14` and `DI-15` are A4. Forced native/Agent failures and unusual real Windows
environment configurations remain A5 evidence.

PR #210 exact head `8a217751` adds the first-class Dioxus Artifacts route. It
collects and deduplicates URLs, paths and structured artifact metadata from a
bounded window of recent session history (30 sessions, 2,000 messages per
session, six concurrent history requests and 1,000 displayed artifacts), then
routes preview/open actions through the typed native preview service. The same
credentialed-URL, sensitive-path, MIME and size guards used by Files protect
Artifacts, and focused tests cover collection bounds, unsafe inputs and the
native/UI boundary. Rust validation `31761763025`, Windows packaging
`31761763062`, install lifecycle `31761762972`, footprint `31761763007` and
native-client boundary `31761762975` all passed for that exact head; SSH
interoperability was the expected skip. It merged as `a0f323c2`, so `AG-08` is
A4. Live Agent artifact diversity and visual comparison remain A5 evidence.

PR #211 exact head `d248b42d` ports the Starmap through a typed, profile-scoped
learning-graph service. The native Gateway adapter bounds responses to 4 MiB,
2,000 nodes, 8,000 edges, 256 clusters and 512 memory cards; rejects invalid,
self-referential and dangling graph identifiers; and never exposes connection
controls to the WebView. The Dioxus route applies a second deterministic render
budget of 300 nodes and 1,200 edges and provides search, kind/category filters,
ranked navigation and selection. Rust validation `31763053007`, Windows
packaging `31763053040`, install lifecycle `31763052975` and footprint
`31763052971` all passed for that exact head; SSH interoperability was the
expected skip. It merged as `452a44f1`, so `AG-10` is A4. Large live graphs,
navigation feel and visual comparison remain A5 evidence.

PR #189 exact implementation head `cbeb44c4` closes the machine-verifiable
Projects/Files/Git/Terminal/SSH slice. Desktop SSH mode now routes the existing
typed Terminal surface through an OpenSSH-backed native PTY while local terminals
continue to use the existing Desktop PTY implementation. Dioxus Rust validation
run `31686964451` passed architecture, rustfmt, all-target/WASM compilation,
workspace tests, Clippy, SBOM tooling and optimized Windows artifact production;
Windows packaging `31686964346`, install lifecycle `31686964368`, footprint
`31686964419`, and the Hermes Agent/native-client boundary `31686964412` also
passed. The opt-in SSH interoperability run `31686964490` provisioned an isolated
runner-local Linux `sshd` and passed both the real Desktop `ssh.rs` runtime probe
and the `ssh_terminal.rs` PTY input/output/resize/dispose round trip. All
`PF-01`…`PF-08`, `GT-01`…`GT-07`, `TM-01`…`TM-03` and `SS-01`…`SS-05` rows are
therefore A4. A5 still requires row-specific human/live production-host
acceptance, including ssh-agent/ProxyJump/hardware-key and non-Linux host
scenarios where applicable.

## Roadmap waves

The waves are ordered to minimize rework and make the Rust client progressively
usable as a standalone product. A later wave may start early where dependencies
are already available, but its acceptance gate remains explicit.

| Wave | Goal | Principal rows | Exit condition |
| --- | --- | --- | --- |
| **0 — Architecture and contracts** | Preserve typed Rust ownership, Agent protocol compatibility and Web/WASM boundary. | SH/CH foundations, project/settings/provider contracts | Architecture + protocol/service foundations are stable; no generic native escape hatch. |
| **1 — Connections and standalone boot** | Make Local/Remote/OAuth/Cloud/SSH connection modes production-complete. | CN-01…08, SS-01…05, SH-01 | All connection modes work from cold launch/re-home; real-host SSH and Cloud are exercised. |
| **2 — Core chat parity** | Finish the primary product workflow before peripheral pages. | CH-07…21 | Composer, queue, attachments, controls and rich content match OG behavior and safety. |
| **3 — Workspace tools** | Make Files, Git, Review, Worktrees and Terminal genuinely usable. | PF-05…08, GT-01…07, TM-01…03 | Real project workflows can be completed without falling back to Electron. |
| **4 — Workstation and Agent surfaces** | Port Local Workstation, tasks/models/runtime and Hermes Agent feature surfaces. | RT-01…07, AG-01…14 | All OG workstation/Agent destinations have functional Rust equivalents. |
| **5 — Native Windows integration** | Replace Electron-only OS integration. | DI-01…15 | Notifications, windows, shortcuts, power, protocols, media and recovery work natively. |
| **6 — Distribution and cutover** | Make Rust the only production client. | DI-16…22 | Installer/updater/release flows pass; production entry points use Rust; Electron/React/Node are absent from production artifacts. |
| **7 — Human acceptance** | Product-owner review of the complete port. | every applicable row | Every non-retired row is A6, all blockers are closed, and final regression is signed off. |

### Current critical path

1. Port Hermes Cloud discovery/org/agent connection and finish complete Gateway
   re-home behavior.
2. Finish rich chat/composer parity.
3. Port Workstation/Agent surfaces, then finish native Windows integration.
4. Finish updater/cutover, remove Electron/React/Node from production.
5. Run the full human acceptance pass, including production-host SSH scenarios.

## Known blockers and deliberate debt

No `BX` rows remain at this checkpoint. Remaining deliberate debt includes:

- **Hermes Cloud:** discovery/org selection/agent connection is not yet ported
  (`CN-06`).
- **Rendered native QA:** earlier migration checkpoints recorded an intermittent
  missing top-level native window during some automated visual runs. It must be
  re-proven before rows depending on visual evidence can move to A5.
- **Distribution/cutover:** verified Rust installer/portable distributions and
  native update activation/rollback now exist, but release-channel discovery,
  download and production cutover are not complete. Electron/React remains
  intentionally present as the oracle and current production path.
- **Clippy debt:** CI passes, but non-fatal pedantic/dead-code warnings remain.
  They should be reduced as touched code is modularized rather than normalized
  into permanent warning debt.

## Ownership and architecture guardrails

- `hermes-protocol`: platform-neutral DTOs, validation, JSON-RPC frames and
  forward-compatible event contracts.
- `hermes-agent-client`: WebSocket/REST Agent adapter and connection policy.
- `hermes-core`: product state machines and cohesive typed service traits.
- `hermes-desktop`: Desktop service implementations and Windows/native authority.
- `hermes-ui`: platform-neutral Dioxus components and route surfaces.
- `apps/desktop`: composition root, Dioxus Desktop launch configuration and
  packaging identity.

`hermes-ui` may consume typed core/protocol abstractions. It may not acquire
arbitrary filesystem, process, Git, PTY, Windows, updater or secret-store
authority. The CI architecture guard enforces this boundary, and the shared UI
must continue compiling for `wasm32-unknown-unknown`.

## Capability matrix

The `Human acceptance` column is the minimum manual scenario for the row. It
does not replace the common checklist in `DIOXUS_HUMAN_VALIDATION.md`; both
apply. `Human` is intentionally blank until the product owner records a result.

### Shell, navigation and interaction

| ID | Capability | Rust owner | Dioxus surface | Stage | Current evidence / gap | Human acceptance | Human |
| --- | --- | --- | --- | --- | --- | --- | --- |
| SH-01 | Startup, local Agent bootstrap and failure recovery | `ConnectionService` + Desktop startup authority | boot/failure recovery | A4 Auto-verified | Local mode boots through canonical `Start-Hermes-Local.ps1`; loopback/token validation, bounded diagnostics and retry path are implemented and covered by Rust tests. | Launch from a cold machine state; verify healthy boot, Agent-not-ready, bad/missing runtime, Retry, and recovery without restarting the client. | ⬜ |
| SH-02 | Main window lifecycle and native window actions | composition root window actions | main shell | A4 Auto-verified | Typed native window/lifecycle contracts cover drag, minimize, maximize/restore, close, relaunch, minimum sizing and startup phases; the Desktop single-instance lease and deterministic lifecycle tests pass with the exact-head optimized/package Windows gates. Duplicate instances currently exit rather than proving foreground transfer, and packaged human runtime comparison remains A5 evidence. | Compare startup, close, minimize/maximize/restore, taskbar presence and relaunch behavior with OG in packaged form. | ⬜ |
| SH-03 | Routes and legacy session redirects | `hermes-ui` route model | workspace router | A4 Auto-verified | All audited top-level destinations have typed Dioxus routes; route compilation and architecture checks pass. | Navigate every route and legacy session URL; verify correct destination, back/forward behavior and no state loss. | ⬜ |
| SH-04 | Titlebar, drag regions and window controls | typed composition-root window actions | titlebar | A4 Auto-verified | The custom titlebar uses typed native drag/window actions plus explicit minimum-size and lifecycle contracts with deterministic tests; exact-head optimized Windows, installer and portable gates validate production wiring. Side-by-side normal/maximized visual parity remains A5 evidence. | Side-by-side OG comparison at normal/maximized states; test drag, double-click maximize, buttons and no accidental drag from controls. | ⬜ |
| SH-05 | Sidebar, session navigation and row actions | `SessionService` | sidebar | A4 Auto-verified | Persisted-session list, search, pin/recent sections, active state, rename/archive/delete, optimistic rollback and lineage-root pinning are implemented with deterministic coverage. | Exercise search, pin/unpin, rename, archive, delete/cancel and active/running states on real sessions; compare geometry and hover/context behavior to OG. | ⬜ |
| SH-06 | Pane tree, splits, tabs and floating panes | Desktop `LayoutModel` | pane shell | A4 Auto-verified | PR #188 wires a serializable production Desktop layout model with horizontal/vertical splits, focused groups, tab add/activate/cycle/reorder/close, split resizing, floating/docking, bounded floating geometry, invariant validation and `hermes.desktop.layoutTree.v3` round-trip persistence. The user-facing layout controls and deterministic model/integration tests pass the exact-head Rust/Windows gates. | Create, resize, reorder, close and restore every OG pane configuration; verify persistence and focus. | ⬜ |
| SH-07 | Right rail and persistent tools | typed File/Terminal/Preview services + Desktop layout state | right rail / pane shell | A4 Auto-verified | PR #188 integrates Files, Terminal, Review and Preview as persistent layout pane kinds with retained selection, floating/docking state and typed route launching, while preserving the PR #187 right-rail toggle/state contract. Automated layout/UI contracts and exact-head gates pass; tool bodies remain route-backed rather than claiming complete embedded OG composition. | Open/close/switch tools, hide/show rail, preserve tool state and compare layout with OG. | ⬜ |
| SH-08 | Status bar and model/gateway state | runtime/session/connection read models | status bar | A4 Auto-verified | PR #187 polls typed connection/runtime/task read models and renders gateway state, runtime phase, active model/provider and active task count; deterministic connection/task-state coverage plus the exact-head Rust/Dioxus gates pass. | Verify every status indicator under connected, connecting, offline, model-switching and task-active states. | ⬜ |
| SH-09 | Dark, light, system themes and skin persistence | `SettingsService` + typed window actions | theme provider/settings | A4 Auto-verified | Appearance settings apply immediately, persist atomically and roll back on failure; shared CSS compiles for Desktop and WASM. | Review dark/light/system side-by-side with OG, restart in each mode and test live OS theme changes. | ⬜ |
| SH-10 | Zoom and find-in-page | Desktop shell/WebView authority | settings/find bar | A4 Auto-verified | PR #187 ports the OG 90% baseline, `1.2^level` scale, `[-9,9]` bounds, 0.1 step and `hermes:desktop:zoomLevel` persistence using native Dioxus/WebView zoom. Find-in-page supports live search, next/previous and Escape close; zoom invariants are deterministic-tested and exact-head gates pass. | Test zoom bounds/reset/persistence and find-next/previous/close on long pages. | ⬜ |
| SH-11 | Keyboard routing and keybindings | Desktop shell interaction controller + focus model | all interactive surfaces | A4 Auto-verified | PR #188 adds a typed focus-aware resolver and capture-phase DOM guard so editor, composer, terminal, dialog/overlay and pane contexts retain their owned chords; pane tab selection/cycling/close/new, split and floating shortcuts are deterministic-tested alongside the PR #187 global inventory. Full human shortcut-inventory comparison and any future configurable rebinding remain A5/product follow-up. | Run the OG keyboard shortcut inventory across chat, dialogs, editor, terminal and overlays; verify focus ownership and no collisions. | ⬜ |
| SH-12 | Command palette | typed command registry | palette | A4 Auto-verified | PR #187 implements the typed shell command registry, `Ctrl/Cmd+K/P` opening, label/id/category search, arrow selection and Enter invocation with deterministic registry/search coverage; exact-head Rust/Dioxus gates pass. | Open by keyboard, search/rank commands, invoke representative commands and verify disabled/contextual states. | ⬜ |
| SH-13 | Command Centre | typed command registry | overlay | A4 Auto-verified | PR #187 implements `Ctrl/Cmd+.` Command Centre on the same typed registry with grouped Navigation/View actions and native keyboard-focusable controls; exact-head Rust/Dioxus gates pass. | Review sections, actions, keyboard navigation and visual parity against OG. | ⬜ |
| SH-14 | Accessibility and reduced motion | `hermes-ui` + Desktop shell | all surfaces | A4 Auto-verified | PR #188 extends the existing semantic/focus-visible/reduced-motion work with focus save/restore, capture-safe keyboard ownership and automated audits for accessible control names, tab semantics, duplicate IDs and reduced-motion presence. Deterministic accessibility contracts and exact-head gates pass; product-wide keyboard and screen-reader review remains A5 evidence. | Keyboard-only pass, focus visibility, accessible names, reduced-motion mode and representative screen-reader smoke test. | ⬜ |
| SH-15 | i18n and locale persistence | Desktop shell locale context + `SettingsService` | shell/navigation/settings | A4 Auto-verified | PR #188 implements an auto-verified Shell/global static-label localization layer for `en`, `zh`, `zh-hant`, `ja` and `ar`, including alias normalization, `hermes.desktop.locale` persistence, document language/direction, Arabic RTL and DOM re-application across route renders for the known text/ARIA/title/placeholder catalogue. Complete non-empty catalogue tests pass; this does not claim every `hermes-ui` literal has been refactored, and visual overflow/fallback review remains A5 evidence. | Switch all supported locales, restart, inspect overflow/truncation and verify fallback behavior. | ⬜ |

### Chat, sessions and rich content

| ID | Capability | Rust owner | Dioxus surface | Stage | Current evidence / gap | Human acceptance | Human |
| --- | --- | --- | --- | --- | --- | --- | --- |
| CH-01 | Agent gateway URL/auth resolution | `hermes-agent-client` + `ConnectionService` | connection/recovery | A3 Ported | Local, remote token and OAuth paths are substantially wired; Cloud and full cross-mode reconnect/re-home behavior remain open. | Test Local, remote token, remote OAuth, Cloud and SSH end-to-end once all modes are available; verify selected scope survives restart. | ⬜ |
| CH-02 | JSON-RPC framing, calls, cancellation and unknown fields | `hermes-agent-client` | n/a | A4 Auto-verified | Actor-based client has bounded queues/frames, request timeouts, cancellation cleanup, event routing and protocol tests. | Smoke-test calls/cancellation against a real Agent while streaming; confirm no visible protocol regressions. | ⬜ |
| CH-03 | WebSocket lifecycle, reconnect and degraded recovery | `hermes-agent-client` | connecting/degraded states | A4 Auto-verified | PR #190 exact head `3bd7832e` adds bounded exponential reconnect, deterministic degraded/recovery states, reconnect-time request rejection, pending-call cleanup and responsive close behavior. Loopback recovery tests and exact-head Rust/distribution gates pass; adverse real-network review remains A5 evidence. | Drop/recover network and Agent repeatedly; verify reconnect, no duplicate messages, no lost foreground state and understandable degraded UI. | ⬜ |
| CH-04 | Session identity, lineage and profile scope | `SessionService` | chat/sidebar | A4 Auto-verified | Stored/runtime identities, lineage-root behavior and profile-scoped contracts are implemented/tested. | Open/resume sessions across profiles and restarts; verify correct history, cwd/project and no cross-profile leakage. | ⬜ |
| CH-05 | Session list, search, pin/archive/delete/rename | `SessionService` | sidebar | A4 Auto-verified | Persisted REST list and mutation contracts are implemented with stale-response rollback guards. | Run all mutations on real persisted and active sessions, including failure/rollback cases. | ⬜ |
| CH-06 | New, resume and switch session | `SessionService` | chat workspace | A4 Auto-verified | Create/resume/switch contracts and identity reconciliation exist with live-harness coverage. | Create multiple sessions and rapidly switch/resume while one is running; verify isolation and route selection. | ⬜ |
| CH-07 | Transcript loading, pagination and large-history virtualization | `SessionService` | transcript | A4 Auto-verified | PR #199 adds bounded 100k-history hydration coverage, an explicit million-message pagination contract and an expandable fixed-window Dioxus transcript with containment hints; exact-head Rust/distribution gates pass. Real-device responsiveness and memory comparison remain A5 evidence. | Open small, medium and very large sessions; scroll/search/revisit and compare responsiveness/memory to OG. | ⬜ |
| CH-08 | Streaming deltas, reasoning and tool event reconciliation | `SessionService` | assistant turns/tool cards | A4 Auto-verified | Delta coalescing, interim settlement, reasoning, tool upsert, terminal completion and runtime isolation are tested. | Run real multi-tool/long-reasoning turns, interrupt mid-stream and verify ordering, completion and no duplicate/stranded UI. | ⬜ |
| CH-09 | Prompt queue and background sessions | `SessionService` | composer/status | A4 Auto-verified | PR #192 exact head `655995db` implements a route-persistent typed prompt queue with stored/runtime identity binding, FIFO background draining, park/resume/remove/clear controls and failed-submit requeue semantics. Rust validation `31699023935`, packaging `31699023859`, install lifecycle `31699023843` and footprint `31699023864` all passed; SSH was not applicable. | Queue multiple prompts and run background sessions; verify ordering, cancellation and foreground isolation. | ⬜ |
| CH-10 | Composer drafts, undo and directives | `SessionService` | composer | A4 Auto-verified | PR #195 exact head `df9920cf` adds bounded per-session draft persistence, undo/redo, restore across route/restart, typed slash-directive execution and queue-aware directive sends. Rust validation `31703681496`, packaging `31703681390`, install lifecycle `31703681477` and footprint `31703681363` all passed; SSH was not applicable. | Type/edit drafts, switch routes/sessions, restart, undo and exercise supported directives. | ⬜ |
| CH-11 | Attachments, images and path selection | `FileService` + attachment protocol | composer/preview | A4 Auto-verified | PR #196 exact head `e90cc709` ports the native multi-file picker behind opaque capability IDs, bounded previews/size limits, local path staging, remote/SSH byte staging, file context refs, image fallback prompting, attachment-aware background queues/retries and staged-image cleanup. Exact-head Rust validation `31714415036`, packaging `31714415069`, install lifecycle `31714414994` and footprint `31714415039` all passed; SSH interoperability was the expected skip. | Attach representative text/image/binary files, invalid/oversize files and paths; verify preview/send/cancel/security behavior. | ⬜ |
| CH-12 | Voice recording, transcription and playback | future `MediaService` | composer/settings | A1 Designed | Voice settings exist; media capture/playback runtime is not ported. | Grant/deny microphone permission, record, cancel, transcribe and play audio; verify device/failure behavior. | ⬜ |
| CH-13 | Model, tool and YOLO controls | runtime/trust services | composer/model menus | A4 Auto-verified | PR #199 wires session-scoped live model/tool/approval controls through typed directives and configured-session startup, without mutating global defaults; unsafe pre-chat approval modes are constrained and failure cleanup is covered. Exact-head gates pass. | Change model/tool/approval modes before and during chats; verify scope and safety semantics. | ⬜ |
| CH-14 | Reactions and message metadata | `SessionService` | message actions | A4 Auto-verified | PR #199 implements canonical `message.react` writes with durable row identity, optimistic Tapback UI, authoritative event reconciliation, rollback and reload-safe local projection; focused protocol/service/UI contracts and exact-head gates pass. | Exercise all OG message actions/reactions and verify persistence/identity after reload. | ⬜ |
| CH-15 | Markdown and safe links | bounded rich-content renderer | transcript | A4 Auto-verified | PR #199 adds a bounded Markdown subset for headings, lists, quotes, emphasis, inline code and safe HTTP(S) links; raw HTML remains literal, credentialed/unsafe targets are blocked and navigation uses the typed platform boundary. Exact-head gates pass. | Review headings/lists/quotes/links and malicious HTML/URL fixtures; verify external-link policy. | ⬜ |
| CH-16 | Code blocks and syntax highlighting | bounded rich renderer | transcript/code card | A4 Auto-verified | PR #199 adds bounded fenced-code cards with language labels and conservative Rust/Python/JavaScript token styling, including long-content truncation/fallback contracts; exact-head gates pass. Copy behavior and visual language parity remain A5 evidence. | Review representative languages, long lines, copy, fallback and large-block performance. | ⬜ |
| CH-17 | Math/KaTeX-equivalent behavior | bounded rich renderer | transcript | A4 Auto-verified | PR #199 adds bounded non-executing inline/display math recognition with safe text fallback and malformed-input coverage; exact-head gates pass. This slice does not claim full KaTeX visual equivalence, which remains an A5 comparison item. | Render inline/display math and malformed expressions; compare wrapping and fallback. | ⬜ |
| CH-18 | ANSI, tables and diffs | Rust parsers/read models | transcript/review | A4 Auto-verified | PR #199 adds bounded ANSI styling, Markdown tables and diff blocks with control-sequence stripping, size caps and deterministic parser/render contracts; exact-head gates pass. | Exercise ANSI color/control sequences, wide tables and large diffs; compare readability and safety. | ⬜ |
| CH-19 | Images and generated-image results | `FileService` + safe media protocol | transcript/preview | A4 Auto-verified | PR #199 adds lazy contained remote images and allowlisted image-data URLs while rejecting credentialed, unsupported and active-content targets; exact-head security/render contracts pass. Local/generated failure and MIME parity remain A5 evidence. | Render local/generated/remote image cases, failures and oversized files; verify containment and MIME handling. | ⬜ |
| CH-20 | Mermaid diagrams | bounded non-privileged helper if retained | transcript | A4 Auto-verified | PR #199 adds a bounded in-process graph renderer for simple Mermaid edge syntax; active click/href/URL/script directives are rejected and no privileged helper is exposed. Exact-head gates pass; broader diagram-family visual parity remains A5 evidence. | Render valid/invalid diagrams and hostile labels/URLs under CSP/sanitization policy. | ⬜ |
| CH-21 | External/social embeds | allowlisted embed policy | transcript | A4 Auto-verified | PR #199 identifies allowlisted YouTube, X/Twitter, Reddit, Bluesky and GitHub references as explicit external cards without automatic third-party embeds, preserving offline/privacy behavior; blocked-origin contracts and exact-head gates pass. | Test each supported provider plus blocked origins, navigation escape and privacy/offline behavior. | ⬜ |

### Projects, files, Git, terminal and SSH

| ID | Capability | Rust owner | Dioxus surface | Stage | Current evidence / gap | Human acceptance | Human |
| --- | --- | --- | --- | --- | --- | --- | --- |
| PF-01 | Project registry, active project and project scope | `ProjectService` | sidebar/Project Centre/chat | A4 Auto-verified | Typed project snapshot/selection and chat scope are ported with exact Agent contract fixtures. | Create/select/switch projects and verify chat cwd/scope, restart persistence and sidebar/centre consistency. | ⬜ |
| PF-02 | Project Centre create, attach and clone | `ProjectService` + `PlatformService` | Project Centre | A4 Auto-verified | Folderless create, attach and clone flows plus picker boundary are implemented. | Run Empty/Attach/Clone against disposable projects, including invalid paths/repos and cancellation. | ⬜ |
| PF-03 | Project pin, archive, remove registration | `ProjectService` | Project Centre | A4 Auto-verified | Pin/archive/restore and registration-only removal are implemented. | Verify ordering/filtering, archive/restore and that registration removal never deletes files. | ⬜ |
| PF-04 | Broken-path repair and confirmed filesystem deletion | `ProjectService` + `PlatformService` | Project Centre dialogs | A4 Auto-verified | Exact repair/delete RPC shapes and typed `DELETE <name>` confirmation are covered by fixtures. | Using disposable data, break/repair a path, test wrong confirmations, then perform a real confirmed delete and inspect filesystem result. | ⬜ |
| PF-05 | File tree read and text write service | `FileService` | Files/editor | A4 Auto-verified | Dioxus Files browses nested directories and edits/saves UTF-8 text through the typed `FileService`; canonical root containment/symlink defenses, Files contracts, all-target compile, WASM and PR #179 gates pass. | Browse nested trees, edit/save UTF-8 and edge-case files, verify symlink/path escape rejection. | ⬜ |
| PF-06 | Rename, trash, reveal and open | `FileService` + `PlatformService` | tree/context actions | A4 Auto-verified | Dioxus Files exposes rename, recycle-bin trash, reveal and open through typed native services; containment/security and UI contracts pass with the merged Rust/WASM/Clippy gates. | Rename files/dirs, recycle-bin trash, reveal/open, permission failures and containment checks. | ⬜ |
| PF-07 | Directory and preview watchers | `FileService` | tree/preview | A4 Auto-verified | Dioxus Files now owns the typed directory-watch stream and refreshes the current directory on external changes. The existing bounded Windows watcher authority is preserved, and deterministic Drop cleanup proves stream disposal removes the registry lease without spawning a flaky helper in tests; full PR #179 Rust/WASM/Clippy gates pass. | Edit/create/delete files externally; verify coalesced updates, disposal and no hidden watcher leaks. | ⬜ |
| PF-08 | Preview target normalization and safe preview | `PreviewNormalizationService` | preview pane | A4 Auto-verified | A typed native Preview service now owns normalization and bounded content loading for Dioxus: credentialed HTTP(S) URLs and sensitive/device paths are rejected, local HTML is rendered as escaped source, remote previews are sandboxed, binary/oversize states are explicit, and inline text is capped at 512 KiB. Focused security/UI contracts plus full PR #179 Rust/WASM/Clippy gates pass. | Preview supported/unsupported local files and URLs; test traversal, credentialed URLs, MIME/size and navigation restrictions. | ⬜ |
| GT-01 | Git root/status service | `GitService` | Project Centre/status | A4 Auto-verified | Dioxus Project Centre/status now consumes typed repo root/status state; clean/dirty/unborn/detached fixtures plus architecture, WASM and full Rust gates pass. | Run on clean/dirty/unborn/detached repos and nested paths; verify branch/ahead/behind/change counts. | ⬜ |
| GT-02 | Branches and branch switching | `GitBranchService` | Project Centre/Git | A4 Auto-verified | Dioxus Source Control now lists/switches local branches through the typed native branch service, including default/worktree state; disposable-repo, architecture, WASM and Clippy gates pass. | List/switch/create representative branches, dirty-conflict cases and verify project/session cwd remains coherent. | ⬜ |
| GT-03 | Worktree list/add/remove | `GitWorktreeService` | Project Centre/worktrees | A4 Auto-verified | Dioxus Worktrees now lists/adds/removes Hermes-managed worktrees with collision, main-checkout and force safeguards; disposable-repo, architecture, WASM and Clippy gates pass. | Create/list/remove disposable worktrees; verify path/branch collision and dirty-worktree safeguards. | ⬜ |
| GT-04 | Review diff, stage and unstage foundations | `GitService` | review pane | A4 Auto-verified | The Dioxus Review surface consumes structured Git X/Y state, working-tree and staged diffs, and typed stage/unstage mutations. Disposable-repository round trips, traversal rejection, architecture/WASM and full PR #179 Rust/Clippy gates pass; discard/ship/richer diff rendering remain separate rows. | Inspect binary/text diffs, partial states, stage/unstage and verify exact Git result. | ⬜ |
| GT-05 | Revert/discard changes | `GitDiscardService` | review pane | A4 Auto-verified | Dioxus Review now exposes confirmed scoped/all discard through the typed service; staged/unstaged/untracked/ignored safety fixtures plus architecture, WASM and Clippy gates pass. | Use disposable changes to verify confirmations, staged/unstaged cases and no unintended file loss. | ⬜ |
| GT-06 | Commit, push, ship and PR actions | `GitShipService` | review pane | A4 Auto-verified | Dioxus Review now exposes commit, push, ship and PR actions through bounded Git/gh authority; disposable bare-remote round trips, UI contracts, architecture, WASM and Clippy gates pass. | Commit with edge-case messages, push branches, exercise remote failures and PR creation without shell injection. | ⬜ |
| GT-07 | Repository scanning/discovery | `GitRepoScanService` | project discovery | A4 Auto-verified | Project Centre now consumes profile-scoped repo-scan policy, supports bounded cancellable discovery and explicit registration; native discovery/UI contracts, architecture, WASM and Clippy gates pass. | Scan representative roots, cancellations, permissions, deep trees and repo limits. | ⬜ |
| TM-01 | PTY/ConPTY lifecycle service | `TerminalService` | terminal | A4 Auto-verified | Dioxus Terminal now owns project-scoped PTY start/read/write/resize/dispose with route teardown via synchronous typed disposal; a real Windows PTY round trip including ESC[6n DSR handling, WASM and Clippy gates pass. | After renderer lands: start shell in project cwd, type/resize, run long/interactive commands, exit/dispose and verify no orphan processes. | ⬜ |
| TM-02 | Terminal ANSI rendering, scrollback and persistence | terminal read model | terminal pane | A4 Auto-verified | Dioxus Terminal now uses a bounded ANSI-aware Rust read model with SGR/16/256/truecolor styles, split escape/UTF-8 handling, carriage-return/erase behavior, OSC suppression and per-project in-memory scrollback restoration across route/project reopen. Deterministic parser/persistence tests plus exact-head architecture, all-target, WASM, workspace-test and Clippy gates pass; the same head also passes the optimized Windows footprint build. | Stress ANSI/Unicode/large output, scrollback, hidden/reopened panes and memory/CPU. | ⬜ |
| TM-03 | Remote/SSH terminal behavior | `TerminalService` + Desktop SSH PTY adapter | terminal pane | A4 Auto-verified | In SSH mode the existing Dioxus Terminal service is wrapped by Desktop-owned `ssh_terminal.rs`, launching system OpenSSH with a native PTY while local mode delegates unchanged. Typed start/read/write/resize/dispose, bounded output, DSR handling and option-safe argv/config mapping are unit-tested; exact-head live run `31686964490` passed a runner-local OpenSSH PTY marker round trip including resize and disposal. Remote project-cwd mapping, reconnect and agent/ProxyJump behavior remain A5 evidence. | Connect to a production SSH target, verify expected remote cwd, resize, reconnect, ssh-agent/ProxyJump/hardware-key behavior where available, and cleanup. | ⬜ |
| SS-01 | SSH config Host suggestions, Include traversal and `ssh -G` enrichment | `ConnectionService`/SSH helper | Settings → Gateway | A4 Auto-verified | Native `Host`/`Include` discovery, bounded glob/cycle traversal and 5-second `ssh -G` enrichment are wired into the Dioxus Gateway selector with parity/contract coverage; manual user/port/key values are never overwritten. The exact-head Rust/Dioxus gate is green. | Use aliases, Includes, wildcard exclusions and custom/raw hosts; verify resolved fields match `ssh -G` and manual values are not overwritten. | ⬜ |
| SS-02 | SSH probe/discovery and actionable failure classification | native OpenSSH transport | Settings → Gateway | A4 Auto-verified | System OpenSSH argv, Linux/macOS/Windows Hermes discovery, ownership-capability checks and auth/host-key/network classification are unit-tested. | Test real hosts with success, bad auth, changed host key, timeout, unreachable and missing/old Hermes. | ⬜ |
| SS-03 | POSIX SSH owned backend lifecycle and tunnel reuse | native SSH lifecycle | connection runtime | A4 Auto-verified | Profile-scoped ownership, secure token upload, lock/protocol, owned spawn, readiness, loopback forward, authenticated reuse and safe stale cleanup are implemented/tested. | On real Linux/macOS host: connect, reuse, restart desktop, interrupt network, stale lock/process, remote upgrade and quit cleanup. | ⬜ |
| SS-04 | Windows SSH owned backend lifecycle and tunnel reuse | native Windows SSH lifecycle | connection runtime | A4 Auto-verified | Canonical `hermes_cli.windows_ssh_runtime` helper is used; ownership binds PID + creation time + Hermes path + spawn nonce and reuse requires matching profile/token/path/home. | On real Windows SSH host: same lifecycle matrix as SS-03, including process identity changes and PowerShell/helper failures. | ⬜ |
| SS-05 | Live SSH interoperability matrix | native SSH transport/lifecycle | Settings/Gateway/terminal | A4 Auto-verified | Dedicated opt-in `SSH interoperability` CI provisions an isolated runner-local Linux `sshd` with an ephemeral Ed25519 key and key-only auth, then exercises the real Desktop OpenSSH runtime probe plus remote PTY input/output/resize/dispose. Exact-head run `31686964490` is green, removing the previous external-validation blocker. macOS/Windows production hosts and ssh-agent/ProxyJump/hardware-key/adverse-network cases remain A5/manual evidence, not claimed by this gate. | Human/live integration must cover at least one representative production remote OS plus ssh-agent/ProxyJump/hardware-key setups when available, and representative auth/host-key/network failures. | ⬜ |

### Product, settings, connections and workstation

| ID | Capability | Rust owner | Dioxus surface | Stage | Current evidence / gap | Human acceptance | Human |
| --- | --- | --- | --- | --- | --- | --- | --- |
| PW-01 | Settings overlay, navigation and per-device persistence | `SettingsService` | settings overlay | A4 Auto-verified | OG overlay topology exists; settings JSON persists atomically and theme reload tests pass. | Open every settings section, resize/scroll/close/reopen, restart and compare shell/spacing/navigation to OG. | ⬜ |
| PW-02 | Agent config record/defaults/schema | `AgentConfigService` | settings | A4 Auto-verified | Official profile-aware GET/PUT config contracts preserve unrelated/unknown keys. | Change representative values then reload/profile-switch; verify Agent config and untouched keys. | ⬜ |
| PW-03 | Workspace Agent settings | `AgentConfigService` | Workspace settings | A4 Auto-verified | Working directory, repo discovery, roots/exclusions, execution/shell/env/read-limit controls are ported with nested-key preservation tests. | Exercise every field, invalid values and reload; verify actual Agent behavior where observable. | ⬜ |
| PW-04 | Safety Agent settings | `AgentConfigService` | Safety settings | A4 Auto-verified | Approval, timeout, MCP confirm, allowlist, redaction, private URL, browser routing and checkpoint controls are ported. | Review every control and verify representative approval/redaction/private-URL behavior with real Agent operations. | ⬜ |
| PW-05 | Memory and Context settings | `AgentConfigService` | Memory & Context | A4 Auto-verified | Persistent memory/profile/budgets/provider/context/compression controls are ported. | Change each control, reload and run chats that expose memory/context behavior. | ⬜ |
| PW-06 | Voice settings | `AgentConfigService` | Voice settings | A4 Auto-verified | TTS/STT controls and provider-dependent/open identifier behavior are tested; capture/playback runtime remains CH-12. | Review visibility/defaults/custom identifiers and reload across provider switches. | ⬜ |
| PW-07 | Advanced Agent settings | `AgentConfigService` | Advanced settings | A4 Auto-verified | Curated advanced inventory including terminal, tools, checkpoints, delegation and update policy is ported with whole-record preservation. | Review every advanced field against OG/schema, save/reload and spot-check behavior. | ⬜ |
| PW-08 | Notification preferences, sounds and native test notification | `SettingsService` + future `NotificationService` | Notifications settings | A3 Ported | Preference switches and 14 completion sounds are ported; Windows toast registration/action routing is incomplete. | Toggle each kind, preview every sound, restart, trigger real notifications and verify action routing once native registration lands. | ⬜ |
| PW-09 | Main and auxiliary model configuration | `ModelService` | Model settings | A4 Auto-verified | Official info/options/auxiliary/set contracts and assignment UI are implemented/tested. | Change main and each auxiliary model/reasoning/Fast setting; apply/cancel/reset and verify Agent uses the selections. | ⬜ |
| PW-10 | Mixture-of-Agents configuration | `ModelService` | Model settings | A4 Auto-verified | Presets, aggregator/reference slots, fanout, temperatures/tokens/timeouts/degraded policy and unknown-field preservation are implemented/tested. | Create/clone/edit/delete presets, invalid/incomplete slots and verify saved Agent config and actual MoA invocation. | ⬜ |
| PW-11 | Provider account discovery/disconnect | `ProviderService` | Providers → Accounts | A4 Auto-verified | OG ordering/topology and account list/sign-out contracts are ported. | Review populated/empty/externally-managed accounts and perform real disconnect/reconnect. | ⬜ |
| PW-12 | Provider OAuth PKCE/device/external flows | `ProviderService` + `PlatformService` | provider sign-in overlay | A4 Auto-verified | Start/submit/poll/cancel lifecycle and session cleanup are contract-tested. | Run each available provider flow end-to-end; cancel/expire/reject/error and verify browser/device instructions. | ⬜ |
| PW-13 | Provider API keys and credential editing | `ProviderService` | Providers → API Keys | A4 Auto-verified | Provider grouping, longest-prefix fallback, redacted edit/save/remove and external ownership hints are implemented/tested. | Add/change/remove representative keys; reload, verify masking and actual provider connectivity; check externally-managed keys cannot be silently removed. | ⬜ |
| PW-14 | OpenAI-compatible custom endpoints | `ProviderService` | Providers → Custom Endpoints | A4 Auto-verified | List/add/edit/validate/discover/activate/delete with config-owned guard and blank-secret retention are implemented/tested. | Use real local/remote OpenAI-compatible endpoint, discover models, save/activate/edit/delete and exercise invalid URL/context/auth cases. | ⬜ |
| CN-01 | Local/remote/cloud/SSH connection profile persistence | `ConnectionService` + Credential Manager | Settings/profiles | A4 Auto-verified | Scope DTOs, per-profile/global config, secret indirection, SSH port clear and redaction are tested. | Create global and multiple profile overrides for each mode, restart and verify exact scope/fallback behavior and no plaintext secrets on disk. | ⬜ |
| CN-02 | Gateway connection-mode settings UI | `ConnectionService` | Settings → Gateway | A4 Auto-verified | Local/Cloud/Remote/SSH mode cards, scope chips, auth probe and conditional fields are ported with state tests. | Compare every mode/scope visually and interactively with OG, including env override and unknown/probing auth states. | ⬜ |
| CN-03 | Local Agent bootstrap and live re-home | `ConnectionService` + Desktop startup authority | Settings → Gateway/boot | A4 Auto-verified | Canonical supervisor launch, protected token retrieval, loopback enforcement, Retry and re-home to Local are implemented/tested. | Switch from remote/SSH back to Local during active usage, restart, simulate local Agent failure and verify recovery/no cross-connection leakage. | ⬜ |
| CN-04 | Remote static-token gateway connection | `ConnectionService` + Credential Manager | Settings → Gateway | A4 Auto-verified | Remote token save/reconnect and sanitized preview are implemented. | Connect to a real token gateway, restart, rotate/break token and verify error/recovery without token exposure. | ⬜ |
| CN-05 | Remote Gateway OAuth login/logout, refresh and WS ticket | `ConnectionService` + Credential Manager | Settings → Gateway | A4 Auto-verified | PKCE/state loopback callback, token secret store, refresh and one-time WS ticket path are unit-tested and UI-port exists. | Run real OAuth login/logout/expiry-refresh/cancel/CSRF-error flows and verify no tokens in DOM/URL/logs. | ⬜ |
| CN-06 | Hermes Cloud portal discovery, org selection and agent connection | future `AuthService`/`ConnectionService` | Settings → Gateway | A1 Designed | OG Cloud discovery/org/agent cascade remains unported. | Sign in to Cloud, handle zero/one/multiple orgs, discover agents, connect/switch/reconnect and separate portal auth from Agent connectivity. | ⬜ |
| CN-07 | Gateway/SSH session secrets at rest | native secret stores | no DOM exposure | A4 Auto-verified | Gateway/OAuth/SSH reuse secrets use Credential Manager/keyring indirection; config exposes only markers/previews. | Inspect persisted files/process args/logs/DOM while connecting; rotate/delete secrets and verify ACL/user isolation. | ⬜ |
| CN-08 | Remaining product secrets at rest | future `SecretService` | no DOM exposure | A1 Designed | Complete inventory/migration of every non-Gateway credential is not finished. | Audit every credential source; verify DPAPI/Credential Manager storage, migration, deletion and log/DOM/process-arg redaction. | ⬜ |
| RT-01 | Local Workstation snapshot/home | `RuntimeService` | workstation home | A4 Auto-verified | PR #200 replaces the Runtime placeholder with a typed live status surface for phase, gateway, model, provider, Agent version and detail, including polling plus explicit loading/error states. Service/UI contracts and exact-head Rust/distribution gates pass. | Compare all workstation cards/data/refresh/degraded states against OG using real local services. | ⬜ |
| RT-02 | Runtime actions and Task Centre | `TaskService`/`RuntimeService` | Tasks/status | A4 Auto-verified | PR #200 replaces the Tasks placeholder with a typed live Task Centre using validated list/start/cancel contracts, 1.5-second reconciliation, bounded progress and busy/error states. Focused contracts and exact-head gates pass. | Start/cancel/pause/retry representative tasks, restart client and verify durable progress/errors. | ⬜ |
| RT-03 | Models and model downloads | `RuntimeService` | Models | A1 Designed | Not ported. | Discover/download/cancel/resume/verify/delete representative models; test checksum/disk/network failures. | ⬜ |
| RT-04 | Inference profiles and switching | `RuntimeService` | Models/Profiles | A1 Designed | Not ported. | Switch profiles/models during idle and active sessions; verify rollback, runtime restart and provider-neutral behavior. | ⬜ |
| RT-05 | Benchmarks | `RuntimeService` | Benchmarks | A4 Auto-verified | PR #208 extends the typed benchmark launch/progress/cancel slice with bounded persisted stage/output/timing/failure/result metadata, Dioxus result detail and traversal-safe JSON export through a selected folder. Focused DTO/service/UI contracts and exact-head Rust/distribution gates pass. | Run/cancel benchmark, verify progress/results persistence, exported report content and error handling against OG. | ⬜ |
| RT-06 | Security scan and reports | `RuntimeService` | Security/Tasks | A4 Auto-verified | PR #208 extends the typed security-scan launch/progress/cancel slice with bounded persisted stage/output/timing/failure/result metadata, Dioxus result detail and traversal-safe JSON export through a selected folder. Focused DTO/service/UI contracts and exact-head Rust/distribution gates pass; live finding quality/redaction remains human evidence. | Run scan against disposable targets, inspect progress/redaction/report export and cancellation. | ⬜ |
| RT-07 | Restore and repair | `UpdateService` + `RuntimeService` | recovery | A1 Designed | Not ported. | Corrupt representative runtime/install state and verify data-preserving repair/restore/rollback. | ⬜ |
| AG-01 | Skills hub and local skills | `SkillsService` + authenticated `GatewayServices` | Skills | A4 Auto-verified | Dioxus Skills now lists/toggles profile-scoped local skills and provides Hub sources/search/preview/on-demand scan/install/uninstall/update with independent 1.2s action polling/logs, profile-switch abandonment and trust/verdict display. Authenticated REST stays native with 4 MiB/input bounds; architecture, all-target/WASM, workspace tests and Clippy pass. | List/install/enable/disable/remove skills, trust prompts and invalid packages; compare OG. | ⬜ |
| AG-02 | MCP servers and catalog | Agent/Trust services | Skills/MCP | A1 Designed | Not ported. | Add/config/enable/test/disable/remove MCP servers across scopes with trust/reload prompts. | ⬜ |
| AG-03 | Trust Centre and diagnostics | `TrustService` | Trust Centre | A4 Auto-verified | Native exact `trust.get`/`trust.set_policy` contracts, typed policy decoding, forward-compatible diagnostics and invalid/path-like policy rejection are consumed by the functional Dioxus Trust Centre. Contract, architecture, WASM, workspace-test and Clippy gates are green. | Review every trust policy/diagnostic, change policy and trigger representative protected actions. | ⬜ |
| AG-04 | Memory and curator | `NativeMemoryClient` | Memory/Starmap | A4 Auto-verified | PR #204 wires the profile-bound Memory/Curator service into Dioxus with live status/provider state, provider configuration and OAuth status, curator pause/run controls and a two-step destructive reset. Encoded provider segments, header-only auth, bounded responses/config values and cross-profile rejection remain enforced. Exact-head Rust `31754564185`, packaging `31754564160`, install `31754564163`, footprint `31754564133` and native-client `31754564091` gates passed before merge as `5b2869be`. | Inspect/search/reset memory and curator flows across profiles/sessions; verify destructive confirmation. | ⬜ |
| AG-05 | Cron and scheduled tasks | `NativeCronClient` | Cron overlay | A4 Auto-verified | PR #205 wires typed Cron list/run history, create/update, pause/resume, trigger and confirmed delete into Dioxus with live refresh. Routing-vs-filter profile semantics, encoded IDs, bounded history/input, authenticated transport and 30/60-second timeouts remain enforced. Exact-head Rust `31756212001`, packaging `31756211983`, install `31756212034` and footprint `31756211990` passed before merge as `9b5f2b58`. | Create/edit/enable/disable/run/delete schedules; verify persistence and timezone/error cases. | ⬜ |
| AG-06 | Messaging integrations | `NativeMessagingClient` | Integrations/Messaging | A4 Auto-verified | PR #205 wires platform state/test/configuration and pairing approval/revocation into Dioxus. Read DTOs remain redacted; secret inputs are write-only and mutation payloads are bounded behind encoded IDs and header-only auth. Exact-head Rust `31756212001`, packaging `31756211983`, install `31756212034` and footprint `31756211990` passed before merge as `9b5f2b58`. | Configure supported messaging connectors, test connection/state/errors and secret handling. | ⬜ |
| AG-07 | Webhooks | `NativeWebhookClient` | Integrations/Webhooks | A4 Auto-verified | PR #205 wires webhook gateway enablement, create, enable/disable and confirmed delete into Dioxus, with one-time secret display/dismissal and secret-free read DTOs. Profile scope, encoded one-segment names, bounded authenticated transport and input checks remain enforced. Exact-head Rust `31756212001`, packaging `31756211983`, install `31756212034` and footprint `31756211990` passed before merge as `9b5f2b58`. | Create/test/update/delete webhooks, invalid URLs, secret redaction and scope behavior. | ⬜ |
| AG-08 | Artifacts | Agent + `FileService` | Artifacts | A4 Auto-verified | PR #210 adds a bounded Dioxus artifact index over recent assistant/tool history, with deduplication, search/filter/session links and typed safe preview/open actions. Collection bounds, credentialed URL and sensitive-path negatives, UI contracts, WASM and exact-head Rust/distribution gates pass. | Open representative artifacts, previews/actions, missing files and unsafe path/URL cases. | ⬜ |
| AG-09 | Agents/subagents | Agent services | Agents overlay | A1 Designed | Not ported. | Launch/observe/cancel subagents, switching/background states and error/reconnect behavior. | ⬜ |
| AG-10 | Starmap | `LearningService` + authenticated `GatewayServices` | Starmap overlay | A4 Auto-verified | PR #211 wires a profile-scoped, bounded learning graph into a deterministic Dioxus radial view with search, kind/category filters, ranked navigation and selection. Native and render budgets, identifier/edge validation, source contracts, WASM and exact-head Rust/distribution gates pass. | Load large graph, navigate/select/filter and review performance/visual parity. | ⬜ |
| AG-11 | Hermes TUI panel | Runtime/terminal adapter | TUI page | A1 Designed | Hermes Agent owns the TUI; Dioxus integration/embedding is not ported. | Launch/use TUI inside Hermes Local, verify input/resize/exit/reconnect and no duplicate runtime. | ⬜ |
| AG-12 | Embedded Hermes Agent dashboard | future `DashboardService` | Dashboard/workstation | A1 Designed | Agent dashboard exists upstream; secure Dioxus embed/launch partition is not ported. | Open dashboard, verify exact-loopback/auth partition, navigation restrictions, TUI tab and no token exposure. | ⬜ |
| AG-13 | Logs and diagnostics export | `DiagnosticsExportService` | Logs/About | A4 Auto-verified | PR #203 wires a typed sanitized snapshot, filter/refresh states and native folder-picker export. Allowlisted log tails are bounded to 200 lines/2 MiB; exports include crash state and a SHA-256 sidecar, while privacy-negative tests cover credentials, tokens, private paths and addresses. Exact-head Rust `31753324310`, packaging `31753324266`, install `31753324257`, footprint `31753324291` and native-client `31753324245` gates passed before merge as `7cf5cefe`. | View/filter/copy/export logs, trigger failures and verify secrets/private paths are redacted. | ⬜ |
| AG-14 | About, version and provenance | `PlatformService` | About | A4 Auto-verified | PR #201 wires a typed About surface for product, Agent and runtime versions, sanitized update status, build provenance, SBOM, checksum and attestation availability. Exact-head Rust validation `31750226260`, packaging `31750226289`, install lifecycle `31750226295` and footprint `31750226317` passed before merge as `097f31b2`; packaged visual and manifest-by-manifest comparison remain A5 evidence. | Compare product/Agent/runtime/build versions to manifests and packaged artifact; verify copy/open actions. | ⬜ |

### Desktop integration, lifecycle, distribution and cutover

| ID | Capability | Rust owner | Dioxus surface | Stage | Current evidence / gap | Human acceptance | Human |
| --- | --- | --- | --- | --- | --- | --- | --- |
| DI-01 | Native notifications and action routing | `NotificationPlatform` | session/settings | A2 Service | Native Windows notification wrapper uses a fixed PowerShell/WinForms helper with bounded/sanitized title/body passed through child environment variables and no shell interpolation; notification preferences UI exists. AppUserModelID/toast action registration remains incomplete. | Trigger each notification kind in packaged app; click actions, test duplicates/focus/background and Windows notification settings. | ⬜ |
| DI-02 | Clipboard text/images and save dialogs | `ClipboardService` | chat/context actions | A2 Service | Native Windows text read/write and PNG clipboard-image export use trusted STA PowerShell helpers with stdin/environment data, transient-busy retries, UTF-8/size/NUL/PNG/path checks and fixed helper paths. Dioxus consumers and save-dialog parity remain incomplete. | Copy/paste text/images, WSL edge cases, save dialogs, unsupported formats and size limits. | ⬜ |
| DI-03 | Camera/microphone/media permissions | future `MediaService` | permission surfaces | A1 Designed | Not ported. | Grant/deny/revoke permissions, restart and verify only trusted app origin receives media capability. | ⬜ |
| DI-04 | External browser opening and safe link policy | `PlatformService` | links/preview | A4 Auto-verified | PR #199 wires bounded rich-link cards to the typed external opener; PR #202 exact composed head `ffda6b8a` aligns the native boundary by allowing bounded HTTP, HTTPS and `mailto` targets while rejecting empty, oversized, control-character, hostless, credentialed and privileged-scheme targets. Rust validation `31752025260`, packaging `31752025345`, install lifecycle `31752025322` and footprint `31752025377` passed before merge as `39ed1047`. | Open allowed HTTP(S) links, reject unsafe schemes/credentialed/private targets where policy applies and verify no in-app navigation escape. | ⬜ |
| DI-05 | Deep links and protocol registration | native deep-link service | routed surfaces | A2 Service | Native `hermes://` parsing and per-user Windows protocol registration exist with exact executable command identity and deterministic malformed-input tests; running-instance/single-instance Dioxus delivery is incomplete. | Register/use protocol from cold/running app, malformed payloads, duplicate instance and route/state handling. | ⬜ |
| DI-06 | Session and secondary app windows | native window-state service | shared Dioxus roots | A2 Service | Rust Desktop now consumes the existing bounded `window-state.json` contract, restores the historical 1220×800 default/minimum 400×620 size and maximized state, and unit-tests sanitization, display caps and 48px visibility rules. Safe x/y restoration, live move/resize persistence and secondary/session-window orchestration remain incomplete. | Open multiple session windows, focus/reuse/close/restore and test bounds across monitors/DPI. | ⬜ |
| DI-07 | Quick Entry global shortcut/window | `QuickEntryShortcutController` + native window geometry | Quick Entry | A2 Service | Native shortcut parsing/settings/controller and 640×168 window-geometry foundation matches Electron alias/order/reserved-key semantics, uses bounded fail-soft settings loading and deterministic controller/monitor tests. Actual OS global-hotkey registration, secondary Dioxus window lifecycle and composer submission remain unported. | Register shortcut, summon/dismiss across apps, submit, move monitors and restart. | ⬜ |
| DI-08 | Pet overlay and generator | `WindowService` | pet roots | A1 Designed | Not ported. | Generate/show/hide/move pet, focus/input behavior and persistence. | ⬜ |
| DI-09 | Wake indicator | `WindowService` | wake root | A1 Designed | Not ported. | Trigger/show/hide/reposition indicator and verify lifecycle. | ⬜ |
| DI-10 | Keep-awake, battery and resume | native power service | settings/status | A4 Auto-verified | PR #207 wires bounded Windows AC/battery status plus the native keep-awake lease into General settings, with atomic persistence, startup restore, rollback on failure and deterministic fail-closed parsing/service/UI contracts. The blocker remains `ES_CONTINUOUS | ES_SYSTEM_REQUIRED`, so it never forces the display awake. Exact-head Rust `31757593207`, packaging `31757593161`, install `31757593172`, footprint `31757593235` and native-client `31757593199` gates passed before merge as `7e8250b5`. | Start/stop blocker, sleep/resume laptop, battery/power changes and no leaked blocker. | ⬜ |
| DI-11 | Login item/startup | native login-item service | startup settings | A4 Auto-verified | PR #207 wires the current-user Run-key service into General settings with live read-back, optimistic interaction rollback and atomic setting persistence. Registration remains bound to the exact executable plus `--hermes-local-autostart` through trusted explicit registry argv and deterministic identity/negative tests. Exact-head Rust `31757593207`, packaging `31757593161`, install `31757593172`, footprint `31757593235` and native-client `31757593199` gates passed before merge as `7e8250b5`. | Enable/disable per-user startup, reboot/sign-in and verify correct executable/arguments. | ⬜ |
| DI-12 | Bootstrap, install and uninstall | Rust/Inno install tooling | onboarding/uninstall | A4 Auto-verified | Windows CI verifies clean per-user install, exact payload identity, same-version repair/reinstall, uninstall cleanup and byte-preserving `%APPDATA%\Hermes Local` user data. Older-version upgrade/manual clean-VM review remains. | Clean install, upgrade, repair, uninstall choices and data preservation on a disposable Windows user/VM. | ⬜ |
| DI-13 | Desktop update, stage, promote and rollback | native update activation service | updates/recovery | A2 Service | Native staged activation/rollback verifies exact SHA-256 and PE identity, uses schema-versioned operation-local plans, a copied offline helper, capped retries and probation rollback; tamper/path-escape/non-PE/promotion/rollback tests pass. Update discovery/download/UI/cutover remain incomplete. | Test update available/no-update, interrupted download/apply, locked files, rollback, relaunch and data preservation. | ⬜ |
| DI-14 | Crash forensics and recovery | native crash diagnostics | boot/recovery | A4 Auto-verified | PR #209 wires the bounded privacy-safe crash record into the Dioxus Logs recovery surface and adds a typed clear action that removes only `crashes/latest.json`, rejects symlinks/non-files and is idempotent when absent. Panic hashing/redaction, replacement and UI/native boundary tests plus exact-head gates pass. | Force renderer/native/runtime crashes/corrupt state; verify bounded diagnostics, recovery and no secret leakage. | ⬜ |
| DI-15 | Windows environment, PATH, CA and platform recovery | native platform diagnostics | Logs recovery | A4 Auto-verified | PR #209 exposes only presence/counts for PATH, proxy, CA, WSL, display and app-data state, proves values never cross the DTO/UI boundary, and adds a fixed `SystemPropertiesAdvanced.exe` recovery action without shell interpolation. Focused privacy/action contracts and exact-head gates pass. | Test unusual PATH, user env, custom CA/proxy, WSL/remote display and representative broken-install recovery. | ⬜ |
| DI-16 | Optimized Windows Rust executable build | Rust/Dioxus release tooling | release artifact | A4 Auto-verified | CI builds `hermes-local.exe` with pinned Rust 1.97.1, architecture/WASM/tests/Clippy gates and uploads a Windows x64 artifact. | Run the exact CI artifact on the target Windows machine, verify launch/identity/icon/version and basic navigation. | ⬜ |
| DI-17 | Installer/portable package, install stamp and artifact identity | Rust/Dioxus packaging | installer/portable | A4 Auto-verified | Windows CI builds a per-user Inno installer and exact portable ZIP, verifies install stamp/payload hashes, Start Menu/uninstall registration, silent install/uninstall cleanup and package identity. Trusted attestation wiring exists; no human package review is recorded. | Install both supported distribution forms on clean Windows; verify paths, shortcuts, icon, version, uninstall registration and no Electron runtime. | ⬜ |
| DI-18 | SBOM, hashes and release provenance | release tooling | About/update | A4 Auto-verified | PR #140 / Dioxus Rust validation run #150 passed the optimized Windows build, resolved Cargo CycloneDX generation, SHA-256 release manifest/checksums, focused SBOM tests and artifact upload. Trusted-run attestation wiring is implemented; no human provenance/package review has been recorded. | Verify SBOM/hashes/signatures/version provenance against exact packaged binaries and dependencies. | ⬜ |
| DI-19 | Production script/launcher cutover to Rust | build/setup/launcher tooling | product entry points | A1 Designed | Electron/React scripts remain production-authoritative while migration is in progress. | Run every official install/start/update/repair entry point and prove it launches only Rust client plus intended Agent/runtime children. | ⬜ |
| DI-20 | Remove Electron/React/Node production runtime | repository/build tooling | n/a | A1 Designed | Legacy client intentionally remains as oracle; no production-runtime deletion has happened. | Inspect package/build/install outputs and process tree; prove Electron/Chromium/Node/React client code is absent from production artifacts. | ⬜ |
| DI-21 | Clean-clone, package and full release regression | release/QA | whole product | A1 Designed | Not possible until feature/cutover waves are complete. | From clean clone/clean VM: build, package, install, run full automated suite and execute the human regression plan. | ⬜ |
| DI-22 | Performance and footprint acceptance | release/QA | whole product | A2 Service | Dedicated footprint CI measures the optimized EXE at 14.61 MiB and portable ZIP at 5.09 MiB, both below 64 MiB regression ceilings. Live packaged startup, RAM/CPU and process-count comparison remains incomplete. | Measure packaged cold/warm start, first usable window, idle/active RAM/CPU, process count and disk size on same host; compare against OG baseline and investigate regressions. | ⬜ |

### Contributions and plugins

| ID | Capability | Rust owner | Dioxus surface | Stage | Current evidence / gap | Human acceptance | Human |
| --- | --- | --- | --- | --- | --- | --- | --- |
| PL-01 | Built-in contribution registry | typed contribution model | routes/panes/menus/status | A1 Designed | Not ported as complete extension composition model. | Load built-in contributions and verify route/menu/pane/status composition and isolation. | ⬜ |
| PL-02 | Local runtime JavaScript UI plugins | migration adapter or explicit versioned replacement | contributed surfaces | A0 Audited | Legacy capability is inventoried; final migration/sandbox strategy is intentionally unresolved. | After design: install representative plugin, verify sandbox/CSP/no-native-authority and migration/error UX. | ⬜ |
| PL-03 | Agent integrations and built-in features | Agent protocol/services | native Dioxus surfaces | A1 Designed | Individual Agent feature surfaces are tracked under AG rows; contribution plumbing is incomplete. | Enable representative Agent integrations and verify they appear/behave without giving UI arbitrary native authority. | ⬜ |

## Evidence and stage-update rules

Every stage promotion must be based on evidence from the current branch, not
memory or intent.

- **A0 → A1:** cite/record the OG implementation surfaces and define the Rust
  owner plus acceptance criteria.
- **A1 → A2:** implementation must exist behind typed boundaries, with
  validation for privileged inputs/outputs and meaningful unit/contract tests.
- **A2 → A3:** the actual Dioxus/user surface must use the service; placeholders
  do not count.
- **A3 → A4:** all applicable automated gates must pass. Contract-heavy slices
  require deterministic peer/fixture coverage; privileged slices require
  security-negative cases.
- **A4 → A5:** complete any real-system integration that CI cannot prove,
  capture OG-vs-Rust comparison evidence, resolve known blockers and prepare a
  reproducible build for the human reviewer.
- **A5 → A6:** only the human reviewer may perform this transition, following
  `DIOXUS_HUMAN_VALIDATION.md` and recording the result in the `Human` cell.
- **Regression rule:** a material change to an A6 capability invalidates its
  human sign-off unless the change is demonstrably non-behavioral. Return it to
  A4/A5 and re-review.

Useful evidence belongs in tests, CI, the migration journal, PR discussion, or
`reports/qa/dioxus-human/<build-sha>/<capability-id>/`. The matrix should stay
concise: record the stage and a short evidence pointer rather than embedding
large logs.

## Human cell conventions

- `⬜` — not yet manually reviewed.
- `🟦 READY <sha>` — optional marker when the row is A5 and a review build is
  prepared.
- `✅ @reviewer YYYY-MM-DD <sha>` — PASS; the row may be A6.
- `❌ @reviewer YYYY-MM-DD <sha> — <issue/ref>` — FAIL; row remains at or returns
  below A6 until fixed and re-reviewed.
- `⏸ YYYY-MM-DD — <blocker/ref>` — manual review blocked by an environment or
  external dependency.

The build SHA in a human result is mandatory. A sign-off against one build must
not silently authorize materially different later code.

## Test migration policy

Critical behavior is not considered covered merely because a component renders.
Each old test is classified during its feature slice as one of:

1. Rust unit test;
2. Rust integration/contract test;
3. retained external black-box/E2E test;
4. superseded by a stronger test with an explicit mapping; or
5. obsolete with a documented reason.

Test-only Node/Playwright tooling may remain while it proves visual and packaged
parity, but it may not enter production artifacts after cutover.

## Definition of migration complete

The Rust/Dioxus migration is complete only when all of the following are true:

- every applicable matrix row is **A6 Human-validated**;
- no `BX` blocker remains;
- final distribution/cutover rows are A6;
- Rust is the canonical production client and official launch/update/install
  paths exercise it;
- Electron, React and Node are absent from the production client runtime and
  packaged artifacts;
- Hermes Agent remains the separate pinned Python harness;
- the shared Dioxus UI still respects the typed native boundary and Web/WASM
  compile guard;
- the full automated release regression is green; and
- the product owner has completed the final whole-app regression record defined
  in `DIOXUS_HUMAN_VALIDATION.md`.

Nothing less should be described as the completed port.
