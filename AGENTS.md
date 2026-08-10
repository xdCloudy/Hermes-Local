# Hermes Local Engineering Rules

This repository is the Windows-native integration and product layer for the
Hermes Local workstation. The product-owned native client lives in
`apps/desktop`. The authoritative Hermes Agent harness checkout lives at
`source/hermes-agent` on branch `hermes-local-harness`; keep its `upstream`
remote pointed at `https://github.com/NousResearch/hermes-agent.git`.

## Non-negotiable boundaries

- Keep all substantial project state beneath the resolved project root.
- Never introduce Docker or WSL as a build or runtime prerequisite.
- Bind local HTTP and WebSocket services to `127.0.0.1` by default.
- Treat renderer, Electron main, Hermes backend, model server, and local files
  as separate trust boundaries.
- Store secrets with per-user DPAPI or Windows Credential Manager. Never place
  plaintext secrets in Git, renderer state, command lines, screenshots, or logs.
- Do not disable Defender, UAC, the Windows firewall, or platform mitigations.
- Do not add broad antivirus exclusions.
- Do not run the stack elevated. Elevation is allowed only for an individually
  identified prerequisite installer that requires it.
- Resolve paths from the project root. Never depend on the caller's current
  working directory.
- Keep virtual environments outside `source/hermes-agent`.
- Use argument arrays for child processes; do not construct shell commands from
  renderer-controlled text.
- Preserve user data during repair, update, rollback, and uninstall unless the
  user explicitly selects data removal.

## Repository strategy

- Root Git tracks the complete product client, its shared Agent client package,
  integration scripts, configuration, documentation, tests, and harness patches.
- `source/hermes-agent` is a separate official upstream clone and is intentionally
  ignored by the root repository.
- Make product UI and Electron changes directly in root `apps/desktop` and
  protocol changes in `packages/hermes-agent-client`.
- Keep Agent runtime changes focused on `hermes-local-harness`; the ordered
  series in `source/hermes-launcher/patches` must never contain Desktop paths.
- Never replace the Hermes agent core with a custom chat backend.
- Record every source, model, and runtime revision in `VERSION.json`.

## Verification

- Follow `apps/desktop/AGENTS.md` for client work and the scoped Agent rules for
  harness work.
- Run Hermes Python tests through `scripts/run_tests.sh`, not direct `pytest`.
- Run Desktop typecheck, lint, unit, Electron, and Playwright tests from the
  root npm workspace.
- Test runtime behavior on native Windows, including paths with spaces.
- A build is not complete until the packaged executable, installer, local model
  endpoint, real Hermes tool call, security report, SBOM, benchmarks, update
  rollback, and data-preserving uninstall have been verified.
