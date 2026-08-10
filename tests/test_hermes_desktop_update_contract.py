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

    def read_updater(self) -> str:
        return "\n".join(
            self.read(relative)
            for relative in (
                "Invoke-Hermes-DesktopUpdate.ps1",
                "scripts/desktop-update/DesktopUpdate-Git.ps1",
                "scripts/desktop-update/DesktopUpdate-State.ps1",
                "scripts/desktop-update/DesktopUpdate-Promotion.ps1",
                "scripts/desktop-update/DesktopUpdate-Stage.ps1",
                "scripts/desktop-update/DesktopUpdate-NestedSource.ps1",
                "scripts/desktop-update/DesktopUpdate-SafeActivation.ps1",
                "scripts/desktop-update/DesktopUpdate-Reliability.ps1",
                "scripts/desktop-update/DesktopUpdate-Reliability-Platform.ps1",
                "scripts/desktop-update/DesktopUpdate-Activation.ps1",
                "scripts/desktop-update/DesktopUpdate-Activation-Core.ps1",
                "scripts/desktop-update/DesktopUpdate-StackDrain.ps1",
            )
        )

    def test_updater_stages_then_relaunches_after_draining_launcher_tree(self) -> None:
        text = self.read_updater()
        promotion = self.read("scripts/desktop-update/DesktopUpdate-Promotion.ps1")
        activation = "\n".join(
            (
                self.read("scripts/desktop-update/DesktopUpdate-Activation-Core.ps1"),
                self.read("scripts/desktop-update/DesktopUpdate-StackDrain.ps1"),
            )
        )

        for required in (
            "'Promote'",
            "pending-desktop-update.json",
            "ready-to-restart",
            "Start-HermesDesktopPromotionHelper",
            "Wait-HermesDesktopLauncherExit",
            "Test-HermesDesktopProcessIdentity",
            "Get-HermesDesktopLauncherProcesses",
            "Request-HermesDesktopLauncherClose",
            "CloseMainWindow",
            "activation-failed",
            "activationAttempts",
            "Get-HermesDesktopOwnedProcesses",
            "Stop-HermesDesktopOwnedProcesses",
            "'-DestinationDirectory'",
            "launcherStayedOpen",
            "Preparing the update in the background. Hermes Launcher will remain open.",
            "the updated launcher will reopen automatically",
            "Start-Process",
            "relaunched = $relaunched",
            "ConvertTo-HermesDesktopUpdateMarker -Name result",
            "scripts\\desktop-update",
        ):
            self.assertIn(required, text)

        self.assertIn("renderer, GPU, utility and crashpad children", activation)
        self.assertIn("Stop-Process", activation)
        self.assertIn("Get-CimInstance Win32_Process", activation)
        self.assertNotIn("ConvertTo-HermesDesktopUpdateMarker -Name helper", text)
        self.assertNotIn("Start-HermesKnownGoodLauncher", text)
        self.assertNotIn("while ($true) {\n        while (", promotion)

    def test_staged_success_marker_reports_deferred_activation(self) -> None:
        text = self.read("Invoke-Hermes-DesktopUpdate.ps1")
        for required in (
            "$stageOutput = @(Invoke-HermesDesktopUpdateStage -Plan $plan)",
            "$stageResult = @(",
            "Desktop update staging did not return a structured result.",
            "Get-HermesDesktopObjectValue",
            "-Name activationDeferred",
            "-Default $false",
            "handedOff = $activationDeferred",
            "pendingActivation = $activationDeferred",
            "Hermes Local will restart automatically to activate it.",
            "foreach ($property in $stageResult.PSObject.Properties)",
        ):
            self.assertIn(required, text)

        self.assertNotIn(
            "pendingActivation = [bool]$stageResult.activationDeferred",
            text,
        )

    def test_staging_is_isolated_and_never_moves_local_changes(self) -> None:
        text = self.read_updater()
        for required in (
            "Test-HermesDesktopUpdateOrigin",
            "Assert-HermesDesktopUpdateDiskSpace",
            "Enter-HermesDesktopUpdateLock",
            "Setup-Hermes-Local.ps1",
            "New-HermesDesktopCandidateWorktree",
            "Remove-HermesDesktopCandidateWorktree",
            "Set-HermesDesktopSourceRevision",
            "'worktree', 'add', '--detach'",
            "'fetch', '--atomic', '--no-tags'",
            "'merge', '--ff-only', '--no-edit'",
            "'reset', '--keep'",
            "pendingSource",
            "active-source-at-activation",
            "prepared Hermes Agent source promotion",
            "Hermes Agent source rollback",
            "preservesLocalChanges = $true",
            "SkipModel",
            "activeLauncherUntouched = $true",
        ):
            self.assertIn(required, text)
        for forbidden in (
            "Save-HermesDesktopWorkingTree",
            "Restore-HermesDesktopWorkingTree",
            "'stash', 'push', '--include-untracked'",
            "'stash', 'apply', '--index'",
            "'reset', '--hard'",
        ):
            self.assertNotIn(forbidden, text)
        self.assertNotRegex(text, re.compile(r"\bgit\s+clean\b", re.I))
        self.assertNotRegex(
            text,
            re.compile(
                r"Remove-Item[^\n]+(?:models|data\\hermes|config\\launcher)",
                re.I,
            ),
        )

        stack_drain = self.read(
            "scripts/desktop-update/DesktopUpdate-StackDrain.ps1"
        )
        stage_wrapper = stack_drain.split(
            "function Invoke-HermesDesktopUpdateStage", 1
        )[1]
        self.assertNotIn("-Stage stopping-services", stage_wrapper)
        self.assertNotIn(
            "-Reason 'source and dependency synchronisation'",
            stage_wrapper,
        )
        self.assertIn(
            "Stop-HermesDesktopOwnedProcesses",
            stack_drain.split("function Wait-HermesDesktopLauncherExit", 1)[1],
        )

    def test_candidate_checkout_supports_windows_long_paths_and_recovery(self) -> None:
        for relative in (
            "Setup-Hermes-Local.Impl.ps1",
            "Setup-Hermes-Local.Prebuilt.ps1",
        ):
            text = self.read(relative)
            with self.subTest(relative=relative):
                self.assertIn("'-c', 'core.longpaths=true'", text)
                self.assertIn(
                    "'-C', $Path, 'config', 'core.longpaths', 'true'",
                    text,
                )

        reliability = self.read(
            "scripts/desktop-update/DesktopUpdate-Reliability.ps1"
        )
        self.assertIn(
            "'candidate-source\\source\\hermes-agent'",
            reliability,
        )
        self.assertIn("$sourceIsUpdaterOwned -or", reliability)
        self.assertIn(
            "-not (Get-HermesDesktopNestedSourceChanges -Repository $source)",
            reliability,
        )

        updater_git = self.read(
            "scripts/desktop-update/DesktopUpdate-Git.ps1"
        )
        candidate_removal = updater_git.split(
            "function Remove-HermesDesktopCandidateWorktree", 1
        )[1]
        self.assertIn("'-c', 'core.longpaths=true'", candidate_removal)
        self.assertIn("'worktree', 'remove', '--force'", candidate_removal)

    def test_launcher_builder_supports_an_isolated_destination(self) -> None:
        text = self.read("Build-Hermes-Launcher.ps1")
        for required in (
            "[string] $DestinationDirectory",
            "Resolve-HermesLauncherDestination",
            "Launcher build destination cannot be the Hermes Local root",
            "Launcher build destination is outside the Hermes Local root",
            "Launcher build destination overlaps protected Hermes Local state",
            "Copy-Item -Destination $destination",
        ):
            self.assertIn(required, text)

    def test_overlay_replaces_dead_end_and_restores_source(self) -> None:
        wrapper = self.read("Apply-Hermes-LauncherOverlay.ps1")
        self.assertIn("GzipStream", wrapper)
        for required in (
            "$strictRequiredReplacers",
            "$idempotentRequiredReplacers",
            "expected one source match or one applied match",
            "$sourceCount -eq 0 -and $appliedCount -eq 1",
            "$Description -eq 'Hermes Local update poller bypass'",
            "$Text -notmatch 'window\\.hermesDesktop\\?\\.localWorkstation'",
        ):
            self.assertIn(required, wrapper)
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

    def test_overlay_recovers_a_stale_selected_model_registration(self) -> None:
        text = self.read_embedded("Apply-Hermes-LauncherOverlay.ps1")
        for required in (
            "apps\\desktop\\electron\\hermes-local-settings.ts",
            "Selected model recovery plan",
            "const requestedModelId",
            "models.find(model => model.installed)",
            "No registered Hermes Local model is available",
        ):
            self.assertIn(required, text)

    def test_custom_model_manifests_are_machine_owned(self) -> None:
        text = self.read(".gitignore")
        self.assertIn("/models/manifests/*", text)
        self.assertIn(
            "!/models/manifests/Qwen3.6-35B-A3B-APEX-MTP-I-Quality.json",
            text,
        )
        self.assertNotIn("!/models/manifests/**", text)

    def test_bridge_validates_component_channel_and_native_arguments(self) -> None:
        text = self.read(
            "source/hermes-launcher/overlay-src/apps/desktop/electron/"
            "hermes-local-desktop-update.ts"
        )
        self.assertIn("HERMES_LOCAL_APPLICATION_COMPONENT", text)
        self.assertIn(
            "Pinned Hermes Local updates require a target commit",
            text,
        )
        self.assertRegex(text, r"\^\[0-9a-f\]\{40\}\$")
        self.assertIn(
            "scriptRelative: 'Invoke-Hermes-DesktopUpdate.ps1'",
            text,
        )
        self.assertIn(
            "scriptRelative: 'Update-Hermes-Local.ps1'",
            text,
        )

    def test_build_and_release_packages_restore_overlay(self) -> None:
        for relative in (
            "Build-Hermes-Launcher.ps1",
            "Package-Hermes-Launcher.ps1",
        ):
            text = self.read(relative)
            self.assertIn("Apply-Hermes-LauncherOverlay.ps1", text)
            self.assertIn("-Mode Apply", text)
            self.assertIn("-Mode Restore", text)
            self.assertIn("finally", text)

    def test_automated_bridge_tests_cover_handoff_and_agent_route(self) -> None:
        text = self.read(
            "source/hermes-launcher/overlay-src/apps/desktop/electron/"
            "hermes-local-desktop-update.test.ts"
        )
        self.assertIn("routes explicit application checks", text)
        self.assertIn(
            "preserves the explicit Hermes Agent transactional route",
            text,
        )
        self.assertIn(
            "parses status, ready-to-restart results, and legacy helper handoffs",
            text,
        )
        self.assertIn("rejects unsafe identities and branch input", text)

    def test_startup_recovers_valid_pending_activation_with_current_code(self) -> None:
        helper = self.read("scripts/Repair-Hermes-DesktopUpdateState.ps1")
        startup = self.read("Start-Hermes-Local.ps1")

        for required in (
            "pending-desktop-update.json",
            "git -C $root rev-parse HEAD",
            "$currentCommit -eq $targetCommit",
            "Hermes Launcher.exe",
            "Test-HermesPendingPromotionProcess",
            "Invoke-HermesPendingActivationRecovery",
            "desktop-activation-recovery.lock",
            "-Mode Promote",
            "currently installed updater code",
            "Wait-HermesPendingActivationResolution",
            "Move-Item",
            ".stale-$stamp",
            "Archived stale Desktop update state",
        ):
            self.assertIn(required, helper)

        self.assertIn("Repair-Hermes-DesktopUpdateState.ps1", startup)
        self.assertIn("Desktop update-state repair failed", startup)
        self.assertNotIn("Remove-Item -LiteralPath $pendingDist", helper)


if __name__ == "__main__":
    unittest.main()
