#!/usr/bin/env python3
"""Emit a credential-free Hermes messaging gateway lifecycle snapshot."""

from __future__ import annotations

import argparse
import json
from typing import Any

from gateway.config import Platform, load_gateway_config
from gateway.status import (
    get_running_pid,
    read_runtime_status,
    runtime_status_is_stale,
    runtime_status_pid_is_live,
)
from hermes_cli.env_loader import load_hermes_dotenv

_HEALTHY_PLATFORM_STATES = {"connected", "healthy", "ready", "running"}
_FAILED_PLATFORM_STATES = {"disconnected", "error", "failed", "fatal", "stopped"}


def _safe_platform_state(runtime: dict[str, Any], name: str) -> dict[str, Any]:
    raw = runtime.get("platforms", {}).get(name, {})
    if not isinstance(raw, dict):
        raw = {}
    state = str(raw.get("state") or "unknown").strip().lower()
    return {
        "name": name,
        "state": state,
        "errorCode": str(raw.get("error_code") or "") or None,
        "healthy": state in _HEALTHY_PLATFORM_STATES,
        "failed": state in _FAILED_PLATFORM_STATES,
    }


def _resolve_enabled_platforms(encoded: str | None) -> list[str]:
    if encoded is None:
        # This helper runs directly rather than through hermes_cli.main, so the
        # first inspection must load the active HERMES_HOME dotenv explicitly.
        load_hermes_dotenv()
        config = load_gateway_config()
        return sorted(
            platform.value
            for platform, platform_config in config.platforms.items()
            if platform != Platform.LOCAL and platform_config.enabled
        )

    try:
        parsed = json.loads(encoded)
    except json.JSONDecodeError as exc:
        raise SystemExit(f"invalid --enabled-platforms-json: {exc}") from exc
    if not isinstance(parsed, list) or not all(isinstance(item, str) for item in parsed):
        raise SystemExit("--enabled-platforms-json must encode a string array")
    return sorted({item.strip().lower() for item in parsed if item.strip()})


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--discover",
        action="store_true",
        help="Include a process-table scan for independent logical gateway roots.",
    )
    parser.add_argument(
        "--enabled-platforms-json",
        help="Reuse an already resolved enabled-platform list without reloading secrets.",
    )
    args = parser.parse_args()

    enabled = _resolve_enabled_platforms(args.enabled_platforms_json)
    runtime = read_runtime_status() or {}
    if not isinstance(runtime, dict):
        runtime = {}
    authoritative_pid = get_running_pid(cleanup_stale=True)
    gateway_state = str(runtime.get("gateway_state") or "stopped").strip().lower()
    platform_states = [_safe_platform_state(runtime, name) for name in enabled]
    runtime_live = runtime_status_pid_is_live(runtime)
    runtime_stale = runtime_status_is_stale(runtime)

    logical_pids: list[int] = []
    if args.discover:
        from hermes_cli.gateway import find_gateway_pids

        logical_pids = sorted({int(pid) for pid in find_gateway_pids() if int(pid) > 0})

    required = bool(enabled)
    healthy = (
        required
        and authoritative_pid is not None
        and runtime_live
        and not runtime_stale
        and gateway_state == "running"
        and all(item["healthy"] for item in platform_states)
    )

    payload = {
        "schemaVersion": 1,
        "required": required,
        "enabledPlatforms": enabled,
        "pid": authoritative_pid,
        "running": authoritative_pid is not None,
        "healthy": healthy,
        "state": gateway_state if required else "disabled",
        "runtimeLive": runtime_live,
        "runtimeStale": runtime_stale,
        "platforms": platform_states,
        "logicalPids": logical_pids,
        "duplicateLogicalRoots": len(logical_pids) > 1,
    }
    print(json.dumps(payload, separators=(",", ":"), sort_keys=True))


if __name__ == "__main__":
    main()
