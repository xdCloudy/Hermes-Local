# Skills and MCP Trust Centre

Hermes Local treats integrations as executable security principals, not as a list of friendly names. The Trust Centre implements the shared trust contracts defined in `docs/design/TRUST_CONTRACTS.md` and makes those contracts enforceable for managed MCP integrations while exposing source-bound skill inventory.

## Security model

The native/local trust store is authoritative. Renderer state, MCP-server annotations, tool descriptions, remote content and model output cannot grant permissions, change principals, change scope or promote an integration to a stronger trust state.

Every managed MCP integration has an identity bound to:

- integration type and stable local id;
- source/provenance type;
- source revision or configuration fingerprint;
- SHA-256 configuration identity that excludes secret values;
- declared capabilities;
- trust and health state;
- manifest revision and, after approval, the approved manifest SHA-256.

A source, configuration or declared-capability change increments the manifest revision, returns the integration to `restricted`, clears the approved manifest hash and revokes prior grants. Unknown capability names make the identity `unknown` and fail closed.

## Capability vocabulary

Trust Centre uses the canonical #38 vocabulary. It does not create integration-specific aliases:

- `filesystem.read`, `filesystem.write`, `filesystem.delete`
- `process.execute`
- `network.outbound`, `network.listen`
- `browser.control`
- `clipboard.read`, `clipboard.write`
- `media.microphone`, `media.camera`
- `credentials.use`, `credentials.manage`
- `communications.read`, `communications.send`
- `calendar.read`, `calendar.write`
- `external.side-effect`
- `runtime.read`, `runtime.manage`
- `project.read`, `project.write`
- `index.read`, `index.write`
- `audit.read`
- `remote.connect`, `remote.admin`
- `embedded.render`, `embedded.navigate`

Local stdio MCP servers intrinsically declare `process.execute`. Remote MCP servers intrinsically declare `network.outbound` and `remote.connect`. MCP integrations also declare `external.side-effect`, and integrations configured with environment/header/auth material declare `credentials.use`. Additional capabilities must be explicitly declared under the managed trust metadata and must be recognized by the canonical vocabulary.

Server-provided annotations such as MCP `readOnlyHint` are advisory metadata only. They never grant authority.

## Trust states

The Trust Centre displays the shared states without remapping them:

- `built-in-verified`
- `reviewed-managed`
- `user-trusted`
- `restricted`
- `quarantined`
- `disabled`
- `unknown`

New MCP integrations migrate into `restricted`, not trusted. Bundled skills are inventoried as `built-in-verified` using their local content hash. User-installed skills are inventoried as `restricted`. Skill records are source visibility today; executable capability enforcement remains at their underlying tool/runtime boundary rather than allowing the renderer to manufacture a skill grant.

Health is separate from trust. A `healthy` integration does not receive permissions, and health recovery cannot restore a revoked grant.

## Authorization and confirmation

Authorization is reconstructed before startup and before every MCP tool call.

For startup, Hermes Local requires the exact transport grant before it starts a stdio child or connects to a remote MCP endpoint. Credential-bearing configurations additionally require `credentials.use`. If the trust store is missing, corrupt or unavailable, MCP startup fails closed.

For invocation, the integration identity, trust state, principal, scope, active grants and confirmation policy are read again immediately before the call. Explicit deny wins. No matching allow grant means deny.

Confirmation is separate from permission. Supported policies are:

- `never`
- `always`
- `writes-or-side-effects`
- `local-only`

The Trust Centre uses Hermes' existing human approval gate for confirmation-required MCP calls. Non-interactive contexts therefore retain the approval subsystem's fail-closed behavior for a required tool approval.

## Scope and delegation

Grants are bound to a principal and one canonical scope. Supported scope kinds are `global`, `project`, `session`, `profile`, `agent` and `user`.

Project/session/profile scope is reconstructed from local execution context. Scoped ids are canonicalized before matching. A delegated child is treated as a different agent principal and does **not** inherit the main agent's integration grants merely because it runs inside the same Hermes process.

This is intentional: delegation is not a permission-escalation mechanism.

## Process isolation

MCP stdio startup keeps the upstream Hermes process controls and adds the Trust Centre gate before spawn:

- suspicious command/exfiltration preflight remains active;
- only the bounded safe operating-system environment plus explicitly configured server environment is provided by the existing MCP transport implementation;
- the Trust Centre helper itself receives an explicit minimal environment and does not inherit API keys or unrelated Hermes secrets;
- connection timeouts, reconnect limits, circuit breakers and process-tree cleanup remain enforced by the MCP runtime;
- disabling or revoking an integration prevents new startup and new tool calls immediately.

The Trust Centre does not claim that an arbitrary third-party MCP executable is a sandbox. Capability grants describe what Hermes will broker or permit, not what an untrusted native executable could theoretically do outside an OS sandbox. High-risk integrations should remain restricted or be run in an external sandbox when stronger containment is required.

## Audit and diagnostics

The local audit records:

- integration install/update/remove transitions;
- grant changes and revocations;
- authorization allows and denials;
- confirmation requirement state;
- integration id, capability, operation and scope-safe metadata.

Audit and diagnostic exports deliberately omit:

- MCP argument values;
- environment values;
- HTTP header values;
- prompts and response bodies;
- request/response payloads;
- credential/token values.

Configuration identity records argument **count**, environment/header **key names**, sanitized remote endpoint identity and SHA-256 fingerprints instead of secret-bearing values.

Use **Trust Centre → Export diagnostics** to generate `data/trust/TRUST-DIAGNOSTICS.json` and reveal it in Explorer.

## Desktop workflow

Open `/trust` from the workstation sidebar. The view exposes:

1. managed skill/MCP inventory;
2. source, revision, manifest hash and approved hash;
3. trust and health state;
4. declared capabilities and unknown-capability alerts;
5. exact capability grants;
6. confirmation policy;
7. global or scoped grant target;
8. immediate disable;
9. recent redacted trust audit and event counts;
10. redacted diagnostics export.

The renderer can request a policy change only with an integration id, declared capability subset, mutable trust state, confirmation mode and bounded scope. The native bridge validates those fields, and the Python trust authority independently reconstructs the integration identity and principal before writing policy.

## Migration behavior

Existing configured MCP servers appear as `restricted` on first Trust Centre inventory. They do not silently inherit trust from pre-Trust-Centre configuration. Review their source/revision and capabilities, choose the smallest useful scope, then approve only the capabilities they require.

Changing command arguments, endpoint identity, auth shape, environment/header key set, declared capabilities or declared source revision changes the configuration fingerprint and suspends prior approval. This is the intended review boundary.
