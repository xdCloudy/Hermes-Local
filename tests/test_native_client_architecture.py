from __future__ import annotations

import importlib.util
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CHECKER = ROOT / "scripts" / "ci" / "check_native_client_architecture.py"
SPEC = importlib.util.spec_from_file_location("native_client_architecture", CHECKER)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class NativeClientArchitectureTests(unittest.TestCase):
    def test_repository_owns_desktop_and_patches_only_harness(self) -> None:
        result = MODULE.validate(ROOT)
        self.assertEqual(result["clientSource"], "apps/desktop")
        self.assertGreater(result["trackedClientFiles"], 1_000)
        self.assertEqual(result["harnessPatchCount"], 25)


if __name__ == "__main__":
    unittest.main()

