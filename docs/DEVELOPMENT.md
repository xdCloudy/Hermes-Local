# Development

[← Documentation home](README.md) · [Project home](../README.md) ·
[Contributing](../CONTRIBUTING.md)

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
   connection;
9. workstation action, profile, polling, portability and build fixes;
10. packaged functional workstation coverage;
11. manifest-derived packaged source-pin verification;
12. launch-at-login restoration coverage;
13. authoritative gateway state reporting;
14. authoritative gateway status in global desktop chrome;
15. projectless packaged-chat startup and reconnect behavior;
16. per-chat project selection and explicit project removal;
17. theme-consistent, accessible dropdowns across every launcher surface.

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
  e2e\hermes-local-functional.spec.ts `
  --workers=1 --reporter=list
```

Use the root `node_modules\.bin` path if workspace bin shims are hoisted.

Run the practical complete functional gate from the root:

```powershell
& '.\scripts\qa\Invoke-FullFunctionalQA.ps1' `
  -Scope Full `
  -OutputDirectory '.\temp\qa-runs\final-full'
```

`Fast` omits full UI/Electron/build/recovery/operational steps. `Package` adds
the official packaging command. Every run captures per-step stdout/stderr and
a machine-readable summary; full runs also produce JUnit Python output and
finalize the UI, script, and workflow inventories. A related component suite
is not counted as proof that every control was individually actuated.

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
native portable smoke, and package hashing after every build-stamp or source
change. Playwright should target the unpacked or installed executable; the
portable executable is a self-extractor and receives a recorded native smoke
test.

## Release gates

Before promotion:

1. nested source and root Git worktrees are clean;
2. setup BootstrapOnly reconstruction passes;
3. the documented Windows-critical Python selection is green;
4. TypeScript and Ruff are green, ESLint has no errors;
5. focused Electron/local-control/theme tests pass;
6. real model, auth, loopback, tool schema and Hermes terminal tests pass;
7. unpacked, portable and installed launcher E2E pass;
8. installer/uninstaller preserve model/runtime/user data;
9. benchmark and other separately scoped release evidence identify the current
   source;
10. package hashes and the clean source stamp are recorded;
11. docs and known limitations are current.

The current Windows full desktop and UI Vitest projects are green. Do not
suppress future platform failures: classify and fix a first-party portability
defect or document a proven upstream-only limitation with exact output.

Security auditing and vulnerability assessment were excluded from the
2026-07-29 functional QA engagement.
