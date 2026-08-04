# ADR 0003: Runtime adapter, capability, optimisation and execution identity contracts

[← Documentation index](../README.md) · [Architecture](../ARCHITECTURE.md) ·
[Models and profiles](../MODEL_TUNING.md)

- Status: Accepted
- Date: 2026-08-04
- Decision issue: [#39](https://github.com/xdCloudy/Hermes-Local/issues/39)
- Programme: [#4](https://github.com/xdCloudy/Hermes-Local/issues/4)
- Package lifecycle dependency: [#6](https://github.com/xdCloudy/Hermes-Local/issues/6)
- Update lifecycle dependency: [#8](https://github.com/xdCloudy/Hermes-Local/issues/8)
- Canonical schema: [`config/schemas/runtime-contracts.schema.json`](../../config/schemas/runtime-contracts.schema.json)

## Context

Hermes Local currently owns one native inference path: a managed `llama.cpp`
server configured from a model manifest and an inference profile. That path is
useful and must remain stable, but its settings, process construction, package
identity, health checks and optimisation flags are coupled to one backend.

The Inference Fabric programme introduces additional owned runtimes and generic
external OpenAI-compatible endpoints. A backend-neutral product layer cannot
safely achieve that by adding an arbitrary executable field, passing renderer
arguments through to a process, or treating every endpoint as if Hermes Local
owns its installation and update lifecycle.

The shared contract must answer six different questions without conflating them:

1. Which adapter translates Hermes operations into a backend?
2. Which model formats, hardware backends, metrics and optimisation modules does
   that exact adapter revision support?
3. Which typed settings are valid for this backend?
4. Which runtime distribution or external endpoint is being used?
5. Which complete model, profile, runtime, hardware, optimisation and Hermes
   Agent identity produced a result?
6. Which lifecycle operation is in progress, and does Hermes Local have
   authority to perform it?

## Decision

### 1. Canonical versioned records

Hermes Local adopts JSON Schema Draft 2020-12 as the language-neutral source of
truth for runtime records. Version 1 is the strict discriminated union in
`config/schemas/runtime-contracts.schema.json`:

| Record | Purpose |
|---|---|
| `runtimeAdapter` | Adapter identity, lifecycle surface, declared capabilities and privilege boundary |
| `runtimeConfiguration` | Typed common and backend-specific launch or connection settings |
| `runtimePackageIdentity` | Verified identity for an owned runtime distribution |
| `optimisationPlan` | Typed, capability-gated optimisation modules |
| `executionIdentity` | Complete reproducibility and certification identity |
| `runtimeOperation` | Durable lifecycle operation and evidence state |

Every record rejects unknown properties. Every persisted record carries
`schemaVersion` and a stable identifier. Desktop, CLI and backend implementations
must validate at their native authority boundary before storage or use.

Generated TypeScript or Python types may be derived from the schemas. Generated
types do not replace runtime validation, because renderer values, imported
configuration, update manifests and external endpoint definitions are untrusted.

Schema evolution follows these rules:

- a meaning change requires a new schema version;
- a new backend setting or optimisation module requires a reviewed schema change;
- an older validator encountering an unknown field, backend or module rejects it;
- invalid persisted records are quarantined rather than partially loaded; and
- migrations are idempotent and retain a restorable copy of the previous data.

### 2. Adapter contract and lifecycle vocabulary

An adapter is a reviewed native component identified by `id` and
`manifestRevision`. It declares:

- ownership class;
- provider protocol;
- lifecycle operations;
- model formats;
- hardware backends;
- optimisation modules;
- supported metrics and provider features;
- configuration schema ID;
- command-construction policy; and
- endpoint, authentication and process-authority boundary.

Version 1 lifecycle operations are:

`detect`, `install`, `validate`, `launch`, `stop`, `health`, `metrics`,
`benchmark`, `diagnostics`, `update` and `rollback`.

The operation names describe observable product behavior, not backend command
names. An adapter may implement them with a process, an API call, a package
manager or a bounded probe, but it must return the same `runtimeOperation`
shape.

Owned adapters must implement the full lifecycle. External adapters may only
declare `detect`, `validate`, `health`, `metrics`, `benchmark` and
`diagnostics`. They cannot claim installation, launch, stop, update or rollback
authority over a service Hermes Local does not own.

The first canonical adapters are:

| Adapter ID | Ownership | Backend |
|---|---|---|
| `llama.cpp.native` | `owned` | Managed native `llama.cpp` |
| `openai.external` | `external` | User-configured OpenAI-compatible endpoint |

Adapter IDs are stable product identifiers. A material adapter implementation
or capability change increments `manifestRevision`.

### 3. Capability declaration and gating

Adapter capability declarations use closed vocabularies.

Model formats:

- `gguf`;
- `safetensors`;
- `onnx`; and
- `remote-provider`.

Hardware backends:

- `cpu`;
- `cuda`;
- `vulkan`;
- `hip`;
- `sycl`;
- `metal`; and
- `remote`.

Optimisation modules:

- `flash-attention`;
- `kv-cache-quantization`;
- `prompt-cache`;
- `tensor-offload`;
- `batch-tuning`;
- `speculative-draft-model`; and
- `speculative-mtp`.

Capabilities are declarations, not proof. A selected model, package, profile and
hardware identity must all be compatible with the adapter declaration before
launch. An optimisation module is accepted only when:

1. the module validates against the canonical schema;
2. the selected adapter declares the module;
3. the selected runtime distribution supports it;
4. the model identity is compatible;
5. the hardware plan permits it; and
6. required benchmark or certification evidence exists when policy demands it.

Unknown fields and modules fail closed. The UI may hide unsupported controls,
but native validation is authoritative even when the renderer is stale or
modified.

### 4. Typed configuration and native command construction

`runtimeConfiguration` contains common identity fields and exactly one
backend-specific settings object.

The managed `llama.cpp` configuration supports typed fields for:

- loopback host and port;
- native credential reference;
- context size;
- generation and batch threads;
- logical and physical batch sizes;
- KV cache types;
- GPU offload and VRAM reserve;
- Flash Attention;
- prompt caching; and
- deterministic seed.

The generic external configuration supports typed fields for:

- base URL;
- authentication mode and credential reference;
- TLS requirements;
- bounded health probe; and
- timeout, concurrency and provider model alias.

There is intentionally no `commandLine`, `arguments`, `extraArguments`,
`executablePath` or renderer-defined environment map in the public
configuration contract.

Owned adapters use `commandPolicy.mode = adapter-generated`. Native adapter code
selects an executable from a verified package role and translates validated
settings into an argument array. It never concatenates shell text. Renderer,
model, imported manifest and remote-client values cannot supply executable
paths, flags, environment variables, working directories or process ownership.

External adapters use `commandPolicy.mode = none`. Hermes Local does not spawn
or control the external service. It validates the endpoint, applies the
configured credential at the native request boundary and exposes only the
operations allowed to external ownership.

### 5. Runtime identifiers shared by packaging and updates

All package, configuration, operation and execution records use the same
`runtimeRef` shape:

```json
{
  "adapterId": "llama.cpp.native",
  "runtimeId": "llama.cpp",
  "ownership": "owned",
  "distribution": {
    "id": "official-cuda-windows-x64",
    "version": "b7000",
    "revision": "abcdef1234567890",
    "artifactSha256": "<sha256>"
  }
}
```

The fields have these meanings:

- `adapterId`: the native adapter contract;
- `runtimeId`: the backend family;
- `ownership`: the privilege boundary;
- `distribution.id`: independently updateable package or endpoint identity;
- `distribution.version`: user-visible distribution version;
- `distribution.revision`: exact source, build or endpoint-config revision; and
- `artifactSha256`: required for an owned package identity and omitted for an
  endpoint without a local artifact.

Issue #6 runtime package manifests produce this identity. Issue #8 update
records use the installed, target and backup `runtimeRef` values without
inventing a second runtime naming system. Diagnostics, Task Centre and rollback
history display the same identifiers.

A package identity additionally records platform, hardware backends, exact
source revision, build flags, architectures, file-level hashes, provenance,
compatibility constraints and verification state. A package is not promotable
until its identity validates and integrity state is `verified`.

### 6. Owned and external privilege boundaries

Ownership is part of the runtime identity and cannot be inferred from a URL or
display name.

#### Owned runtime

Hermes Local may:

- select a verified executable by package role;
- install, validate, launch, stop, update and roll back the runtime;
- bind it to approved loopback addresses;
- inject a managed credential reference;
- place the process in the managed Windows Job Object;
- collect local process and hardware metrics; and
- retain a previous verified distribution for rollback.

Owned runtime endpoints are `owned-loopback` and require
`managed-token-ref` authentication in the execution identity.

#### External runtime

Hermes Local may:

- store a user-approved endpoint configuration;
- validate its URL, TLS and credential policy;
- perform bounded health, metrics, benchmark and diagnostics requests; and
- use it as a provider when selected.

Hermes Local may not:

- install or update the service;
- start, stop or kill its process;
- claim package provenance for it;
- bypass its TLS or authentication policy;
- expose its credential to the renderer; or
- describe it as locally verified merely because a health probe succeeded.

An external execution identity uses endpoint class `external` and authentication
`credential-ref` or `none`.

### 7. Complete execution identity

Every benchmark, certification result, routing decision and reproducibility
record binds to an `executionIdentity`.

The identity includes:

- exact `runtimeRef`;
- adapter manifest revision;
- model ID, SHA-256, format, architecture, quantisation and source revision;
- profile ID, revision and content hash;
- hardware fingerprint, operating system, CPU features, GPU identities, driver
  versions, RAM and VRAM;
- Hermes Agent revision;
- optimisation-plan ID and hash;
- provider protocol, endpoint class, normalized base URL and authentication
  class; and
- creation time.

The `id` is a SHA-256 content identity calculated from the canonical normalized
record without the `id` field. Implementations must use deterministic property
ordering and normalized values before hashing.

A result missing a model hash, runtime revision, profile hash, hardware
fingerprint or Hermes Agent revision may be retained as diagnostic evidence,
but it cannot be promoted to certified execution evidence or used for certified
routing.

External providers still receive a complete execution identity. Provider-managed
model internals may be described as `remote-provider` and `provider-managed`,
but the configured model alias, endpoint configuration revision and local
hardware identity remain explicit.

### 8. Optimisation plans

An optimisation plan is separate from both the adapter manifest and the user
profile.

- The adapter manifest declares what the backend can support.
- The profile expresses workload intent and typed settings.
- The optimisation plan records the exact enabled modules and values used for
  one runtime/model/profile combination.
- The execution identity hashes the selected plan.

This separation prevents a UI toggle from becoming a claim that the runtime
actually used an optimisation.

Each module is a strict discriminated object. Raw backend flags are not valid
optimisation settings. Duplicate module IDs, contradictory modules and modules
not declared by the adapter are rejected by native semantic validation.

Speculative decoding distinguishes:

- `speculative-draft-model`, which binds the exact draft model ID and SHA-256;
  and
- `speculative-mtp`, which represents a model-native multi-token prediction
  module.

The two are not interchangeable and must be reported separately in benchmarks.

### 9. Durable lifecycle operations

`runtimeOperation` is the shared Desktop, CLI, Task Centre and update
orchestration record.

It contains:

- operation ID and type;
- adapter ID;
- installed `runtimeRef`;
- target and backup identities when applicable;
- durable task ID;
- status, stage and progress;
- redacted log and report paths;
- timestamps; and
- structured failure details.

Install and update require `targetRuntimeRef`. Rollback requires
`backupRuntimeRef`. External ownership rejects lifecycle operations that imply
process or package authority.

Operation stages are stable product stages:

`detect`, `download`, `verify`, `stage`, `smoke-test`, `promote`, `launch`,
`stop`, `health-check`, `collect`, `rollback` and `complete`.

Backend-specific substeps belong in redacted evidence, not in the public state
machine. A failed promotion leaves the installed runtime identity unchanged.
Rollback records both the failed target and restored backup identity.

### 10. Migration from current configuration

Migration is performed once through a versioned, idempotent transaction.

#### Model manifests

For every current model manifest:

1. retain the stable model ID, display name, alias, source and local path;
2. set format to `gguf`;
3. use the recorded SHA-256 or calculate it before certification;
4. normalize architecture and quantisation metadata;
5. bind it to adapter `llama.cpp.native`; and
6. leave the original manifest untouched until the new registry validates.

A model without a verified hash may run under explicit unverified policy during
migration, but it cannot produce certified execution evidence.

#### Profiles

Current profile fields map deterministically:

| Current field | New location |
|---|---|
| `contextTokens` | `backend.settings.contextTokens` |
| `threads` | `backend.settings.threads` |
| `batch` | `backend.settings.batch` and `batch-tuning` module |
| `kvCache` | `backend.settings.kvCache` and `kv-cache-quantization` module |
| `gpu.layers` / `vramReserveMiB` | `backend.settings.gpu` and `tensor-offload` module |
| `flashAttention` | typed setting and `flash-attention` module |
| `promptCache` | typed setting and `prompt-cache` module |
| `seed` | `backend.settings.seed` |
| `speculativeDecoding` | disabled, or migrated only with exact typed draft/MTP identity |

Profile names are normalized into stable IDs while retaining the display name
in product state. A profile revision and canonical content hash are created.

#### Existing server arguments

`model.server.extraArguments` is not copied into the new configuration.

Tracked, trusted manifests may migrate an allowlisted known option into a typed
field after parsing and validation. Unknown, repeated, conflicting or
value-bearing flags that lack a typed schema cause the migrated model/profile
pair to be quarantined for review. Raw arguments are retained only in the
migration backup and redacted diagnostic report; they never reach the renderer
or new launch contract.

#### Runtime and endpoint identity

The current source-built `llama.cpp` installation becomes:

- adapter `llama.cpp.native`;
- runtime `llama.cpp`;
- distribution `local-source-build`;
- version from the current runtime build identity; and
- revision from the exact source commit and build manifest.

The configured local model endpoint becomes an owned loopback provider with a
managed credential reference.

Generic external endpoints are created only through an explicit new user
configuration. Migration never interprets an arbitrary existing URL, command or
environment value as an approved external adapter.

#### Transaction guarantees

Migration:

1. acquires the runtime-configuration resource lock;
2. writes a versioned backup;
3. builds new records in staging;
4. validates every schema and cross-record reference;
5. runs a no-process construction smoke check;
6. atomically promotes the registry;
7. records a migration report; and
8. leaves old data readable for rollback until the migration is accepted.

A repeated migration over the same source identity produces the same IDs and
does not duplicate records.

### 11. Required semantic validation

JSON Schema enforces record shape. Native semantic validation additionally
enforces relationships that JSON Schema cannot safely express alone:

- adapter ID, ownership and runtime ID agree across records;
- requested lifecycle operation is declared by the adapter;
- every optimisation module is declared by the adapter and appears once;
- model format and hardware backend are supported;
- package compatibility matches detected hardware before promotion;
- owned executable roles resolve inside the verified package root;
- no renderer or imported record supplies an executable or argument vector;
- endpoint normalization and credential resolution occur natively;
- installed, target and backup identities are coherent;
- execution identity hashes match canonical content; and
- profile, model, runtime, hardware and Hermes Agent revisions are current.

A renderer-side validation pass is optional user feedback only. Native failure
is authoritative and must occur before process launch, credential use, update
promotion or external request.

## Verification

`tests/test_issue39_runtime_contracts.py` validates every schema with
`Draft202012Validator.check_schema` and covers:

- the current official managed `llama.cpp` path;
- a generic external OpenAI-compatible endpoint;
- strict rejection of unsupported backend fields;
- rejection of renderer command lines and arguments;
- owned versus external lifecycle privilege boundaries;
- shared package, update and execution runtime identifiers;
- unknown optimisation modules;
- complete fail-closed execution identity; and
- unknown record types.

Dependent implementation work must add adapter-level integration tests for
argument construction, package verification, endpoint normalization, migration,
hardware compatibility, Task Centre persistence and rollback recovery.

## Consequences

- Hermes Local can add runtimes without exposing a generic command launcher.
- `llama.cpp` remains the canonical first owned adapter and migrates without
  discarding current models or profiles.
- External endpoints participate in one provider and execution identity model
  without receiving false local ownership.
- Package distribution and update orchestration share exact runtime identifiers.
- Benchmark, certification, telemetry and routing can compare complete execution
  identities instead of display names.
- Adding a backend or optimisation requires schema, adapter and threat-test
  review, which intentionally slows unsafe extension points.
