from __future__ import annotations

import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


class HermesDesktopUpdateContractTests(unittest.TestCase):
    def read(self, relative: str) -> str:
        path = ROOT / relative
        self.assertTrue(path.is_file(), f"missing {relative}")
        return path.read_text(encoding="utf-8")

    def read_updater(self) -> str:
        files = [ROOT / "Invoke-Hermes-DesktopUpdate.ps1"]
        files.extend(sorted((ROOT / "scripts" / "desktop-update").glob("*.ps1")))
        return "\n".join(path.read_text(encoding="utf-8") for path in files)

    def test_updater_preserves_isolated_safe_activation(self) -> None:
        text = self.read_updater()
        for required in (
            "pending-desktop-update.json",
            "New-HermesDesktopCandidateWorktree",
            "Remove-HermesDesktopCandidateWorktree",
            "Wait-HermesDesktopLauncherExit",
            "Stop-HermesDesktopOwnedProcesses",
            "preservesLocalChanges = $true",
            "'worktree', 'add', '--detach'",
            "'-DestinationDirectory'",
        ):
            self.assertIn(required, text)
        self.assertNotIn("'reset', '--hard'", text)
        self.assertNotRegex(text, re.compile(r"\bgit\s+clean\b", re.I))

    def test_native_client_is_directly_tracked(self) -> None:
        for relative in (
            "apps/desktop/package.json",
            "apps/desktop/electron/hermes-local-desktop-update.ts",
            "apps/desktop/electron/hermes-local-desktop-update.test.ts",
            "packages/hermes-agent-client/package.json",
        ):
            self.read(relative)
        self.assertFalse((ROOT / "Apply-Hermes-LauncherOverlay.ps1").exists())
        self.assertFalse((ROOT / "source/hermes-launcher/overlay-src").exists())

    def test_build_and_package_use_root_workspace_without_overlay(self) -> None:
        for relative in ("Build-Hermes-Launcher.ps1", "Package-Hermes-Launcher.ps1"):
            text = self.read(relative)
            self.assertRegex(text, r"apps[\\/]desktop|product\.client\.sourcePath")
            self.assertIn("check_native_client_architecture.py", text)
            self.assertNotIn("Apply-Hermes-LauncherOverlay.ps1", text)
            self.assertNotIn("-Mode Restore", text)

    def test_bootstrap_and_update_bridge_still_have_native_contract(self) -> None:
        bridge = self.read("apps/desktop/electron/hermes-local-desktop-update.ts")
        for required in (
            "HERMES_LOCAL_APPLICATION_COMPONENT",
            "Pinned Hermes Local updates require a target commit",
            "Invoke-Hermes-DesktopUpdate.ps1",
            "Update-Hermes-Local.ps1",
        ):
            self.assertIn(required, bridge)

    def test_startup_recovers_pending_activation(self) -> None:
        helper = self.read("scripts/Repair-Hermes-DesktopUpdateState.ps1")
        startup = self.read("Start-Hermes-Local.ps1")
        self.assertIn("pending-desktop-update.json", helper)
        self.assertIn("Invoke-HermesPendingActivationRecovery", helper)
        self.assertIn("Repair-Hermes-DesktopUpdateState.ps1", startup)


if __name__ == "__main__":
    unittest.main()
