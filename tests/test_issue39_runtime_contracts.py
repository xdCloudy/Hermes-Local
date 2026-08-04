from __future__ import annotations

import copy
import json
import unittest
from pathlib import Path

from jsonschema import Draft202012Validator, FormatChecker, ValidationError
from referencing import Registry, Resource

ROOT = Path(__file__).resolve().parents[1]
SCHEMA = ROOT / "config" / "schemas" / "runtime-contracts.schema.json"
SCHEMA_DIR = ROOT / "config" / "schemas" / "runtime"


def load(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def build_registry() -> Registry:
    result = Registry()
    for path in [SCHEMA, *sorted(SCHEMA_DIR.glob("*.schema.json"))]:
        resource = Resource.from_contents(load(path))
        result = result.with_resource(resource.id(), resource)
    return result


def runtime_ref(external: bool = False) -> dict:
    if external:
        return {
            "adapterId": "openai.external",
            "runtimeId": "openai-compatible.example",
            "ownership": "external",
            "distribution": {
                "id": "external-endpoint",
                "version": "api-v1",
                "revision": "endpoint-config-7",
            },
        }
    return {
        "adapterId": "llama.cpp.native",
        "runtimeId": "llama.cpp",
        "ownership": "owned",
        "distribution": {
            "id": "official-cuda-windows-x64",
            "version": "b7000",
            "revision": "abcdef1234567890",
            "artifactSha256": "a" * 64,
        },
    }


def adapter(external: bool = False) -> dict:
    operations = ["detect", "validate", "health", "metrics", "benchmark", "diagnostics"]
    if not external:
        operations = [
            "detect", "install", "validate", "launch", "stop", "health",
            "metrics", "benchmark", "diagnostics", "update", "rollback",
        ]
    return {
        "kind": "runtimeAdapter",
        "schemaVersion": 1,
        "id": "openai.external" if external else "llama.cpp.native",
        "displayName": "External OpenAI-compatible endpoint" if external else "Managed llama.cpp",
        "ownership": "external" if external else "owned",
        "protocol": "openai-compatible",
        "manifestRevision": 1,
        "lifecycleOperations": operations,
        "capabilities": {
            "modelFormats": ["remote-provider"] if external else ["gguf"],
            "hardwareBackends": ["remote"] if external else ["cpu", "cuda", "vulkan"],
            "optimisations": [] if external else [
                "flash-attention", "kv-cache-quantization", "prompt-cache",
                "tensor-offload", "batch-tuning", "speculative-draft-model",
                "speculative-mtp",
            ],
            "streaming": True,
            "toolCalling": True,
            "embeddings": not external,
            "metrics": ["tokens-per-second", "time-to-first-token"],
        },
        "configurationSchemaId": "https://local.hermes.invalid/schemas/runtime/configuration.schema.json",
        "commandPolicy": {
            "mode": "none" if external else "adapter-generated",
            "allowRendererArguments": False,
            "executableRoles": [] if external else ["server", "cli", "benchmark"],
        },
        "boundary": {
            "endpointClass": "external" if external else "owned-loopback",
            "authentication": "configured-credential" if external else "required-managed-token",
            "processAuthority": "none" if external else "managed-job-object",
        },
    }


def configuration(external: bool = False) -> dict:
    common = {
        "kind": "runtimeConfiguration",
        "schemaVersion": 1,
        "id": "runtime-config.external" if external else "runtime-config.daily",
        "adapterId": "openai.external" if external else "llama.cpp.native",
        "ownership": "external" if external else "owned",
        "runtimeRef": runtime_ref(external),
        "profileId": "external-balanced" if external else "daily",
        "modelId": "provider-model" if external else "qwen3.6-starter",
        "optimisationPlanId": "optimisation.external-none" if external else "optimisation.daily",
    }
    if external:
        common["backend"] = {
            "kind": "openai-compatible",
            "baseUrl": "https://api.example.test/v1",
            "authentication": {
                "mode": "named-header",
                "credentialReferenceId": "credential.external-example",
                "headerName": "X-API-Key",
            },
            "tls": {
                "requireTls": True,
                "allowLoopbackHttp": False,
                "verifyCertificates": True,
            },
            "health": {"path": "/models", "timeoutSeconds": 10},
            "settings": {
                "requestTimeoutSeconds": 300,
                "maxConcurrentRequests": 4,
                "modelAlias": "provider-model",
            },
        }
    else:
        # Deterministic mapping of the current "Daily" profile.
        common["backend"] = {
            "kind": "llama.cpp",
            "host": "127.0.0.1",
            "port": 8011,
            "credentialReferenceId": "credential.local-model-api",
            "executableRole": "server",
            "settings": {
                "contextTokens": 65536,
                "threads": {"generation": "auto", "batch": "auto"},
                "batch": {"logical": 1024, "physical": 256},
                "kvCache": {"keyType": "q8_0", "valueType": "q8_0"},
                "gpu": {"layers": "auto", "vramReserveMiB": "auto"},
                "flashAttention": True,
                "promptCache": True,
            },
        }
    return common


def execution(external: bool = False) -> dict:
    return {
        "kind": "executionIdentity",
        "schemaVersion": 1,
        "id": "e" * 64,
        "runtimeRef": runtime_ref(external),
        "adapterManifestRevision": 1,
        "model": {
            "id": "provider-model" if external else "qwen3.6-starter",
            "sha256": "b" * 64,
            "format": "remote-provider" if external else "gguf",
            "architecture": "provider-managed" if external else "qwen3-moe",
            "quantisation": "provider-managed" if external else "IQ3_S",
            "sourceRevision": "provider-release-2026-08" if external else "main",
        },
        "profile": {
            "id": "external-balanced" if external else "daily",
            "revision": 1,
            "sha256": "c" * 64,
        },
        "hardware": {
            "fingerprint": "d" * 64,
            "os": {"platform": "windows-x64", "version": "11.0.26100"},
            "cpu": {"architecture": "x86_64", "features": ["avx2"]},
            "gpus": [] if external else [{
                "id": "gpu-0",
                "vendor": "nvidia",
                "backend": "cuda",
                "deviceName": "GeForce RTX 3060",
                "driverVersion": "610.74",
                "vramMiB": 12288,
            }],
            "ramMiB": 65536,
        },
        "hermesAgent": {"revision": "85148f79f78af6c5dafdf0fa4e7545ec7f7a1731"},
        "optimisationPlan": {
            "id": "optimisation.external-none" if external else "optimisation.daily",
            "sha256": "f" * 64,
        },
        "provider": {
            "protocol": "openai-compatible",
            "endpointClass": "external" if external else "owned-loopback",
            "baseUrl": "https://api.example.test/v1" if external else "http://127.0.0.1:8011/v1",
            "authentication": "credential-ref" if external else "managed-token-ref",
        },
        "createdAt": "2026-08-04T13:20:00Z",
    }


def operation(external: bool = False, action: str = "health") -> dict:
    value = {
        "kind": "runtimeOperation",
        "schemaVersion": 1,
        "id": f"runtime-operation.{action}-001",
        "operation": action,
        "adapterId": "openai.external" if external else "llama.cpp.native",
        "runtimeRef": runtime_ref(external),
        "status": "succeeded",
        "requestedAt": "2026-08-04T13:20:00Z",
        "updatedAt": "2026-08-04T13:20:01Z",
        "taskId": f"task-runtime-{action}-001",
        "stage": "complete",
        "progress": 100,
        "evidence": {
            "logPath": f"data/logs/runtime-{action}-001.log",
            "reportPath": f"reports/runtime-{action}-001.json",
        },
    }
    if action in {"install", "update"}:
        value["targetRuntimeRef"] = {
            **runtime_ref(False),
            "distribution": {
                "id": "official-cuda-windows-x64",
                "version": "b7001",
                "revision": "abcdef1234567891",
                "artifactSha256": "4" * 64,
            },
        }
    return value


class RuntimeContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        for path in [SCHEMA, *sorted(SCHEMA_DIR.glob("*.schema.json"))]:
            Draft202012Validator.check_schema(load(path))
        cls.registry = build_registry()
        cls.validator = Draft202012Validator(
            load(SCHEMA), registry=cls.registry, format_checker=FormatChecker()
        )

    def valid(self, value: dict) -> None:
        self.validator.validate(value)

    def invalid(self, value: dict) -> None:
        with self.assertRaises(ValidationError):
            self.validator.validate(value)

    def test_current_llama_cpp_path_is_representable(self) -> None:
        records = [
            adapter(),
            configuration(),
            {
                "kind": "runtimePackageIdentity",
                "schemaVersion": 1,
                "id": "package.llama-cpp.cuda.windows-x64.b7000",
                "runtimeRef": runtime_ref(),
                "platform": "windows-x64",
                "hardwareBackends": ["cuda"],
                "source": {
                    "repository": "https://github.com/ggml-org/llama.cpp",
                    "revision": "abcdef1234567890",
                },
                "build": {
                    "toolchain": "MSVC 19.44 + CUDA 13",
                    "flags": ["GGML_CUDA=ON"],
                    "architectures": ["75", "80", "86", "89"],
                },
                "files": [{
                    "path": "bin/llama-server.exe",
                    "sizeBytes": 1000000,
                    "sha256": "1" * 64,
                    "role": "server",
                }],
                "provenance": {
                    "provider": "github-artifact-attestations",
                    "uri": "https://github.com/xdCloudy/Hermes-Local/actions/runs/123",
                    "runId": 123,
                },
                "compatibility": {
                    "minimumWindowsBuild": 19045,
                    "cpuFeatures": ["avx2"],
                    "minimumDriver": "560.00",
                },
                "integrity": {
                    "manifestSha256": "3" * 64,
                    "status": "verified",
                    "verifiedAt": "2026-08-04T13:20:00Z",
                },
            },
            {
                "kind": "optimisationPlan",
                "schemaVersion": 1,
                "id": "optimisation.daily",
                "adapterId": "llama.cpp.native",
                "runtimeId": "llama.cpp",
                "modelId": "qwen3.6-starter",
                "profileId": "daily",
                "modules": [
                    {"id": "flash-attention", "enabled": True},
                    {"id": "kv-cache-quantization", "keyType": "q8_0", "valueType": "q8_0"},
                    {"id": "prompt-cache", "enabled": True},
                    {"id": "tensor-offload", "layers": "auto", "splitMode": "layer"},
                    {"id": "batch-tuning", "logical": 1024, "physical": 256},
                ],
                "createdAt": "2026-08-04T13:20:00Z",
            },
            execution(),
            operation(action="update"),
        ]
        for record in records:
            with self.subTest(kind=record["kind"]):
                self.valid(record)

    def test_generic_external_endpoint_is_representable(self) -> None:
        for record in [adapter(True), configuration(True), execution(True), operation(True)]:
            with self.subTest(kind=record["kind"]):
                self.valid(record)

    def test_unsupported_backend_fields_are_rejected(self) -> None:
        value = configuration()
        value["backend"]["settings"]["extraArguments"] = ["--renderer-value"]
        self.invalid(value)
        value = configuration(True)
        value["backend"]["settings"]["gpuLayers"] = 99
        self.invalid(value)

    def test_renderer_command_lines_cannot_enter_contract(self) -> None:
        value = adapter()
        value["commandPolicy"]["rendererArguments"] = ["--host", "0.0.0.0"]
        self.invalid(value)
        value = configuration()
        value["backend"]["commandLine"] = "renderer-defined"
        self.invalid(value)

    def test_external_runtime_cannot_claim_owned_lifecycle_authority(self) -> None:
        value = adapter(True)
        value["lifecycleOperations"].append("install")
        self.invalid(value)
        self.invalid(operation(True, "update"))

    def test_package_update_and_execution_share_runtime_identifier_shape(self) -> None:
        refs = [runtime_ref(), copy.deepcopy(runtime_ref()), copy.deepcopy(runtime_ref())]
        self.assertEqual(refs[0], refs[1])
        self.assertEqual(refs[1], refs[2])
        validator = Draft202012Validator(
            {
                "$schema": "https://json-schema.org/draft/2020-12/schema",
                "$ref": "https://local.hermes.invalid/schemas/runtime/common.schema.json#/$defs/runtimeRef",
            },
            registry=self.registry,
            format_checker=FormatChecker(),
        )
        for value in refs:
            validator.validate(value)

    def test_unknown_optimisation_module_is_rejected(self) -> None:
        value = {
            "kind": "optimisationPlan",
            "schemaVersion": 1,
            "id": "optimisation.invalid",
            "adapterId": "llama.cpp.native",
            "runtimeId": "llama.cpp",
            "modelId": "model",
            "profileId": "daily",
            "modules": [{"id": "renderer-defined-turbo", "enabled": True}],
            "createdAt": "2026-08-04T13:20:00Z",
        }
        self.invalid(value)

    def test_execution_identity_is_complete_and_fail_closed(self) -> None:
        value = execution()
        del value["model"]["sha256"]
        self.invalid(value)
        value = execution()
        value["provider"]["baseUrl"] = "http://0.0.0.0:8011/v1"
        self.invalid(value)
        value = execution(True)
        value["provider"]["authentication"] = "managed-token-ref"
        self.invalid(value)

    def test_unknown_record_kind_is_rejected(self) -> None:
        self.invalid({"kind": "rendererRuntimeCommand", "schemaVersion": 1})


if __name__ == "__main__":
    unittest.main()
