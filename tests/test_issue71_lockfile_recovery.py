from __future__ import annotations

import importlib.util
import tempfile
import unittest
from pathlib import Path
from unittest import mock

MODULE_PATH = Path(__file__).resolve().parents[1] / "scripts" / "ci" / "compatibility.py"
SPEC = importlib.util.spec_from_file_location("hermes_compatibility_issue71", MODULE_PATH)
assert SPEC and SPEC.loader
compatibility = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(compatibility)


class Issue71LockfileRecoveryTests(unittest.TestCase):
    def test_recovery_requires_both_package_manifests(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            source = Path(temporary)
            (source / "package-lock.json").write_text("{}\n", encoding="utf-8")
            with mock.patch.object(compatibility, "run") as run_mock:
                recovered = compatibility.recover_npm_lockfile_conflict(
                    source,
                    ["package-lock.json"],
                    log=Path("logs/patches.log"),
                )

        self.assertFalse(recovered)
        run_mock.assert_not_called()

    def test_recovery_rejects_an_unresolved_lockfile(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            source = Path(temporary)
            (source / "package.json").write_text("{}\n", encoding="utf-8")
            (source / "package-lock.json").write_text("{}\n", encoding="utf-8")

            def fake_run(command, **kwargs):
                if command == ["git", "diff", "--name-only"]:
                    return "package-lock.json"
                if command == ["git", "diff", "--name-only", "--diff-filter=U"]:
                    return "package-lock.json"
                return ""

            with mock.patch.object(compatibility, "run", side_effect=fake_run):
                with self.assertRaisesRegex(RuntimeError, "unresolved paths"):
                    compatibility.recover_npm_lockfile_conflict(
                        source,
                        ["package-lock.json"],
                        log=Path("logs/patches.log"),
                    )


if __name__ == "__main__":
    unittest.main()
