# Hermes Local documentation

[← Project home](../README.md) · [Install](INSTALLATION.md) ·
[User guide](USER_GUIDE.md) · [Troubleshoot](TROUBLESHOOTING.md) ·
[Contribute](../CONTRIBUTING.md)

Welcome to the technical documentation for Hermes Local. The
[project README](../README.md) is the product overview; this directory contains
the operational detail, design contracts and validation evidence.

## Choose your path

| I want to… | Start here | Then read |
|---|---|---|
| Install Hermes Local | [Installation](INSTALLATION.md) | [User guide](USER_GUIDE.md) |
| Add or tune a model | [Models and profiles](MODEL_TUNING.md) | [Benchmarks and feature coverage](FREE_FEATURE_MATRIX.md) |
| Operate or maintain a workstation | [Operations](OPERATIONS.md) | [Update and rollback](UPDATE_AND_ROLLBACK.md) |
| Diagnose a problem | [Troubleshooting](TROUBLESHOOTING.md) | [Security and safe diagnostics](SECURITY.md) |
| Understand the system | [Architecture](ARCHITECTURE.md) | [Launcher design](design/DESIGN_SPEC.md) |
| Contribute code or docs | [Contributor guide](../CONTRIBUTING.md) | [Development](DEVELOPMENT.md) |
| Validate upstream or Stable candidates | [Upstream compatibility](UPSTREAM_COMPATIBILITY.md) | [Stable promotion](STABLE_PROMOTION.md) |
| Evaluate maturity | [Acceptance results](ACCEPTANCE_RESULTS.md) | [QA reports](../reports/qa/FULL_FUNCTIONAL_QA_REPORT.md) |
| Follow future work | [Live roadmap](../README.md#roadmap) | [Project views](PROJECT-VIEWS.md) |

## Product and operations

- [Installation](INSTALLATION.md) — requirements, acceleration selection,
  source setup and release packages.
- [User guide](USER_GUIDE.md) — launcher navigation and day-to-day workflows.
- [Models and profiles](MODEL_TUNING.md) — GGUF registration, context, cache,
  offload and tuning.
- [Operations](OPERATIONS.md) — lifecycle commands, runtime layout, backups,
  diagnostics and QA.
- [Update and rollback](UPDATE_AND_ROLLBACK.md) — transactional maintenance and
  recovery.
- [Troubleshooting](TROUBLESHOOTING.md) — symptoms, diagnostics and corrective
  actions.
- [Screenshot catalog](SCREENSHOTS.md) — approved product imagery and capture
  gaps.

## Engineering and governance

- [Architecture](ARCHITECTURE.md) — components, process ownership, data flow and
  trust boundaries.
- [Task lifecycle and resource locks](decisions/0001-task-lifecycle-and-resource-locks.md)
  — authoritative task schema, state machine and admission policy.
- [Development](DEVELOPMENT.md) — source reconstruction, testing and packaging.
- [Security](SECURITY.md) — local threat model, controls and reporting.
- [Upstream compatibility](UPSTREAM_COMPATIBILITY.md) — scheduled candidate
  validation, report schema and intervention handling.
- [Stable promotion](STABLE_PROMOTION.md) — mandatory trusted GPU evidence and
  fail-closed Stable compatibility approval.
- [Free/local feature matrix](FREE_FEATURE_MATRIX.md) — present capabilities
  and boundaries.
- [Project views](PROJECT-VIEWS.md) — maintainer planning conventions.
- [Acceptance results](ACCEPTANCE_RESULTS.md) — current and historical evidence.
- [Launcher design specification](design/DESIGN_SPEC.md) — interface system and
  product behavior.

## Documentation conventions

- The README owns product positioning and the shortest successful install path.
- These pages own technical explanations; link instead of copying long sections.
- Commands resolve paths from the project root and target PowerShell 7.
- Historical hardware results must be labelled as evidence, not requirements.
- Every page links back to this index and the project home.
- Behavior changes update the relevant guide in the same pull request.

Found a gap? Open a
[documentation issue](https://github.com/xdCloudy/Hermes-Local/issues/new?template=feature_request.yml)
or submit a focused pull request.
