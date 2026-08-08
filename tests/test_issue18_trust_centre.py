from __future__ import annotations

import re
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
    "0088-fix-trust-harden-identities-and-manage-skill-access.patch",
    "0089-test-trust-cover-source-bound-skills-and-agent-scopes.patch",
]
HUNK_RE = re.compile(r"^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@")


def patch_text() -> str:
    return "\n".join((PATCH_ROOT / name).read_text(encoding="utf-8") for name in PATCH_NAMES)


def hunk_count_errors(text: str) -> list[str]:
    lines = text.splitlines()
    errors: list[str] = []
    index = 0
    while index < len(lines):
        match = HUNK_RE.match(lines[index])
        if not match:
            index += 1
            continue
        expected_old = int(match.group(2) or 1)
        expected_new = int(match.group(4) or 1)
        old_count = 0
        new_count = 0
        header = lines[index]
        index += 1
        while index < len(lines) and (old_count < expected_old or new_count < expected_new):
            line = lines[index]
            if HUNK_RE.match(line) or line.startswith("diff --git ") or line == "-- ":
                break
            if line.startswith("\\ No newline at end of file"):
                index += 1
                continue
            if line.startswith(" "):
                old_count += 1
                new_count += 1
            elif line.startswith("-"):
                old_count += 1
            elif line.startswith("+"):
                new_count += 1
            else:
                break
            index += 1
        if (old_count, new_count) != (expected_old, expected_new):
            errors.append(
                f"{header}: expected old/new {expected_old}/{expected_new}, "
                f"counted {old_count}/{new_count}"
            )
    return errors


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

    def test_patch_hunk_counts_are_internally_consistent(self) -> None:
        for name in PATCH_NAMES:
            with self.subTest(name=name):
                errors = hunk_count_errors((PATCH_ROOT / name).read_text(encoding="utf-8"))
                self.assertEqual([], errors, "\n".join(errors))

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
            "commandSha256",
            "argumentsSha256",
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
        tests = (PATCH_ROOT / PATCH_NAMES[4]).read_text(encoding="utf-8") + (PATCH_ROOT / PATCH_NAMES[12]).read_text(encoding="utf-8")
        for marker in (
            "test_delegated_child_does_not_inherit_main_agent_grant",
            "test_source_change_and_capability_expansion_suspend_old_grants",
            "test_disable_and_remove_revoke_new_calls_immediately",
            "test_health_cannot_endorse_or_restore_permissions",
            "test_secret_values_never_land_in_identity_audit_or_diagnostics",
            "test_renderer_or_server_trust_claims_cannot_grant_authority",
            "test_same_basename_command_and_same_count_argument_changes_revoke_grants",
            "test_agent_scoped_grant_targets_delegated_agent_without_main_inheritance",
            "test_user_skill_is_default_denied_scoped_and_revision_bound",
            "test_user_skill_disable_revokes_load_immediately",
        ):
            with self.subTest(marker=marker):
                self.assertIn(marker, tests)

    def test_skills_are_native_managed_and_fail_closed(self) -> None:
        text = patch_text()
        for marker in (
            "sync_skill_identity",
            "authorize_skill_load",
            "built-in verified skill",
            "no matching agent access",
            "no matching scope access",
            "is denied by Hermes Local Trust Centre",
            "sourceLabel",
        ):
            with self.subTest(marker=marker):
                self.assertIn(marker, text)

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
