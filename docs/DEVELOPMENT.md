# Development

## Repositories

The root repository owns the Windows product scripts, configuration, patch
series, documentation, benchmark methodology and security evidence.

The nested official checkout is:

```text
D:\Hermes-Local\source\hermes-agent
```

It preserves the `upstream` remote and has:

- upstream base `3be565fbdee3115ab5b9338551768b8e5e655c56`;
- integration branch `hermes-local-integration`;
- integration head `ee683263aaa7f3bca33f785630926350fa119c38`;
- integration tree `67e4ce9137866dbb7febc3cc8b4072ffda816542`.

Do not edit generated `dist`, model, runtime, cache, log, database or backup
files into Git.

## Ordered integration patches

`source\hermes-launcher\patches` contains six mail patches:

1. Windows-native Hermes Launcher workstation;
2. security hardening and dependencies;
3. offline built-in themes;
4. current-user launch at login;
5. native Windows skill preprocessing and WSL rejection;
6. portable Windows compression-persistence test fixtures.

Setup reconstructs from the pinned upstream base with `git am`, verifies the
resulting tree and tolerates a different local commit ID caused only by
committer metadata.

To refresh the series after a reviewed commit, generate the next numbered
patch with `git format-patch`, apply the complete series to a fresh official
base, compare its tree to `HEAD^{tree}`, then update `VERSION.json`.

## Desktop development

From the nested source root:

```powershell
npm.cmd ci --cache D:\Hermes-Local\cache\npm --no-audit
npm.cmd run typecheck --workspace apps/desktop
npm.cmd run lint --workspace apps/desktop
npm.cmd run build --workspace apps/desktop
```

Run the focused local control tests:

```powershell
& 'D:\Hermes-Local\source\hermes-agent\node_modules\.bin\vitest.cmd' `
  run --project electron electron/hermes-local-control.test.ts
```

Run the packaged workstation acceptance against the unpacked binary:

```powershell
$env:HERMES_LOCAL_ACCEPTANCE = '1'
$env:HERMES_LOCAL_ROOT = 'D:\Hermes-Local'
$env:HERMES_LOCAL_LAUNCHER_PATH =
  'D:\Hermes-Local\source\hermes-agent\apps\desktop\release\win-unpacked\Hermes Launcher.exe'
Set-Location 'D:\Hermes-Local\source\hermes-agent\apps\desktop'
& '.\node_modules\.bin\playwright.cmd' test `
  e2e\hermes-local-workstation.spec.ts --workers=1 --reporter=list
```

Use the root `node_modules\.bin` path if workspace bin shims are hoisted.

## Python tests

Use the upstream canonical per-file runner, not direct pytest, for evidence:

```powershell
& 'C:\Program Files\Git\bin\bash.exe' -lc @'
cd /d/Hermes-Local/source/hermes-agent
HERMES_PYTHON=/d/Hermes-Local/runtimes/python/hermes/Scripts/python.exe \
  scripts/run_tests.sh tests/agent/test_skill_commands.py -q
'@
```

The final Windows-critical selection and exact output are retained at
`reports\acceptance\source-regression-final.txt`.

## Build and package

```powershell
& 'D:\Hermes-Local\Build-Hermes-Launcher.ps1' -NonInteractive
& 'D:\Hermes-Local\Package-Hermes-Launcher.ps1' -NonInteractive
```

Packaging builds the renderer/main bundles and uses electron-builder for NSIS
and portable x64 artifacts. Re-run unpacked, portable and installed E2E tests,
Defender and package hashing after every build-stamp or source change.

## Release gates

Before promotion:

1. nested source and root Git worktrees are clean;
2. setup BootstrapOnly reconstruction passes;
3. 13-file Windows-critical source suite is green;
4. TypeScript and Ruff are green, ESLint has no errors;
5. focused Electron/local-control/theme tests pass;
6. real model, auth, loopback, tool schema and Hermes terminal tests pass;
7. unpacked, portable and installed launcher E2E pass;
8. installer/uninstaller preserve model/runtime/user data;
9. benchmark and security reports identify the current source;
10. package hashes are recorded and Defender is clean;
11. docs and known limitations are current.

The upstream full desktop Vitest command currently contains POSIX-only
fixtures and two billing assertions for features disabled in Hermes Local.
Do not suppress them. The exact final result and waiver are in
`ACCEPTANCE_RESULTS.md`.
