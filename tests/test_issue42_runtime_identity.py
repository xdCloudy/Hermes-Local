from __future__ import annotations

import json
import unittest
from pathlib import Path

from jsonschema import Draft202012Validator, FormatChecker

ROOT = Path(__file__).resolve().parents[1]


class RuntimeIdentityContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.catalog = json.loads((ROOT / "config/runtime/llama-runtime-catalog.json").read_text(encoding="utf-8"))
        cls.schema = json.loads((ROOT / "config/schemas/llama-runtime-catalog.schema.json").read_text(encoding="utf-8"))
        Draft202012Validator.check_schema(cls.schema)
        Draft202012Validator(cls.schema, format_checker=FormatChecker()).validate(cls.catalog)

    def test_catalog_declares_managed_lifecycle_locations(self) -> None:
        lifecycle = self.catalog["lifecycle"]
        self.assertEqual(lifecycle["stagingRoot"], "runtimes/llama.cpp/managed/staging")
        self.assertEqual(lifecycle["activePath"], "runtimes/llama.cpp/build")
        self.assertEqual(lifecycle["retainedRoot"], "runtimes/llama.cpp/managed/rollback")
        self.assertEqual(lifecycle["statePath"], "runtimes/llama.cpp/managed/current.json")
        self.assertEqual(lifecycle["historyPath"], "runtimes/llama.cpp/managed/history.json")
        self.assertEqual(lifecycle["diagnosticPath"], "data/runtime/llama-runtime.json")

    def test_every_package_has_full_identity_and_provenance_contract(self) -> None:
        ids: set[str] = set()
        for package in self.catalog["packages"]:
            with self.subTest(package=package["id"]):
                self.assertNotIn(package["id"], ids)
                ids.add(package["id"])
                self.assertEqual(package["modelFormats"], ["gguf"])
                self.assertEqual(package["integrity"]["algorithm"], "sha256")
                self.assertTrue(package["integrity"]["requirePublishedDigests"])
                self.assertTrue(package["integrity"]["recordPayloadInventory"])
                self.assertEqual(package["dependencyInventory"]["mode"], "payload-file-inventory")
                self.assertEqual(package["dependencyInventory"]["hashAlgorithm"], "sha256")
                self.assertIn(".exe", package["dependencyInventory"]["includeExtensions"])
                self.assertIn(".dll", package["dependencyInventory"]["includeExtensions"])
                self.assertEqual(package["provenance"]["provider"], "github-release-assets")
                self.assertEqual(package["provenance"]["sourceCommit"], package["sourceCommit"])
                self.assertTrue(package["licenses"])
                for artifact in package["artifacts"]:
                    self.assertEqual(artifact["repository"], package["provenance"]["repository"])
                    self.assertEqual(artifact["tag"], package["provenance"]["tag"])

    def test_identity_is_not_filename_based_and_is_rechecked_before_download(self) -> None:
        identity = (ROOT / "scripts/runtime/Hermes-RuntimeIdentity.ps1").read_text(encoding="utf-8")
        install = (ROOT / "scripts/runtime/Hermes-RuntimeInstall.ps1").read_text(encoding="utf-8")
        self.assertIn("Get-HermesRuntimeIdentityFingerprint", identity)
        self.assertIn("SHA256]::HashData", identity)
        self.assertIn("family =", identity)
        self.assertIn("hardwareBackend =", identity)
        self.assertIn("revision =", identity)
        self.assertIn("buildFlags =", identity)
        self.assertIn("modelFormats =", identity)
        guard = install.index("Assert-HermesLlamaRuntimeDecision")
        first_release_resolution = install.index("Get-HermesReleaseAsset -Artifact")
        self.assertLess(guard, first_release_resolution)

    def test_rollback_verifies_integrity_and_compatibility_before_swap(self) -> None:
        recovery = (ROOT / "scripts/runtime/Hermes-RuntimeRecovery.ps1").read_text(encoding="utf-8")
        integrity = recovery.index("Get-FileHash -LiteralPath $candidate -Algorithm SHA256")
        compatibility = recovery.index("Assert-HermesRetainedRuntimeCompatible -Validation $retained")
        mutation = recovery.index("Move-Item -LiteralPath $script:BuildRoot -Destination $displaced")
        self.assertLess(integrity, mutation)
        self.assertLess(compatibility, mutation)

    def test_update_surfaces_share_managed_runtime_adapter(self) -> None:
        adapter = (ROOT / "scripts/Hermes-RuntimeUpdateAdapter.psm1").read_text(encoding="utf-8")
        update_local = (ROOT / "Update-Hermes-Local.ps1").read_text(encoding="utf-8")
        update_runtime = (ROOT / "Update-Hermes-Runtime.ps1").read_text(encoding="utf-8")
        rollback_runtime = (ROOT / "Rollback-Hermes-Runtime.ps1").read_text(encoding="utf-8")
        self.assertIn("Register-HermesUpdateAdapter -Name LlamaCpp", adapter)
        self.assertIn("Get-HermesLlamaRuntimeUpdateSnapshot", adapter)
        self.assertIn("Install-HermesLlamaRuntime -Decision", adapter)
        self.assertIn("Restore-HermesLlamaRuntime", adapter)
        for source in (update_local, update_runtime, rollback_runtime):
            self.assertIn("Hermes-RuntimeUpdateAdapter.psm1", source)
        self.assertIn("Invoke-HermesUpdateOperation", update_runtime)
        self.assertIn("-Component LlamaCpp", update_runtime)
        self.assertIn("Invoke-HermesUpdateOperation", rollback_runtime)
        self.assertIn("-Component LlamaCpp", rollback_runtime)


if __name__ == "__main__":
    unittest.main()
