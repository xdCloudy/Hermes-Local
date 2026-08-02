from __future__ import annotations

import importlib.util
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).resolve().parents[1] / "scripts" / "verify_model_identity.py"
SPEC = importlib.util.spec_from_file_location("verify_model_identity", MODULE_PATH)
assert SPEC and SPEC.loader
identity = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(identity)


class VerifyModelIdentityTests(unittest.TestCase):
    def status(self):
        return {
            "phase": "running",
            "selectedModelId": "agents-a1",
            "model": {"alias": "agents-a1-local", "healthy": True},
            "hermes": {"healthy": True},
            "gateway": {"required": True, "healthy": True},
        }

    def config(self):
        return {
            "model": {"provider": "local-llama", "default": "agents-a1-local"},
            "providers": {
                "local-llama": {
                    "default_model": "agents-a1-local",
                    "models": {"agents-a1-local": {"context_length": 65536}},
                }
            },
        }

    def test_accepts_agreeing_identity(self):
        result = identity.validate(self.status(), self.config(), "agents-a1", "agents-a1-local")
        self.assertTrue(result["ok"])
        self.assertEqual(result["mismatches"], [])

    def test_reports_runtime_and_provider_mismatches(self):
        status = self.status()
        status["selectedModelId"] = "qwen"
        status["model"]["alias"] = "qwen-local"
        config = self.config()
        config["providers"]["local-llama"]["models"]["stale"] = {}
        result = identity.validate(status, config, "agents-a1", "agents-a1-local")
        self.assertFalse(result["ok"])
        self.assertGreaterEqual(len(result["mismatches"]), 3)

    def test_requires_healthy_required_gateway(self):
        status = self.status()
        status["gateway"]["healthy"] = False
        result = identity.validate(status, self.config(), "agents-a1", "agents-a1-local")
        self.assertIn("required gateway health is not ready", result["mismatches"])


if __name__ == "__main__":
    unittest.main()
