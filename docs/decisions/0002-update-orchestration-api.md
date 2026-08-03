# ADR 0002: Typed update orchestration and durable operation state

- Status: Accepted
- Date: 2026-08-03
- Issue: #43

## Context

Hermes Local previously had three overlapping update views:

1. `Update-Hermes-Local.ps1` owned launcher inventory, promotion and rollback.
2. `Update-Hermes-Agent.ps1` owned the transactional Hermes Agent workflow.
3. Desktop wrapped update checks as generic Task Centre child processes.

The Task Centre already persisted background tasks and modelled resource conflicts, but update-specific identity, stages, recovery and reports were not represented by one API. A renderer or caller could disappear while the underlying update continued, and CLI callers did not participate in a cross-surface update lock.

## Decision

`scripts/Hermes-UpdateOrchestrator.psm1` is the authoritative update control-plane API.

Every operation has:

- a stable operation ID;
- component, mode and caller identity;
- the ordered stages `check`, `compatibility`, `prepare`, `verify`, `backup`, `promote`, `validate` and `rollback`;
- bounded structured logs, progress, failure and recovery evidence;
- atomic durable state under `data/runtime/update-operations`;
- a single exclusive lock under `data/runtime/locks/update-orchestrator.json`;
- a report projection under `build/updates/operations`.

The durable state file is the source of truth. Markdown and JSON files under `build/updates` are report projections and must not be used to resume or mutate an operation.

Component behavior is provided through registered adapters. An adapter can implement any stage as a script block; omitted stages are explicitly recorded as skipped. The launcher adapter implements each promotion stage directly. The Hermes Agent adapter delegates its existing, separately recoverable transaction to `Update-Hermes-Agent.ps1`, preserving that recovery entry point while placing invocation, identity, locking and reporting behind the common API.

`Update-Hermes-Local.ps1` remains the supported CLI entry point and now calls the orchestration API. Desktop already invokes this script, so Desktop and PowerShell use the same state machine and reports without a renderer-owned implementation.

Native commands are invoked only through validated argument arrays. NUL, CR, LF, null arguments and oversized arguments are rejected before process creation.

## Lock and recovery rules

Lock creation uses create-new filesystem semantics. An existing lock is not stolen while its owner PID is alive. If the owner no longer exists, the lock is moved to timestamped recovery evidence before a new operation acquires the resource.

The lock protects both `update-orchestrator` and `workstation`, matching the disruptive update resource represented by the Task Centre. Desktop tasks still provide user-facing progress and cancellation ownership; the orchestration state remains authoritative if the renderer exits.

## Compatibility

The following behavior is retained:

- `Update-Hermes-Local.ps1 -Mode Check|Apply|Rollback`;
- `Update-Hermes-Agent.ps1 -Mode Check|Apply|Rollback` as the Hermes Agent adapter and emergency recovery entry point;
- `build/updates/LATEST.json` and `LATEST.md` for inventory consumers;
- launcher known-good snapshots and rollback history.

`All` remains the default check scope. For compatibility, `Apply` or `Rollback` with `Component All` resolves to the launcher adapter, matching the previous general updater behavior.

## Verification

`tests/Test-HermesUpdateOrchestrator.ps1` exercises a fixture adapter through both CLI and Desktop caller identities. It verifies equivalent normalized state, stage persistence after the caller module exits, successful promotion, controlled failure with rollback, explicit rollback, stale-lock recovery and native argument rejection.
