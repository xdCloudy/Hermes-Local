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
    candidate_sha = "c" * 40
    matrix = {"schemaVersion": 1, "scenarios": [{"id": "fixture"}]}

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

    def lifecycle_report(self):
        def scenario(scenario_id: str, runner_class: str):
            environment = {
                "runnerClass": runner_class,
                "os": "Windows",
                "release": "11",
                "architecture": "AMD64",
            }
            if runner_class == "physical-nvidia":
                environment["gpu"] = {"name": "NVIDIA fixture", "driver": "999.1"}
            return {
                "schemaVersion": 1,
                "component": "windows-lifecycle-scenario",
                "scenarioId": scenario_id,
                "candidate": self.candidate_sha,
                "status": "passed",
                "environment": environment,
            }

        return {
            "schemaVersion": 1,
            "component": "windows-lifecycle",
            "candidate": self.candidate_sha,
            "status": "passed-with-warnings",
            "stableEvaluation": True,
            "generatedAt": "2026-08-01T00:00:00Z",
            "matrixSha256": stable_promotion.canonical_digest(self.matrix),
            "summary": {"total": 40, "passed": 35, "failed": 0, "skipped": 5},
            "scenarios": [
                scenario("physical-cpu", "physical-cpu"),
                scenario("physical-nvidia", "physical-nvidia"),
            ],
            "metadata": {"workflowRunId": "456"},
            "warnings": [{"stage": "optional", "message": "Explicit skip"}],
            "failures": [],
        }

    def test_valid_report_emits_stable_manifest(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            report_path = root / "compatibility-report.json"
            lifecycle_path = root / "windows-lifecycle-report.json"
            matrix_path = root / "windows-lifecycle-matrix.json"
            manifest_path = root / "stable-promotion.json"
            report_path.write_text(json.dumps(self.report()), encoding="utf-8")
            lifecycle_path.write_text(json.dumps(self.lifecycle_report()), encoding="utf-8")
            matrix_path.write_text(json.dumps(self.matrix), encoding="utf-8")

            exit_code = stable_promotion.main([
                "--report", str(report_path),
                "--compatibility-run-id", "123",
                "--lifecycle-report", str(lifecycle_path),
                "--lifecycle-run-id", "456",
                "--candidate-sha", self.candidate_sha,
                "--matrix", str(matrix_path),
                "--manifest", str(manifest_path),
            ])

            self.assertEqual(exit_code, 0)
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
            self.assertEqual(manifest["channel"], "stable")
            self.assertEqual(manifest["status"], "approved")
            self.assertEqual(manifest["compatibility"]["workflowRunId"], "123")
            self.assertEqual(manifest["lifecycle"]["workflowRunId"], "456")
            self.assertEqual(manifest["lifecycle"]["candidate"], self.candidate_sha)
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

    def test_lifecycle_report_must_enforce_stable_inventory(self):
        report = self.lifecycle_report()
        report["stableEvaluation"] = False

        errors = stable_promotion.validate_lifecycle_report(report, "456", self.candidate_sha)

        self.assertIn("lifecycle report did not enforce the Stable scenario inventory", errors)

    def test_lifecycle_candidate_must_match_selected_revision(self):
        report = self.lifecycle_report()

        errors = stable_promotion.validate_lifecycle_report(report, "456", "e" * 40)

        self.assertIn("lifecycle candidate does not match the selected Hermes Local revision", errors)

    def test_both_physical_lifecycle_lanes_are_mandatory(self):
        report = self.lifecycle_report()
        report["scenarios"] = [
            scenario for scenario in report["scenarios"]
            if scenario["scenarioId"] != "physical-nvidia"
        ]

        errors = stable_promotion.validate_lifecycle_report(report, "456", self.candidate_sha)

        self.assertIn("lifecycle report is missing physical-nvidia evidence", errors)

    def test_lifecycle_matrix_must_match_trusted_checkout(self):
        report = self.lifecycle_report()

        errors = stable_promotion.validate_lifecycle_report(
            report, "456", self.candidate_sha, "e" * 64
        )

        self.assertIn(
            "lifecycle report matrix digest does not match the trusted matrix", errors
        )


if __name__ == "__main__":
    unittest.main()
