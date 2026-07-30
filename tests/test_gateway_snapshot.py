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
    def run_snapshot(self, *, enabled: bool, discover: bool, logical_pids=None):
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
        gateway_config = module(
            "gateway.config",
            Platform=FakePlatform,
            load_gateway_config=lambda: config,
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
        packages = {
            "gateway": module("gateway"),
            "gateway.config": gateway_config,
            "gateway.status": gateway_status,
            "hermes_cli": module("hermes_cli"),
            "hermes_cli.gateway": hermes_gateway,
        }
        argv = [str(SCRIPT)] + (["--discover"] if discover else [])
        output = io.StringIO()
        with patch.dict(sys.modules, packages, clear=False), patch.object(sys, "argv", argv), redirect_stdout(output):
            runpy.run_path(str(SCRIPT), run_name="__main__")
        return json.loads(output.getvalue())

    def test_enabled_healthy_platform_and_duplicate_roots(self):
        snapshot = self.run_snapshot(enabled=True, discover=True, logical_pids=[4200, 4300])
        self.assertTrue(snapshot["required"])
        self.assertTrue(snapshot["healthy"])
        self.assertEqual(snapshot["enabledPlatforms"], ["discord"])
        self.assertTrue(snapshot["duplicateLogicalRoots"])
        self.assertEqual(snapshot["logicalPids"], [4200, 4300])
        self.assertNotIn("secret text", json.dumps(snapshot))

    def test_disabled_gateway_is_explicit_and_not_healthy(self):
        snapshot = self.run_snapshot(enabled=False, discover=False)
        self.assertFalse(snapshot["required"])
        self.assertFalse(snapshot["running"])
        self.assertFalse(snapshot["healthy"])
        self.assertEqual(snapshot["state"], "disabled")
        self.assertEqual(snapshot["enabledPlatforms"], [])


if __name__ == "__main__":
    unittest.main()
