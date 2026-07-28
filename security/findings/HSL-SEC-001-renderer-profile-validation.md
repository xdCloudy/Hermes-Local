# HSL-SEC-001 — renderer profile input reached process creation

Severity: Medium  
Status: Fixed and regression-tested  
Trust boundary: Electron renderer → main process → backend process

## Source to sink

The renderer-controlled argument of `hermes:connection` flowed through `ensureBackend(profile)` into `spawnPoolBackend`, where the profile became `--profile <value>` in the backend argument list. The normal installed Python backend uses `shell: false`, but an existing Windows Hermes command shim may require shell execution. The main process did not independently enforce the CLI profile grammar on this IPC path.

## Impact and reachability

This required a compromised or hostile renderer and a command-script fallback backend, so it was not directly exploitable through the normal packaged workstation. It nevertheless violated the renderer/main boundary and the requirement that renderer strings cannot influence shell execution.

## Fix

- Added `profile-name.ts` as the single main-process validator.
- Accepted grammar is identical to the CLI: `^[a-z0-9][a-z0-9_-]{0,63}$`.
- Non-string, path-traversal, uppercase, and shell-metacharacter values fail before backend resolution.
- Process creation continues to use argument arrays and the installed workstation backend remains `shell: false`.

## Verification

`profile-name.test.ts` covers valid names, fallback behavior, non-string input, path traversal, and common shell syntax. Desktop typecheck, ESLint, and Electron unit tests pass.
