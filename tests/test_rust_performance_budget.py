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


if __name__ == "__main__":
    unittest.main()
