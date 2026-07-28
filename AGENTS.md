# Hermes Local Engineering Rules

This repository is the Windows-native integration and product layer for the
Hermes Local workstation. The authoritative upstream Hermes checkout lives at
`source/hermes-agent` on branch `hermes-local-integration`; keep its `upstream`
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

- Root Git tracks integration scripts, configuration schemas, documentation,
  benchmarks, security artifacts, and the launcher patch series.
- `source/hermes-agent` is a separate official upstream clone and is intentionally
  ignored by the root repository.
- Prefer extending `apps/desktop` and `apps/shared` in the upstream checkout.
  Never replace the Hermes agent core with a custom chat backend.
- Keep upstream changes as focused commits on `hermes-local-integration` and
  export a documented patch series under `security/patches` or
  `source/hermes-launcher/patches` as appropriate.
- Record every source, model, and runtime revision in `VERSION.json`.

## Verification

- Follow `source/hermes-agent/AGENTS.md` and scoped `AGENTS.md` files.
- Run Hermes Python tests through `scripts/run_tests.sh`, not direct `pytest`.
- Run Desktop typecheck, lint, unit, Electron, and Playwright tests from the
  official workspace.
- Test runtime behavior on native Windows, including paths with spaces.
- A build is not complete until the packaged executable, installer, local model
  endpoint, real Hermes tool call, security report, SBOM, benchmarks, update
  rollback, and data-preserving uninstall have been verified.
