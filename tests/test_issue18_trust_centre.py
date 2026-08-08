from __future__ import annotations

import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PATCH_ROOT = ROOT / "source" / "hermes-launcher" / "patches"
PATCH_NAMES = [
    "0079-feat-agent-enforce-managed-MCP-trust-policy.patch",
    "0080-fix-agent-align-trust-records-with-shared-schema.patch",
    "0081-feat-agent-sync-MCP-trust-identities.patch",
    "0081b-feat-agent-gate-MCP-startup-and-invocation.patch",
    "0081c-test-agent-cover-managed-MCP-trust-policy.patch",
    "0082-feat-desktop-add-native-Trust-Centre-bridge.patch",
    "0083-test-desktop-cover-native-Trust-Centre-bridge.patch",
    "0084-feat-desktop-model-Trust-Centre-view-types.patch",
    "0085-feat-desktop-wire-native-Trust-Centre-IPC.patch",
    "0086-feat-desktop-build-Skills-and-MCP-Trust-Centre.patch",
    "0087-feat-desktop-route-Trust-Centre.patch",
]


def patch_text() -> str:
    return "\n".join((PATCH_ROOT / name).read_text(encoding="utf-8") for name in PATCH_NAMES)


class TrustCentreContractTests(unittest.TestCase):
    def test_patch_series_is_present_and_mail_boundaries_are_not_embedded(self) -> None:
        for name in PATCH_NAMES:
            with self.subTest(name=name):
                path = PATCH_ROOT / name
                self.assertTrue(path.is_file())
                text = path.read_text(encoding="utf-8")
                self.assertTrue(text.startswith("From "))
                self.assertNotIn("\n+diff --git ", text)
                self.assertNotIn("\n diff --git ", text)

    def test_native_authority_is_default_deny_and_source_bound(self) -> None:
        text = patch_text()
        for marker in (
            "integration identity is missing",
            "capability is unknown or undeclared",
            "no matching allow grant",
            "explicit deny grant matched",
            "manifest-changed",
            "approvedManifestSha256",
            "configurationSha256",
            "filter_authorized_mcp_servers",
            "Hermes Local MCP trust gate failed closed",
        ):
            with self.subTest(marker=marker):
                self.assertIn(marker, text)

    def test_renderer_cannot_submit_principal_or_manifest_authority(self) -> None:
        bridge = (PATCH_ROOT / PATCH_NAMES[5]).read_text(encoding="utf-8")
        policy_shape = bridge.split("export interface TrustPolicyInput", 1)[1].split("function localRoot", 1)[0]
        self.assertIn("integrationId", policy_shape)
        self.assertIn("capabilities", policy_shape)
        self.assertIn("confirmation", policy_shape)
        self.assertIn("scope", policy_shape)
        self.assertIn("state", policy_shape)
        for forbidden in ("principal:", "manifestRevision:", "approvedManifestSha256:", "allow:"):
            with self.subTest(forbidden=forbidden):
                self.assertNotIn(forbidden, policy_shape)

    def test_delegation_revocation_and_health_invariants_are_covered(self) -> None:
        tests = (PATCH_ROOT / PATCH_NAMES[4]).read_text(encoding="utf-8")
        for marker in (
            "test_delegated_child_does_not_inherit_main_agent_grant",
            "test_source_change_and_capability_expansion_suspend_old_grants",
            "test_disable_and_remove_revoke_new_calls_immediately",
            "test_health_cannot_endorse_or_restore_permissions",
            "test_secret_values_never_land_in_identity_audit_or_diagnostics",
            "test_renderer_or_server_trust_claims_cannot_grant_authority",
        ):
            with self.subTest(marker=marker):
                self.assertIn(marker, tests)

    def test_native_bridge_uses_minimal_child_environment(self) -> None:
        bridge = (PATCH_ROOT / PATCH_NAMES[5]).read_text(encoding="utf-8")
        self.assertIn("trustCliEnvironment", bridge)
        self.assertIn("HERMES_LOCAL_ROOT", bridge)
        self.assertIn("HERMES_HOME", bridge)
        self.assertNotIn("...process.env", bridge)
        test_patch = (PATCH_ROOT / PATCH_NAMES[6]).read_text(encoding="utf-8")
        self.assertIn("OPENAI_API_KEY", test_patch)
        self.assertIn("toBeUndefined", test_patch)

    def test_trust_centre_route_and_controls_are_wired(self) -> None:
        text = patch_text()
        for marker in (
            "TRUST_ROUTE = '/trust'",
            "Skills and MCP Trust Centre",
            "Declared capabilities",
            "Save policy",
            "Disable now",
            "Export diagnostics",
            "trustSnapshot",
            "setTrustPolicy",
            "exportTrustDiagnostics",
        ):
            with self.subTest(marker=marker):
                self.assertIn(marker, text)

    def test_documentation_explains_non_endorsement_and_redaction(self) -> None:
        docs = (ROOT / "docs" / "TRUST_CENTRE.md").read_text(encoding="utf-8")
        self.assertIn("Health is separate from trust", docs)
        self.assertIn("readOnlyHint", docs)
        self.assertIn("fail closed", docs)
        self.assertIn("does **not** inherit", docs)
        self.assertIn("environment values", docs)


if __name__ == "__main__":
    unittest.main()
