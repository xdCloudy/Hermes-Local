# Windows lifecycle validation

[← Documentation home](README.md) · [Development](DEVELOPMENT.md) ·
[Stable promotion](STABLE_PROMOTION.md) · [Project home](../README.md)

Hermes Local's Windows lifecycle gate is a versioned, fail-closed test system
for install, upgrade, repair, rollback and uninstall behavior. Its canonical
inventory is `config/validation/windows-lifecycle-matrix.json`; scenario
evidence is created and checked by `scripts/ci/windows_lifecycle.py`.

## Coverage model

The matrix currently contains 49 scenarios:

| Area | Scenarios |
|---|---:|
| Clean install | 11 |
| Upgrade | 8 |
| Repair | 7 |
| Rollback | 7 |
| Uninstall and migration | 6 |
| Adverse conditions | 8 |
| Physical hardware | 2 |

Each scenario declares its runner class, automation mode, criticality,
preservation requirement and whether it is mandatory for Stable. The current
matrix has 35 Stable-required scenarios and 38 preservation checks. A scenario
that cannot be exercised records `skipped` with a reason; a skip is never
converted into a pass.

## Preservation fixture

Before each scenario, the runner creates deterministic user-owned content for
models, profiles, settings, sessions, memory, skills, cron jobs, projects,
backups and model registrations. The manifest records the SHA-256 and size of
every relative file. After the operation, the runner compares a fresh snapshot
byte-for-byte and reports added, removed and changed paths.

All automated lifecycle work runs below an explicit disposable sandbox. The
runner kills only the process tree it started and retains the sandbox and log
for diagnosis. It does not clean a real Hermes Local home or a maintainer's
installation.

## Validation lanes

- Pull requests validate the matrix, Python policy and PowerShell syntax without
  installing the product.
- A manual disposable lane reconstructs the pinned source, builds NSIS and
  portable packages, records their hashes, and exercises hosted Windows
  scenarios.
- Trusted physical lanes require self-hosted Windows x64 runners labelled
  `hermes-lifecycle, cpu-only` and `hermes-lifecycle, nvidia` respectively.
  They install and fake-boot the exact package, then run the full provisioned
  runtime smoke through `Test-Hermes-Local.ps1`. Runner administrators must
  provide the candidate's runtime and model prerequisites in the checked-out
  workspace before that smoke. NVIDIA evidence must include the adapter name
  and driver version.

Run **Actions → Windows lifecycle validation → Run workflow** from the default
branch. An upgrade from the previous Stable release requires both an HTTPS
`previous_installer_url` and its 64-character
`previous_installer_sha256`; the workflow rejects a digest mismatch before it
runs the upgrade.

Use `stable_evaluation: false` for candidate diagnostics. Use all of the
following for release evidence:

- `run_disposable: true`;
- `run_physical: true`;
- `stable_evaluation: true`;
- an immutable previous-Stable installer URL and SHA-256.

The aggregate artifact is named `windows-lifecycle-aggregate`. Scenario and
aggregate evidence is retained for 90 days and includes the candidate commit,
matrix digest, runner class, timestamps, checks, failures, skip reasons,
fixture comparison and log references.

## Blocking policy

Stable evaluation requires every `stableRequired` scenario to have one valid
`passed` record for the exact candidate. Missing, skipped or failed mandatory
evidence blocks the aggregate. Critical failures also block candidate
evaluation. The Stable promotion gate independently verifies the matrix digest,
candidate commit, source workflow run and successful physical CPU and NVIDIA
records.

The framework intentionally exposes current product and infrastructure gaps.
Scenarios that depend on the unfinished guided installer and transactional
Update Centre flows remain explicit skips until those features implement their
test hooks. Stable promotion is therefore unavailable while those scenarios are
skipped or either trusted physical runner class is not provisioned. Historical
QA reports do not substitute for evidence from the current candidate.

## Local tooling

```powershell
python .\scripts\ci\windows_lifecycle.py validate
python -m unittest .\tests\test_windows_lifecycle.py -v
```

The CLI also exposes `create-fixture`, `snapshot-fixture`, `record` and
`aggregate`. Run `python .\scripts\ci\windows_lifecycle.py --help` for the
exact arguments. A Stable aggregate requires `aggregate --stable`; do not edit
evidence JSON by hand.
