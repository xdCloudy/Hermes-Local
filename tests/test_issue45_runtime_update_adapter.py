from __future__ import annotations

import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


class RuntimeUpdateAdapterTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.adapter = (ROOT / "scripts/Hermes-RuntimeUpdateAdapter.psm1").read_text(encoding="utf-8")
        cls.install = (ROOT / "scripts/runtime/Hermes-RuntimeInstall.ps1").read_text(encoding="utf-8")
        cls.recovery = (ROOT / "scripts/runtime/Hermes-RuntimeRecovery.ps1").read_text(encoding="utf-8")
        cls.update_local = (ROOT / "Update-Hermes-Local.ps1").read_text(encoding="utf-8")
        cls.update_runtime = (ROOT / "Update-Hermes-Runtime.ps1").read_text(encoding="utf-8")
        cls.rollback_runtime = (ROOT / "Rollback-Hermes-Runtime.ps1").read_text(encoding="utf-8")

    def test_adapter_registers_every_transaction_stage(self) -> None:
        self.assertIn("Register-HermesUpdateAdapter -Name LlamaCpp", self.adapter)
        self.assertIn("AutoRollbackOnFailure = $true", self.adapter)
        for stage in (
            "check",
            "compatibility",
            "prepare",
            "verify",
            "backup",
            "promote",
            "validate",
            "rollback",
        ):
            with self.subTest(stage=stage):
                self.assertIn(f"        {stage} = {{", self.adapter)

    def test_identity_and_compatibility_are_resolved_before_install(self) -> None:
        check = self.adapter.index("$decision = Get-HermesRuntimeUpdateDecision")
        snapshot = self.adapter.index("Get-HermesLlamaRuntimeUpdateSnapshot -Decision $decision")
        guard = self.adapter.index("$identity = Assert-HermesLlamaRuntimeDecision -Decision $decision")
        install = self.adapter.index("Install-HermesLlamaRuntime -Decision $decision")
        self.assertLess(check, snapshot)
        self.assertLess(snapshot, guard)
        self.assertLess(guard, install)
        self.assertIn("modelFormat = [string]$decision.ModelFormat", self.adapter)

    def test_candidate_is_verified_and_smoke_tested_before_active_swap(self) -> None:
        stage = self.install.index("$stage = Join-Path $script:StagingRoot $transactionId")
        release_identity = self.install.index("Get-HermesReleaseAsset -Artifact $artifactSpec")
        downloaded_hash = self.install.index("Get-FileHash -LiteralPath $archive -Algorithm SHA256")
        payload_smoke = self.install.index("Test-HermesRuntimePayload -Path $payload -SmokeTest")
        manifest_validation = self.install.index("Test-HermesRuntimeAtPath -Path $payload -SmokeTest")
        retain_active = self.install.index("Move-Item -LiteralPath $script:BuildRoot -Destination $previousPath")
        promote = self.install.index("Move-Item -LiteralPath $payload -Destination $script:BuildRoot")

        self.assertLess(stage, release_identity)
        self.assertLess(release_identity, downloaded_hash)
        self.assertLess(downloaded_hash, payload_smoke)
        self.assertLess(payload_smoke, manifest_validation)
        self.assertLess(manifest_validation, retain_active)
        self.assertLess(retain_active, promote)
        self.assertIn("Published digest", self.install)
        self.assertIn("runtime-manifest.json", self.install)
        self.assertIn("provenance = [ordered]@{", self.install)

    def test_rollback_revalidates_retained_runtime_before_swap(self) -> None:
        retained_smoke = self.recovery.index("Test-HermesRuntimeAtPath -Path $previousPath -SmokeTest")
        compatibility = self.recovery.index("Assert-HermesRetainedRuntimeCompatible -Validation $retained")
        active_mutation = self.recovery.index("Move-Item -LiteralPath $script:BuildRoot -Destination $displaced")
        self.assertLess(retained_smoke, compatibility)
        self.assertLess(compatibility, active_mutation)
        self.assertIn("Get-FileHash -LiteralPath $candidate -Algorithm SHA256", self.recovery)

    def test_apply_and_rollback_preserve_user_profile(self) -> None:
        self.assertIn("config\\launcher\\user-settings.json", self.adapter)
        self.assertIn("ReadAllBytes", self.adapter)
        self.assertIn("WriteAllBytes", self.adapter)

        promote_start = self.adapter.index("        promote = {")
        validate_start = self.adapter.index("        validate = {")
        promote = self.adapter[promote_start:validate_start]
        self.assertIn("Invoke-HermesRuntimeProfilePreservingAction", promote)
        self.assertIn("Install-HermesLlamaRuntime -Decision $decision", promote)
        self.assertIn("userProfilePreserved = $true", promote)

        rollback_start = self.adapter.index("        rollback = {")
        rollback = self.adapter[rollback_start:]
        self.assertIn("Invoke-HermesRuntimeProfilePreservingAction", rollback)
        self.assertIn("Restore-HermesLlamaRuntime", rollback)
        self.assertIn("userProfilePreserved = $true", rollback)

        # The underlying installer/recovery intentionally records resolved acceleration;
        # the Update Centre adapter must contain that side effect to the runtime transaction.
        self.assertIn("Set-HermesResolvedAcceleration", self.install)
        self.assertIn("Set-HermesResolvedAcceleration", self.recovery)
        self.assertNotIn("Set-HermesSelectedModel", self.adapter)
        self.assertNotIn("Set-HermesSelectedProfile", self.adapter)
        self.assertNotIn("Save-HermesUserSettings", self.adapter)

    def test_task_centre_receives_runtime_stages_and_task_identity(self) -> None:
        for stage in (
            "check",
            "compatibility",
            "prepare",
            "verify",
            "backup",
            "promote",
            "validate",
            "rollback",
        ):
            with self.subTest(stage=stage):
                self.assertIn(f"Write-HermesRuntimeUpdateStage -Stage {stage}", self.adapter)
        self.assertIn("::hermes-update-stage::", self.adapter)

        for source in (self.update_runtime, self.rollback_runtime):
            self.assertIn("HERMES_LOCAL_TASK_ID", source)
            self.assertIn("$options.TaskId = $desktopTaskId", source)
            self.assertIn("-Component LlamaCpp", source)
            self.assertIn("-Caller $caller", source)

        self.assertIn("'LlamaCpp'", self.update_local)
        self.assertIn("HERMES_LOCAL_TASK_ID", self.update_local)
        self.assertIn("$inputRecord.TaskId = $desktopTaskId", self.update_local)

    def test_all_inventory_exposes_runtime_candidate_without_promoting_it(self) -> None:
        self.assertIn("$inventory.components.LlamaCpp = Get-HermesLlamaRuntimeUpdateSnapshot -Decision $decision", self.adapter)
        self.assertIn("Register-HermesUpdateAdapter -Name All -Adapter $managedAll -Force", self.adapter)


if __name__ == "__main__":
    unittest.main()
