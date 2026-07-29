# Functional QA defect register

All 14 reproduced first-party defects were fixed. No reproducible first-party
functional defect found during this engagement remains open.

Security auditing and vulnerability assessment were excluded from this QA
engagement.

## QA-001 — Rename created a duplicate profile

- **Subsystem / severity / status:** Desktop profile persistence / Medium /
  Fixed.
- **Impact:** Renaming could leave the old profile behind or overwrite an
  unrelated profile, making the selected configuration ambiguous.
- **Reproduction:** Load an existing profile, change its name, and save.
- **Expected / actual:** Replace the original and migrate selection / save
  treated the new name as an unrelated entry.
- **Root cause / files:** The save contract did not carry the original name;
  `hermes-local-settings.ts`, `preload.ts`, `global.d.ts`, and
  `local-workstation/index.tsx`.
- **Fix / regression / evidence:** Added an original-name-aware typed contract,
  collision and missing-source errors, replacement, and selected-profile
  migration. `hermes-local-settings.test.ts` and the full Electron suite pass.

## QA-002 — Valid Unicode profile names were rejected

- **Subsystem / severity / status:** Profile validation / Medium / Fixed.
- **Impact:** Normal international names could not be saved.
- **Reproduction:** Save a profile with ordinary Unicode letters.
- **Expected / actual:** Accept normal Unicode letters and numbers / ASCII-only
  validation rejected them.
- **Root cause / files:** ASCII-centric regular expression in
  `hermes-local-settings.ts`.
- **Fix / regression / evidence:** Switched to Unicode-aware validation with
  bounded length and retained ordinary invalid-input checks.
  `hermes-local-settings.test.ts` passes.

## QA-003 — Overlapping service actions could race

- **Subsystem / severity / status:** Electron process control / High / Fixed.
- **Impact:** Rapid Start, Stop, Restart, Repair, or Update actions could run
  concurrently and publish contradictory state.
- **Reproduction:** Begin one long-running action, then invoke a different
  action before completion.
- **Expected / actual:** One global action at a time / task IDs only deduplicated
  identical calls.
- **Root cause / files:** No global action ownership in
  `hermes-local-control.ts`; peer buttons were not disabled in
  `local-workstation/index.tsx`.
- **Fix / regression / evidence:** Added an exclusive action lock, idempotent
  same-action reuse, clear rejection of a different active action, and
  renderer-wide disabled state. Focused and full Electron suites pass.

## QA-004 — Completed action history grew without a bound

- **Subsystem / severity / status:** Electron task registry / Low / Fixed.
- **Impact:** A long-running launcher could retain an ever-growing task map.
- **Reproduction:** Complete many native actions in one launcher process.
- **Expected / actual:** Retain a useful bounded history / retain every entry.
- **Root cause / files:** No eviction policy in `hermes-local-control.ts`.
- **Fix / regression / evidence:** Retain at most 50 completed tasks while
  preserving active tasks. The control regression suite passes.

## QA-005 — Service snapshot used duplicate/misleading health probes

- **Subsystem / severity / status:** Service status / Medium / Fixed.
- **Impact:** Dashboard health could be inferred from the backend API probe,
  and concurrent refreshes duplicated process/network work.
- **Reproduction:** Refresh snapshots concurrently while a service endpoint has
  a different route state.
- **Expected / actual:** Probe model `/health`, Hermes `/api/health`, and
  dashboard `/` once per refresh / reuse or duplicate the wrong checks.
- **Root cause / files:** Shared probe URL and no single-flight snapshot in
  `hermes-local-control.ts`.
- **Fix / regression / evidence:** Added distinct URLs and a single-flight
  promise. Focused control tests and operational diagnostics pass.

## QA-006 — Polling errors and stale responses damaged UI state

- **Subsystem / severity / status:** React workstation renderer / Medium /
  Fixed.
- **Impact:** Failed saves/log reads could be silent, dirty edits could be
  cleared, and late polling responses could overwrite newer state.
- **Reproduction:** Fail an IPC call or navigate/unmount during a snapshot or
  log request.
- **Expected / actual:** Visible error, retained edits, stale-response guard /
  silent or stale state update.
- **Root cause / files:** Distributed polling and error handling in
  `local-workstation/index.tsx`.
- **Fix / regression / evidence:** Centralized visible errors, preserved dirty
  state, added request generations/unmount guards and a manual refresh trigger.
  UI and Electron suites pass.

## QA-007 — Python subprocess test assumed a POSIX platform

- **Subsystem / severity / status:** Windows Python test infrastructure / Low /
  Fixed.
- **Impact:** A valid Windows implementation failed its regression suite.
- **Reproduction:** Run `test_windows_subprocess_no_window_flags.py` on Windows.
- **Expected / actual:** Use the real Windows platform identity / mocked
  `sys.platform` did not satisfy `platform.win32_ver`.
- **Root cause / files:** Incomplete platform fixture in
  `tests/test_windows_subprocess_no_window_flags.py`.
- **Fix / regression / evidence:** Mocked the appropriate platform query. The
  selected Python run passes 136 tests with two skips.

## QA-008 — Electron portability fixtures failed on Windows paths

- **Subsystem / severity / status:** Electron test infrastructure / Low /
  Fixed.
- **Impact:** The full Electron release suite reported 19 platform-only
  failures and import errors.
- **Reproduction:** Run the full Electron project on Windows.
- **Expected / actual:** Test Windows and POSIX path forms portably / fixtures
  assumed `/`, POSIX sockets, Bash quoting, or SSH ControlMaster behavior.
- **Root cause / files:** `desktop-installation.test.ts`,
  `ssh-config.test.ts`, `ssh-connection.test.ts`,
  `update-relaunch.test.ts`, `update-relaunch.ts`, and
  `windows-hermes-path.ts`.
- **Fix / regression / evidence:** Added platform-aware path helpers, fixtures,
  SSH mux expectations, and relaunch arguments. Full Electron result is 835
  passed, three skipped.

## QA-009 — Native-dependency staging script broke under Vitest

- **Subsystem / severity / status:** Desktop build/test infrastructure / Low /
  Fixed.
- **Impact:** A script import failed when the shebang was transformed.
- **Reproduction:** Import the staging script through the Windows test runner.
- **Expected / actual:** Portable module execution / transformed shebang caused
  a parse/import error.
- **Root cause / files:** Shebang-dependent script shape in
  `scripts/stage-native-deps.mjs`.
- **Fix / regression / evidence:** Converted the entry point to a portable
  module form. Full Electron tests and the final build pass.

## QA-010 — USD amounts depended on the machine locale

- **Subsystem / severity / status:** Billing presentation / Medium / Fixed.
- **Impact:** Tests and UI copy rendered different separators on non-US
  machines.
- **Reproduction:** Format billing amounts under a non-US Windows locale.
- **Expected / actual:** Stable USD formatting / ambient locale determined
  output.
- **Root cause / files:** `billing-amounts.ts`.
- **Fix / regression / evidence:** Pinned `Intl.NumberFormat` to `en-US` and
  added locale regression tests. Focused tests and 2,555-test UI suite pass.

## QA-011 — Malformed CSS comment produced a build warning

- **Subsystem / severity / status:** Renderer styles / Low / Fixed.
- **Impact:** Build output contained a misleading CSS parse warning.
- **Reproduction:** Build the desktop renderer.
- **Expected / actual:** Clean valid comment / malformed delimiter.
- **Root cause / files:** `apps/desktop/src/styles.css`.
- **Fix / regression / evidence:** Corrected the comment. The warning is absent
  from the final build.

## QA-012 — Tabler barrel imports expanded build work

- **Subsystem / severity / status:** Renderer build performance / Low / Fixed.
- **Impact:** Vite processed a 6,149-export barrel and emitted a warning.
- **Reproduction:** Build the renderer with the local workstation files.
- **Expected / actual:** Import only used icons / import the top-level barrel.
- **Root cause / files:** `local-workstation/index.tsx`, `tui-panel.tsx`, and
  `src/lib/icons.ts`.
- **Fix / regression / evidence:** Switched to direct ESM icon modules. The
  warning disappeared, modules fell from 4,539 to 4,538, and median build time
  improved from 6.137 s to 5.693 s (7.2%; one-run baseline caveat).

## QA-013 — Skills suite could time out only in the full run

- **Subsystem / severity / status:** UI test infrastructure / Low / Fixed.
- **Impact:** Full-suite scheduling could time out a test that passed alone.
- **Reproduction:** Run the complete UI project under the constrained Windows
  worker pool.
- **Expected / actual:** Deterministic module availability / dynamic import
  competed with the suite timeout.
- **Root cause / files:** `src/app/skills/index.test.tsx`.
- **Fix / regression / evidence:** Use a static import for the tested module.
  All 299 UI files pass.

## QA-014 — Log source selector lacked an accessible name

- **Subsystem / severity / status:** Workstation accessibility / Low / Fixed.
- **Impact:** Keyboard/screen-reader users could not reliably identify the
  log-source selector.
- **Reproduction:** Inspect the Logs form accessibility tree.
- **Expected / actual:** Stable descriptive name / unnamed selector.
- **Root cause / files:** `local-workstation/index.tsx`.
- **Fix / regression / evidence:** Added `aria-label="Log source"` and asserted
  it in packaged functional E2E.
