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
| Lifecycle | `status`, `exitCode`, structured `failure`, nullable `result` |
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
| `queued` | `running`, `cancelled`, `failed`, `interrupted` |
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
at most 50 terminal records without pruning active work.

### Persistence and restart reconciliation

Serializable task records are stored in schema-versioned JSON at
`data/runtime/desktop-tasks.json`. Writes use a same-directory temporary file
and atomic rename. A task is durably queued before its process is spawned;
output writes are coalesced, while ownership and terminal transitions flush
immediately. Loading rejects malformed records and reconstructs resource claims
from the canonical action policy, so persisted data cannot forge locks. Active
records are always retained and only terminal history is bounded.

On Desktop startup and before task admission or status reads, the controller
reconciles every recovered non-terminal record:

- a live recorded PID becomes an `external-process` owner and keeps its locks;
- a queued task that never started becomes `interrupted`;
- a completed action with fresh, action-specific report, archive or runtime
  evidence becomes `succeeded` or `failed` and records the evidence path;
- a cancelling task whose owner exited becomes `cancelled`; and
- an ownerless or stale ambiguous record becomes `interrupted` and releases its
  locks.

Reconciliation repeats once per second so an externally owned process that
exits is classified promptly. Snapshots and the task-list IPC return the same
authoritative records, and renderer reloads restore the newest active or recent
task instead of starting with an empty in-memory view.

### Task Centre and control authority

The Task Centre never reconstructs lifecycle state in the renderer. Its all,
active, queued, completed, failed and cancelled views filter the task records
returned by the snapshot, and selection is view state only. Stage, elapsed
time, project, model/profile association, owner, resource claims, bounded
redacted output, result paths and recovery details are presentations of that
record. Navigation links return to the owning feature, while result links ask
the main process to resolve and open a path under the Hermes Local root.

The main process adds non-persisted `cancel`, `pause`, `resume` and `retry`
capabilities to public task views. Queued cancellable work may be cancelled
immediately. Running cancellation is exposed only while the current Desktop
retains the exact child-process handle and PID; recovered external processes
are never signalled. Pause and resume remain false until an action implements a
durable protocol. A retry creates a new admission through the same policy
instead of mutating terminal history. The sidebar active count also comes from
the task-list IPC rather than a second renderer-owned registry.

Filter buttons implement tab, arrow, Home and End keyboard navigation. The
master/detail layout stacks below the large breakpoint, so task controls,
output and recovery evidence remain reachable in narrow Desktop windows.

## Consequences

- The benchmark and gateway-recovery case from #26 follows general resource
  rules instead of a named-action exception.
- Admission decisions and Task Centre controls are deterministic and
  explainable from one main-process policy.
- The renderer polls authoritative task views and leaves lifecycle transitions,
  process ownership and conflict enforcement to the main process.

## Verification

Behavior tests cover valid and invalid transitions, duplicate starts,
benchmark/readiness compatibility, maintenance conflicts, observational access,
queue versus reject decisions, cancellation, stale ownership and bounded
output. Store tests cover atomic replacement, malformed input, canonical lock
restoration and terminal-history bounds. Recovery tests cover live external
owners, fresh and stale evidence, queued restart interruption and action report
classification. Electron boundary tests verify terminal waiting and retention
of active tasks while completed history is pruned. Task Centre tests cover
filtering without state mutation, elapsed time, empty filtered selection,
keyboard navigation and capability-gated retry controls; browser QA covers the
failed-task recovery flow and desktop/narrow layouts.
