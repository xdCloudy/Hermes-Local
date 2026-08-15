from __future__ import annotations

import importlib.util
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).resolve().parents[1] / "scripts" / "ci" / "rust_performance_budget.py"
SPEC = importlib.util.spec_from_file_location("rust_performance_budget", MODULE_PATH)
assert SPEC and SPEC.loader
budget = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(budget)


class RustPerformanceBudgetTests(unittest.TestCase):
    def test_passes_artifacts_inside_budget(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary = root / "hermes-local.exe"
            portable = root / "Hermes-Local-Portable-x64.zip"
            binary.write_bytes(b"x" * 1024)
            portable.write_bytes(b"y" * 2048)
            report = budget.evaluate(binary, portable, 1.0, 1.0)
            self.assertEqual(report["status"], "passed")
            self.assertTrue(all(check["passed"] for check in report["checks"]))

    def test_fails_when_binary_exceeds_budget(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary = root / "hermes-local.exe"
            binary.write_bytes(b"x" * 2048)
            report = budget.evaluate(binary, None, 0.001, 1.0)
            self.assertEqual(report["status"], "failed")
            self.assertFalse(report["checks"][0]["passed"])

    def test_missing_artifact_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            missing = Path(temporary) / "missing.exe"
            with self.assertRaisesRegex(budget.BudgetError, "artifact not found"):
                budget.evaluate(missing, None, 1.0, 1.0)

    def test_runtime_metrics_inside_budget_are_recorded(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            binary = Path(temporary) / "hermes-local.exe"
            binary.write_bytes(b"x")
            runtime = {
                "schemaVersion": 1,
                "windowReady": True,
                "startupSeconds": 2.75,
                "workingSetMiB": 384.5,
                "cpuPercent": 3.25,
                "processCount": 9,
            }
            report = budget.evaluate(
                binary,
                None,
                1.0,
                1.0,
                runtime=runtime,
                max_startup_seconds=10.0,
                max_working_set_mib=512.0,
                max_cpu_percent=10.0,
                max_process_count=12,
            )
            self.assertEqual(report["schemaVersion"], 2)
            self.assertEqual(report["status"], "passed")
            names = {check["name"] for check in report["checks"]}
            self.assertEqual(
                names,
                {
                    "optimized-binary-size",
                    "window-ready-startup",
                    "idle-working-set",
                    "idle-cpu",
                    "desktop-process-tree",
                },
            )

    def test_runtime_metric_over_budget_fails_report(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            binary = Path(temporary) / "hermes-local.exe"
            binary.write_bytes(b"x")
            runtime = {
                "windowReady": True,
                "startupSeconds": 25.0,
                "workingSetMiB": 1200.0,
                "cpuPercent": 60.0,
                "processCount": 24,
            }
            report = budget.evaluate(
                binary,
                None,
                1.0,
                1.0,
                runtime=runtime,
                max_startup_seconds=15.0,
                max_working_set_mib=1024.0,
                max_cpu_percent=50.0,
                max_process_count=20,
            )
            self.assertEqual(report["status"], "failed")
            self.assertEqual(sum(not check["passed"] for check in report["checks"]), 4)

    def test_runtime_requires_ready_window_and_valid_numbers(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            binary = Path(temporary) / "hermes-local.exe"
            binary.write_bytes(b"x")
            with self.assertRaisesRegex(budget.BudgetError, "ready top-level window"):
                budget.evaluate(
                    binary,
                    None,
                    1.0,
                    1.0,
                    runtime={
                        "windowReady": False,
                        "startupSeconds": 1,
                        "workingSetMiB": 1,
                        "cpuPercent": 1,
                        "processCount": 1,
                    },
                )
            with self.assertRaisesRegex(budget.BudgetError, "workingSetMiB"):
                budget.evaluate(
                    binary,
                    None,
                    1.0,
                    1.0,
                    runtime={
                        "windowReady": True,
                        "startupSeconds": 1,
                        "workingSetMiB": -1,
                        "cpuPercent": 1,
                        "processCount": 1,
                    },
                )


if __name__ == "__main__":
    unittest.main()
