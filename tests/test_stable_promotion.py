from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path
from types import ModuleType


ROOT = Path(__file__).resolve().parents[1]
MODULE_PATH = ROOT / "scripts" / "ci" / "stable_promotion.py"
SPEC = importlib.util.spec_from_file_location("stable_promotion", MODULE_PATH)
assert SPEC and SPEC.loader
stable_promotion = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(stable_promotion)


class StablePromotionTests(unittest.TestCase):
    def component(self, name: str, *, run_id: str = "123", acceleration: str | None = None):
        metadata = {"workflowRunId": run_id}
        if acceleration is not None:
            metadata["acceleration"] = acceleration
        return {
            "schemaVersion": 1,
            "component": name,
            "candidate": "a" * 40,
            "base": "b" * 40,
            "status": "compatible-with-warnings" if name == "llama-cpp-gpu" else "compatible",
            "testedPlatforms": [{"os": "Windows", "release": "11", "architecture": "AMD64"}],
            "metadata": metadata,
            "warnings": [],
            "failures": [],
        }

    def report(self):
        return {
            "schemaVersion": 1,
            "component": "hermes-local-upstream-compatibility",
            "generatedAt": "2026-07-31T00:00:00Z",
            "status": "compatible-with-warnings",
            "components": [
                self.component("hermes-agent"),
                self.component("llama-cpp-cpu", acceleration="cpu"),
                self.component("llama-cpp-gpu", acceleration="cuda"),
            ],
            "warnings": [],
            "failures": [],
        }

    def test_valid_report_emits_stable_manifest(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            report_path = root / "compatibility-report.json"
            manifest_path = root / "stable-promotion.json"
            report_path.write_text(json.dumps(self.report()), encoding="utf-8")

            exit_code = stable_promotion.main([
                "--report", str(report_path),
                "--compatibility-run-id", "123",
                "--manifest", str(manifest_path),
            ])

            self.assertEqual(exit_code, 0)
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
            self.assertEqual(manifest["channel"], "stable")
            self.assertEqual(manifest["status"], "approved")
            self.assertEqual(manifest["compatibility"]["workflowRunId"], "123")
            self.assertEqual(
                [component["component"] for component in manifest["components"]],
                list(stable_promotion.REQUIRED_COMPONENTS),
            )

    def test_gpu_report_is_mandatory(self):
        report = self.report()
        report["components"] = [
            component for component in report["components"]
            if component["component"] != "llama-cpp-gpu"
        ]
        errors = stable_promotion.validate_report(report, "123")
        self.assertIn("missing required component reports: llama-cpp-gpu", errors)

    def test_gpu_report_must_be_cuda(self):
        report = self.report()
        gpu = next(
            component for component in report["components"]
            if component["component"] == "llama-cpp-gpu"
        )
        gpu["metadata"]["acceleration"] = "cpu"
        errors = stable_promotion.validate_report(report, "123")
        self.assertIn("llama-cpp-gpu report was not produced by a CUDA build", errors)

    def test_every_component_must_come_from_selected_run(self):
        report = self.report()
        report["components"][0]["metadata"]["workflowRunId"] = "999"
        errors = stable_promotion.validate_report(report, "123")
        self.assertTrue(any("hermes-agent belongs to workflow run" in error for error in errors))

    def test_blocked_component_rejects_promotion(self):
        report = self.report()
        report["components"][1]["status"] = "blocked-build"
        errors = stable_promotion.validate_report(report, "123")
        self.assertIn("component llama-cpp-cpu has non-promotable status 'blocked-build'", errors)


if __name__ == "__main__":
    unittest.main()
