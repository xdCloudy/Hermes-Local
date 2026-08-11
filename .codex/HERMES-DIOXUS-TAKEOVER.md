# Hermes Local Dioxus takeover checkpoint

Updated: 2026-08-11

This checkpoint records the continuation of the existing `refactor/dioxus-rust-client` migration. It supplements, rather than replaces, `HERMES-DIOXUS-MIGRATION.md` and `docs/DIOXUS_MIGRATION_MATRIX.md`.

## Product intent

This remains a technology port of the existing Hermes Launcher, not a redesign.

Reference order:

1. Existing React/Electron source under `apps/desktop` is the implementation oracle.
2. Existing packaged Hermes Launcher is the rendered/interaction oracle.
3. Existing QA screenshots are visual regression evidence.
4. Rust/Dioxus must reproduce visual, interaction, state, and behavioral parity before the corresponding legacy implementation is removed.

Hermes Agent remains the agent harness. llama.cpp remains the current/default inference engine behind the OpenAI-compatible inference boundary. The Dioxus UI remains shared/WebAssembly-compatible and receives native authority only through typed application services.

## Current Rust composition

- `apps/desktop`: Dioxus Desktop composition root and Windows bundle identity.
- `crates/hermes-ui`: shared Dioxus UI; no direct filesystem/process/secret/PTY/Windows authority.
- `crates/hermes-core`: typed service traits and state machines.
- `crates/hermes-protocol`: DTOs and protocol types.
- `crates/hermes-agent-client`: bounded actor-style Hermes Agent WebSocket client.
- `crates/hermes-desktop`: native service implementations, Agent REST/WS adapters, Credential Manager, files, Git, PTY and other privileged authority.

Large modules should be split by cohesive ownership as affected slices are ported; do not create a micro-crate per feature.

## Takeover work completed

### Local runtime bootstrap and re-home

Local mode no longer requires the user to supply a Gateway URL manually.

The Desktop composition root installs a typed `ConnectionService` decorator that fills the missing local-runtime rung while leaving remote token/OAuth behavior with the existing native connection service. Local startup:

1. resolves the Hermes Local installation root from explicit root, portable location, executable ancestry, or cwd;
2. invokes the canonical root `Start-Hermes-Local.ps1` with argument-array process spawning;
3. reads the protected local API token through `scripts/launch/Get-Hermes-Local-Token.ps1`;
4. reads the effective loopback Hermes Agent host/port from default + user workstation configuration;
5. constructs the `/api/ws` URL with encoded token query data;
6. connects through the existing typed Gateway client.

This intentionally reuses the established PowerShell supervisor rather than duplicating model/process lifecycle policy in Rust during the migration.

The same decorator handles later live Local re-home through Settings. A per-profile Local selection retains the original source semantics of “use the default gateway”: it removes/saves the profile override and re-resolves the global connection instead of forcing the machine-local gateway.

Startup failure is recoverable in the same window through Retry. The current small startup/failure view is migration plumbing, not final parity for the original first-run/install/boot surfaces.

### Local bootstrap security

- local runtime host must be loopback;
- explicit root and PowerShell overrides must be absolute/valid;
- protected token shape is validated before URL construction;
- URL query encoding is used instead of string concatenation;
- child-process arguments are arrays;
- startup/token helpers have bounded timeouts and kill-on-drop;
- displayed diagnostics redact private root/user paths and long credential-like values and are length-bounded.

### Hermes Agent WebSocket hardening

The typed Rust Gateway client now has explicit resource ceilings rather than relying on transport defaults:

- command queue capacity: 128;
- event broadcast capacity: 512;
- maximum WebSocket frame/message: 16 MiB;
- connect timeout: 15 seconds;
- default request timeout: 2 minutes;
- timed-out requests are explicitly removed from the pending actor map;
- graceful close and unknown-event preservation are covered by tests.

### Native SSH probe parity

The first native SSH slice is now wired behind the typed `ConnectionService::test_config` boundary. It deliberately uses the system OpenSSH client, matching the OG launcher's behavior so existing `~/.ssh/config`, `ProxyJump`, `ssh-agent` and hardware-backed keys continue to work.

Current probe behavior:

- local SSH execution is argv-based with `BatchMode=yes`, bounded connect/command timeouts, bounded captured output and kill-on-drop;
- SSH host, user, key path and explicit remote Hermes path are validated before they can become OpenSSH arguments or remote-shell data;
- host-key changes, authentication failures, timeouts and network failures are classified into the existing typed `SshErrorKind` contract;
- Linux/macOS targets use login-shell Hermes discovery plus the known installer locations;
- Windows targets use a UTF-16LE PowerShell `-EncodedCommand` probe and the same known Hermes runtime layout as the OG source;
- both platform paths verify that remote `hermes serve --help` supports `ssh-session-token-file` and `ssh-owner-nonce` before reporting the host as Desktop-SSH-capable;
- remote diagnostics are control-character stripped and length-bounded before entering UI-facing results;
- the Dioxus UI still receives only `ConnectionTestResult`, never raw process authority.

The small RFC 4648 encoder under `apps/desktop/src/base64.rs` exists only to encode the Windows PowerShell probe. It avoids adding a runtime dependency solely for `-EncodedCommand`; it is not used for cryptography and carries RFC test vectors.

**Important:** SSH connection testing is ported; full remote-process ownership, token adoption, forwarding/tunnel leases, reuse and provably-owned cleanup are not yet complete. Do not treat SSH mode as production-parity until those OG lifecycle invariants are ported and tested.

### Rust validation and release evidence

`.github/workflows/dioxus-rust.yml` validates on Windows:

- Rust/Dioxus architecture guard;
- rustfmt;
- workspace/all-target check with locked dependencies;
- shared `hermes-ui` compile for `wasm32-unknown-unknown`;
- workspace tests;
- Clippy;
- optimized Windows client build with the production `bundle` feature;
- release EXE size recording and short-lived artifact upload.

`scripts/ci/check_dioxus_rust_architecture.py` enforces the shared-UI/native authority boundary and bundle identity.

For branch head `f898a7ab4be6219080f9684aaeeff0c059ba268a`, both the Dioxus Rust validation run (`31493308690`) and the existing Hermes Agent harness/native-client run (`31493308653`) completed successfully. Reopening the draft PR triggered redundant validation runs against the same head; the prior successful runs remain the evidence for that exact commit.

The optimized Windows client is uploaded by CI as `hermes-local-rust-windows-x64`; the run-81 artifact ID is `9101197220`.

### Windows bundle groundwork

The Dioxus application preserves `com.nousresearch.hermes.local` as its bundle identifier and uses the existing Hermes icon. Production `bundle` builds suppress the Windows console subsystem. This is groundwork only: the final installer/portable/update/rollback path is not complete until the actual Dioxus bundle artifacts and lifecycle are validated.

## Known remaining high-priority work

1. Finish Gateway parity: SSH ownership/tunnel lifecycle and Cloud discovery/sign-in, then prove soft/hard/live re-home semantics.
2. Port rich chat/composer behavior: drafts, queueing, attachments, model/tool controls, voice, safe Markdown/code/math/media/diagram rendering.
3. Port Files/editor/previews/watchers onto the existing typed file boundary.
4. Port Git/worktrees/review/ship flows onto the existing typed Git boundary.
5. Port terminal UI and persistent ConPTY lifecycle; the native PTY service already exists, but the placeholder Dioxus route is not parity.
6. Port Local Workstation/runtime/Task Centre/model-download/benchmark/security/repair surfaces.
7. Port Skills/MCP/Trust/Memory/Cron/Messaging/Webhooks/Artifacts/Agents/Starmap.
8. Port TUI/dashboard integration with the existing token/origin isolation contract.
9. Port Windows integration: notifications, clipboard/media permissions, deep links, secondary windows, Quick Entry, pet/wake, power/startup and recovery.
10. Complete Dioxus installer/portable/update/rollback/provenance and clean-clone/package acceptance.
11. Only after parity, switch production build authority and remove Electron/React/Node runtime code.
12. Run final security, accessibility, performance, long-session, package, update/rollback, uninstall and visual/behavior regression acceptance.

## Current next action

Continue the SSH slice by porting the OG ownership/tunnel lifecycle rather than approximating it: protected per-connection token handling, stable ownership identity, spawn nonce, ephemeral remote port readiness, local forwarding, authenticated reuse proof, exact process ownership verification and safe stale cleanup on Linux/macOS and Windows. If that work exposes a reusable native transport abstraction, keep it Desktop-native and typed rather than widening the shared UI boundary.

Do not merge the draft migration PR to `main` until the product owner explicitly authorizes it after final acceptance.
