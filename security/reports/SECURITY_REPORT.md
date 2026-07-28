# Hermes Local security report

Assessment date: 2026-07-28  
Result: Pass with three documented, non-reachable/optional residual dependency advisories  
Primary evidence: `security\scans\latest.json`  
Threat model: `security\threat-model\THREAT_MODEL.md`  
Hardening record: `security\reports\HARDENING.md`

## Scope and method

The review covered the pinned upstream Hermes revision, local launcher integration, Electron main/preload/renderer, PTY and process management, local HTTP/WebSocket services, authentication, configuration, update/rollback, downloads and archives, previews and projects, logging, plugins/MCP, terminal tools, cron, delegation, packaging, and distribution artifacts.

Evidence combined:

- manual trust-boundary and source-to-sink review;
- npm audit, pip-audit, OSV-Scanner, Semgrep community security/secrets rules;
- Gitleaks production working-tree and recent integration-history scans;
- Ruff, TypeScript compiler, ESLint, focused Python/Electron regression tests;
- CycloneDX SBOM and dependency-license inventory;
- Windows Defender custom archive-aware scan of `dist`;
- packaged Electron Playwright and real local-agent/tool acceptance tests.

The Codex Security connector was attempted but its workspace helper referenced a removed plugin version and could not reopen the scan. No connector result is claimed. The complete manual/repeatable evidence set above is the authoritative assessment.

## Scanner results

| Control | Result |
|---|---|
| npm production audit | 2 High, 0 Critical: React Router RSC-only advisory, accepted as unreachable |
| npm full audit | 18 High, 0 Critical: production RSC advisory plus build/lint-only chains |
| pip-audit, installed Python environment | 0 known vulnerabilities across 128 installed dependencies |
| OSV lockfiles | brace-expansion build-only; React Router RSC-only; optional/uninstalled PyNaCl |
| Semgrep | 133 candidates reviewed; 0 secret-rule candidates; 4 validated issue classes fixed |
| Gitleaks production source | 0 findings after documented test/docs/reference exclusions and public-ID false-positive rules |
| Gitleaks root integration history | 0 findings |
| Ruff | Pass |
| TypeScript | Pass |
| ESLint | 0 errors, 51 pre-existing warnings |
| Windows Defender `dist` scan | No threats |
| CycloneDX SBOM | Node 616 components; Python 127 components; spec 1.6 |

The unfiltered Gitleaks working-tree run produced 839 candidates: 621 in two generated reference corpora, 172 synthetic tests, 36 documentation examples, 3 vendored headers, and 7 public OAuth/voice identifiers or the redactor's private-key pattern. The scoped production scan documents and suppresses only those categories; it is clean.

## Validated findings and remediation

| ID | Severity | Finding | Resolution |
|---|---:|---|---|
| HSL-SEC-001 | Medium | Renderer profile input reached backend process creation without main-process grammar validation | Added shared validator and injection/path tests |
| HSL-SEC-002 | Medium | User/network XML reached standard ElementTree parsing | Added pinned `defusedxml` and entity-expansion regression tests |
| HSL-SEC-003 | Medium | Audio permission was not scoped to owned trusted renderer content | Fail-closed origin/window/media decision with tests |
| HSL-SEC-004 | High | No renderer CSP; broad navigation prefix/file allowlist | Exact navigation policy, strict CSP, external bootstrap, packaged enforcement test |

Finding write-ups are in `security\findings`.

## Electron control verification

| Required control | State |
|---|---|
| Context isolation | Explicitly enabled |
| Node integration | Explicitly disabled |
| Sandbox | Explicitly enabled |
| Web security | Explicitly enabled |
| Remote module | Not used; unavailable in current Electron |
| CSP | Same-origin scripts; inline/eval blocked; object/base/form restrictions |
| Navigation allowlist | Exact dev origin or exact packaged renderer URL |
| New windows | Denied by default; validated external opening |
| Permissions | Denied by default; narrow audio-only trusted-renderer exception |
| Preload API | Context bridge only; no general Node/Electron exposure |
| IPC validation | Profiles, actions, paths, sizes, and log reads bounded/validated |
| Renderer secrets | No API token in workstation snapshots or normal renderer state |
| Remote privileged content | OAuth uses isolated session windows; previews do not receive owned-window permission |

## Local-service verification

- Ports 8011 and 9119 listen only on loopback.
- Model inference without the Bearer token returns 401/403.
- Hermes uses a generated session token and constant-time comparison.
- WebSocket upgrades repeat peer, Host, Origin, and authentication checks before accept.
- Safe CORS/host defaults are preserved.
- File reads and project operations canonicalise paths and enforce directory boundaries.
- No token appears in command-line arguments; the persistent token is DPAPI-protected.
- Logs pass through redaction before UI/diagnostic export.

## Residual decisions

Three dependency advisories remain accepted:

1. React Router RSC CSRF behavior is absent from this client-only application.
2. brace-expansion is present only in build/lint tooling and has no fixed compatible release.
3. PyNaCl 1.5 exists only in the optional Discord voice lock graph, is not installed, and is constrained below the fixed line by `discord.py`.

Full rationale and re-review triggers are in `security\findings\ACCEPTED-RESIDUALS.md`.

## Limitations

- The local build is not Authenticode-signed.
- Full upstream Git history secret scanning exceeded the bounded scan window; the selected working tree, production source, root history, and integration commit history were scanned.
- External providers, messaging adapters, cloud sandboxes, and arbitrary MCP servers are opt-in and were not authenticated/configured for this local acceptance run.
- Scanner warnings are not treated as vulnerabilities without a reachable source, sink, trust boundary, and realistic impact.

## Release decision

The default loopback-only Windows workstation is suitable for local use. Release remains conditional on preserving the three residual feature exclusions, keeping Maximum 128K marked experimental, and rerunning the scan after dependency, IPC, navigation, update, terminal, or integration changes.
