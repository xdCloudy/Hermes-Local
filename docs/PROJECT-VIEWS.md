# Hermes Local Roadmap Views

This document defines the canonical saved views for the **Hermes Local Roadmap** GitHub Project.

- Project: <https://github.com/users/xdCloudy/projects/1>
- Repository: <https://github.com/xdCloudy/Hermes-Local>
- Source issues: <https://github.com/xdCloudy/Hermes-Local/issues>

The Project contains the maintained issue backlog, including tracking parents, implementation issues, bugs, research, testing, maintenance, and architecture work.

## Purpose

The saved views are intended to answer distinct maintainer questions without duplicating issue state:

- What needs triage or a design decision?
- What belongs in the current release?
- Which workstreams are active?
- What is blocked, and by what?
- Which changes are likely to collide?
- Which issues are suitable for contributors?
- What is the long-term delivery sequence?

Issue labels remain the repository-wide classification system. Project fields provide richer planning metadata, ordering, and reporting.

## Canonical Project fields

The Project uses the following custom fields.

| Field | Purpose |
|---|---|
| `Roadmap Status` | Maintainer workflow state independent of GitHub issue open/closed state. |
| `Priority` | Critical, High, Medium, or Low delivery priority. |
| `Type` | Bug, Feature, Enhancement, Maintenance, Refactor, Testing, Research, or Tracking. |
| `Component` | Primary subsystem that owns the work. |
| `Cluster` | Higher-level programme or workstream. |
| `Target release` | Intended milestone or `Unscheduled`. |
| `Regression risk` | Expected blast radius of an incorrect implementation. |
| `Implementation order` | Relative sequencing number; lower numbers should generally be addressed first. |
| `Effort` | S, M, L, XL, or Programme. |
| `Decision area` | Architecture domain requiring coordinated decisions. |
| `Contributor readiness` | Whether the issue is suitable for external contribution. |

## View 1: Maintainer triage

### Question answered

What requires classification, clarification, or a design decision before implementation can begin?

### Configuration

- **Layout:** Table
- **Filter:**
  - `Roadmap Status` is `Triage`
  - **OR** `Roadmap Status` is `Needs design`
- **Group by:** `Type`
- **Sort:**
  1. `Priority` descending
  2. `Updated` descending

### Recommended visible fields

- Title
- Roadmap Status
- Priority
- Type
- Component
- Decision area
- Regression risk
- Updated

### Maintainer use

Review this view before assigning implementation work. Issues should leave this view only when the problem, acceptance criteria, dependencies, and required design decisions are sufficiently clear.

---

## View 2: Current release

### Question answered

What is planned for the active release, in what order, and what is its current state?

### Configuration

- **Layout:** Table
- **Filter:** `Target release` is the active release, normally `v0.18.x`, `v0.19`, or `v1.0`
- **Group by:** `Roadmap Status`
- **Sort:**
  1. `Implementation order` ascending
  2. `Priority` descending

### Recommended visible fields

- Title
- Roadmap Status
- Priority
- Component
- Implementation order
- Effort
- Regression risk
- Milestone

### Maintainer use

Use this as the release execution view. Change the `Target release` filter when the active release changes rather than creating a new permanent view for every release.

---

## View 3: Workstreams

### Question answered

How is active work distributed across the major Hermes Local programmes?

### Configuration

- **Layout:** Board
- **Filter:** `Roadmap Status` is not `Done`
- **Group by:** `Cluster`
- **Sort within groups:**
  1. `Priority` descending
  2. `Implementation order` ascending

### Recommended card fields

- Roadmap Status
- Priority
- Component
- Target release
- Effort

### Maintainer use

Use this view for broad programme balancing. It should expose clusters that have too much concurrent work, no active prerequisite work, or unclear ownership.

---

## View 4: Blocked work

### Question answered

Which issues cannot proceed, and where are the largest dependency bottlenecks?

### Configuration

- **Layout:** Table
- **Filter:** `Roadmap Status` is `Blocked`
- **Group by:** `Component`
- **Sort:**
  1. `Priority` descending
  2. `Implementation order` ascending

### Recommended visible fields

- Title
- Priority
- Component
- Cluster
- Blocked by
- Target release
- Implementation order
- Regression risk

### Maintainer use

Review native **Blocked by** relationships rather than duplicating dependency lists in comments. Prioritise blockers that release several high-priority downstream issues.

---

## View 5: Architecture decisions

### Question answered

Which issues require an architecture decision, design document, or cross-component agreement?

### Configuration

- **Layout:** Table
- **Filter:**
  - `Roadmap Status` is `Needs design`
  - **OR** `Type` is `Research`
- **Group by:** `Decision area`
- **Sort:**
  1. `Priority` descending
  2. `Implementation order` ascending

### Recommended visible fields

- Title
- Priority
- Type
- Decision area
- Component
- Cluster
- Regression risk
- Target release

### Maintainer use

Resolve high-leverage architecture issues before accepting dependent implementation pull requests. Approved decisions should be recorded in `docs/design/` where the issue requires a durable architecture decision record.

---

## View 6: Implementation collisions

### Question answered

Which issues are most likely to conflict because they touch shared lifecycle, state, data, or control-plane code?

### Configuration

- **Layout:** Table
- **Filter:**
  - `Regression risk` is `High`
  - **OR** `Regression risk` is `Very high`
- **Group by:** `Component`
- **Sort:** `Implementation order` ascending

### Recommended visible fields

- Title
- Roadmap Status
- Priority
- Component
- Decision area
- Regression risk
- Implementation order
- Assignees

### Maintainer use

Use this view before starting parallel work. Coordinate branches and pull requests when two issues modify the same updater, supervisor, task-state, project-identity, trust, or persistent-data contract.

---

## View 7: Bugs

### Question answered

What verified or suspected defects remain open, and which require immediate attention?

### Configuration

- **Layout:** Table
- **Filter:**
  - `Type` is `Bug`
  - `Roadmap Status` is not `Done`
- **Group by:** `Component`
- **Sort:**
  1. `Priority` descending
  2. `Updated` descending

### Recommended visible fields

- Title
- Roadmap Status
- Priority
- Component
- Target release
- Regression risk
- Assignees
- Updated

### Maintainer use

Issues labelled `needs reproduction` should remain in `Triage` until reproduced against current `main` or a supported packaged build. Do not close a bug solely because adjacent code changed.

---

## View 8: Contributor-ready

### Question answered

Which issues are sufficiently bounded and documented for an external contributor?

### Configuration

- **Layout:** Table
- **Filter:**
  - `Contributor readiness` is `Ready`
  - **OR** `Contributor readiness` is `Good first issue`
- **Group by:** `Component`
- **Sort:**
  1. `Contributor readiness` ascending
  2. `Priority` descending
  3. `Updated` descending

### Recommended visible fields

- Title
- Contributor readiness
- Priority
- Component
- Effort
- Roadmap Status
- Target release

### Maintainer use

Only place an issue here when its scope, acceptance criteria, affected files or subsystem, validation method, and dependencies are clear. Architecture programmes and high-risk refactors should normally remain `Not ready` or `Needs maintainer`.

---

## View 9: Roadmap

### Question answered

What is the planned delivery sequence across releases and long-term programmes?

### Configuration

- **Layout:** Roadmap
- **Group by:** `Target release`
- **Sort:** `Implementation order` ascending
- **Date fields:** Use milestone dates only when maintainers have approved a credible release window.

### Recommended visible fields

- Title
- Roadmap Status
- Priority
- Cluster
- Target release
- Implementation order
- Effort

### Maintainer use

Do not invent dates for unscheduled or research-heavy work. The roadmap should communicate sequencing and grouping first; dates should be added only when prerequisites and capacity are understood.

## Optional operational views

The following temporary views can be useful but do not need to be permanent:

### Recently inactive

- **Layout:** Table
- **Filter:** `Roadmap Status` is not `Done`
- **Sort:** `Updated` ascending

Use this during periodic backlog maintenance to find valid issues that have lost momentum or need re-triage.

### Release-risk review

- **Layout:** Table
- **Filter:**
  - `Target release` is the active release
  - `Regression risk` is `High` or `Very high`
- **Group by:** `Component`
- **Sort:** `Implementation order` ascending

Use before release candidate creation and final packaging tests.

## Status guidance

| Roadmap Status | Use when |
|---|---|
| `Triage` | The issue needs reproduction, classification, evidence, or maintainer review. |
| `Needs design` | The outcome is valid, but a design or architecture decision is unresolved. |
| `Ready` | Scope, acceptance criteria, dependencies, and validation are clear. |
| `In progress` | Implementation has actively started. |
| `In review` | A pull request or equivalent reviewable implementation exists. |
| `Blocked` | A native dependency or explicit decision prevents progress. |
| `Done` | The issue is implemented and verified; the GitHub issue should normally also be closed. |
| `Deferred` | The work remains valid but is intentionally outside current delivery plans. |

## Maintenance rules

1. **GitHub issue state remains authoritative for open versus closed.** Project status adds planning detail but should not contradict the issue state for long periods.
2. **Use native parent and blocked-by relationships.** Do not maintain competing dependency graphs in Project text fields.
3. **One primary component per issue.** Cross-cutting concerns belong in labels, dependencies, or the issue body.
4. **Update `Implementation order` when prerequisites change.** Decimal values are allowed for work inserted between established steps.
5. **Do not use milestones as broad themes.** Milestones represent coherent delivery targets; `Cluster` represents programmes.
6. **Keep tracking parents open until their required children are complete.** Tracking issues should not be treated as independently implementable work.
7. **Reassess contributor readiness after major design changes.** A previously ready issue may become unsuitable after its dependencies or contracts change.
8. **Avoid arbitrary due dates.** Add dates only when maintainers have committed to a credible delivery window.

## Suggested review cadence

### Per pull request

- Move the linked issue to `In review` when the pull request is ready for review.
- Confirm the issue and pull request use the same component and intended release.
- Recheck blocked-by relationships when the pull request changes a shared contract.

### Weekly

- Review **Maintainer triage**.
- Review **Blocked work** for high-leverage blockers.
- Check **Implementation collisions** before starting parallel high-risk work.

### Before a release

- Review **Current release** in implementation order.
- Review the temporary **Release-risk review** view.
- Confirm every included issue has verification evidence and a recovery or rollback path where applicable.

### Monthly

- Review **Recently inactive**.
- Reassess deferred work and long-term milestone placement.
- Remove obsolete Project items only after closing or explicitly superseding their source issues.

## Creating the views

GitHub Project saved views must currently be configured in the Project interface:

1. Open the **Hermes Local Roadmap** Project.
2. Select **New view**.
3. Choose the documented layout.
4. Apply the filters, grouping, sorting, and visible fields listed above.
5. Rename the view using the exact heading name from this document.
6. Save the view.

This document is the source of truth for the intended configuration. Update it in the same pull request whenever Project fields or backlog-management policy change.
