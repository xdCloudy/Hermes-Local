# ADR 0002: Trust, capability, credential and audit contracts

[← Documentation index](../README.md) · [Architecture](../ARCHITECTURE.md) ·
[Security](../SECURITY.md)

- Status: Accepted
- Date: 2026-08-04
- Decision issue: [#38](https://github.com/xdCloudy/Hermes-Local/issues/38)
- Programme: [#37](https://github.com/xdCloudy/Hermes-Local/issues/37)
- Dependent implementation: [#18](https://github.com/xdCloudy/Hermes-Local/issues/18),
  [#19](https://github.com/xdCloudy/Hermes-Local/issues/19),
  [#17](https://github.com/xdCloudy/Hermes-Local/issues/17), and the remote-origin
  phase of [#29](https://github.com/xdCloudy/Hermes-Local/issues/29)
- Canonical schema: [`config/schemas/trust-contracts.schema.json`](../../config/schemas/trust-contracts.schema.json)

## Context

Hermes Local currently has strong local boundaries: internal services bind to
loopback, Electron renderer inputs are revalidated by the main process, model
and dashboard credentials stay outside normal renderer state, and embedded
content is restricted to the configured loopback origin. Planned Skills, MCP,
workspace indexing and remote access features add principals and data flows
that cannot safely be governed by one global trusted/untrusted flag.

The shared model must distinguish four facts that are often conflated:

1. **Identity and provenance:** what integration, device or component is this,
   and which exact source revision is running?
2. **Capability declaration:** what classes of effect can it request?
3. **Permission:** which principal may exercise a declared capability, in which
   scope, against which resources and under which confirmation policy?
4. **Operational evidence:** what happened, was it healthy, and which policy
   decision was applied?

A successful health check is not a publisher endorsement. Installing an
integration is not a permission grant. A trusted publisher does not grant every
agent access. A parent agent's permissions do not automatically transfer to a
delegated agent, remote client or scheduled task.

## Decision

### 1. One canonical, versioned contract

Hermes Local adopts JSON Schema Draft 2020-12 as the language-neutral source of
truth for trust records. Version 1 is defined in
`config/schemas/trust-contracts.schema.json` as a strict discriminated union of:

- `integrationIdentity`;
- `capabilityGrant`;
- `credentialReference`;
- `auditEvent`;
- `trustedOrigin`;
- `sessionAuthorization`; and
- `redactionPolicy`.

Every object rejects unknown properties. Every record carries `schemaVersion`
and a stable ID. Desktop and backend implementations must validate records at
the native authority boundary before persistence or use. TypeScript and Python
types may be generated from the schema, but generated types are not a substitute
for runtime validation.

Schema additions follow these rules:

- adding a capability requires a reviewed schema change and threat tests;
- old validators encountering the new capability deny it;
- changing the meaning of an existing field requires a new schema version; and
- persisted records that fail validation are quarantined, not partially loaded.

### 2. Native authority and request reconstruction

Renderer, embedded page, model, retrieved document, skill, MCP server and remote
client values are untrusted input. They may describe an intended action, but
they cannot author an authorization result.

The Electron main process, Hermes backend or remote gateway must reconstruct the
authorization request from trusted state:

- authenticated principal and session;
- canonical integration identity and manifest revision;
- canonical project/profile/agent IDs;
- canonicalized filesystem, host, account, command and collection targets;
- current grant and policy revisions; and
- the requested capability inferred from the native operation being invoked.

A renderer-provided `allowed`, trust state, principal, grant, scope, project ID,
credential handle, origin class or policy revision is ignored and rejected if
it appears in a contract where it is not defined. The renderer receives a
presentation-safe decision and reason, never authority that can be replayed as
a grant.

### 3. Integration identity and provenance

Each managed built-in tool, skill, MCP server, bridge, executable and remote
client has an `integrationIdentity` record. Approval binds to the normalized
manifest and exact provenance tuple, not only a display name or download URL.

Required identity evidence includes:

- stable integration ID and type;
- source type, source/repository URI where applicable, exact revision and a
  stable provenance ID;
- executable or normalized manifest hash when available;
- declared capability list;
- manifest revision;
- trust state; and
- health state and freshness timestamp where available.

Trust and health are separate. `healthy` means the bounded health check passed;
it does not imply `reviewed-managed` or grant a capability. A source, revision,
hash or declared-capability change creates a new manifest revision. Existing
allow grants are suspended until the change is reviewed when provenance changes
or capabilities expand.

Trust states are:

- `built-in-verified`;
- `reviewed-managed`;
- `user-trusted`;
- `restricted`;
- `quarantined`;
- `disabled`; and
- `unknown`.

`quarantined`, `disabled` and `unknown` identities cannot receive an effective
allow decision. `restricted` identities require an explicit grant and the
configured confirmation policy.

### 4. Capability vocabulary and default deny

Version 1 uses a closed capability vocabulary:

| Area | Capability IDs |
|---|---|
| Files and processes | `filesystem.read`, `filesystem.write`, `filesystem.delete`, `process.execute` |
| Network and browser | `network.outbound`, `network.listen`, `browser.control` |
| Local device surfaces | `clipboard.read`, `clipboard.write`, `media.microphone`, `media.camera` |
| Secrets and accounts | `credentials.use`, `credentials.manage` |
| Communications | `communications.read`, `communications.send`, `calendar.read`, `calendar.write` |
| External effects | `external.side-effect` |
| Runtime and projects | `runtime.read`, `runtime.manage`, `project.read`, `project.write` |
| Indexing and audit | `index.read`, `index.write`, `audit.read` |
| Remote and embedded | `remote.connect`, `remote.admin`, `embedded.render`, `embedded.navigate` |

Capability declaration is necessary but insufficient. An integration can request
only a declared capability, and a principal can exercise it only through a
matching effective grant. Unknown, malformed or undeclared capabilities are
denied before confirmation or tool execution.

Broad aliases such as `admin`, `full-access`, `trusted`, `network` or `files`
are not capabilities. Implementations map each native operation to the narrowest
applicable capability and may require more than one capability for a compound
action.

### 5. Grants, scopes and authorization order

A `capabilityGrant` binds one principal to one capability, one scope, resource
constraints, an effect and a confirmation policy. Principals include users,
profiles, agents, integrations, remote clients, sessions and system-owned
components. Scopes are `global`, `user`, `profile`, `agent`, `project` or
`session`.

`global` is reserved for reviewed built-in policy and explicit administrative
configuration. User-created grants default to the narrowest applicable scope.
Project data, project indexes and project credentials require a `project` scope
with the canonical project ID. A project-scoped allow never falls back to a
global or similarly named project, and a caller cannot substitute a renderer
label for the canonical ID.

Resource constraints may restrict paths, hosts, accounts, command families,
collection IDs and integration IDs. Native code resolves paths, symlinks and
Windows reparse points before matching. Host matching uses normalized hostnames
and resolved connection policy; string suffix matching is insufficient.

Authorization uses this deterministic order:

1. Validate and reconstruct the request at the native authority boundary.
2. Deny unknown or undeclared capabilities.
3. Deny disabled, quarantined, unknown or stale integration identities.
4. Deny expired, revoked or stale sessions.
5. Select grants matching principal, capability, exact scope and every resource
   constraint.
6. Apply explicit deny grants before allow grants.
7. If no allow remains, deny.
8. Apply the strongest matching confirmation requirement.
9. Re-check revisions immediately before the side effect.
10. Record a redacted audit event for the decision and result.

Delegation creates a new principal and session. It receives only grants issued
to that delegated principal or explicitly marked for the run; it does not
inherit the parent agent's grants or credentials by default. The same rule
applies to scheduled work and remote sessions.

### 6. Credential boundaries

Trust records store only `credentialReference` metadata. Secret values, bearer
tokens, cookies, authorization headers, refresh tokens and decrypted material
are not valid schema fields.

Credential requirements are:

- storage is per-user DPAPI, Windows Credential Manager, an OS keyring or an
  explicit external broker;
- each reference has a narrow scope and allowlist of integration IDs;
- the renderer receives redacted metadata, not a dereferenceable handle;
- native code resolves the reference only after a fresh `credentials.use`
  authorization decision;
- integrations receive only the required secret through the narrowest supported
  channel, preferably a brokered call or exact request header;
- environment injection is exceptional and passes only the selected variable;
- credentials are never inherited by child processes, delegated agents or MCP
  servers without an explicit grant;
- revocation and rotation increment the credential revision and invalidate
  dependent sessions; and
- diagnostic export never includes the value or a reversible representation.

The local inference token remains a separate internal credential. It is not a
general-purpose integration credential and is never issued to remote clients.

### 7. Audit, redaction and retention

Security-relevant state changes and authorization decisions produce an
`auditEvent`. The event records identity and scope metadata, outcome, reason,
correlation ID and whether redaction was applied. It does not store full prompts,
retrieved document bodies, request/response bodies, cookies, authorization
headers or credential values.

Audit detail keys are normalized to lower case before validation. Sensitive key
families such as `authorization`, `cookie`, `credential`, `password`, `prompt`,
`secret`, `token`, `body` and `content` are rejected. Arbitrary nested payloads
are not accepted; implementations store bounded scalar metadata or short string
lists.

The baseline `redactionPolicy` is:

- prompt bodies, credential values and authorization headers are never recorded;
- task output remains bounded to 128 KiB after redaction;
- audit metadata is retained for 180 days;
- task output is retained for 30 days; and
- generated diagnostics are retained for 14 days.

A user may shorten retention or purge records. A purge request is itself audited
before deletion, unless the audit store is being securely reset. Security
failures must not fall back to raw logging when redaction fails; the event is
reduced to fixed safe metadata or omitted with a local health alert.

### 8. Revocation and stale sessions

Every policy, grant, credential, identity manifest and trusted origin has a
monotonic revision. A `sessionAuthorization` records the grant and policy
revisions used to create it, its capability snapshot, scope, expiry and optional
origin/credential bindings.

A session is stale and denied when any of the following is true:

- idle or absolute expiry has passed;
- `revokedAt` is set;
- current grant or policy revision differs;
- a referenced grant, credential, integration or origin is revoked, disabled or
  changed;
- the canonical project no longer exists or is no longer accessible;
- a remote device credential is rotated or removed; or
- remote access is disabled locally.

Long-lived HTTP, streaming and WebSocket operations re-check revocation at
bounded intervals and before every side effect. Revision changes close remote
WebSockets and require re-authentication. The emergency-disable action increments
the remote policy revision, revokes all remote sessions and prevents new
sessions before network cleanup begins.

### 9. Trusted origins and untrusted content

A `trustedOrigin` is an exact scheme/host/port tuple. Wildcard ports, wildcard
hosts and implicit subdomain trust are forbidden.

For the existing embedded dashboard:

- only the active configured loopback origin is enabled;
- Electron-owned request-header injection is allowed only for that exact origin;
- same-origin navigation is allowed according to the origin record;
- external links open through the system browser;
- cross-origin requests, permissions, downloads and native window creation are
  denied; and
- a configuration or credential change destroys the old embedded view and
  invalidates its session.

Remote origins remain disabled until #17 provides a separately authenticated
gateway. Remote origins require HTTPS plus secure-cookie or proof-of-possession
credentials; Electron's local request-header mode is not valid for them. Network
location, LAN membership, a reverse proxy header or a successful page load does
not establish identity or permission.

Embedded pages, browser content and locally retrieved/indexed text are data, not
policy. Their content cannot alter system instructions, grants, confirmations,
project identity, origin policy or tool permissions.

## Migration rules

Migration to schema version 1 is idempotent and fail closed.

### Built-in components

- Generate `built-in-verified` identity records bound to the packaged component
  revision and normalized manifest hash.
- Create only the minimum baseline grants required to preserve current local
  behavior.
- Scope baseline grants to the local user and Desktop/backend system principals;
  do not create remote, delegated-agent or scheduled-task grants.

### Existing skills, MCP servers and user-added executables

- Inventory each integration and record the best available source, revision and
  hash without treating discovery as approval.
- Integrations with complete unchanged provenance may be imported as
  `restricted`; missing or conflicting provenance imports as `unknown` and is
  disabled.
- Preserve an existing enabled state only through a temporary compatibility
  grant bound to the exact integration revision, current user and main agent.
- Compatibility grants include only declared or safely inferred capabilities,
  exclude `credentials.manage`, `runtime.manage`, `remote.admin`,
  `network.listen`, `filesystem.delete` and other unresolved high-risk effects,
  and expire after review or 30 days.
- Capability expansion, source changes or hash changes suspend compatibility
  grants and require approval.
- Delegated, project-wide, remote and unattended access is never inferred from
  an existing main-agent configuration.

### Credentials

- Move existing secrets into the selected protected store and persist only a
  `credentialReference` after a successful round-trip verification.
- Do not delete the old protected value until the new reference is verified.
- Plaintext values found in tracked configuration, logs or manifests are not
  migrated; the integration is disabled and the user is directed to rotate the
  credential.

### Projects, indexes and sessions

- Resolve stored project names/paths to canonical project IDs before creating
  project grants. Ambiguous projects receive no grant.
- Existing indexes are not exposed until their root and project identity are
  revalidated.
- All pre-contract remote, embedded, agent-run and scheduled sessions are
  invalidated. Local Desktop may create a fresh session after policy loading.

### Failure and evidence

- Write a backup of the pre-migration trust-related configuration.
- Validate every generated record before atomically publishing the new store.
- Invalid records are quarantined individually; migration does not weaken the
  default policy to keep an integration working.
- Record a redacted `migration.completed` event with counts of imported,
  restricted, disabled and failed records.

## Threat-focused verification requirements

Dependent implementations must pass the issue-specific security gates in
[Trust contract threat-test requirements](TRUST_CONTRACT_TEST_REQUIREMENTS.md).
Those gates cover origin enforcement, default-deny capability handling, native
boundary reconstruction, project isolation, credential handling, revocation and
redacted audit evidence for #18, #19, #17 and the remote phase of #29.

## Consequences

- Dependent features share one vocabulary and decision order instead of creating
  separate permission models.
- Existing integrations may require review after migration; preserving access
  is secondary to avoiding silent authority expansion.
- Schema validation catches malformed and forged records, while native policy
  code remains responsible for canonical identity, path/origin resolution,
  cross-record revision checks and authorization decisions.
- New capabilities and remote exposure require explicit schema, threat-model and
  regression-test changes.

## Verification for this decision

`tests/test_issue38_trust_contracts.py` validates the canonical schema and
covers unknown-capability denial, strict renderer-boundary fields, project-scope
identity, secret-free credential references, audit redaction constraints,
remote-origin TLS/credential rules and unknown record rejection.
