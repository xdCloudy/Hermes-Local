from __future__ import annotations

import json
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]


class ReleaseIntegrityIntegrationTests(unittest.TestCase):
    def test_release_workflow_is_not_available_to_pull_request_signing(self) -> None:
        workflow = (ROOT / ".github/workflows/release-integrity.yml").read_text(encoding="utf-8")
        self.assertIn("if: github.event_name != 'pull_request'", workflow)
        self.assertIn("environment: stable-release", workflow)
        self.assertIn("id-token: write", workflow)
        self.assertIn("attestations: write", workflow)
        self.assertIn("HERMES_SIGNING_CERTIFICATE_PFX", workflow)
        self.assertIn("github.ref == 'refs/heads/main'", workflow)
        self.assertIn("git merge-base --is-ancestor", workflow)
        self.assertIn("dist\\attestations", workflow)

    def test_package_script_generates_authoritative_release_metadata(self) -> None:
        package = (ROOT / "Package-Hermes-Launcher.ps1").read_text(encoding="utf-8")
        self.assertIn("release_integrity.py", package)
        self.assertIn("'create'", package)
        self.assertIn("release-manifest.json", package)
        self.assertIn("SHA256SUMS", (ROOT / "scripts/ci/release_integrity.py").read_text(encoding="utf-8"))
        self.assertIn("package-manifest.json", package)

    def test_installer_verifies_before_update_operation(self) -> None:
        update = (ROOT / "Update-Hermes-Local.ps1").read_text(encoding="utf-8")
        verifier = update.index("Invoke-HermesReleasePreflight")
        operation = update.rindex("Invoke-HermesUpdateOperation")
        self.assertLess(verifier, operation)
        self.assertIn("Installer promotion requires -ReleaseManifestPath", update)
        self.assertIn("--require-attestation", update)

    def test_diagnostics_export_verification_evidence(self) -> None:
        diagnostics = (ROOT / "Export-Hermes-Diagnostics.ps1").read_text(encoding="utf-8")
        for name in ("release-manifest.json", "SHA256SUMS", "release-integrity\\LATEST.json"):
            self.assertIn(name, diagnostics)
        self.assertIn("verificationStatus", diagnostics)

    def test_release_schema_is_valid_json(self) -> None:
        schema = json.loads(
            (ROOT / "config/schemas/release-manifest.schema.json").read_text(encoding="utf-8")
        )
        self.assertEqual(schema["$schema"], "https://json-schema.org/draft/2020-12/schema")
        self.assertEqual(schema["properties"]["schemaVersion"]["const"], 1)


if __name__ == "__main__":
    unittest.main()
