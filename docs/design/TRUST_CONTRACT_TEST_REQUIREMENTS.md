# Trust contract threat-test requirements

[← Trust contract ADR](TRUST_CONTRACTS.md) · [Security](../SECURITY.md)

These tests are mandatory implementation gates for issues #18, #19, #17 and
the remote-origin phase of #29. Unit tests cover policy logic, integration tests
cross the real native boundary, and security tests include malformed and
adversarial input.

### #18 Skills and MCP Trust Centre

| ID | Required test |
|---|---|
| `TRUST-18-01` | Unknown and undeclared capabilities are denied before process launch or confirmation. |
| `TRUST-18-02` | Renderer-supplied trust state, allow result, principal, scope or manifest revision cannot change the native decision. |
| `TRUST-18-03` | Source/revision/hash or capability expansion suspends prior allow grants. |
| `TRUST-18-04` | Delegated agents and scheduled tasks receive no parent credentials or grants unless explicitly issued. |
| `TRUST-18-05` | Path, host, account and command constraints are checked after canonicalization. |
| `TRUST-18-06` | Revocation stops new calls and a running managed process loses brokered credential access immediately. |
| `TRUST-18-07` | Audit and diagnostics contain provenance and decision metadata but no arguments, prompts, environment secrets or credential values. |
| `TRUST-18-08` | Health state changes do not change trust state or grant permissions. |

### #19 Permission-scoped indexing and RAG

| ID | Required test |
|---|---|
| `TRUST-19-01` | A project grant cannot read, retrieve or cite a second project's collection. |
| `TRUST-19-02` | Canonical path, symlink, junction and reparse-point escapes outside approved roots are denied. |
| `TRUST-19-03` | Retrieved instructions cannot create grants, change project identity, bypass confirmation or invoke tools. |
| `TRUST-19-04` | Remote, delegated and integration principals cannot query a collection without their own `index.read` grant. |
| `TRUST-19-05` | External embedding is denied without explicit host/account scope and a user-visible disclosure decision. |
| `TRUST-19-06` | Deleting an index removes only index data and does not obtain `filesystem.delete` authority over sources. |
| `TRUST-19-07` | Source citations use the canonical project ID and stay within the approved root after revalidation. |
| `TRUST-19-08` | Index/audit diagnostics redact excluded secret files and extracted content bodies. |

### #17 Remote gateway and device pairing

| ID | Required test |
|---|---|
| `TRUST-17-01` | Pairing creates a unique device identity and credential that cannot authenticate another device. |
| `TRUST-17-02` | Expired pairing codes, stale grant revisions and revoked devices fail closed for HTTP and WebSocket requests. |
| `TRUST-17-03` | Local emergency disable prevents new sessions, closes existing streams and revokes all remote sessions before firewall cleanup completes. |
| `TRUST-17-04` | Remote clients never receive or proxy the local inference/dashboard bearer token. |
| `TRUST-17-05` | Non-loopback access requires TLS, exact host/origin validation, authenticated WebSockets and safe forwarded-header policy. |
| `TRUST-17-06` | High-risk operations require the configured local/remote confirmation and cannot be downgraded by the client. |
| `TRUST-17-07` | Project, profile and agent scopes remain isolated across concurrent device sessions. |
| `TRUST-17-08` | Pairing, failures, permission changes, confirmations, administrative actions and revocation are audited without prompts or secrets. |

### Remote-origin phase of #29

| ID | Required test |
|---|---|
| `TRUST-29-01` | A remote origin cannot be enabled without an active #17 gateway identity, HTTPS and a non-Electron credential mode. |
| `TRUST-29-02` | Scheme, host or port changes invalidate the old origin record, embedded session and credential binding. |
| `TRUST-29-03` | Wildcards, subdomains, redirects and renderer-provided origin values cannot expand the exact native allowlist. |
| `TRUST-29-04` | Authentication material is injected only at the trusted network boundary and is absent from URL, DOM, renderer state and logs. |
| `TRUST-29-05` | External navigation opens outside the privileged surface; cross-origin frames, downloads, permissions and new windows remain denied. |
| `TRUST-29-06` | Remote page content cannot invoke native operations without a fresh capability decision for the authenticated remote principal. |
