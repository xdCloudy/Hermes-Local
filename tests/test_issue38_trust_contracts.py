from __future__ import annotations

import copy
import json
import unittest
from pathlib import Path

from jsonschema import Draft202012Validator, FormatChecker, ValidationError
from referencing import Registry, Resource

ROOT = Path(__file__).resolve().parents[1]
SCHEMA = ROOT / "config" / "schemas" / "trust-contracts.schema.json"
SCHEMA_DIR = ROOT / "config" / "schemas" / "trust"


def load(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def registry() -> Registry:
    result = Registry()
    for path in [SCHEMA, *sorted(SCHEMA_DIR.glob("*.schema.json"))]:
        resource = Resource.from_contents(load(path))
        result = result.with_resource(resource.id(), resource)
    return result


class TrustContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        for path in [SCHEMA, *sorted(SCHEMA_DIR.glob("*.schema.json"))]:
            Draft202012Validator.check_schema(load(path))
        cls.validator = Draft202012Validator(
            load(SCHEMA), registry=registry(), format_checker=FormatChecker()
        )

    def valid(self, value: dict) -> None:
        self.validator.validate(value)

    def invalid(self, value: dict) -> None:
        with self.assertRaises(ValidationError):
            self.validator.validate(value)

    @staticmethod
    def identity() -> dict:
        return {
            "kind": "integrationIdentity",
            "schemaVersion": 1,
            "id": "example-mcp",
            "displayName": "Example MCP",
            "integrationType": "mcp-local",
            "provenance": {
                "sourceType": "github",
                "provenanceId": "github.example-mcp.abc123",
                "uri": "https://github.com/example/mcp",
                "revision": "abc123",
                "sha256": "a" * 64,
            },
            "declaredCapabilities": ["filesystem.read", "network.outbound"],
            "trustState": "restricted",
            "healthState": "healthy",
            "manifestRevision": 1,
        }

    @staticmethod
    def grant() -> dict:
        return {
            "kind": "capabilityGrant",
            "schemaVersion": 1,
            "id": "grant.project-alpha.index-read",
            "principal": {"kind": "agent", "id": "main-agent"},
            "scope": {"kind": "project", "id": "project-alpha"},
            "capability": "index.read",
            "effect": "allow",
            "confirmation": "never",
            "constraints": {"collectionIds": ["project-alpha-index"]},
            "issuedAt": "2026-08-04T12:00:00Z",
            "revision": 1,
            "source": {"kind": "user-approval", "id": "approval-001"},
        }

    def test_all_record_types_validate(self) -> None:
        records = [
            self.identity(),
            self.grant(),
            {
                "kind": "credentialReference",
                "schemaVersion": 1,
                "id": "credential.github.example-mcp",
                "provider": "github",
                "storage": "dpapi-current-user",
                "handle": "dpapi://current-user/example-mcp/github",
                "scope": {"kind": "project", "id": "project-alpha"},
                "allowedIntegrationIds": ["example-mcp"],
                "accessMode": "brokered-call",
                "createdAt": "2026-08-04T12:00:00Z",
                "revision": 1,
            },
            {
                "kind": "auditEvent",
                "schemaVersion": 1,
                "id": "audit-001",
                "eventType": "authorization.denied",
                "timestamp": "2026-08-04T12:01:00Z",
                "actor": {"kind": "agent", "id": "main-agent"},
                "scope": {"kind": "project", "id": "project-alpha"},
                "outcome": "denied",
                "correlationId": "request-001",
                "details": {"capability": "filesystem.write", "reason": "no-grant"},
                "redaction": {"applied": True, "fields": ["arguments"]},
            },
            {
                "kind": "trustedOrigin",
                "schemaVersion": 1,
                "id": "origin.dashboard.local",
                "originClass": "loopback",
                "origin": "http://127.0.0.1:9119",
                "allowedActions": ["render", "websocket"],
                "credentialMode": "electron-request-header",
                "allowSubdomains": False,
                "allowWildcardPort": False,
                "enabled": True,
                "revision": 1,
            },
            {
                "kind": "sessionAuthorization",
                "schemaVersion": 1,
                "id": "session.remote.phone-001",
                "sessionType": "remote-device",
                "principal": {"kind": "remote-client", "id": "phone-001"},
                "scope": {"kind": "project", "id": "project-alpha"},
                "capabilitySnapshot": ["remote.connect", "project.read"],
                "grantRevision": 4,
                "policyRevision": 7,
                "issuedAt": "2026-08-04T12:00:00Z",
                "expiresAt": "2026-08-04T20:00:00Z",
            },
            {
                "kind": "redactionPolicy",
                "schemaVersion": 1,
                "id": "redaction.default",
                "revision": 1,
                "recordPromptBodies": False,
                "recordCredentialValues": False,
                "recordAuthorizationHeaders": False,
                "prohibitedFieldPatterns": ["(?i)secret", "(?i)token"],
                "maxTaskOutputBytes": 131072,
                "retention": {"auditDays": 180, "taskOutputDays": 30, "diagnosticsDays": 14},
            },
        ]
        for record in records:
            with self.subTest(kind=record["kind"]):
                self.valid(record)

    def test_unknown_capability_fails_closed(self) -> None:
        identity = self.identity()
        identity["declaredCapabilities"].append("host.unrestricted")
        self.invalid(identity)
        grant = self.grant()
        grant["capability"] = "host.unrestricted"
        self.invalid(grant)

    def test_renderer_cannot_forge_native_decision(self) -> None:
        grant = self.grant()
        grant.update({"allowed": True, "nativeValidationBypassed": True})
        self.invalid(grant)

    def test_project_scope_requires_project_id(self) -> None:
        grant = self.grant()
        grant["scope"] = {"kind": "project"}
        self.invalid(grant)
        grant["scope"] = {"kind": "global", "id": "project-alpha"}
        self.invalid(grant)

    def test_credential_reference_rejects_secret_value(self) -> None:
        credential = {
            "kind": "credentialReference",
            "schemaVersion": 1,
            "id": "credential.example",
            "provider": "example",
            "storage": "dpapi-current-user",
            "handle": "dpapi://current-user/example",
            "scope": {"kind": "user", "id": "local-user"},
            "allowedIntegrationIds": ["example-mcp"],
            "accessMode": "never-export",
            "createdAt": "2026-08-04T12:00:00Z",
            "revision": 1,
            "token": "plaintext-secret",
        }
        self.invalid(credential)

    def test_audit_event_rejects_sensitive_or_nested_payload(self) -> None:
        event = {
            "kind": "auditEvent",
            "schemaVersion": 1,
            "id": "audit-002",
            "eventType": "credential.denied",
            "timestamp": "2026-08-04T12:01:00Z",
            "actor": {"kind": "integration", "id": "example-mcp"},
            "scope": {"kind": "project", "id": "project-alpha"},
            "outcome": "denied",
            "correlationId": "request-002",
            "details": {"token": "plaintext-secret"},
            "redaction": {"applied": False, "fields": []},
        }
        self.invalid(event)
        nested = copy.deepcopy(event)
        nested["details"] = {"metadata": {"arbitrary": "payload"}}
        self.invalid(nested)

    def test_remote_origin_requires_https_and_gateway_credentials(self) -> None:
        origin = {
            "kind": "trustedOrigin",
            "schemaVersion": 1,
            "id": "origin.remote.gateway",
            "originClass": "remote-gateway",
            "origin": "http://gateway.example.test",
            "allowedActions": ["render", "api-proxy"],
            "credentialMode": "electron-request-header",
            "allowSubdomains": False,
            "allowWildcardPort": False,
            "enabled": True,
            "revision": 1,
        }
        self.invalid(origin)
        origin.update({"origin": "https://gateway.example.test", "credentialMode": "proof-of-possession"})
        self.valid(origin)

    def test_unknown_record_kind_is_rejected(self) -> None:
        self.invalid({"kind": "rendererAuthorization", "schemaVersion": 1})


if __name__ == "__main__":
    unittest.main()
