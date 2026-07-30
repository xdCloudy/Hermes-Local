from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).resolve().parents[1] / "scripts" / "ci" / "compatibility.py"
SPEC = importlib.util.spec_from_file_location("hermes_compatibility", MODULE_PATH)
assert SPEC and SPEC.loader
compatibility = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(compatibility)


class CompatibilityReportTests(unittest.TestCase):
    def test_aggregate_prefers_the_most_severe_status(self) -> None:
        reports = [
            {"status": "compatible"},
            {"status": "compatible-with-warnings"},
            {"status": "blocked-tests"},
            {"status": "blocked-dependency"},
        ]
        self.assertEqual(compatibility.aggregate_status(reports), "blocked-dependency")

    def test_infrastructure_failure_is_not_upstream_incompatibility(self) -> None:
        report = compatibility.base_report("hermes-agent", "a" * 40, None, Path("logs"))
        compatibility.fail_report(
            report,
            stage="patches",
            message="network unavailable",
            infrastructure=True,
        )
        self.assertEqual(report["status"], "infrastructure-failure")
        self.assertEqual(report["failures"][0]["status"], "infrastructure-failure")

    def test_stage_failure_maps_to_specific_blocked_status(self) -> None:
        for stage, expected in compatibility.BLOCKED_BY_STAGE.items():
            with self.subTest(stage=stage):
                report = compatibility.base_report("component", "a" * 40, "b" * 40, Path("logs"))
                compatibility.fail_report(report, stage=stage, message="failed")
                self.assertEqual(report["status"], expected)

    def test_success_with_warning_is_not_reported_as_fully_compatible(self) -> None:
        report = compatibility.base_report("component", "a" * 40, "b" * 40, Path("logs"))
        compatibility.stage_pass(report, "patches")
        compatibility.stage_warning(report, "health", "real model smoke is pending")
        compatibility.finalize_success(report)
        self.assertEqual(report["status"], "compatible-with-warnings")

    def test_package_script_uses_known_windows_packaging_names(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            package = Path(temporary) / "package.json"
            package.write_text(
                json.dumps({"scripts": {"build": "vite build", "package:win": "electron-builder --win"}}),
                encoding="utf-8",
            )
            self.assertEqual(compatibility.package_script(package), "package:win")

    def test_verify_requires_named_component_reports(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            report_path = Path(temporary) / "aggregate.json"
            report_path.write_text(
                json.dumps(
                    {
                        "schemaVersion": compatibility.SCHEMA_VERSION,
                        "status": "compatible-with-warnings",
                        "components": [{"component": "hermes-agent"}],
                    }
                ),
                encoding="utf-8",
            )
            args = type(
                "Args",
                (),
                {
                    "report": str(report_path),
                    "allowed_status": ["compatible", "compatible-with-warnings"],
                    "require_component": ["hermes-agent", "llama-cpp-cpu"],
                },
            )()
            self.assertEqual(compatibility.verify(args), 1)


if __name__ == "__main__":
    unittest.main()
