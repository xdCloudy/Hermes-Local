# Security policy

## Reporting a vulnerability

Please do not disclose suspected vulnerabilities in a public issue.

Use GitHub's **Security → Report a vulnerability** flow for this repository.
Include the affected commit, reachable attack path, reproduction steps,
expected impact and any proposed mitigation. Do not include live credentials,
tokens, conversations or private files.

## Supported version

Security fixes target the latest tagged release and the default branch. Older
local snapshots may receive a documented workaround but are not maintained as
separate supported release lines.

## Security boundaries

Hermes Local treats the Electron renderer, Electron main process, Hermes
backend, model server, terminal tools and local filesystem as separate trust
boundaries. Its supported default is Windows-native, per-user, authenticated
and loopback-only.

See [the security architecture](docs/SECURITY.md), [threat
model](security/threat-model/THREAT_MODEL.md), and [latest reviewed
assessment](security/reports/SECURITY_REPORT.md).
