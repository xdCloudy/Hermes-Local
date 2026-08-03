from __future__ import annotations

import base64
import gzip
import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


class HermesDesktopUpdateContractTests(unittest.TestCase):
    def read(self, relative: str) -> str:
        path = ROOT / relative
        self.assertTrue(path.is_file(), f"missing {relative}")
        return path.read_text(encoding="utf-8")


    def read_embedded(self, relative: str) -> str:
        wrapper = self.read(relative)
        block = wrapper.split("$payload = @(", 1)[1].split(") -join", 1)[0]
        payload = "".join(re.findall(r"'([A-Za-z0-9+/=]+)'", block))
        return gzip.decompress(base64.b64decode(payload)).decode("utf-8")

    def test_detached_helper_is_fail_closed_and_data_preserving(self) -> None:
        text = self.read_embedded("Invoke-Hermes-DesktopUpdate.ps1")
        for required in (
            "Wait-HermesDesktopUpdateParent",
            "Test-HermesDesktopUpdateOrigin",
            "Assert-HermesDesktopUpdateDiskSpace",
            "Enter-HermesDesktopUpdateLock",
            "Setup-Hermes-Local.ps1",
            "Update-Hermes-Local.ps1",
            "-Component Launcher",
            "Restore-PreviousLauncher",
            "SkipModel",
        ):
            self.assertIn(required, text)
        self.assertNotRegex(text, re.compile(r"\bgit\s+clean\b", re.I))
        self.assertNotRegex(text, re.compile(r"Remove-Item[^\n]+(?:models|data\\hermes|config\\launcher)", re.I))

    def test_overlay_replaces_dead_end_and_restores_source(self) -> None:
        wrapper = self.read("Apply-Hermes-LauncherOverlay.ps1")
        self.assertIn("GzipStream", wrapper)
        text = self.read_embedded("Apply-Hermes-LauncherOverlay.ps1")
        for required in (
            "checkHermesLocalDesktopUpdates",
            "applyHermesLocalDesktopUpdate",
            "waitForDesktopUpdateTask",
            "desktopUpdateHandoff",
            "expectedUpdateOperationComponent",
            "Restore-State",
        ):
            self.assertIn(required, text)
        self.assertIn("Hermes Local update poller bypass", text)

    def test_bridge_validates_component_channel_and_native_arguments(self) -> None:
        text = self.read(
            "source/hermes-launcher/overlay-src/apps/desktop/electron/hermes-local-desktop-update.ts"
        )
        self.assertIn("HERMES_LOCAL_APPLICATION_COMPONENT", text)
        self.assertIn("Pinned Hermes Local updates require a target commit", text)
        self.assertRegex(text, r"\^\[0-9a-f\]\{40\}\$")
        self.assertIn("scriptRelative: 'Invoke-Hermes-DesktopUpdate.ps1'", text)
        self.assertIn("scriptRelative: 'Update-Hermes-Local.ps1'", text)

    def test_build_and_release_packages_include_overlay_with_finally_restore(self) -> None:
        for relative in ("Build-Hermes-Launcher.ps1", "Package-Hermes-Launcher.ps1"):
            text = self.read(relative)
            self.assertIn("Apply-Hermes-LauncherOverlay.ps1", text)
            self.assertIn("-Mode Apply", text)
            self.assertIn("-Mode Restore", text)
            self.assertIn("finally", text)

    def test_automated_bridge_tests_cover_handoff_and_agent_route(self) -> None:
        text = self.read(
            "source/hermes-launcher/overlay-src/apps/desktop/electron/hermes-local-desktop-update.test.ts"
        )
        self.assertIn("routes application checks", text)
        self.assertIn("preserves the existing Hermes Agent transactional route", text)
        self.assertIn("parses status and detached-helper handoff markers", text)
        self.assertIn("rejects unsafe pinned identities", text)


if __name__ == "__main__":
    unittest.main()
