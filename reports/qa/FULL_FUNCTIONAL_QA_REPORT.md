# Hermes Local full functional QA report

## Executive summary

Hermes Local was inventoried, baselined, exercised, repaired, refactored,
rebuilt, packaged, and tested on Windows 11. Fourteen reproducible first-party
functional, reliability, accessibility, portability, build, or test defects
were fixed; no reproduced first-party functional defect remains open. The
corrected nested integration reconstructs exactly from the pinned upstream
commit and 12 ordered patches. The final unpacked, portable, and installed
launcher paths passed their recorded acceptance checks, and protected user
settings and semantic session state were restored to baseline.

Final result: **PASS with explicit functional coverage limitations**.

Security auditing and vulnerability assessment were excluded from this QA engagement.

## Tested source and environment

| Item | Value |
|---|---|
| Root branch | `codex/full-qa-refactor` |
| Root base | `b47e87e710c4f7509da5db54ad25b435ecc679e5` |
| Root PR base | `e61a9e926fddeacbfe5733d3ad7de961c53ac3e0` |
| Root implementation commits | `324872e`, `bfc09fe` |
| Nested initial commit | `f85f5a3c87bb43b5e19e5e3031b7da6b361806b2` |
| Pinned upstream | `3be565fbdee3115ab5b9338551768b8e5e655c56` |
| Final nested integration | `0c1dbce2930bdb8b9c09ac768e71f20a3b87f814` |
| Final integration tree | `5df883962b78c8b29b98bcbfa1ebc5c939a3f6f4` |
| OS | Windows 11 Pro x64, build 26200 |
| PowerShell | 7.6.4 |
| Node / npm | 24.18.0 / 11.16.0 |
| Managed Python / SQLite | 3.13.14 / 3.53.1 |
| Git | 2.55.0 |
| Launcher | 0.18.1 |

Baseline runtime was healthy on the Daily profile, Laguna XS 2.1 Q4_K_M,
automatic acceleration, model port 8011, and Hermes/dashboard port 9119.
The original 12-step full orchestrator ran against nested commit `ba953455…`
and the QA working tree on baseline root `b47e87e…`. Final commit `0c1dbce…` changes only the
packaged E2E login-item restoration assertion; that exact commit passed the
final packaged E2E and the later 8-step Fast gates. The PR was then rebased
onto `e61a9e9…`; the current branch passed another 8-step Fast gate. These
distinctions are retained in `qa-results.json`.

## Scope and architecture inventory

The inventory followed the root PowerShell/CMD product layer, shared modules,
configuration/defaults/manifests, nested React renderer, Electron main and
preload boundary, IPC contracts, supervisor and Job Object lifecycle, ConPTY
TUI, dashboard, Hermes Python backend, persistence, backup/restore,
update/rollback, repair, build/package scripts, installer, tests, and ordered
integration patch architecture.

Major control flow:

```text
React control
→ typed handler/state
→ preload method
→ Electron IPC handler
→ PowerShell/native operation
→ refreshed snapshot/log/task state
→ visible success or ordinary error feedback
```

The final design adds explicit action ownership, bounded task retention,
single-flight snapshot resolution, distinct endpoint health checks, typed
profile rename semantics, and stale/unmounted polling guards.

## Inventory and coverage

### UI

The TypeScript AST inventory covers 322 production TSX files:

| Disposition | Count |
|---|---:|
| Interactive controls | 1,184 |
| Loading/error/empty/dialog states | 133 |
| Total entries | 1,317 |
| Control label asserted in automation | 78 |
| Owning component suite passed, not control-specific | 1,182 |
| Source-inventoried, not individually exercised | 57 |
| Automated or related-suite disposition | 95.67% |

The 57 source-only entries comprise 48 controls and nine states. Related-suite
coverage is deliberately not described as individual actuation. The packaged
functional E2E directly covers all local workstation routes, primary actions,
logs, TUI, renderer errors, keyboard focus, narrow layout, package source pin,
and launch-at-login restoration.

### Executable scripts

| Disposition | Count |
|---|---:|
| Total executable entries | 235 |
| Functional scope | 228 |
| Security-excluded | 7 |
| Direct execution passed | 21 |
| PowerShell static parse only | 14 |
| Inventoried, not independently executed | 193 |

All entries have a final disposition. The inventory does not treat a filename
reference or syntax pass as an executed success/failure-path test.

### Workflows

| Disposition | Count |
|---|---:|
| Total | 31 |
| Functional scope | 30 |
| Security-excluded | 1 |
| End-to-end pass | 11 |
| Focused automated pass | 6 |
| Controlled fixture pass | 3 |
| Partial evidence | 10 |

The partial group includes model mutation, sessions/projects/chat depth,
benchmark, repair, and transactional Hermes Agent Apply/Rollback workflows
where Check/surface/related evidence passed but the state-mutating operation
was not fully run against the protected installation.

## Functional execution

- The dashboard was exercised across 17 routes: chat, sessions, files, models,
  logs, cron, skills, plugins, MCP, channels, webhooks, profiles, config,
  system, docs, kanban, and achievements. No console warning/error was
  observed, and the 900×700 check had no horizontal overflow.
- Home, Services, Dashboard, Tasks, Models, Profiles, Tools, Memory, Sessions,
  Projects, Benchmarks, Logs, About, TUI, and narrow TUI package screens were
  captured.
- Native stop, start, and restart completed successfully in 30.725 s,
  45.125 s, and 72.079 s.
- A controlled model-process exit recovered automatically in 54.208 s; the PID
  changed and the recovery counter moved from zero to one.
- `Update-Hermes-Local.ps1 -Mode Check` completed successfully.
- The rebased transactional `Update-Hermes-Agent.ps1 -Mode Check` completed
  successfully, reporting candidate `c3ffe273…` without mutating the active
  installation. Apply and Rollback remain partial rather than falsely passed.
- The integration was reconstructed from pinned upstream plus patches
  0001–0012 in a clean worktree and the resulting tree matched
  `5df883962…`.
- Backup/restore fixtures passed custom naming, checksum sidecar, byte-exact
  restore, missing backup, ordinary invalid name, checksum mismatch, and
  missing manifest cases.

## Fresh regression results

The final `Full` orchestrator passed 12/12 required steps in 170,179 ms:

| Suite | Result |
|---|---|
| TypeScript | Pass |
| ESLint | 0 errors, 51 warnings |
| Focused local Electron | Pass |
| Billing regressions | Pass |
| Windows-critical Python | 136 passed, two skipped |
| Recovery fixtures | 7/7 passed |
| Full Electron | 74 files passed, one skipped; 835 passed, three skipped |
| Full UI | 299 files passed; 2,555 passed, one skipped |
| PowerShell parser | 25 files, zero failures; one security file excluded |
| Desktop build | Pass |
| Operational diagnostics | Pass |

The orchestrator now includes inventory finalization as a required thirteenth
step for subsequent full runs. After rebasing onto current `main`, its `Fast`
scope passed 8/8 in 48,522 ms, including that finalization step.

## Defects and regression coverage

Fourteen defects were found and fixed:

1. true profile rename and collision semantics;
2. Unicode profile names;
3. global exclusive native actions;
4. bounded task history;
5. distinct/single-flight health snapshots;
6. stale polling and visible IPC errors;
7. Windows Python platform fixture;
8. Windows Electron path/SSH/relaunch fixtures;
9. portable native-dependency staging import;
10. locale-stable USD formatting;
11. malformed CSS comment;
12. Tabler barrel import build overhead;
13. timing-sensitive Skills test import;
14. Logs selector accessible name.

Regression coverage was added to Electron control/settings tests, billing UI
tests, the Windows Python test, and a 256-line packaged functional E2E. Full
reproduction, root cause, affected files, and verification evidence are in
`defect-register.md`.

## Refactoring and optimisation

The work preserved existing responsibilities rather than introducing a new
framework. It clarified typed IPC boundaries, profile persistence ownership,
action coordination, health endpoints, polling lifecycle, and Windows path
handling. The QA harness adds deterministic inventory, final disposition,
PowerShell syntax, recovery fixture, JSON/Markdown, JUnit, screenshot, and
process-log evidence.

Direct icon imports reduced median build time from 6.137 s to 5.693 s
(7.2%, with a single-run baseline caveat), reduced transformed modules by one,
and removed the 6,149-export Tabler warning. The final build completed in
5.532 s. The malformed CSS warning was also removed. One intentional
large-chunk warning remains.

## Build, package, install, and uninstall

The final package stamp records integration commit `0c1dbce…` with
`dirty: false`.

| Artifact | Bytes | SHA-256 |
|---|---:|---|
| Portable / `dist\Hermes Launcher.exe` | 126,983,589 | `eb6227c867d28de3a062be095ddfbdfcaadcca7fd89b48c6d2d9754769ae3ab5` |
| NSIS setup | 127,245,805 | `f8d24f2b1d32b0f819808a5c6aa64d5c8a56702606f8701c483f6becebbf8ba6` |
| Setup blockmap | 125,820 | `05c443ba92e2becc9123938c37161e52961ca7b094b332da733e69cb4231911a` |

Unpacked packaged E2E passed in 12.0 s. Installed E2E passed from a path
containing spaces in 11.9 s. Silent install and uninstall returned zero;
installed files and shortcuts were removed. The login item was toggled and
restored. The direct portable executable was launched and its healthy Home,
connected TUI, and maximize/restore path were verified. The user stopped a
later final re-launch, so no further native UI takeover was attempted.

## Protected data and recovery

A supported named safety backup was created before intrusive work:

`Hermes-Local-20260729T163813Z-before-full-functional-qa-20260729.zip`,
14,402,189 bytes, SHA-256
`747f156e643cc872f7ec70864b62fe3a5c9562ffdffc6450814c6e9fbed428ef`.

The manifest and all 739 entries were verified. User settings remained
byte-identical (`7129eff2…`). The two pre-existing model-manifest worktree
items and nested lockfile remained uncommitted; the lockfile hash stayed
`d63bf263…`. Three QA-created CLI sessions were deleted through the supported
CLI and semantic session/message/model-usage counts returned to 5/1,256/16.
Three route-created achievements files were identified, verified as QA-only,
and removed. The final managed stack is healthy on the original Daily profile.

## Exact primary commands

```powershell
& '.\Backup-Hermes-Local.ps1' -Name before-full-functional-qa-20260729 -NonInteractive
& '.\scripts\qa\Invoke-FullFunctionalQA.ps1' -Scope Full -OutputDirectory '.\temp\qa-runs\final-full'
& '.\scripts\qa\Test-RecoveryFixtures.ps1' -OutputPath '.\temp\qa-runs\final-full\recovery-fixtures.json'
& '.\Test-Hermes-Local.ps1' -NonInteractive
& '.\Stop-Hermes-Local.ps1' -NonInteractive
& '.\Start-Hermes-Local.ps1' -NonInteractive
& '.\Restart-Hermes-Local.ps1' -NonInteractive
& '.\Update-Hermes-Local.ps1' -Mode Check -Component All -NonInteractive
& '.\Update-Hermes-Agent.ps1' -Mode Check -NonInteractive
& '.\Package-Hermes-Launcher.ps1' -NonInteractive
```

```powershell
Set-Location '.\source\hermes-agent\apps\desktop'
npm.cmd run typecheck
npm.cmd run lint
npm.cmd run test:desktop:platforms
npm.cmd run test:ui
npm.cmd run build
& '.\node_modules\.bin\playwright.cmd' test e2e\hermes-local-functional.spec.ts --workers=1 --reporter=list
```

The complete executable paths, arguments, working directories, timestamps,
durations, exit codes, and log paths are in
`temp/qa-runs/final-full/qa-run.json`.

## Remaining functional limitations

- 48 controls and nine UI states were source-inventoried but not individually
  exercised.
- 193 executable entries were not independently executed; 14 more PowerShell
  entries have parser-only evidence.
- Ten workflows have partial evidence rather than a full state-mutating pass.
- A physical reboot, Windows logoff/shutdown, real insufficient-disk event,
  interrupted live update, and full display-scaling matrix were not performed.
- Idle CPU/memory and long-duration leak profiles were not controlled well
  enough to report a defensible before/after number.
- The installer left an empty directory after uninstall; it was verified empty
  and removed.

These are explicit evidence gaps, not waivers for a known reproducible defect.

## Final release gate

**PASS with explicit functional coverage limitations.** All reproduced
first-party defects were fixed, the corrected package was rebuilt and passed
the recorded package checks, the ordered patch architecture reconstructs the
expected tree, and protected user state was restored. The detailed inventories
must be consulted before interpreting this as universal control-by-control or
script-by-script execution coverage.
