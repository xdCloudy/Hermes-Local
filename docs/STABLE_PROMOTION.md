# Stable compatibility promotion

[← Documentation home](README.md) · [Upstream compatibility](UPSTREAM_COMPATIBILITY.md) ·
[Project home](../README.md)

Hermes Local treats Stable as a fail-closed release channel. A candidate cannot
receive Stable compatibility approval unless one explicit `Upstream
compatibility` workflow run passes every mandatory component gate and the
separate `Stable compatibility gate` validates that evidence.

## Mandatory evidence

Stable approval requires one aggregate compatibility report containing all of:

- `hermes-agent`;
- `llama-cpp-cpu`;
- `llama-cpp-gpu`.

Every component must be `compatible` or `compatible-with-warnings`, identify a
full candidate commit, and belong to the same selected workflow run. The GPU
report must identify CUDA acceleration and Windows test evidence.

A missing GPU report is a hard failure. CPU-only evidence can support development
or candidate testing but cannot approve Stable.

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

## Produce a Stable-eligible compatibility run

1. Open **Actions → Upstream compatibility → Run workflow**.
2. Run the workflow from the repository default branch.
3. Set **Run the CUDA build gate on a trusted self-hosted runner** to `true`.
4. Select explicit Hermes Agent or `llama.cpp` revisions when validating a
   release candidate; otherwise the configured branches are resolved.
5. Confirm that the complete workflow concludes successfully.
6. Record its numeric workflow run ID.

A run cannot become Stable evidence when:

- the trusted GPU runner is unavailable or skipped;
- any component is blocked;
- the aggregate report is missing;
- the source run did not execute from the default branch;
- the source run was not an explicit workflow or repository dispatch.

## Approve the evidence

Open **Actions → Stable compatibility gate → Run workflow** and enter the
successful compatibility run ID.

The gate:

1. verifies that the source run is the repository's `Upstream compatibility`
   workflow;
2. requires a successful explicit run from the default branch;
3. downloads the source run's `compatibility-aggregate` artifact;
4. verifies aggregate and per-component statuses;
5. requires Hermes Agent, CPU runtime, and trusted CUDA runtime reports;
6. verifies that every component belongs to the selected workflow run;
7. rejects a GPU report that is not CUDA or lacks Windows evidence;
8. emits `stable-promotion.json` as the retained approval artifact.

The gate uses the `stable` GitHub environment. Repository administrators should
configure required reviewers or other environment protection rules before this
workflow becomes part of a public release process.

## Canonical integration contract

`.github/workflows/stable-compatibility-gate.yml` is both manually dispatchable
and reusable through `workflow_call`. Any workflow that publishes a Stable
release, Stable update manifest, or Stable Update Centre entry must call this
gate with the compatibility run ID and proceed only after it succeeds.

No release or updater workflow may infer Stable compatibility from branch state,
a successful build alone, or an unverified JSON file. The retained
`stable-promotion-<run-id>` artifact is the machine-readable approval record.

## Boundaries

This gate approves compatibility evidence only. Artifact signing, checksums,
SBOMs, provenance, installer validation, and transactional updater promotion are
covered by their respective release and Update Centre work. Those systems must
consume this gate rather than duplicate or weaken it.
