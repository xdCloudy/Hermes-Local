# Refactor summary

The refactor stayed inside the existing Hermes Local integration architecture:
authored changes live in the nested integration branch, the root repository
stores ordered mail patches, and `VERSION.json` pins the reconstructed tree.

## Workstation state and contracts

- Extended the typed profile-save contract with the original profile name.
- Centralized profile replacement, collision handling, selection migration,
  and Unicode-aware validation.
- Made UI error ownership explicit for profile, settings, log, and service
  actions.
- Preserved form dirty state after failed persistence.
- Added request generations and mounted-state guards to polling.

## Process and health ownership

- Added one global native-action owner with idempotent same-action reuse.
- Bounded completed task history at 50.
- Split model, Hermes API, and dashboard health probes by their real endpoints.
- Shared concurrent snapshot work through one in-flight promise.

## Windows portability and tests

- Consolidated Windows/POSIX path expectations in installation, SSH, and
  relaunch tests.
- Removed shebang-dependent native-dependency staging behavior.
- Made the Windows subprocess fixture use the platform API the implementation
  actually reads.
- Added a single packaged functional E2E that owns route, control, error,
  accessibility, source-stamp, narrow-layout, TUI, and login-item restoration
  checks.

## Build clarity

- Replaced Tabler barrel imports with direct icon modules.
- Corrected a malformed stylesheet comment.
- Replaced a timing-sensitive dynamic test import with a static import.
- Added a reusable root QA orchestrator plus AST inventories, PowerShell parser
  checks, recovery fixtures, machine-readable results, and evidence
  finalization.

No generated bundles, user settings, models, runtimes, session databases,
caches, or private logs are committed.

Security auditing and vulnerability assessment were excluded from this QA
engagement.
