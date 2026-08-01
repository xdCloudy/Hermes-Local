# Upstream compatibility CI

[← Documentation home](README.md) · [Development](DEVELOPMENT.md) ·
[Project home](../README.md)

Hermes Local tests candidate Hermes Agent and `llama.cpp` revisions before they
are treated as promotable updates. The workflow is defined in
`.github/workflows/upstream-compatibility.yml` and runs daily, on manual demand,
or through an `upstream-candidate` repository dispatch.

A focused pull-request smoke workflow also reconstructs the pinned Hermes Agent
integration and checks the current upstream patch application path. A genuine
candidate conflict is preserved as compatibility evidence; missing source
objects, a failed pinned reconstruction, or other infrastructure failures fail
the smoke workflow.

## Security boundary

Candidate upstream code runs only in isolated compatibility jobs with:

- read-only repository permissions;
- no repository, release, signing, or deployment secrets;
- checkout credentials disabled after clone;
- bounded job timeouts;
- separate reporting code checked out again before the workflow receives
  `issues: write` permission.

The optional CUDA gate is restricted to a self-hosted Windows runner carrying
the `hermes-gpu-gate` label. It must be a disposable or dedicated validation
machine, not a maintainer workstation or a production Hermes Local host.

## Hermes Agent gate

The gate:

1. resolves the requested branch, tag, or commit;
2. creates an isolated full-object clone and fetches both the recorded base and
   requested candidate revisions;
3. reconstructs the pinned integration once and verifies its exact recorded
   tree, creating intermediate patch objects required by later three-way merges;
4. checks out the candidate and applies every ordered patch from
   `source/hermes-launcher/patches` one at a time with `git am --3way`;
5. records each patch, application mode, failed patch, conflicting files, and
   skipped later patches;
6. installs locked Node and Python dependencies when requested, using the
   repository's exact supported npm CLI;
7. runs the documented Windows-critical Python selection;
8. runs Desktop type checking, linting, and the focused Electron control test;
9. builds the Desktop workspace;
10. runs the available Windows packaging script when requested;
11. emits a machine-readable report even when a stage fails.

The pinned reconstruction is important because later mail patches can reference
preimage blobs created by earlier integration patches. Those blobs are not part
of a clean upstream repository. Reconstructing and verifying the known-good
integration prevents missing-object errors from being misclassified as upstream
patch conflicts.

A hosted runner does not claim full runtime-health certification because it does
not launch the packaged workstation against a real local model. That limitation
is represented as `compatible-with-warnings`, not silently treated as a complete
release certification.

## llama.cpp gates

The hosted CPU gate installs the exact Python dependency used by llama.cpp's
Jinja comparison test, configures and builds with CUDA disabled, runs CTest,
locates `llama-cli` and `llama-server`, records their SHA-256 digests, and
executes binary `--version` smoke tests.

The optional trusted CUDA gate uses the same process with `GGML_CUDA=ON` and
requires `nvcc`. A licensed tiny-model API/authentication smoke test remains a
separate release requirement; its absence is an explicit warning.

## Report statuses

Reports use schema version 1 and distinguish:

- `compatible`;
- `compatible-with-warnings`;
- `blocked-patch-conflict`;
- `blocked-dependency`;
- `blocked-build`;
- `blocked-tests`;
- `blocked-packaging`;
- `blocked-runtime-health`;
- `infrastructure-failure`.

Infrastructure failures, such as a runner outage, candidate-resolution network
failure, or inability to reconstruct the recorded pinned integration, never
classify upstream as incompatible.

Each component report contains candidate and base commits, platform details,
per-stage state, artifact digests, warnings, failures, command tails, and the
workflow identity. The aggregate report is retained as the
`compatibility-aggregate` workflow artifact for 90 days.

## Rolling intervention issue

When the aggregate result is blocked or the infrastructure prevents a valid
result, the workflow creates or updates one issue containing the marker:

```text
<!-- hermes-local:compatibility-rollup:v1 -->
```

The same issue is refreshed on later failures and closed after a compatible or
compatible-with-warnings result. Duplicate compatibility issues are not created.

## Manual use

Run the report tooling locally from the repository root:

```powershell
python .\scripts\ci\compatibility.py hermes-agent `
  --repository-root . `
  --candidate-ref main `
  --work-dir .\temp\compatibility\hermes-agent `
  --log-dir .\artifacts\compatibility\hermes-agent\logs `
  --report .\artifacts\compatibility\hermes-agent\hermes-agent-report.json `
  --run-desktop-checks `
  --run-python-checks `
  --run-package-checks
```

Validate an aggregate report before a Stable promotion:

```powershell
python .\scripts\ci\compatibility.py verify `
  --report .\artifacts\compatibility\aggregate\compatibility-report.json `
  --require-component hermes-agent `
  --require-component llama-cpp-cpu
```

The command fails closed when the schema is invalid, a required component report
is absent, or the result is not `compatible` or `compatible-with-warnings`.
Release workflows should call this verifier before publishing a candidate as
Stable.

## Repository dispatch

An upstream-release watcher can request a run with the `upstream-candidate`
event and this payload:

```json
{
  "event_type": "upstream-candidate",
  "client_payload": {
    "hermes_agent_ref": "<branch-tag-or-commit>",
    "llama_cpp_ref": "<branch-tag-or-commit>",
    "run_gpu_gate": false
  }
}
```

The dispatch credential belongs to the watcher, not to candidate code, and must
have only the permissions required to dispatch this workflow.
