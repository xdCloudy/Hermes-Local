# Dioxus Human Validation Protocol

This document defines the **final manual acceptance gate** for the Hermes Local
Electron/React → Rust/Dioxus migration.

It is intentionally separate from automated validation. CI, unit tests,
integration tests, coding agents and AI reviewers are necessary evidence, but
they cannot decide whether the port actually feels, looks and behaves like the
product it replaces.

## Authority

The final `A6 Human-validated` state in
[`DIOXUS_MIGRATION_MATRIX.md`](DIOXUS_MIGRATION_MATRIX.md) may be assigned only
by a human product-owner/reviewer.

**AI/automation must never mark a row A6 or write a PASS result on behalf of the
human reviewer.**

Automated work may move a row as far as `A5 Human-ready`, prepare evidence,
produce a review build and identify the exact OG references to compare.

A human PASS applies to the exact build SHA recorded in the validation entry.
A later material change to that capability requires re-review.

## What the human reviewer is validating

For each capability, the reviewer is answering a stronger question than
"does it run?":

> Does this exact Rust/Dioxus build preserve the Hermes Local capability's
> visual design, interactions, state model, behavior, failure handling and
> safety well enough to replace the OG implementation?

The OG Hermes Local Electron/React client on the migration base/current oracle
is the comparison source. The migration is a technology port, not a redesign.

## Review build requirements

Do not begin final human review of a capability until its matrix row is
`A5 Human-ready`.

The A5 preparation must provide:

- exact Git commit SHA;
- exact Windows artifact/package being reviewed;
- green applicable automated gates;
- links or paths to the relevant OG implementation;
- links or paths to the Rust/Dioxus implementation;
- any required test data/accounts/remotes;
- known limitations, if any;
- screenshots/video where visual comparison is meaningful; and
- a clear statement that no known blocker prevents a fair review.

For whole-product/package review, use a clean release build rather than a
developer hot-reload build.

## Standard review environment

Unless the row explicitly requires another environment, use:

- Windows target matching the supported Hermes Local Desktop environment;
- the exact migration branch/build SHA under review;
- OG Hermes Local available side-by-side as the behavioral/visual oracle;
- a healthy local Hermes Agent/runtime for normal-state testing;
- a deliberate offline/failure setup where the row has degraded/recovery
  behavior;
- 1296×809 or equivalent standard window size **and** maximized;
- dark, light and system appearance for UI rows where theme matters;
- 100% display scale, with 125%/150% spot checks for layout/windowing rows where
  DPI can materially change behavior;
- representative persisted data rather than empty-state-only testing; and
- disposable projects/accounts/remote targets for destructive or security
  scenarios.

For SSH, Cloud, OAuth, media, notification, installer or updater rows, the
review environment must include the real external/native dependency. A mock is
not sufficient for the final human gate.

## Per-capability manual checklist

Use this checklist for **every** matrix row. A row-specific `Human acceptance`
scenario in the matrix adds requirements; it does not replace these.

### 1. Scope and evidence

- [ ] Confirm the capability ID and exact build SHA.
- [ ] Read the row's current evidence/gap and row-specific human acceptance
      scenario.
- [ ] Identify the corresponding OG source/component/flow.
- [ ] Identify the corresponding Rust/Dioxus source/service.
- [ ] Confirm all required test accounts, projects, remote hosts or devices are
      available.
- [ ] Confirm the row is genuinely A5 and has no hidden known blocker.

### 2. Visual parity

For rows with a visible UI:

- [ ] Compare OG and Rust side-by-side at standard window size.
- [ ] Compare maximized state.
- [ ] Check spacing, density, typography, iconography, borders, radii, colors,
      hierarchy and alignment.
- [ ] Check hover, selected, active, disabled, loading, empty, error and
      destructive states where applicable.
- [ ] Check dark/light/system appearance where applicable.
- [ ] Check text clipping, wrapping and overflow.
- [ ] Check representative DPI/scaling behavior when relevant.
- [ ] Reject unintended redesigns even if functionality works.

Mark N/A only when the capability genuinely has no visible representation.

### 3. Interaction parity

- [ ] Mouse/pointer interactions match the OG intent.
- [ ] Keyboard navigation and shortcuts work where the OG supports them.
- [ ] Focus moves to the expected control and is visibly represented.
- [ ] Escape/cancel/back behavior is correct.
- [ ] Context menus, dialogs, overlays and confirmations behave correctly.
- [ ] Rapid/repeated actions do not create duplicate operations or stale UI.
- [ ] Background activity does not steal focus unexpectedly.

### 4. State, scope and persistence

- [ ] State updates immediately and correctly after the operation.
- [ ] Switching sessions/projects/profiles does not leak state across scopes.
- [ ] Close/reopen the relevant page/overlay and verify retained state.
- [ ] Restart Hermes Local and verify persisted state where the OG persists it.
- [ ] Cold-start with pre-existing data and verify correct rehydration.
- [ ] Stale asynchronous results cannot overwrite a newer user action.
- [ ] Where applicable, run the same operation with empty and populated state.

### 5. Functional behavior

- [ ] Execute the real operation, not merely the UI path.
- [ ] Verify the resulting Agent/runtime/filesystem/Git/native state directly
      where possible.
- [ ] Compare the observable result with the OG.
- [ ] Exercise a representative happy path and at least one edge case.
- [ ] For long-running work, verify progress, cancellation and completion.
- [ ] For destructive work, verify exactly what changed and what did not.

### 6. Failure, recovery and degraded behavior

- [ ] Trigger at least one realistic failure.
- [ ] Error copy is actionable and does not expose secrets/private internals.
- [ ] Loading state cannot become permanently stranded.
- [ ] Retry/reconnect/recovery works without unnecessary app restart where the
      OG supports it.
- [ ] Cancel/close during in-flight work leaves a coherent state.
- [ ] Network/process disappearance is handled safely where relevant.
- [ ] Restart after failure and confirm recovery/persistence behavior.

### 7. Security and safety

Apply the relevant checks:

- [ ] Secrets do not appear in DOM text, URLs, command lines, logs or persisted
      plaintext files.
- [ ] File operations remain inside the selected/allowed root.
- [ ] Symlinks/path traversal cannot escape containment.
- [ ] Native process/Git/SSH operations use constrained typed inputs rather than
      a generic shell escape hatch.
- [ ] Destructive operations require the intended confirmation.
- [ ] External URLs and embedded navigation obey the allowlist/origin policy.
- [ ] OAuth state/callback/session behavior cannot be confused across attempts.
- [ ] A UI surface does not gain native authority that belongs in
      `hermes-desktop`.
- [ ] Privilege/elevation is no broader than the OG capability requires.

### 8. Performance and responsiveness

When performance is material to the row:

- [ ] No obvious UI freeze during representative operation.
- [ ] Streaming/scrolling/typing remains responsive.
- [ ] Large lists/history/output remain usable.
- [ ] Hidden/background surfaces do not consume unreasonable CPU.
- [ ] Repeated open/close operations do not produce obvious memory/process leaks.
- [ ] Any measured regression against the recorded OG/Rust baseline is reviewed
      before PASS.

### 9. Neighboring regression check

- [ ] Navigate into and out of the capability through normal product flows.
- [ ] Verify at least one neighboring surface still works.
- [ ] Verify shared sidebar/titlebar/status/settings state remains coherent.
- [ ] If the capability changes connection/project/profile/runtime state, verify
      the rest of the app reflects that change.

### 10. Record the result

A review is not complete until the result is recorded.

**PASS**

1. Save evidence.
2. Update the matrix row to `A6 Human-validated`.
3. Set the `Human` cell to:
   `✅ @reviewer YYYY-MM-DD <build-sha>`
4. Add a validation record using the template below.

**FAIL**

1. Do **not** set A6.
2. Keep or return the row to A4/A5 as appropriate.
3. Set the `Human` cell to:
   `❌ @reviewer YYYY-MM-DD <build-sha> — <issue/ref>`
4. Record the observed mismatch and expected OG behavior.
5. Re-review after the fix on the new build SHA.

**BLOCKED**

1. Do not convert uncertainty into PASS.
2. Set the `Human` cell to:
   `⏸ YYYY-MM-DD — <blocker/ref>`
3. Keep the implementation stage truthful; use `BX Blocked` if the blocker
   prevents meaningful progress/validation.
4. Resume the same checklist when the dependency is available.

## Validation record template

Store concise records in the PR, a tracked QA report, or under:

`reports/qa/dioxus-human/<build-sha>/<capability-id>/`

Recommended Markdown template:

```md
# Dioxus human validation — <CAPABILITY-ID>

- Capability: <name>
- Build SHA: <full SHA>
- Artifact/package: <exact filename or CI artifact>
- Reviewer: @<name>
- Date: YYYY-MM-DD
- Result: PASS | FAIL | BLOCKED

## OG references
- <source file / route / screenshot / behavior reference>

## Rust/Dioxus references
- <source file / service / test / route reference>

## Evidence
- <screenshot/video/log/test-data references>

## Checklist
- Visual parity: PASS | FAIL | N/A
- Interaction parity: PASS | FAIL | N/A
- State/persistence: PASS | FAIL | N/A
- Functional behavior: PASS | FAIL | N/A
- Failure/recovery: PASS | FAIL | N/A
- Security/safety: PASS | FAIL | N/A
- Performance/responsiveness: PASS | FAIL | N/A
- Neighboring regression: PASS | FAIL | N/A

## Notes
<what was exercised and anything worth preserving for future regressions>

## Follow-up
<issue/PR/reference if failed or blocked>
```

A screenshot alone is not a validation record. A text record alone without
actually exercising the behavior is also not sufficient.

## Review batching

Several capabilities may be reviewed in one session/build to reduce setup cost,
but **every matrix row needs an individual outcome**.

Good examples of safe batching:

- Appearance + settings navigation + related persistence rows.
- Provider Accounts + OAuth + API Keys + Custom Endpoints.
- Project Centre create/attach/clone/pin/archive/remove/repair rows.
- SSH probe + POSIX/Windows lifecycle when the required hosts are available.
- Files + Git + terminal against one disposable test repository.

Do not use a single broad "Settings looks good" or "SSH works" sign-off to
validate multiple rows without exercising the row-specific acceptance scenario.

## Destructive and security-sensitive review

Use disposable data for:

- project filesystem deletion;
- trash/revert/reset/restore;
- worktree removal;
- Git discard/revert/push tests;
- updater/rollback/uninstall;
- secret rotation/removal;
- OAuth logout/revocation;
- security scanning; and
- remote SSH owned-process stale cleanup.

Before destructive review, verify the target path/account/host explicitly.
Human validation is not a reason to weaken the same safeguards being tested.

## Whole-product final regression

After every individual applicable row has reached A6, perform one final
whole-product pass on the release candidate SHA.

- [ ] Clean install on a clean Windows environment.
- [ ] First-run/local Agent boot.
- [ ] Existing-user upgrade/data migration.
- [ ] Core chat, streaming and interruption.
- [ ] Project Centre.
- [ ] Files/Git/terminal.
- [ ] Settings/models/providers.
- [ ] Local/Remote/OAuth/Cloud/SSH connection modes.
- [ ] Workstation/Agent feature surfaces.
- [ ] Native notifications/windows/shortcuts/media/power/deep links.
- [ ] Update and rollback.
- [ ] Restart/relaunch/crash recovery.
- [ ] Dark/light/system and representative DPI/window sizes.
- [ ] Performance/footprint measurements.
- [ ] Inspect process tree and packaged files: no production Electron/React/Node
      client runtime remains.
- [ ] Confirm Hermes Agent is still the separate Python harness.
- [ ] Confirm the shared UI architecture/WASM guard is green.
- [ ] No unresolved blocker or failed human row remains.

Record this as `FINAL-RC` evidence under the same build SHA.

## Completion rule

The migration is not complete merely because all code has been written, all CI
is green, or the Rust application can launch.

It is complete only when:

1. every applicable capability row is A6;
2. every A6 row has an exact human reviewer/date/build SHA;
3. all failed rows have been fixed and re-reviewed;
4. no blocker remains;
5. distribution/updater/cutover rows are A6;
6. Electron/React/Node are absent from the production client runtime/artifacts;
7. the final whole-product release-candidate regression passes; and
8. the human product owner accepts that release candidate.

That final human gate is intentional and cannot be delegated to automation.
