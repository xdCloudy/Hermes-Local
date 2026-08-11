# Dioxus Migration Matrix

This is the living capability ledger for the Hermes Local Desktop migration.
The starting point is `main` at
`def1f22aabc36f1e03b9fb72edbf33da71b27cf7`. The existing client remains the
behavioral and visual oracle until every applicable row is validated.

Status meanings:

- **Audited**: current owner and contract were identified.
- **Designed**: a Rust owner and Dioxus surface are assigned.
- **Ported**: implementation exists but final parity validation is incomplete.
- **Validated**: behavior, security boundary, and relevant UI are proven.
- **Blocked**: an external dependency prevents validation; the blocker is named.

## Audit summary

- Product-owned client: `apps/desktop`.
- Current renderer: React 19 + TypeScript + Vite.
- Current native authority: Electron 40 main process and preload bridge.
- Current package authority: Electron Builder 26.
- Current production PTY: `node-pty` 1.1.
- Current routes: 28 reserved route IDs plus one-segment contributed routes and
  legacy session redirects.
- Current Desktop tree: 1,546 files at initial audit, including 703 `.ts`, 507
  `.tsx`, and 510 test/spec files.
- Main-process literal IPC inventory: 125 unique channels.
- Preload-observed literal IPC inventory: 174 unique channels, including events
  and Hermes Local controller channels registered through helper modules.
- Existing native modules cover backend discovery/lifecycle, connections,
  profiles, OAuth/cloud, encrypted secrets, dashboard embedding, filesystem,
  Git/review/worktrees, PTYs, SSH, updates, bootstrap/install/uninstall, local
  runtime actions, Trust Centre, media permissions, clipboard, notifications,
  deep links, secondary windows, quick entry, pet/wake overlays, window state,
  power, zoom, crash forensics and platform recovery.

## Planned Rust ownership

The exact crate names remain subject to validation, but responsibility is
intentionally split as follows:

- `hermes-protocol`: platform-neutral DTOs, validation, JSON-RPC frames and
  forward-compatible event contracts.
- `hermes-agent-client`: WebSocket/REST Agent adapter and connection policy.
- `hermes-core`: product state machines and cohesive typed service traits.
- `hermes-desktop`: Desktop service implementations and Windows authority.
- `hermes-ui`: platform-neutral Dioxus components and route surfaces.
- `apps/desktop`: composition root, Dioxus Desktop launch configuration and
  packaging identity.

The UI may depend on `hermes-core`, `hermes-protocol` and
`hermes-agent-client` abstractions. It may not depend on `hermes-desktop` or
perform arbitrary filesystem, process, Git, PTY, Windows, secret-store or
updater work itself.

## Shell, navigation and interaction

| Old capability | New Rust owner | New Dioxus surface | Coverage required | Status |
| --- | --- | --- | --- | --- |
| Startup/boot progress and failure recovery | `PlatformService` + `RuntimeService` | boot/onboarding/failure overlays | bootstrap state-machine unit + packaged launch E2E | Designed |
| Main window lifecycle and single-instance behavior | typed composition-root window actions | main shell | Windows integration + packaged relaunch | Ported |
| Routes and legacy session redirects | `hermes-ui` route model | workspace router | route classification and navigation tests | Ported |
| Titlebar, drag regions and window controls | typed composition-root window actions | titlebar | window-state integration + visual parity | Ported |
| Sidebar and session navigation | `SessionService` | sidebar | merge/scope/selection unit + visual/E2E | Ported |
| Pane tree, splits, tabs and floating panes | `hermes-core` layout model | pane shell | layout invariants + interaction E2E | Designed |
| Right rail and persistent tools | cohesive file/terminal/preview services | right rail | lifecycle and hidden-state retention tests | Designed |
| Status bar and model/gateway status | runtime/session read models | status bar | state derivation unit + visual parity | Ported |
| Dark/light/system themes and translucency | `SettingsService` + typed window actions | theme provider/settings | persisted scope + visual baselines | Ported |
| Zoom and find-in-page | `WindowService` | settings/find bar | bounds/shortcut/integration tests | Designed |
| Keyboard routing/keybindings | `ShortcutService` + UI focus model | all interactive surfaces | focus ownership and collision E2E | Designed |
| Command palette | typed command registry | palette | ranking/action tests + keyboard E2E | Designed |
| Command Centre | typed command registry | overlay | section/action tests + visual parity | Designed |
| Accessibility/reduced motion | `hermes-ui` | all surfaces | semantics, keyboard and reduced-motion E2E | Designed |
| i18n and locale persistence | `SettingsService` | locale context/settings | locale fallback + representative visual checks | Designed |

## Chat, sessions and rich content

| Old capability | New Rust owner | New Dioxus surface | Coverage required | Status |
| --- | --- | --- | --- | --- |
| Agent gateway URL/auth resolution | `hermes-agent-client` | connection status/recovery | URL/auth ladder fixtures | Designed |
| JSON-RPC framing, calls and cancellation | `hermes-agent-client` | n/a | protocol fixtures + harness replay | Validated |
| WebSocket lifecycle/reconnect | `hermes-agent-client` | connecting/degraded states | fake socket + real harness tests | Designed |
| Session identities, lineage and profile scope | `SessionService` | chat/sidebar | durable/runtime/lineage mapping tests | Ported |
| Session list merge, pin/archive/delete | `SessionService` | sidebar | stale-response/optimistic rollback tests | Ported |
| New/resume/switch session | `SessionService` | chat workspace | route/race/background isolation E2E | Ported |
| Transcript load and large-history virtualization | `SessionService` | transcript | pagination + realistic long-session benchmark | Ported |
| Streaming deltas and terminal events | `SessionService` | assistant turns/tool cards | coalescing/terminal-flush/perf tests | Ported |
| Prompt queue and background sessions | `SessionService` | composer/status | queue order and foreground isolation tests | Designed |
| Composer drafts, undo and directives | `SessionService` | composer | scope/persistence/keyboard tests | Designed |
| Attachments, images and path selection | `FileService` | composer/preview | path/size/MIME validation + E2E | Designed |
| Voice recording, transcription and playback | `MediaService` | composer/settings | permission/limit/playback tests | Designed |
| Model/tool controls and YOLO state | runtime/trust services | composer/model menus | policy/state tests | Designed |
| Reactions and message metadata | `SessionService` | message actions | reconciliation and identity tests | Designed |
| Markdown and links | safe rich-content renderer | transcript | XSS/link-policy fixture suite | Designed |
| Code blocks and syntax highlighting | bounded renderer | transcript/code card | language/fallback/copy tests + perf | Designed |
| Math/KaTeX behavior | bounded renderer | transcript | fixture and visual parity tests | Designed |
| ANSI, tables and diffs | Rust parsers/view models | transcript/review | parser fixtures + visual tests | Designed |
| Images and generated-image results | `FileService` + safe media protocol | transcript/preview | URL/path/MIME/size security tests | Designed |
| Mermaid diagrams | bounded, non-privileged WebView helper if retained | transcript | sanitization and CSP tests | Designed |
| External/social embeds | allowlisted embed policy | transcript | origin/navigation/privacy tests | Designed |

## Projects, files, Git and terminal

| Old capability | New Rust owner | New Dioxus surface | Coverage required | Status |
| --- | --- | --- | --- | --- |
| Projects registry and project scope | `ProjectService` | sidebar/Project Centre | persistence/merge/scope tests | Ported |
| Project Centre create/edit/remove | `ProjectService` | dialogs | validation/rollback/E2E | Ported |
| Broken project path repair and confirmed file deletion | `ProjectService` + `PlatformService` | Project Centre row/dialog | exact RPC fixtures + typed-confirmation E2E | Validated |
| Default project directory and pickers | `SettingsService` + `PlatformService` | settings/dialogs | canonical path/boundary tests | Ported |
| File tree, read and text write | `FileService` | tree/editor | root containment/symlink/encoding tests | Designed |
| Rename, trash, reveal and open | `FileService` | tree/context actions | containment and platform integration tests | Designed |
| Directory and preview watchers | `FileService` | tree/preview | lifecycle/coalescing/path tests | Designed |
| Preview target normalization | `FileService` | preview pane | URL/path traversal fixtures | Designed |
| Git root/status/branches/switch | `GitService` | Project Centre/status | argument-array/repo-boundary tests | Designed |
| Worktree list/add/remove | `GitService` | Project Centre | branch/path/collision integration tests | Designed |
| Review list/diff/stage/unstage/revert | `GitService` | review pane | porcelain fixtures + destructive confirmation E2E | Designed |
| Commit/push/ship/PR actions | `GitService` | review pane | message/ref injection and remote tests | Designed |
| Repository scanning | `GitService` | project discovery | root/cancellation/limit tests | Designed |
| PTY start/write/resize/cwd/dispose | `TerminalService` using ConPTY on Windows | terminal | lifecycle/escape/cwd/resize tests | Designed |
| Terminal rendering and persistence | terminal read model | terminal pane | ANSI throughput + hidden-state retention benchmark | Designed |
| SSH config/host resolution | `SshService` | connection settings | `ssh -G --` argument and parsing fixtures | Designed |

## Product and workstation features

| Old capability | New Rust owner | New Dioxus surface | Coverage required | Status |
| --- | --- | --- | --- | --- |
| Settings and per-scope persistence | `SettingsService` | settings overlay | schema/scope/migration tests | Ported |
| Agent config record, defaults and schema | `AgentConfigService` | Model/Chat settings | profile-aware REST replacement contract + rendered save E2E | Validated |
| Workspace Agent configuration | `AgentConfigService` | Workspace settings | nested-key preservation + schema controls + rendered save/reload | Validated |
| Safety Agent configuration | `AgentConfigService` | Safety settings | curated security whitelist + enum fixtures + rendered save/reload | Validated |
| Memory and context Agent configuration | `AgentConfigService` | Memory & Context settings | dynamic provider options + compression controls + rendered save/reload | Validated |
| Voice Agent configuration | `AgentConfigService` | Voice settings | provider visibility + open identifiers + rendered switching E2E | Validated |
| Advanced Agent configuration | `AgentConfigService` | Advanced settings | curated enum fixtures + whole-record preservation + rendered save/reload | Validated |
| Per-device notification preferences and sound preview | `SettingsService` + `PlatformService` | Notifications settings | preference round-trip + sound fixture + native toast integration | Ported |
| Main and auxiliary model configuration | `ModelService` | Model settings | provider/catalog/assignment fixtures + interaction E2E | Ported |
| Mixture-of-Agents model configuration | `ModelService` | Model settings | preset/slot round-trip + interaction E2E | Ported |
| Local/remote/cloud connection profiles | `ConnectionService` | settings/profiles | soft/hard/live re-home tests | Designed |
| OAuth login/logout and callbacks | `AuthService` | settings/recovery | RFC 8252/state/origin/token tests | Designed |
| Hermes Cloud discovery/sign-in | `AuthService` | settings/profile menu | auth/connectivity separation tests | Designed |
| Secrets at rest | `SecretService` (DPAPI/Credential Manager) | no DOM exposure | round-trip/ACL/redaction tests | Designed |
| Local Workstation snapshot | `RuntimeService` | workstation home | schema/refresh/degraded-state tests | Designed |
| Runtime actions and Task Centre | `TaskService` | tasks/status | durable lifecycle/cancel/pause/retry tests | Designed |
| Models and model downloads | `RuntimeService` | models | integrity/progress/cancel/recovery tests | Designed |
| Inference profiles and switching | `RuntimeService` | models/profiles | provider-neutral contract + rollback tests | Designed |
| Benchmarks | `RuntimeService` | benchmarks | process/progress/report tests | Designed |
| Security scan and reports | `SecurityService` | security/tasks | argument/redaction/report tests | Designed |
| Restore/repair | `UpdateService` + `RuntimeService` | restore/recovery | staged rollback/data-preservation tests | Designed |
| Skills hub and local skills | `AgentService` + `TrustService` | Skills | protocol/trust/install tests | Designed |
| MCP servers/catalog | `AgentService` + `TrustService` | Skills/MCP | scope/enable/test/trust fixtures | Designed |
| Trust Centre and diagnostics | `TrustService` | Trust Centre | policy schema/privilege/redaction tests | Designed |
| Memory and curator | `AgentService` | memory/starmap | RPC/scope/reset/visual tests | Designed |
| Cron and Task scheduling | `AgentService` | cron overlay | RPC/scope/form/lifecycle tests | Designed |
| Messaging integrations | `AgentService` | messaging | RPC/config/state tests | Designed |
| Webhooks | `AgentService` | webhooks | REST scope/validation tests | Designed |
| Artifacts | `AgentService` + `FileService` | artifacts | record/preview/action tests | Designed |
| Agents/subagents | `AgentService` | agents overlay | stream/status/action tests | Designed |
| Starmap | `AgentService` | starmap overlay | graph/state/visual/perf tests | Designed |
| Hermes TUI panel | `RuntimeService`/terminal adapter | TUI page | lifecycle/input/output tests | Designed |
| Embedded dashboard | `DashboardService` | workstation dashboard | auth partition/origin/navigation/token tests | Designed |
| Logs and diagnostics export | `DiagnosticsService` | logs/About | redaction/path/integration tests | Designed |
| About/version/provenance | `PlatformService` | About | manifest consistency tests | Designed |

## Desktop integration, lifecycle and distribution

| Old capability | New Rust owner | New Dioxus surface | Coverage required | Status |
| --- | --- | --- | --- | --- |
| Native notifications and actions | `NotificationService` | settings/session actions | dedupe/action routing tests | Designed |
| Clipboard text/images and save dialogs | `ClipboardService` + `FileService` | chat/context actions | format/size/path tests | Designed |
| Media/camera/microphone permissions | `MediaService` | permission surfaces | origin/capability tests | Designed |
| External browser and link titles | `PlatformService` | links/preview | scheme/SSRF/origin tests | Designed |
| Deep links and protocol registration | `DeepLinkService` | routed surfaces | parser/state/single-instance tests | Designed |
| Session and additional app windows | `WindowService` | shared Dioxus roots | focus/identity/lifecycle tests | Designed |
| Quick Entry global shortcut/window | `ShortcutService` + `WindowService` | quick-entry root | accelerator/persistence/submit tests | Designed |
| Pet overlay and pet generator | `WindowService` | pet roots/overlay | bounds/focus/input/state tests | Designed |
| Wake indicator | `WindowService` | wake root | bounds/state/lifecycle tests | Designed |
| Keep-awake, battery and resume | `PowerService` | settings/status | blocker lifecycle/poll policy tests | Designed |
| Login item/startup | `InstallService` | startup settings | per-user registration tests | Designed |
| Bootstrap/install/uninstall | `InstallService` | onboarding/uninstall | data-preserving Windows lifecycle tests | Designed |
| Desktop update/stage/promote/rollback | `UpdateService` | updates overlay | interrupted update/rollback/process tests | Designed |
| Crash forensics and recovery | `DiagnosticsService` | boot/recovery | corrupt-state/recovery tests | Designed |
| Windows environment/path/CA handling | `PlatformService` | no direct surface | native integration tests | Designed |
| Packaging/install stamp/artifact identity | Rust/Dioxus + minimal Windows packaging | installer/portable | clean package/install/launch tests | Designed |
| SBOM, hashes and release provenance | release tooling | About/update | release-integrity verification | Designed |

## Contributions/plugins

| Old capability | New Rust owner | New Dioxus surface | Coverage required | Status |
| --- | --- | --- | --- | --- |
| Built-in contribution registry | typed UI contribution model | routes/panes/menus/status | composition and isolation tests | Designed |
| Local runtime JavaScript UI plugins | bounded migration adapter or explicit versioned migration | contributed surfaces | sandbox/CSP/no-native-authority tests | Audited |
| Agent integrations and built-in features | Agent protocol/services | native Dioxus surfaces | per-feature protocol tests | Designed |

## Test migration policy

Critical behavior is not considered covered merely because a component renders.
Each old test is classified during the relevant slice as one of: Rust unit,
Rust integration, retained external black-box/E2E, superseded by a stronger
contract test, or obsolete with an explicit reason. Test-only Node/Playwright
may remain temporarily while it proves visual and packaged parity, but it may
not enter production artifacts.
