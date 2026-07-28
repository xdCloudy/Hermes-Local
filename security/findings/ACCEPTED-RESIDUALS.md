# Accepted security residuals

Reviewed: 2026-07-28

## React Router RSC action CSRF advisory

- Locked package: `react-router` / `react-router-dom` 7.18.1
- Advisory: GHSA-qwww-vcr4-c8h2
- Scanner severity: High
- Decision: accepted, not reachable in this build

The vulnerable behavior is React Server Components action handling. Hermes Launcher is a client-only Vite/Electron SPA and does not expose an RSC server or server actions. No fixed 7.x release is available; npm's proposed 7.11 downgrade predates the affected range but is a breaking, unsupported regression. Reassess if Desktop adopts RSC.

## brace-expansion denial of service

- Locked versions: 1.1.16, 2.1.2, 5.0.7
- Advisory: GHSA-mh99-v99m-4gvg
- Scanner severity: High
- Decision: accepted for build/development tooling

The package is reachable through ESLint/electron-builder dependency chains, not through the packaged runtime or local HTTP services. No fixed release exists in the affected major lines at review time; npm recommends breaking downgrades/upgrades that do not remove every affected chain. Production npm audit contains no brace-expansion finding.

## PyNaCl incomplete disallowed-input validation

- Locked version: 1.5.0
- Advisory: GHSA-mrfv-m5wm-5w6w / PYSEC-2026-3002
- Decision: accepted as absent optional functionality

This entry exists only in the `discord.py[voice]` optional lock graph. The installed workstation environment does not contain PyNaCl, and Discord/messaging is not enabled. `discord.py` 2.7.1 requires `PyNaCl>=1.5,<1.6`, while the fixed PyNaCl line is 1.6+. Do not enable Discord voice until that upstream constraint is updated or the feature is separately revalidated.

## Semgrep candidates

Semgrep community rules report 133 candidates after remediation: 93 dynamic urllib uses, 9 child-process calls, 8 SHA-1 uses, 8 loopback HTTP diagnostics, 4 intentional shell executions, 4 XML import false positives, 3 explicit `exec` paths, 2 pickle uses, one container-root warning, and one DOMPurify-sanitized SVG sink.

These are retained as review leads rather than relabelled as vulnerabilities:

- network tools are user-invoked capability and apply URL/time/size controls at their owning boundary;
- process calls use argument arrays or explicitly user-owned command templates/catalog bootstrap steps;
- SHA-1 sites implement external protocol signatures/identifiers, not password hashing;
- HTTP findings are loopback-only test/CDP helpers;
- remaining XML findings are construction/escaping imports or already use `defusedxml`;
- SVG HTML reaches the sink only after DOMPurify's SVG profile;
- the Dockerfile is not used by this Windows-native workstation.

Any newly enabled integration must be reviewed in its own trust context.
