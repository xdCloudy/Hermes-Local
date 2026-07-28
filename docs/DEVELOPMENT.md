# Development

## Repositories

The root repository owns the Windows product scripts, configuration, patch
series, documentation, benchmark methodology and security evidence.

The nested official checkout is:

```text
<project-root>\source\hermes-agent
```

It preserves the `upstream` remote and uses the
`hermes-local-integration` branch. The currently pinned upstream base,
integration commit and tree are recorded in `VERSION.json`.

Do not edit generated `dist`, model, runtime, cache, log, database or backup
files into Git.

## Ordered integration patches

`source\hermes-launcher\patches` contains the ordered mail patch series:

1. Windows-native Hermes Launcher workstation;
2. security hardening and dependencies;
3. offline built-in themes;
4. current-user launch at login;
5. native Windows skill preprocessing and WSL rejection;
6. portable Windows compression-persistence test fixtures;
7. portable workstation configuration and model/profile controls;
8. automatic cold-start of the supervised workstation before desktop
   connection.

Setup reconstructs from the pinned upstream base with `git am`, verifies the
resulting tree and tolerates a different local commit ID caused only by
committer metadata.

To refresh the series after a reviewed commit, generate the next numbered
patch with `git format-patch`, apply the complete series to a fresh official
base, compare its tree to `HEAD^{tree}`, then update `VERSION.json`.

## Desktop development

From the nested source root:

```powershell
Set-Location 'C:\path\to\Hermes-Local'
$HermesRoot = (Get-Location).Path
Set-Location (Join-Path $HermesRoot 'source\hermes-agent')
npm.cmd ci --cache (Join-Path $HermesRoot 'cache\npm') --no-audit
npm.cmd run typecheck --workspace apps/desktop
npm.cmd run lint --workspace apps/desktop
npm.cmd run build --workspace apps/desktop
```

Run the focused local control tests:

```powershell
& (Join-Path $HermesRoot 'source\hermes-agent\node_modules\.bin\vitest.cmd') `
  run --project electron electron/hermes-local-control.test.ts
```

Run the packaged workstation acceptance against the unpacked binary:

```powershell
$env:HERMES_LOCAL_ACCEPTANCE = '1'
$env:HERMES_LOCAL_ROOT = $HermesRoot
$env:HERMES_LOCAL_LAUNCHER_PATH = Join-Path $HermesRoot 'dist\Hermes Launcher.exe'
Set-Location (Join-Path $HermesRoot 'source\hermes-agent\apps\desktop')
& '.\node_modules\.bin\playwright.cmd' test `
  e2e\hermes-local-workstation.spec.ts e2e\hermes-local-configuration.spec.ts `
  --workers=1 --reporter=list
```

Use the root `node_modules\.bin` path if workspace bin shims are hoisted.

## Python tests

Use the upstream canonical per-file runner, not direct pytest, for evidence:

```powershell
& 'C:\Program Files\Git\bin\bash.exe' -lc @'
cd /path/to/Hermes-Local/source/hermes-agent
HERMES_PYTHON=/path/to/Hermes-Local/runtimes/python/hermes/Scripts/python.exe \
  scripts/run_tests.sh tests/agent/test_skill_commands.py -q
'@
```

The final Windows-critical selection and exact output are retained at
`reports\acceptance\source-regression-final.txt`.

## Build and package

```powershell
& (Join-Path $HermesRoot 'Build-Hermes-Launcher.ps1') -NonInteractive
& (Join-Path $HermesRoot 'Package-Hermes-Launcher.ps1') -NonInteractive
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
