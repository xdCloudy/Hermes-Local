# Stable compatibility promotion

[← Documentation home](README.md) · [Upstream compatibility](UPSTREAM_COMPATIBILITY.md) ·
[Project home](../README.md)

Hermes Local treats Stable as a fail-closed release channel. A candidate cannot
receive approval unless explicit `Upstream compatibility` and `Windows
lifecycle validation` workflow runs pass for the same commit and the separate
`Stable compatibility gate` validates both evidence sets.

## Mandatory evidence

Stable approval requires an aggregate compatibility report containing all of:

- `hermes-agent`;
- `llama-cpp-cpu`;
- `llama-cpp-gpu`.

Every component must be `compatible` or `compatible-with-warnings`, identify a
full candidate commit, and belong to the same selected workflow run. The GPU
report must identify CUDA acceleration and Windows test evidence.

A missing GPU report is a hard failure. CPU-only evidence can support development
or candidate testing but cannot approve Stable.

It also requires a Stable-evaluated
[Windows lifecycle report](WINDOWS_LIFECYCLE_VALIDATION.md). Every mandatory
install, upgrade, repair, rollback, uninstall, adverse-condition and
preservation scenario must pass. The report must include successful trusted
physical CPU-only and NVIDIA hardware records. Missing or skipped lifecycle
evidence is a hard failure.

## Trusted GPU runner

The GPU job runs only on a self-hosted Windows runner carrying all of these
labels:

- `self-hosted`;
- `windows`;
- `hermes-gpu-gate`.

The runner must be dedicated or disposable validation hardware. It must not be a
maintainer's normal workstation or a production Hermes Local host, and it must
not expose signing, release, repository-write, deployment, or personal secrets
to upstream candidate code.

The initial hardware requirement is a supported NVIDIA CUDA system with `nvcc`,
CMake, Git, and the normal workflow prerequisites. Additional trusted runner
labels can be introduced when more runtime backends are added.

## Produce Stable-eligible source runs

1. Open **Actions → Upstream compatibility → Run workflow**.
2. Run the workflow from the repository default branch.
3. Set **Run the CUDA build gate on a trusted self-hosted runner** to `true`.
4. Select explicit Hermes Agent or `llama.cpp` revisions when validating a
   release candidate; otherwise the configured branches are resolved.
5. Confirm that the complete workflow concludes successfully.
6. Record its numeric workflow run ID.

Then open **Actions → Windows lifecycle validation → Run workflow** from the
same default-branch commit:

1. enable the disposable and physical lanes;
2. enable Stable evaluation;
3. supply the HTTPS URL and SHA-256 of the immutable previous-Stable installer;
4. wait for every hosted, CPU-only and NVIDIA job to pass;
5. record the lifecycle workflow run ID.

A run cannot become Stable evidence when:

- the trusted GPU runner is unavailable or skipped;
- any component is blocked;
- the aggregate report is missing;
- the source run did not execute from the default branch;
- the source run was not an explicit workflow or repository dispatch.

Lifecycle evidence is ineligible when any mandatory scenario is absent,
failed or skipped, when either physical runner is unavailable, when the matrix
digest differs from the trusted checkout, or when its candidate commit differs
from the compatibility run.

## Approve the evidence

Open **Actions → Stable compatibility gate → Run workflow** and enter both
successful run IDs.

The gate:

1. verifies that the source run is the repository's `Upstream compatibility`
   workflow;
2. requires a successful explicit run from the default branch;
3. verifies that both source runs target the same commit;
4. downloads `compatibility-aggregate` and `windows-lifecycle-aggregate`;
5. verifies aggregate and per-component statuses;
6. requires Hermes Agent, CPU runtime, and trusted CUDA runtime reports;
7. verifies the lifecycle matrix, mandatory scenario inventory and physical
   CPU/NVIDIA evidence;
8. emits `stable-promotion.json` as the retained approval artifact.

The gate uses the `stable` GitHub environment. Repository administrators should
configure required reviewers or other environment protection rules before this
workflow becomes part of a public release process.

## Canonical integration contract

`.github/workflows/stable-compatibility-gate.yml` is both manually dispatchable
and reusable through `workflow_call`. Any workflow that publishes a Stable
release, Stable update manifest, or Stable Update Centre entry must call this
gate with both source run IDs and proceed only after it succeeds.

No release or updater workflow may infer Stable compatibility from branch state,
a successful build alone, historical QA, or an unverified JSON file. The retained
`stable-promotion-<run-id>` artifact is the machine-readable approval record.

## Boundaries

This gate approves compatibility and lifecycle evidence only. Artifact signing,
SBOMs, provenance and transactional updater publication remain separate release
responsibilities. Those systems must consume this gate rather than duplicate or
weaken it.
