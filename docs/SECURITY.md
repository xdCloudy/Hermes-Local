# Security

## Default posture

Hermes Local is a single-user, local workstation. It is not a multi-tenant
server and does not expose a supported LAN mode.

- llama.cpp listens only on `127.0.0.1:8011`;
- Hermes/dashboard listens only on `127.0.0.1:9119`;
- model inventory and inference require the generated bearer token;
- the token is encrypted per user with DPAPI and never stored in frontend
  state, Git, logs or process arguments;
- the stack runs without administrator privileges;
- Windows Defender, UAC, firewall and mitigations remain enabled;
- no broad antivirus exclusion is created.

## Electron controls

The packaged launcher explicitly enables context isolation, sandboxing and web
security; disables Node integration; uses a strict Content Security Policy;
permits only the exact local renderer navigation; denies new windows and
permissions by default; and exposes a narrow, typed preload bridge.

Media permission is allowed only for the owned trusted renderer window and
audio-only requests. Profile/action/path/size inputs are validated again in
the main process. Untrusted remote content is never loaded into a privileged
workstation WebView.

## Model and tool boundary

Model output is untrusted. Terminal operations show the local working
directory, support cancellation/timeouts/output limits and require approval
for dangerous commands. The configuration denies destructive commands aimed
at `D:\Hermes-Local`, disk formatting and diskpart by default. Memory and skill
writes require approval. Delegation is capped at one child and does not inherit
arbitrary MCP servers.

The browser tool applies loopback/private-address and redirect checks; file
and project operations canonicalise paths and enforce directory boundaries.
Renderer-supplied strings are not concatenated into shell commands.

## Final scan

The final full scan completed at `2026-07-28T09:53:33Z` against integration
commit `ee683263aaa7f3bca33f785630926350fa119c38`:

| Control | Result |
|---|---|
| npm production audit | 2 High, 0 Critical; React Router RSC-only advisory not reachable |
| npm full audit | 18 High, 0 Critical; remaining chain is build/lint tooling |
| pip-audit | 0 known vulnerabilities across 128 installed dependencies |
| OSV lockfiles | Three documented residual package families |
| Gitleaks production source | 0 findings |
| Semgrep | 133 candidates reviewed; 0 secret-rule candidates |
| Ruff | Pass |
| TypeScript | Pass |
| ESLint | 0 errors; 51 pre-existing warnings |
| Windows Defender distribution scan | Clean |
| CycloneDX SBOM | Node 616 components; Python 127 components |

Three residuals are accepted:

1. React Router RSC action behavior is not reachable in this client-only
   Electron SPA.
2. `brace-expansion` is transitive build/lint tooling with no fixed compatible
   release in the locked lines.
3. PyNaCl 1.5.0 appears only in the optional Discord voice lock graph, is not
   installed, and is capped below the fixed release by discord.py.

See `security\findings\ACCEPTED-RESIDUALS.md` for triggers that require a new
decision.

## Validated fixes

| ID | Severity | Fix |
|---|---:|---|
| HSL-SEC-001 | Medium | Main-process profile grammar validation prevents process-argument injection |
| HSL-SEC-002 | Medium | Defused XML parsing for untrusted user/network XML |
| HSL-SEC-003 | Medium | Fail-closed, owned-window, audio-only media permission policy |
| HSL-SEC-004 | High | Strict CSP plus exact navigation policy and packaged enforcement test |
| HSL-WIN-001 | Defense in depth | Native Git Bash resolution rejects the WSL launcher for inline skill commands |

Each code fix has focused regression coverage. The final Windows-critical
source gate passed 463 tests with zero failures.

## Run and inspect

```powershell
& 'D:\Hermes-Local\Security-Scan-Hermes-Local.ps1' -NonInteractive
```

Primary outputs:

- `D:\Hermes-Local\security\reports\SECURITY_REPORT.md`
- `D:\Hermes-Local\security\reports\latest-scan.json`
- `D:\Hermes-Local\security\threat-model\THREAT_MODEL.md`
- `D:\Hermes-Local\security\reports\HARDENING.md`
- `D:\Hermes-Local\security\sbom`

The scan redacts output before evidence is written. Do not attach raw
configuration, browser profiles or session databases to bug reports. Use
`Export-Hermes-Diagnostics.ps1`, whose archive excludes token values,
environment values, conversations and private files.

## Limitations

- Binaries are locally built and not Authenticode-signed; Windows may show a
  reputation warning.
- Full upstream Git history secret scanning exceeded the bounded scan window.
  The selected working tree, production source, root history and integration
  history were scanned.
- Optional external providers, messaging services, MCP servers and cloud
  tools were not authenticated or penetration-tested.
- Local authentication protects accidental/cross-process use on the host; it
  is not a substitute for OS account separation on a compromised machine.
