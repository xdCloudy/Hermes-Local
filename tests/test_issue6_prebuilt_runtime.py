from __future__ import annotations

import json
import unittest
from pathlib import Path

from jsonschema import Draft202012Validator, FormatChecker

ROOT = Path(__file__).resolve().parents[1]


def runtime_sources() -> str:
    paths = [
        ROOT / "scripts" / "Hermes-RuntimeManager.psm1",
        ROOT / "scripts" / "runtime" / "Hermes-RuntimeCatalog.ps1",
        ROOT / "scripts" / "runtime" / "Hermes-RuntimeInstall.ps1",
        ROOT / "scripts" / "runtime" / "Hermes-RuntimeRecovery.ps1",
    ]
    return "".join(path.read_text(encoding="utf-8") for path in paths)


class PrebuiltRuntimeContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.catalog_path = ROOT / "config" / "runtime" / "llama-runtime-catalog.json"
        cls.schema_path = ROOT / "config" / "schemas" / "llama-runtime-catalog.schema.json"
        cls.catalog = json.loads(cls.catalog_path.read_text(encoding="utf-8"))
        cls.schema = json.loads(cls.schema_path.read_text(encoding="utf-8"))
        Draft202012Validator.check_schema(cls.schema)
        cls.validator = Draft202012Validator(cls.schema, format_checker=FormatChecker())

    def test_catalog_is_schema_valid_and_ids_are_unique(self) -> None:
        self.validator.validate(self.catalog)
        ids = [package["id"] for package in self.catalog["packages"]]
        self.assertEqual(len(ids), len(set(ids)))

    def test_initial_matrix_has_cpu_and_grouped_cuda_packages(self) -> None:
        packages = self.catalog["packages"]
        self.assertTrue(any(package["acceleration"] == "cpu" for package in packages))
        cuda = [package for package in packages if package["acceleration"] == "cuda"]
        self.assertGreaterEqual(len(cuda), 2)
        ranges = [package["compatibility"] for package in cuda]
        self.assertTrue(any(item.get("maximumComputeCapability") == "8.9" for item in ranges))
        self.assertTrue(any(item.get("minimumComputeCapability") == "9.0" for item in ranges))

    def test_every_artifact_requires_a_release_published_digest(self) -> None:
        manager = runtime_sources()
        self.assertIn("asset.digest", manager)
        self.assertIn("^sha256:([0-9a-f]{64})$", manager)
        self.assertIn("Get-FileHash -LiteralPath $archive -Algorithm SHA256", manager)
        for package in self.catalog["packages"]:
            for artifact in package["artifacts"]:
                self.assertNotIn("/", artifact["asset"])
                self.assertNotIn("\\", artifact["asset"])

    def test_promotion_is_staged_smoke_tested_and_reversible(self) -> None:
        manager = runtime_sources()
        required = [
            "Archive entry escapes the staging directory",
            "Test-HermesRuntimePayload -Path $payload -SmokeTest",
            "failed SHA-256 verification",
            "Move-Item -LiteralPath $payload -Destination $script:BuildRoot",
            "Move-Item -LiteralPath $previousPath -Destination $script:BuildRoot",
            "runtime-manifest.json",
        ]
        for marker in required:
            with self.subTest(marker=marker):
                self.assertIn(marker, manager)
        self.assertEqual(self.catalog["lifecycle"]["diagnosticPath"], "data/runtime/llama-runtime.json")
        self.assertIn("$script:DiagnosticPath = $script:Lifecycle.DiagnosticPath", manager)

    def test_normal_setup_uses_prebuilt_mode_without_native_toolchain(self) -> None:
        wrapper = (ROOT / "Setup-Hermes-Local.ps1").read_text(encoding="utf-8")
        prebuilt = (ROOT / "Setup-Hermes-Local.Prebuilt.ps1").read_text(encoding="utf-8")
        self.assertIn("[string] $LlamaRuntimeMode = 'prebuilt'", wrapper)
        self.assertIn("'Setup-Hermes-Local.Impl.ps1'", wrapper)
        self.assertIn("Use -LlamaRuntimeMode source", prebuilt)
        for forbidden in ("Kitware.CMake", "Nvidia.CUDA", "Visual Studio 17 2022", "Require-Command -Name nvcc"):
            with self.subTest(forbidden=forbidden):
                self.assertNotIn(forbidden, prebuilt)

    def test_independent_update_rollback_and_verification_entrypoints_exist(self) -> None:
        for name in (
            "Update-Hermes-Runtime.ps1",
            "Rollback-Hermes-Runtime.ps1",
            "Test-Hermes-Runtime.ps1",
        ):
            with self.subTest(name=name):
                self.assertTrue((ROOT / name).is_file())

    def test_runtime_diagnostics_include_identity_and_integrity(self) -> None:
        manager = runtime_sources()
        for marker in (
            "sourceCommit = [string]$manifest.sourceCommit",
            "buildFlags = @($manifest.buildFlags)",
            "integrity = $manifest.integrity",
            "hardware = [ordered]@{",
        ):
            with self.subTest(marker=marker):
                self.assertIn(marker, manager)


if __name__ == "__main__":
    unittest.main()
