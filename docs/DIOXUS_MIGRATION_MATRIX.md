# Dioxus Migration Roadmap and Validation Matrix

> **Branch source of truth:** `refactor/dioxus-rust-client`  
> **Implementation audit checkpoint:** `642f99d6afa053a3750e0d0fbd45276cad286753` (2026-08-12)  
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
| A1 Designed | 54 | Largest remaining implementation backlog. |
| A2 Service | 26 | Native/core foundations exist; UI/parity work remains. |
| A3 Ported | 6 | User-facing implementation exists; more evidence required. |
| A4 Auto-verified | 39 | Automated slice is green; human/live acceptance still required. |
| A5 Human-ready | 0 | Ready for final manual review. |
| A6 Human-validated | 0 | Human-approved capabilities. |
| BX Blocked | 1 | Named external validation blocker. |
| **Total** | **127** | Every row must end at A6 or be explicitly retired with product-owner approval. |

At least-service coverage is **71/127 capabilities (55.9%)**. This is service-foundation coverage, not end-user parity or human acceptance.

Current CI at this checkpoint is green for Documentation validation, the Hermes
Agent harness/native-client boundary workflow, Dioxus Rust validation, Windows
installer/portable packaging and artifact-footprint regression. The Rust
validation gate includes architecture enforcement, rustfmt,
`cargo check --workspace --all-targets`, shared-UI WASM compilation, workspace
tests, Clippy, optimized Windows release build, resolved Cargo CycloneDX SBOM,
release-manifest/SHA-256 generation and artifact upload. Windows distribution CI
also exercises silent install/uninstall, package identity and data-preserving
repair. This is evidence for applicable A4 rows only; it is not a substitute for
A5 or A6.

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

1. Wire native SSH config-host discovery into the Dioxus Gateway selector and
   execute the real SSH-host interoperability matrix.
2. Port Hermes Cloud discovery/org/agent connection and finish complete Gateway
   re-home behavior.
3. Finish rich chat/composer parity.
4. Turn the existing File/Git/PTY native service foundations into complete
   Dioxus workspace tools.
5. Port Workstation/Agent surfaces, then finish native Windows integration.
6. Finish updater/cutover, remove Electron/React/Node from production, and run
   the full human acceptance pass.

## Known blockers and deliberate debt

- **Real SSH-host validation:** POSIX and Windows owned lifecycle code is
  auto-verified, but live Linux/macOS/Windows interoperability has not yet been
  executed (`SS-05`).
- **SSH settings parity:** native `Host`/`Include` discovery and bounded `ssh -G`
  enrichment now exist, but the Dioxus Gateway host-suggestion selector is not
  wired to that discovery service yet (`SS-01`).
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
| SH-02 | Main window lifecycle and native window actions | composition root window actions | main shell | A3 Ported | Dioxus Desktop window launches with typed drag/minimize/maximize/close actions. Packaged/single-instance/relaunch parity is not yet proven. | Compare startup, close, minimize/maximize/restore, taskbar presence and relaunch behavior with OG in packaged form. | ⬜ |
| SH-03 | Routes and legacy session redirects | `hermes-ui` route model | workspace router | A4 Auto-verified | All audited top-level destinations have typed Dioxus routes; route compilation and architecture checks pass. | Navigate every route and legacy session URL; verify correct destination, back/forward behavior and no state loss. | ⬜ |
| SH-04 | Titlebar, drag regions and window controls | typed composition-root window actions | titlebar | A3 Ported | 34px OG-style custom titlebar and native actions exist; final packaged/window-state visual parity is pending. | Side-by-side OG comparison at normal/maximized states; test drag, double-click maximize, buttons and no accidental drag from controls. | ⬜ |
| SH-05 | Sidebar, session navigation and row actions | `SessionService` | sidebar | A4 Auto-verified | Persisted-session list, search, pin/recent sections, active state, rename/archive/delete, optimistic rollback and lineage-root pinning are implemented with deterministic coverage. | Exercise search, pin/unpin, rename, archive, delete/cancel and active/running states on real sessions; compare geometry and hover/context behavior to OG. | ⬜ |
| SH-06 | Pane tree, splits, tabs and floating panes | future `LayoutService` / core layout model | pane shell | A1 Designed | No production-equivalent pane tree/split/tab/floating-pane state model is complete. | Create, resize, reorder, close and restore every OG pane configuration; verify persistence and focus. | ⬜ |
| SH-07 | Right rail and persistent tools | typed File/Terminal/Preview services | right rail | A1 Designed | Native service foundations exist for some tools, but right-rail composition and retained-state behavior are not ported. | Open/close/switch tools, hide/show rail, preserve tool state and compare layout with OG. | ⬜ |
| SH-08 | Status bar and model/gateway state | runtime/session/connection read models | status bar | A3 Ported | OG-style status bar exists with connection state; full runtime/model/task parity remains incomplete. | Verify every status indicator under connected, connecting, offline, model-switching and task-active states. | ⬜ |
| SH-09 | Dark, light, system themes and skin persistence | `SettingsService` + typed window actions | theme provider/settings | A4 Auto-verified | Appearance settings apply immediately, persist atomically and roll back on failure; shared CSS compiles for Desktop and WASM. | Review dark/light/system side-by-side with OG, restart in each mode and test live OS theme changes. | ⬜ |
| SH-10 | Zoom and find-in-page | future `WindowService` | settings/find bar | A1 Designed | Not ported. | Test zoom bounds/reset/persistence and find-next/previous/close on long pages. | ⬜ |
| SH-11 | Keyboard routing and keybindings | future `ShortcutService` + focus model | all interactive surfaces | A1 Designed | Not ported as a complete collision/focus system. | Run the OG keyboard shortcut inventory across chat, dialogs, editor, terminal and overlays; verify focus ownership and no collisions. | ⬜ |
| SH-12 | Command palette | typed command registry | palette | A1 Designed | Not ported. | Open by keyboard, search/rank commands, invoke representative commands and verify disabled/contextual states. | ⬜ |
| SH-13 | Command Centre | typed command registry | overlay | A1 Designed | Not ported. | Review sections, actions, keyboard navigation and visual parity against OG. | ⬜ |
| SH-14 | Accessibility and reduced motion | `hermes-ui` | all surfaces | A1 Designed | Some semantic labels/reduced-motion CSS exist, but product-wide accessibility review is incomplete. | Keyboard-only pass, focus visibility, accessible names, reduced-motion mode and representative screen-reader smoke test. | ⬜ |
| SH-15 | i18n and locale persistence | `SettingsService` + locale context | all surfaces/settings | A1 Designed | OG locale resources remain oracle; Rust client is predominantly English. | Switch all supported locales, restart, inspect overflow/truncation and verify fallback behavior. | ⬜ |

### Chat, sessions and rich content

| ID | Capability | Rust owner | Dioxus surface | Stage | Current evidence / gap | Human acceptance | Human |
| --- | --- | --- | --- | --- | --- | --- | --- |
| CH-01 | Agent gateway URL/auth resolution | `hermes-agent-client` + `ConnectionService` | connection/recovery | A3 Ported | Local, remote token and OAuth paths are substantially wired; Cloud and complete SSH UX remain open. | Test Local, remote token, remote OAuth, Cloud and SSH end-to-end once all modes are available; verify selected scope survives restart. | ⬜ |
| CH-02 | JSON-RPC framing, calls, cancellation and unknown fields | `hermes-agent-client` | n/a | A4 Auto-verified | Actor-based client has bounded queues/frames, request timeouts, cancellation cleanup, event routing and protocol tests. | Smoke-test calls/cancellation against a real Agent while streaming; confirm no visible protocol regressions. | ⬜ |
| CH-03 | WebSocket lifecycle, reconnect and degraded recovery | `hermes-agent-client` | connecting/degraded states | A2 Service | Core transport/state primitives exist, but full reconnect/backoff/re-home behavior is not proven across all modes. | Drop/recover network and Agent repeatedly; verify reconnect, no duplicate messages, no lost foreground state and understandable degraded UI. | ⬜ |
| CH-04 | Session identity, lineage and profile scope | `SessionService` | chat/sidebar | A4 Auto-verified | Stored/runtime identities, lineage-root behavior and profile-scoped contracts are implemented/tested. | Open/resume sessions across profiles and restarts; verify correct history, cwd/project and no cross-profile leakage. | ⬜ |
| CH-05 | Session list, search, pin/archive/delete/rename | `SessionService` | sidebar | A4 Auto-verified | Persisted REST list and mutation contracts are implemented with stale-response rollback guards. | Run all mutations on real persisted and active sessions, including failure/rollback cases. | ⬜ |
| CH-06 | New, resume and switch session | `SessionService` | chat workspace | A4 Auto-verified | Create/resume/switch contracts and identity reconciliation exist with live-harness coverage. | Create multiple sessions and rapidly switch/resume while one is running; verify isolation and route selection. | ⬜ |
| CH-07 | Transcript loading, pagination and large-history virtualization | `SessionService` | transcript | A3 Ported | Transcript loading/reconciliation exists; realistic very-large-history pagination/virtualization benchmark is not complete. | Open small, medium and very large sessions; scroll/search/revisit and compare responsiveness/memory to OG. | ⬜ |
| CH-08 | Streaming deltas, reasoning and tool event reconciliation | `SessionService` | assistant turns/tool cards | A4 Auto-verified | Delta coalescing, interim settlement, reasoning, tool upsert, terminal completion and runtime isolation are tested. | Run real multi-tool/long-reasoning turns, interrupt mid-stream and verify ordering, completion and no duplicate/stranded UI. | ⬜ |
| CH-09 | Prompt queue and background sessions | `SessionService` | composer/status | A1 Designed | Not ported. | Queue multiple prompts and run background sessions; verify ordering, cancellation and foreground isolation. | ⬜ |
| CH-10 | Composer drafts, undo and directives | `SessionService` | composer | A1 Designed | Basic composer exists; durable drafts/undo/directives parity is incomplete. | Type/edit drafts, switch routes/sessions, restart, undo and exercise supported directives. | ⬜ |
| CH-11 | Attachments, images and path selection | `FileService` + attachment protocol | composer/preview | A1 Designed | Attach affordance exists, but production attachment pipeline is not ported. | Attach representative text/image/binary files, invalid/oversize files and paths; verify preview/send/cancel/security behavior. | ⬜ |
| CH-12 | Voice recording, transcription and playback | future `MediaService` | composer/settings | A1 Designed | Voice settings exist; media capture/playback runtime is not ported. | Grant/deny microphone permission, record, cancel, transcribe and play audio; verify device/failure behavior. | ⬜ |
| CH-13 | Model, tool and YOLO controls | runtime/trust services | composer/model menus | A1 Designed | Settings-side model controls exist; composer-time control parity is incomplete. | Change model/tool/approval modes before and during chats; verify scope and safety semantics. | ⬜ |
| CH-14 | Reactions and message metadata | `SessionService` | message actions | A1 Designed | Not ported. | Exercise all OG message actions/reactions and verify persistence/identity after reload. | ⬜ |
| CH-15 | Markdown and safe links | bounded rich-content renderer | transcript | A1 Designed | Current transcript rendering is basic; full Markdown/security policy is pending. | Review headings/lists/quotes/links and malicious HTML/URL fixtures; verify external-link policy. | ⬜ |
| CH-16 | Code blocks and syntax highlighting | bounded rich renderer | transcript/code card | A1 Designed | Not ported to OG parity. | Review representative languages, long lines, copy, fallback and large-block performance. | ⬜ |
| CH-17 | Math/KaTeX-equivalent behavior | bounded rich renderer | transcript | A1 Designed | Not ported. | Render inline/display math and malformed expressions; compare wrapping and fallback. | ⬜ |
| CH-18 | ANSI, tables and diffs | Rust parsers/read models | transcript/review | A1 Designed | Not ported as full rich content. | Exercise ANSI color/control sequences, wide tables and large diffs; compare readability and safety. | ⬜ |
| CH-19 | Images and generated-image results | `FileService` + safe media protocol | transcript/preview | A1 Designed | Not ported. | Render local/generated/remote image cases, failures and oversized files; verify containment and MIME handling. | ⬜ |
| CH-20 | Mermaid diagrams | bounded non-privileged helper if retained | transcript | A1 Designed | Not ported. | Render valid/invalid diagrams and hostile labels/URLs under CSP/sanitization policy. | ⬜ |
| CH-21 | External/social embeds | allowlisted embed policy | transcript | A1 Designed | Not ported. | Test each supported provider plus blocked origins, navigation escape and privacy/offline behavior. | ⬜ |

### Projects, files, Git, terminal and SSH

| ID | Capability | Rust owner | Dioxus surface | Stage | Current evidence / gap | Human acceptance | Human |
| --- | --- | --- | --- | --- | --- | --- | --- |
| PF-01 | Project registry, active project and project scope | `ProjectService` | sidebar/Project Centre/chat | A4 Auto-verified | Typed project snapshot/selection and chat scope are ported with exact Agent contract fixtures. | Create/select/switch projects and verify chat cwd/scope, restart persistence and sidebar/centre consistency. | ⬜ |
| PF-02 | Project Centre create, attach and clone | `ProjectService` + `PlatformService` | Project Centre | A4 Auto-verified | Folderless create, attach and clone flows plus picker boundary are implemented. | Run Empty/Attach/Clone against disposable projects, including invalid paths/repos and cancellation. | ⬜ |
| PF-03 | Project pin, archive, remove registration | `ProjectService` | Project Centre | A4 Auto-verified | Pin/archive/restore and registration-only removal are implemented. | Verify ordering/filtering, archive/restore and that registration removal never deletes files. | ⬜ |
| PF-04 | Broken-path repair and confirmed filesystem deletion | `ProjectService` + `PlatformService` | Project Centre dialogs | A4 Auto-verified | Exact repair/delete RPC shapes and typed `DELETE <name>` confirmation are covered by fixtures. | Using disposable data, break/repair a path, test wrong confirmations, then perform a real confirmed delete and inspect filesystem result. | ⬜ |
| PF-05 | File tree read and text write service | `FileService` | Files/editor | A2 Service | Typed `read_dir`, `read_text`, `write_text` exist with canonical root containment/symlink defenses; Dioxus Files surface is still placeholder. | After UI lands: browse nested trees, edit/save UTF-8 and edge-case files, verify symlink/path escape rejection. | ⬜ |
| PF-06 | Rename, trash, reveal and open | `FileService` + `PlatformService` | tree/context actions | A2 Service | Native trash exists behind typed service; rename/reveal/open surface/coverage is incomplete. | After complete: rename files/dirs, recycle-bin trash, reveal/open, permission failures and containment checks. | ⬜ |
| PF-07 | Directory and preview watchers | `FileService` | tree/preview | A1 Designed | Not ported. | Edit/create/delete files externally; verify coalesced updates, disposal and no hidden watcher leaks. | ⬜ |
| PF-08 | Preview target normalization and safe preview | `PreviewNormalizationService` | preview pane | A2 Service | Native Desktop normalization matches the Electron preview oracle for HTTP(S)/file/local targets, wildcard-host rewriting, directory `index.html`, sensitive/device-path blocking before/after canonicalization, readability, MIME/language/binary/size classification and Windows extended-path normalization; focused Windows/security tests pass. Dioxus preview/watchers are not wired yet. | Preview supported/unsupported local files and URLs; test traversal, credentialed URLs, MIME/size and navigation restrictions. | ⬜ |
| GT-01 | Git root/status service | `GitService` | Project Centre/status | A2 Service | Typed status uses explicit argv and porcelain parsing; full repo-root/branch UI parity is not present. | Run on clean/dirty/unborn/detached repos and nested paths; verify branch/ahead/behind/change counts. | ⬜ |
| GT-02 | Branches and branch switching | `GitBranchService` | Project Centre/Git | A2 Service | Native local-branch listing/switching detects worktree checkout state and trunk via `origin/HEAD`, Git config, then `main`/`master`; canonical repository roots, Git branch validation, explicit argv and disposable-repository tests are in place. Dioxus Project Centre/Git UI is not wired. | List/switch/create representative branches, dirty-conflict cases and verify project/session cwd remains coherent. | ⬜ |
| GT-03 | Worktree list/add/remove | `GitWorktreeService` | Project Centre/worktrees | A2 Service | Native list/add-new/add-existing/remove lifecycle exists under managed `.worktrees`, with branch/ref validation, collision handling, main-worktree removal refusal, registered-path confinement and disposable-repository tests. Dioxus worktree UI is not wired. | Create/list/remove disposable worktrees; verify path/branch collision and dirty-worktree safeguards. | ⬜ |
| GT-04 | Review diff, stage and unstage foundations | `GitService` | review pane | A2 Service | Typed `diff`, `stage`, `unstage` exist with explicit argv/`--`; review UI and revert/full parity remain incomplete. | After UI lands: inspect binary/text diffs, partial states, stage/unstage and verify exact Git result. | ⬜ |
| GT-05 | Revert/discard changes | `GitDiscardService` | review pane | A2 Service | Native scoped/all discard resets index/worktree to `HEAD` and removes only non-ignored untracked files; literal pathspec validation blocks traversal, Git metadata and pathspec magic. Staged-addition, staged+unstaged, untracked and ignored-file cases are covered. Confirmation/review UI is not wired. | Use disposable changes to verify confirmations, staged/unstaged cases and no unintended file loss. | ⬜ |
| GT-06 | Commit, push, ship and PR actions | `GitShipService` | review pane | A2 Service | Native commit/push foundation uses explicit bounded Git/`gh` processes; it stages all only when the index is empty, validates bounded/NUL-safe commit messages, reuses tracking or sets upstream on first push and parses PR info. Disposable bare-remote round-trip tests pass. Dioxus Review ship UI is not wired. | Commit with edge-case messages, push branches, exercise remote failures and PR creation without shell injection. | ⬜ |
| GT-07 | Repository scanning/discovery | `GitRepoScanService` | project discovery | A2 Service | Native bounded repo discovery scans configured/home roots with depth/visited limits, exclusion and junk/hidden skipping, overlapping-root dedupe and segment-aware containment; disabled/tilde/relative/exclusion cases are tested. Discovery UI is not wired. | Scan representative roots, cancellations, permissions, deep trees and repo limits. | ⬜ |
| TM-01 | PTY/ConPTY lifecycle service | `TerminalService` | terminal | A2 Service | Native `portable-pty` start/write/read/resize/dispose exists behind typed service; Dioxus terminal is placeholder. | After renderer lands: start shell in project cwd, type/resize, run long/interactive commands, exit/dispose and verify no orphan processes. | ⬜ |
| TM-02 | Terminal ANSI rendering, scrollback and persistence | terminal read model | terminal pane | A1 Designed | Not ported. | Stress ANSI/Unicode/large output, scrollback, hidden/reopened panes and memory/CPU. | ⬜ |
| TM-03 | Remote/SSH terminal behavior | `TerminalService` + SSH transport | terminal pane | A1 Designed | SSH Agent tunnel exists but interactive terminal parity is not integrated. | Connect to real SSH target, verify cwd, resize, reconnect, auth-agent use and cleanup. | ⬜ |
| SS-01 | SSH config Host suggestions, Include traversal and `ssh -G` enrichment | `ConnectionService`/SSH helper | Settings → Gateway | A2 Service | Native `Host`/`Include` discovery, bounded glob/cycle traversal and 5-second `ssh -G` enrichment are implemented with parity tests; manual user/port/key values are never overwritten. Dioxus host suggestions are not wired yet. | Use aliases, Includes, wildcard exclusions and custom/raw hosts; verify resolved fields match `ssh -G` and manual values are not overwritten. | ⬜ |
| SS-02 | SSH probe/discovery and actionable failure classification | native OpenSSH transport | Settings → Gateway | A4 Auto-verified | System OpenSSH argv, Linux/macOS/Windows Hermes discovery, ownership-capability checks and auth/host-key/network classification are unit-tested. | Test real hosts with success, bad auth, changed host key, timeout, unreachable and missing/old Hermes. | ⬜ |
| SS-03 | POSIX SSH owned backend lifecycle and tunnel reuse | native SSH lifecycle | connection runtime | A4 Auto-verified | Profile-scoped ownership, secure token upload, lock/protocol, owned spawn, readiness, loopback forward, authenticated reuse and safe stale cleanup are implemented/tested. | On real Linux/macOS host: connect, reuse, restart desktop, interrupt network, stale lock/process, remote upgrade and quit cleanup. | ⬜ |
| SS-04 | Windows SSH owned backend lifecycle and tunnel reuse | native Windows SSH lifecycle | connection runtime | A4 Auto-verified | Canonical `hermes_cli.windows_ssh_runtime` helper is used; ownership binds PID + creation time + Hermes path + spawn nonce and reuse requires matching profile/token/path/home. | On real Windows SSH host: same lifecycle matrix as SS-03, including process identity changes and PowerShell/helper failures. | ⬜ |
| SS-05 | Live SSH interoperability matrix | native SSH transport/lifecycle | Settings/Gateway/terminal | BX Blocked | Automated/unit validation is green, but no real Linux/macOS/Windows SSH target matrix has been executed from this branch. | Human/live integration must cover at least one representative remote OS used in production plus ssh-agent/ProxyJump/hardware-key setups when available. | ⬜ |

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
| RT-01 | Local Workstation snapshot/home | `RuntimeService` | workstation home | A1 Designed | Route/shell placeholder exists; production snapshot schema/actions are not ported. | Compare all workstation cards/data/refresh/degraded states against OG using real local services. | ⬜ |
| RT-02 | Runtime actions and Task Centre | future `TaskService`/`RuntimeService` | Tasks/status | A1 Designed | Service trait foundations exist but durable task lifecycle UI is not ported. | Start/cancel/pause/retry representative tasks, restart client and verify durable progress/errors. | ⬜ |
| RT-03 | Models and model downloads | `RuntimeService` | Models | A1 Designed | Not ported. | Discover/download/cancel/resume/verify/delete representative models; test checksum/disk/network failures. | ⬜ |
| RT-04 | Inference profiles and switching | `RuntimeService` | Models/Profiles | A1 Designed | Not ported. | Switch profiles/models during idle and active sessions; verify rollback, runtime restart and provider-neutral behavior. | ⬜ |
| RT-05 | Benchmarks | `RuntimeService` | Benchmarks | A1 Designed | Not ported. | Run/cancel benchmark, verify progress/results persistence and error handling. | ⬜ |
| RT-06 | Security scan and reports | future `SecurityService` | Security/Tasks | A1 Designed | Not ported. | Run scan against disposable targets, inspect progress/redaction/report export and cancellation. | ⬜ |
| RT-07 | Restore and repair | `UpdateService` + `RuntimeService` | recovery | A1 Designed | Not ported. | Corrupt representative runtime/install state and verify data-preserving repair/restore/rollback. | ⬜ |
| AG-01 | Skills hub and local skills | Agent/Trust services | Skills | A1 Designed | Not ported. | List/install/enable/disable/remove skills, trust prompts and invalid packages; compare OG. | ⬜ |
| AG-02 | MCP servers and catalog | Agent/Trust services | Skills/MCP | A1 Designed | Not ported. | Add/config/enable/test/disable/remove MCP servers across scopes with trust/reload prompts. | ⬜ |
| AG-03 | Trust Centre and diagnostics | `TrustService` | Trust Centre | A1 Designed | Typed Trust service exists but full surface/policy parity is not implemented. | Review every trust policy/diagnostic, change policy and trigger representative protected actions. | ⬜ |
| AG-04 | Memory and curator | Agent services | Memory/Starmap | A1 Designed | Not ported. | Inspect/search/reset memory and curator flows across profiles/sessions; verify destructive confirmation. | ⬜ |
| AG-05 | Cron and scheduled tasks | Agent services | Cron overlay | A1 Designed | Not ported. | Create/edit/enable/disable/run/delete schedules; verify persistence and timezone/error cases. | ⬜ |
| AG-06 | Messaging integrations | Agent services | Integrations/Messaging | A1 Designed | Not ported. | Configure supported messaging connectors, test connection/state/errors and secret handling. | ⬜ |
| AG-07 | Webhooks | Agent services | Integrations/Webhooks | A1 Designed | Not ported. | Create/test/update/delete webhooks, invalid URLs, secret redaction and scope behavior. | ⬜ |
| AG-08 | Artifacts | Agent + `FileService` | Artifacts | A1 Designed | Not ported. | Open representative artifacts, previews/actions, missing files and unsafe path/URL cases. | ⬜ |
| AG-09 | Agents/subagents | Agent services | Agents overlay | A1 Designed | Not ported. | Launch/observe/cancel subagents, switching/background states and error/reconnect behavior. | ⬜ |
| AG-10 | Starmap | Agent services | Starmap overlay | A1 Designed | Not ported. | Load large graph, navigate/select/filter and review performance/visual parity. | ⬜ |
| AG-11 | Hermes TUI panel | Runtime/terminal adapter | TUI page | A1 Designed | Hermes Agent owns the TUI; Dioxus integration/embedding is not ported. | Launch/use TUI inside Hermes Local, verify input/resize/exit/reconnect and no duplicate runtime. | ⬜ |
| AG-12 | Embedded Hermes Agent dashboard | future `DashboardService` | Dashboard/workstation | A1 Designed | Agent dashboard exists upstream; secure Dioxus embed/launch partition is not ported. | Open dashboard, verify exact-loopback/auth partition, navigation restrictions, TUI tab and no token exposure. | ⬜ |
| AG-13 | Logs and diagnostics export | `DiagnosticsExportService` | Logs/About | A2 Service | Native diagnostics export writes bounded allowlisted/redacted support data plus a SHA-256 sidecar; it blocks forbidden secrets and redacts credentialed URLs, private roots/private IPs and opaque token-like values. Windows crash/log fixtures and privacy-negative tests pass. Logs/About UI is not wired. | View/filter/copy/export logs, trigger failures and verify secrets/private paths are redacted. | ⬜ |
| AG-14 | About, version and provenance | `PlatformService` | About | A2 Service | Native version accessor exists; full provenance/SBOM/update information surface is incomplete. | Compare product/Agent/runtime/build versions to manifests and packaged artifact; verify copy/open actions. | ⬜ |

### Desktop integration, lifecycle, distribution and cutover

| ID | Capability | Rust owner | Dioxus surface | Stage | Current evidence / gap | Human acceptance | Human |
| --- | --- | --- | --- | --- | --- | --- | --- |
| DI-01 | Native notifications and action routing | `NotificationPlatform` | session/settings | A2 Service | Native Windows notification wrapper uses a fixed PowerShell/WinForms helper with bounded/sanitized title/body passed through child environment variables and no shell interpolation; notification preferences UI exists. AppUserModelID/toast action registration remains incomplete. | Trigger each notification kind in packaged app; click actions, test duplicates/focus/background and Windows notification settings. | ⬜ |
| DI-02 | Clipboard text/images and save dialogs | `ClipboardService` | chat/context actions | A2 Service | Native Windows text read/write and PNG clipboard-image export use trusted STA PowerShell helpers with stdin/environment data, transient-busy retries, UTF-8/size/NUL/PNG/path checks and fixed helper paths. Dioxus consumers and save-dialog parity remain incomplete. | Copy/paste text/images, WSL edge cases, save dialogs, unsupported formats and size limits. | ⬜ |
| DI-03 | Camera/microphone/media permissions | future `MediaService` | permission surfaces | A1 Designed | Not ported. | Grant/deny/revoke permissions, restart and verify only trusted app origin receives media capability. | ⬜ |
| DI-04 | External browser opening and safe link policy | `PlatformService` | links/preview | A2 Service | Typed external URL opener allowlists schemes; link-title/SSRF/full rich-link UX remains incomplete. | Open allowed HTTP(S) links, reject unsafe schemes/credentialed/private targets where policy applies and verify no in-app navigation escape. | ⬜ |
| DI-05 | Deep links and protocol registration | native deep-link service | routed surfaces | A2 Service | Native `hermes://` parsing and per-user Windows protocol registration exist with exact executable command identity and deterministic malformed-input tests; running-instance/single-instance Dioxus delivery is incomplete. | Register/use protocol from cold/running app, malformed payloads, duplicate instance and route/state handling. | ⬜ |
| DI-06 | Session and secondary app windows | native window-state service | shared Dioxus roots | A2 Service | Rust Desktop now consumes the existing bounded `window-state.json` contract, restores the historical 1220×800 default/minimum 400×620 size and maximized state, and unit-tests sanitization, display caps and 48px visibility rules. Safe x/y restoration, live move/resize persistence and secondary/session-window orchestration remain incomplete. | Open multiple session windows, focus/reuse/close/restore and test bounds across monitors/DPI. | ⬜ |
| DI-07 | Quick Entry global shortcut/window | Shortcut + Window services | Quick Entry | A1 Designed | Not ported. | Register shortcut, summon/dismiss across apps, submit, move monitors and restart. | ⬜ |
| DI-08 | Pet overlay and generator | `WindowService` | pet roots | A1 Designed | Not ported. | Generate/show/hide/move pet, focus/input behavior and persistence. | ⬜ |
| DI-09 | Wake indicator | `WindowService` | wake root | A1 Designed | Not ported. | Trigger/show/hide/reposition indicator and verify lifecycle. | ⬜ |
| DI-10 | Keep-awake, battery and resume | native power service | settings/status | A2 Service | Dedicated Windows helper holds `ES_CONTINUOUS | ES_SYSTEM_REQUIRED` without forcing the display awake; idempotent enable/disable/drop behavior is covered by the composed Rust/Windows gate. Battery/resume/settings integration remains. | Start/stop blocker, sleep/resume laptop, battery/power changes and no leaked blocker. | ⬜ |
| DI-11 | Login item/startup | native login-item service | startup settings | A2 Service | Current-user Run-key service binds the exact executable plus `--hermes-local-autostart`, uses trusted explicit registry argv, verifies read-back state and has deterministic negative tests. Settings UI/startup UX is not wired. | Enable/disable per-user startup, reboot/sign-in and verify correct executable/arguments. | ⬜ |
| DI-12 | Bootstrap, install and uninstall | Rust/Inno install tooling | onboarding/uninstall | A4 Auto-verified | Windows CI verifies clean per-user install, exact payload identity, same-version repair/reinstall, uninstall cleanup and byte-preserving `%APPDATA%\Hermes Local` user data. Older-version upgrade/manual clean-VM review remains. | Clean install, upgrade, repair, uninstall choices and data preservation on a disposable Windows user/VM. | ⬜ |
| DI-13 | Desktop update, stage, promote and rollback | native update activation service | updates/recovery | A2 Service | Native staged activation/rollback verifies exact SHA-256 and PE identity, uses schema-versioned operation-local plans, a copied offline helper, capped retries and probation rollback; tamper/path-escape/non-PE/promotion/rollback tests pass. Update discovery/download/UI/cutover remain incomplete. | Test update available/no-update, interrupted download/apply, locked files, rollback, relaunch and data preservation. | ⬜ |
| DI-14 | Crash forensics and recovery | native crash diagnostics | boot/recovery | A2 Service | Native startup panic hook writes bounded timestamp/version/location plus a panic SHA-256 without persisting raw panic text, env, argv or tokens; replacement/redaction tests pass. Renderer/runtime crash capture and recovery UI remain. | Force renderer/native/runtime crashes/corrupt state; verify bounded diagnostics, recovery and no secret leakage. | ⬜ |
| DI-15 | Windows environment, PATH, CA and platform recovery | native platform diagnostics | no direct surface | A2 Service | Privacy-safe diagnostics normalize/dedupe PATH and report only presence for proxy/CA/WSL/display/app-data state; tests prove sensitive values are not retained. Recovery actions/UI remain incomplete. | Test unusual PATH, user env, custom CA/proxy, WSL/remote display and representative broken-install recovery. | ⬜ |
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
