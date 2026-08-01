# ADR 0001: Task lifecycle and resource locks

[← Documentation index](../README.md) · [Architecture](../ARCHITECTURE.md)

- Status: Accepted
- Date: 2026-08-01
- Decision issue: [#46](https://github.com/xdCloudy/Hermes-Local/issues/46)
- Follow-up implementation: [#47](https://github.com/xdCloudy/Hermes-Local/issues/47)
- Follow-up interface: [#48](https://github.com/xdCloudy/Hermes-Local/issues/48)

## Context

Hermes Local launches long-running PowerShell actions from the Electron main
process. The original controller used one global running-action map plus a
special exception for `start` during `benchmark`. That exception prevented the
specific gateway-recovery conflict recorded in #26, but it did not explain
which resources an action owned, could not queue work, and had no durable
contract for cancellation or crash recovery.

The controller also performs health snapshots and reconnect work. Those reads
must remain available while an action runs; treating every operation as a
globally exclusive task would recreate the #26 failure mode.

## Decision

### Schema and identity

Every admitted action is represented by task schema version 1. Its serializable
record contains:

| Area | Fields |
|---|---|
| Identity | `schemaVersion`, UUID `id`, `action` |
| Time | `createdAt`, `queuedAt`, nullable `startedAt` and `completedAt`, `updatedAt` |
| Lifecycle | `status`, `exitCode`, structured `failure` |
| Ownership | owner kind and nullable process ID |
| Admission | conflict policy and explicit shared/exclusive resource claims |
| Evidence | redacted `output` and `outputTruncated` |

Electron-only process handles, event emitters and input payloads are not part of
the public record. A repeated non-terminal action joins the existing task ID;
it does not create a second owner.

### State machine

Tasks begin in `queued`. The allowed transitions are:

| From | To |
|---|---|
| `queued` | `running`, `cancelled`, `failed` |
| `running` | `succeeded`, `failed`, `cancelling`, `interrupted` |
| `cancelling` | `cancelled`, `failed`, `interrupted` |
| `cancelled`, `failed`, `interrupted`, `succeeded` | none |

Terminal transitions set `completedAt`. Starting sets `startedAt`. Invalid
transitions fail explicitly instead of silently rewriting history.

### Resources and admission

Only `running` and `cancelling` tasks own resource locks. Shared claims coexist;
an exclusive claim conflicts with any claim for the same resource.

| Action | Claims | Conflict result |
|---|---|---|
| `diagnostics`, `security` | none (observational) | start |
| `start` | shared `workstation` | queue |
| `benchmark` | shared `workstation`, exclusive `model-runtime` | reject |
| `test` | shared `workstation`, shared `model-runtime` | reject |
| `backup` | shared `workstation`, shared `user-data` | reject |
| `repair`, `restart`, `stop`, `update` | exclusive `workstation` | reject |

This makes maintenance globally exclusive from mutating or long-running
workstation actions while preserving observational access. Health probes,
snapshot reads and socket reconnect attempts do not enter the task lock model
and are never blocked.

Gateway readiness uses `start`, so it remains compatible with a benchmark's
model lease. It queues behind exclusive maintenance and starts in insertion
order when the owning task reaches a terminal state. Other conflicting manual
actions are rejected with deterministic, task-ID-sorted details naming the
action and resource.

### Cancellation, output and stale ownership

`backup`, `benchmark`, `diagnostics`, `security` and `test` are cancellable.
Cancelling queued work is immediate. Running work enters `cancelling` and keeps
its locks until its owner exits; this prevents another writer from starting
while cleanup is incomplete. Maintenance and automatic readiness actions are
not cancellable after admission.

Task output is redacted before storage, keeps only the newest 128 KiB, and sets
`outputTruncated` when earlier content has been discarded. The controller keeps
at most 50 terminal records in memory without pruning queued or running work.

An active record whose non-null owner PID is no longer alive transitions to
`interrupted` with an `owner-exited` failure and releases its locks. Issue #47
will persist these records and run that reconciliation after desktop restarts;
this ADR and the state-machine implementation define the recovery contract now.

## Consequences

- The benchmark and gateway-recovery case from #26 follows general resource
  rules instead of a named-action exception.
- Admission decisions are deterministic and explainable to future Task Centre
  UI work.
- The renderer polls queued, running and cancelling tasks and leaves conflict
  enforcement to the main process.
- Schema persistence, external-process discovery and restart reconciliation
  remain scoped to #47. Task Centre controls and history presentation remain
  scoped to #48.

## Verification

Behavior tests cover valid and invalid transitions, duplicate starts,
benchmark/readiness compatibility, maintenance conflicts, observational access,
queue versus reject decisions, cancellation, stale ownership and bounded
output. Electron boundary tests verify terminal waiting and retention of active
tasks while completed history is pruned.
