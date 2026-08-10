from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock

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

    def test_llama_cpp_python_test_requirements_are_exactly_pinned(self) -> None:
        self.assertEqual(
            compatibility.LLAMA_CPP_TEST_PYTHON_REQUIREMENTS,
            ("jinja2==3.1.6",),
        )
        with mock.patch.object(compatibility, "run") as run_mock:
            compatibility.install_python_requirements(
                compatibility.LLAMA_CPP_TEST_PYTHON_REQUIREMENTS,
                cwd=Path("work/llama.cpp"),
                log=Path("logs/dependencies.log"),
            )
        run_mock.assert_called_once_with(
            [
                compatibility.sys.executable,
                "-m",
                "pip",
                "install",
                "--disable-pip-version-check",
                "jinja2==3.1.6",
            ],
            cwd=Path("work/llama.cpp"),
            log=Path("logs/dependencies.log"),
            timeout=900,
        )

    def test_hermes_agent_uses_exact_supported_npm_cli(self) -> None:
        expected_executable = "npx.cmd" if compatibility.os.name == "nt" else "npx"
        self.assertEqual(
            compatibility.npm_command("ci", "--no-audit"),
            [expected_executable, "--yes", "npm@12.0.0", "ci", "--no-audit"],
        )

    def test_existing_uv_is_reused_without_installation(self) -> None:
        with (
            mock.patch.object(compatibility, "uv", return_value="C:/tools/uv.exe"),
            mock.patch.object(compatibility, "install_python_requirements") as install_mock,
        ):
            executable = compatibility.ensure_uv(
                cwd=Path("work/hermes-agent"),
                log=Path("logs/dependencies.log"),
            )
        self.assertEqual(executable, "C:/tools/uv.exe")
        install_mock.assert_not_called()

    def test_missing_uv_uses_exact_fallback(self) -> None:
        with (
            mock.patch.object(
                compatibility,
                "uv",
                side_effect=[RuntimeError("missing"), "C:/Python/Scripts/uv.exe"],
            ),
            mock.patch.object(compatibility, "install_python_requirements") as install_mock,
        ):
            executable = compatibility.ensure_uv(
                cwd=Path("work/hermes-agent"),
                log=Path("logs/dependencies.log"),
            )
        self.assertEqual(executable, "C:/Python/Scripts/uv.exe")
        install_mock.assert_called_once_with(
            ("uv==0.11.32",),
            cwd=Path("work/hermes-agent"),
            log=Path("logs/dependencies.log"),
        )

    def test_uv_sync_installs_declared_test_extra_and_honors_lock(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            source = Path(temporary)
            (source / "uv.lock").touch()
            self.assertEqual(
                compatibility.uv_sync_command("uv", source),
                ["uv", "sync", "--extra", "all", "--extra", "dev", "--frozen"],
            )

    def test_prepare_agent_checkout_fetches_base_and_candidate_objects(self) -> None:
        base = "a" * 40
        candidate = "b" * 40
        work = Path("work")
        source = work / "hermes-agent"
        log = Path("logs/patches.log")
        with mock.patch.object(compatibility, "run") as run_mock:
            compatibility.prepare_hermes_agent_checkout(
                "https://example.invalid/hermes-agent.git",
                source,
                base=base,
                candidate=candidate,
                work=work,
                log=log,
            )
        commands = [call.args[0] for call in run_mock.call_args_list]
        self.assertEqual(commands[0][:5], ["git", "-c", "core.longpaths=true", "clone", "--no-checkout"])
        self.assertNotIn("--filter=blob:none", commands[0])
        self.assertIn(["git", "config", "core.longpaths", "true"], commands)
        self.assertIn(["git", "fetch", "origin", base], commands)
        self.assertIn(["git", "fetch", "origin", candidate], commands)
        self.assertEqual(commands[-1], ["git", "checkout", "--detach", base])

    def test_prepare_agent_checkout_fetches_identical_revision_once(self) -> None:
        revision = "a" * 40
        with mock.patch.object(compatibility, "run") as run_mock:
            compatibility.prepare_hermes_agent_checkout(
                "https://example.invalid/hermes-agent.git",
                Path("work/hermes-agent"),
                base=revision,
                candidate=revision,
                work=Path("work"),
                log=Path("logs/patches.log"),
            )
        fetches = [
            call.args[0]
            for call in run_mock.call_args_list
            if call.args[0][:2] == ["git", "fetch"]
        ]
        self.assertEqual(fetches, [["git", "fetch", "origin", revision]])

    def test_seed_patch_preimages_replays_series_and_verifies_tree(self) -> None:
        patches = [Path("0001.patch"), Path("0002.patch")]
        expected_tree = "c" * 40

        def fake_run(command, **kwargs):
            if command[:3] == ["git", "rev-parse", "HEAD^{tree}"]:
                return expected_tree
            return ""

        with mock.patch.object(compatibility, "run", side_effect=fake_run) as run_mock:
            tree = compatibility.seed_patch_preimages(
                Path("work/hermes-agent"),
                patches,
                expected_tree=expected_tree,
                log=Path("logs/patches.log"),
            )

        commands = [call.args[0] for call in run_mock.call_args_list]
        self.assertIn(
            [
                "git",
                "am",
                "--3way",
                "--committer-date-is-author-date",
                *patches,
            ],
            commands,
        )
        self.assertEqual(tree, expected_tree)

    def test_seed_patch_preimages_rejects_tree_mismatch(self) -> None:
        with mock.patch.object(compatibility, "run", side_effect=["", "", "d" * 40]):
            with self.assertRaisesRegex(RuntimeError, "expected"):
                compatibility.seed_patch_preimages(
                    Path("work/hermes-agent"),
                    [Path("0001.patch")],
                    expected_tree="c" * 40,
                    log=Path("logs/patches.log"),
                )

    def test_lockfile_recovery_ignores_non_lock_conflicts(self) -> None:
        with mock.patch.object(compatibility, "run") as run_mock:
            recovered = compatibility.recover_npm_lockfile_conflict(
                Path("work/hermes-agent"),
                ["apps/desktop/electron/main.ts"],
                log=Path("logs/patches.log"),
            )
        self.assertFalse(recovered)
        run_mock.assert_not_called()

    def test_lockfile_recovery_regenerates_and_continues_patch(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            source = Path(temporary)
            (source / "package.json").write_text("{}\n", encoding="utf-8")
            (source / "package-lock.json").write_text("{}\n", encoding="utf-8")

            def fake_run(command, **kwargs):
                if command == ["git", "diff", "--name-only"]:
                    return "package-lock.json"
                if command == ["git", "diff", "--name-only", "--diff-filter=U"]:
                    return ""
                return ""

            with mock.patch.object(compatibility, "run", side_effect=fake_run) as run_mock:
                recovered = compatibility.recover_npm_lockfile_conflict(
                    source,
                    [r"package-lock.json"],
                    log=Path("logs/patches.log"),
                )

        self.assertTrue(recovered)
        commands = [call.args[0] for call in run_mock.call_args_list]
        self.assertEqual(commands[0], ["git", "checkout", "--ours", "--", "package-lock.json"])
        self.assertIn(
            compatibility.npm_command(
                "install",
                "--package-lock-only",
                "--ignore-scripts",
                "--no-audit",
                "--fund=false",
            ),
            commands,
        )
        self.assertIn(["git", "add", "--", "package-lock.json"], commands)
        self.assertEqual(commands[-1], ["git", "am", "--continue"])

    def test_lockfile_recovery_rejects_unexpected_npm_changes(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            source = Path(temporary)
            (source / "package.json").write_text("{}\n", encoding="utf-8")
            (source / "package-lock.json").write_text("{}\n", encoding="utf-8")

            def fake_run(command, **kwargs):
                if command == ["git", "diff", "--name-only"]:
                    return "package-lock.json\npackage.json"
                return ""

            with mock.patch.object(compatibility, "run", side_effect=fake_run):
                with self.assertRaisesRegex(RuntimeError, "unexpected files"):
                    compatibility.recover_npm_lockfile_conflict(
                        source,
                        ["package-lock.json"],
                        log=Path("logs/patches.log"),
                    )

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
