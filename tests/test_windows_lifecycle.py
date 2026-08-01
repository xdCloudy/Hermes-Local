from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MODULE_PATH = ROOT / "scripts" / "ci" / "windows_lifecycle.py"
MATRIX_PATH = ROOT / "config" / "validation" / "windows-lifecycle-matrix.json"
SPEC = importlib.util.spec_from_file_location("windows_lifecycle", MODULE_PATH)
assert SPEC and SPEC.loader
windows_lifecycle = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(windows_lifecycle)


class WindowsLifecycleTests(unittest.TestCase):
    candidate = "a" * 40

    def setUp(self) -> None:
        self.matrix = json.loads(MATRIX_PATH.read_text(encoding="utf-8"))
        self.scenarios = windows_lifecycle.scenario_index(self.matrix)

    def evidence(self, scenario_id: str, *, status: str = "passed") -> dict:
        scenario = self.scenarios[scenario_id]
        environment = {
            "runnerClass": scenario["runnerClass"],
            "os": "Windows",
            "release": "11",
            "architecture": "AMD64",
        }
        if scenario["runnerClass"] == "physical-nvidia":
            environment["gpu"] = {"name": "NVIDIA fixture", "driver": "999.1"}
        return {
            "schemaVersion": 1,
            "component": "windows-lifecycle-scenario",
            "scenarioId": scenario_id,
            "category": scenario["category"],
            "candidate": self.candidate,
            "status": status,
            "startedAt": "2026-08-01T00:00:00Z",
            "completedAt": "2026-08-01T00:01:00Z",
            "environment": environment,
            "fixture": {
                "preserved": True,
                "beforeHash": "b" * 64,
                "afterHash": "b" * 64,
                "added": [],
                "removed": [],
                "changed": [],
            } if scenario["preservationRequired"] else None,
            "checks": ["fixture"],
            "logs": ["scenario.log"],
            "failures": [],
            "skipReason": None,
        }

    def test_repository_matrix_covers_all_required_categories_and_scenarios(self) -> None:
        self.assertEqual(windows_lifecycle.validate_matrix(self.matrix), [])
        self.assertEqual(len(self.matrix["scenarios"]), 49)
        self.assertTrue(windows_lifecycle.REQUIRED_SCENARIOS.issubset(self.scenarios))

    def test_matrix_rejects_duplicate_and_missing_required_scenario(self) -> None:
        matrix = json.loads(json.dumps(self.matrix))
        matrix["scenarios"] = [item for item in matrix["scenarios"] if item["id"] != "upgrade-stable"]
        matrix["scenarios"].append(dict(matrix["scenarios"][0]))

        errors = windows_lifecycle.validate_matrix(matrix)

        self.assertIn("duplicate scenario id: clean-standard", errors)
        self.assertTrue(any("upgrade-stable" in error for error in errors))

    def test_fixture_is_deterministic_across_roots_and_contains_every_user_domain(self) -> None:
        with tempfile.TemporaryDirectory() as first, tempfile.TemporaryDirectory() as second:
            first_manifest = windows_lifecycle.create_fixture(Path(first))
            second_manifest = windows_lifecycle.create_fixture(Path(second))

            self.assertEqual(first_manifest, second_manifest)
            files = set(first_manifest["files"])
            for prefix in ("config/", "data/sessions/", "data/memory/", "skills/", "cron/", "projects/", "backups/", "models/"):
                self.assertTrue(any(path.startswith(prefix) for path in files), prefix)

    def test_fixture_comparison_reports_added_removed_and_changed_files(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            manifest = windows_lifecycle.create_fixture(root)
            (root / "config" / "settings.json").write_text("mutated", encoding="utf-8")
            (root / "cron" / "jobs.json").unlink()
            (root / "extra.txt").write_text("added", encoding="utf-8")

            comparison = windows_lifecycle.compare_fixture(root, manifest)

            self.assertFalse(comparison["preserved"])
            self.assertEqual(comparison["added"], ["extra.txt"])
            self.assertEqual(comparison["removed"], ["cron/jobs.json"])
            self.assertEqual(comparison["changed"], ["config/settings.json"])

    def test_skipped_evidence_requires_an_explicit_reason(self) -> None:
        evidence = self.evidence("clean-secondary-drive", status="skipped")

        errors = windows_lifecycle.validate_evidence(
            evidence, self.scenarios["clean-secondary-drive"], self.candidate
        )

        self.assertIn("clean-secondary-drive: skipped evidence requires a reason", errors)

    def test_passed_preservation_scenario_requires_equal_fixture_hashes(self) -> None:
        evidence = self.evidence("upgrade-stable")
        evidence["fixture"]["afterHash"] = "c" * 64

        errors = windows_lifecycle.validate_evidence(
            evidence, self.scenarios["upgrade-stable"], self.candidate
        )

        self.assertIn("upgrade-stable: before/after fixture hashes differ", errors)

    def test_nvidia_evidence_requires_named_hardware_and_driver(self) -> None:
        evidence = self.evidence("physical-nvidia")
        evidence["environment"]["gpu"] = {"name": "", "driver": ""}

        errors = windows_lifecycle.validate_evidence(
            evidence, self.scenarios["physical-nvidia"], self.candidate
        )

        self.assertIn("physical-nvidia: NVIDIA evidence requires GPU name and driver", errors)

    def test_candidate_aggregate_blocks_a_failed_critical_scenario(self) -> None:
        evidence = self.evidence("clean-standard", status="failed")

        report, errors = windows_lifecycle.aggregate(
            self.matrix, [evidence], candidate=self.candidate, stable=False, workflow_run_id="123"
        )

        self.assertEqual(report["status"], "blocked")
        self.assertIn("critical scenario failed: clean-standard", errors)

    def test_candidate_aggregate_retains_noncritical_skip_reason_as_warning(self) -> None:
        evidence = self.evidence("clean-secondary-drive", status="skipped")
        evidence["skipReason"] = "No secondary volume is attached to this hosted runner."

        report, errors = windows_lifecycle.aggregate(
            self.matrix, [evidence], candidate=self.candidate, stable=False, workflow_run_id="123"
        )

        self.assertEqual(errors, [])
        self.assertEqual(report["status"], "passed-with-warnings")
        self.assertIn("clean-secondary-drive: No secondary volume", report["warnings"][0])

    def test_stable_aggregate_fails_closed_when_required_evidence_is_missing(self) -> None:
        report, errors = windows_lifecycle.aggregate(
            self.matrix, [], candidate=self.candidate, stable=True, workflow_run_id="123"
        )

        self.assertEqual(report["status"], "blocked")
        self.assertTrue(any("missing Stable-required scenario: physical-cpu" == error for error in errors))
        self.assertTrue(any("missing Stable-required scenario: physical-nvidia" == error for error in errors))

    def test_stable_aggregate_accepts_complete_hosted_and_physical_evidence(self) -> None:
        evidence = [
            self.evidence(scenario["id"])
            for scenario in self.matrix["scenarios"]
            if scenario["stableRequired"]
        ]

        report, errors = windows_lifecycle.aggregate(
            self.matrix, evidence, candidate=self.candidate, stable=True, workflow_run_id="456"
        )

        self.assertEqual(errors, [])
        self.assertEqual(report["status"], "passed")
        self.assertTrue(report["stableEvaluation"])
        self.assertEqual(report["metadata"]["workflowRunId"], "456")
        self.assertEqual(report["summary"]["passed"], len(evidence))

    def test_cli_create_and_snapshot_fixture(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "fixture"
            snapshot_path = Path(directory) / "snapshot.json"

            self.assertEqual(windows_lifecycle.main(["--matrix", str(MATRIX_PATH), "create-fixture", "--root", str(root)]), 0)
            self.assertEqual(
                windows_lifecycle.main([
                    "--matrix", str(MATRIX_PATH), "snapshot-fixture", "--root", str(root), "--output", str(snapshot_path)
                ]),
                0,
            )
            snapshot = json.loads(snapshot_path.read_text(encoding="utf-8"))
            manifest = json.loads((root / ".lifecycle-fixture.json").read_text(encoding="utf-8"))
            self.assertEqual(snapshot["treeHash"], manifest["treeHash"])


if __name__ == "__main__":
    unittest.main()
