from __future__ import annotations

import io
import json
import runpy
import sys
import types
import unittest
from contextlib import redirect_stdout
from enum import Enum
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "gateway_snapshot.py"


class FakePlatform(Enum):
    LOCAL = "local"
    DISCORD = "discord"


def module(name: str, **members):
    value = types.ModuleType(name)
    for key, member in members.items():
        setattr(value, key, member)
    return value


class GatewaySnapshotTests(unittest.TestCase):
    def run_snapshot(
        self,
        *,
        enabled: bool,
        discover: bool,
        logical_pids=None,
        resolved_platforms: list[str] | None = None,
    ):
        runtime = {
            "pid": 4200,
            "kind": "hermes-gateway",
            "gateway_state": "running",
            "hermes_home": r"D:\Hermes-Local\data\hermes",
            "platforms": {
                "discord": {
                    "state": "running",
                    "error_code": None,
                    "error_message": "secret text that must not be emitted",
                }
            },
        }
        config = SimpleNamespace(
            platforms={
                FakePlatform.LOCAL: SimpleNamespace(enabled=True),
                FakePlatform.DISCORD: SimpleNamespace(enabled=enabled),
            }
        )
        config_calls = []

        def load_gateway_config():
            config_calls.append(True)
            return config

        gateway_config = module(
            "gateway.config",
            Platform=FakePlatform,
            load_gateway_config=load_gateway_config,
        )
        gateway_status = module(
            "gateway.status",
            get_running_pid=lambda cleanup_stale=True: 4200 if enabled else None,
            read_runtime_status=lambda: runtime if enabled else {},
            runtime_status_is_stale=lambda record: False,
            runtime_status_pid_is_live=lambda record: bool(enabled),
        )
        hermes_gateway = module(
            "hermes_cli.gateway",
            find_gateway_pids=lambda: list(logical_pids or []),
        )
        dotenv_calls = []
        env_loader = module(
            "hermes_cli.env_loader",
            load_hermes_dotenv=lambda: dotenv_calls.append(True),
        )
        packages = {
            "gateway": module("gateway"),
            "gateway.config": gateway_config,
            "gateway.status": gateway_status,
            "hermes_cli": module("hermes_cli"),
            "hermes_cli.env_loader": env_loader,
            "hermes_cli.gateway": hermes_gateway,
        }
        argv = [str(SCRIPT)]
        if discover:
            argv.append("--discover")
        if resolved_platforms is not None:
            argv.extend(["--enabled-platforms-json", json.dumps(resolved_platforms)])
        output = io.StringIO()
        with (
            patch.dict(sys.modules, packages, clear=False),
            patch.object(sys, "argv", argv),
            redirect_stdout(output),
        ):
            runpy.run_path(str(SCRIPT), run_name="__main__")

        expected_config_calls = [] if resolved_platforms is not None else [True]
        self.assertEqual(dotenv_calls, expected_config_calls)
        self.assertEqual(config_calls, expected_config_calls)
        return json.loads(output.getvalue())

    def test_enabled_healthy_platform_and_duplicate_roots(self):
        snapshot = self.run_snapshot(enabled=True, discover=True, logical_pids=[4200, 4300])
        self.assertTrue(snapshot["required"])
        self.assertTrue(snapshot["healthy"])
        self.assertEqual(snapshot["enabledPlatforms"], ["discord"])
        self.assertTrue(snapshot["duplicateLogicalRoots"])
        self.assertEqual(snapshot["logicalPids"], [4200, 4300])
        self.assertNotIn("secret text", json.dumps(snapshot))
        self.assertNotIn("hermesHome", snapshot)

    def test_disabled_gateway_is_explicit_and_not_healthy(self):
        snapshot = self.run_snapshot(enabled=False, discover=False)
        self.assertFalse(snapshot["required"])
        self.assertFalse(snapshot["running"])
        self.assertFalse(snapshot["healthy"])
        self.assertEqual(snapshot["state"], "disabled")
        self.assertEqual(snapshot["enabledPlatforms"], [])

    def test_resolved_platforms_skip_secret_and_config_reload(self):
        snapshot = self.run_snapshot(
            enabled=True,
            discover=False,
            resolved_platforms=["Discord", "discord"],
        )
        self.assertTrue(snapshot["healthy"])
        self.assertEqual(snapshot["enabledPlatforms"], ["discord"])


if __name__ == "__main__":
    unittest.main()
