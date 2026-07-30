#!/usr/bin/env python3
"""Emit a credential-free Hermes messaging gateway lifecycle snapshot.

A live gateway can briefly miss its runtime-status freshness deadline while the
model process is being released or reloaded. A bounded, PID-scoped grace period
prevents one stale heartbeat from restarting the complete workstation without
masking a genuinely hung gateway indefinitely.
"""

from __future__ import annotations

import argparse
import json
import os
import tempfile
from datetime import datetime, timezone
from pathlib import Path
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
_DEFAULT_STALE_GRACE_SECONDS = 60.0
_MARKER_PATH = Path(__file__).resolve().parents[1] / "data" / "runtime" / "gateway-stale-grace.json"


def _utc_now() -> datetime:
    return datetime.now(timezone.utc)


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


def _read_grace_marker() -> dict[str, Any]:
    try:
        parsed = json.loads(_MARKER_PATH.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return {}
    return parsed if isinstance(parsed, dict) else {}


def _write_grace_marker(pid: int, first_seen: datetime) -> None:
    _MARKER_PATH.parent.mkdir(parents=True, exist_ok=True)
    payload = {
        "schemaVersion": 1,
        "pid": pid,
        "firstSeen": first_seen.isoformat().replace("+00:00", "Z"),
    }
    descriptor, temporary_name = tempfile.mkstemp(
        prefix=f"{_MARKER_PATH.name}.",
        suffix=".tmp",
        dir=_MARKER_PATH.parent,
    )
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8", newline="\n") as handle:
            json.dump(payload, handle, separators=(",", ":"), sort_keys=True)
            handle.write("\n")
        os.replace(temporary_name, _MARKER_PATH)
    finally:
        try:
            os.unlink(temporary_name)
        except FileNotFoundError:
            pass


def _clear_grace_marker() -> None:
    try:
        _MARKER_PATH.unlink()
    except FileNotFoundError:
        pass
    except OSError:
        # Snapshot generation must remain available even if cleanup is blocked.
        pass


def _parse_utc(value: Any) -> datetime | None:
    if not isinstance(value, str) or not value.strip():
        return None
    text = value.strip()
    if text.endswith("Z"):
        text = f"{text[:-1]}+00:00"
    try:
        parsed = datetime.fromisoformat(text)
    except ValueError:
        return None
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=timezone.utc)
    return parsed.astimezone(timezone.utc)


def _stale_grace(
    *,
    pid: int | None,
    runtime_stale: bool,
    otherwise_healthy: bool,
    grace_seconds: float,
) -> tuple[bool, float | None]:
    """Return whether a stale snapshot is inside its bounded PID-scoped grace."""

    if not runtime_stale or not otherwise_healthy or pid is None or grace_seconds <= 0:
        _clear_grace_marker()
        return False, None

    now = _utc_now()
    marker = _read_grace_marker()
    marker_pid = marker.get("pid")
    first_seen = _parse_utc(marker.get("firstSeen"))
    if marker_pid != pid or first_seen is None or first_seen > now:
        first_seen = now
        try:
            _write_grace_marker(pid, first_seen)
        except OSError:
            # Failure to persist grace is fail-safe: report stale immediately.
            return False, None

    age_seconds = max(0.0, (now - first_seen).total_seconds())
    return age_seconds <= grace_seconds, round(age_seconds, 3)


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
    parser.add_argument(
        "--stale-grace-seconds",
        type=float,
        default=_DEFAULT_STALE_GRACE_SECONDS,
        help="Bounded grace for a live, connected gateway with a transient stale heartbeat.",
    )
    args = parser.parse_args()
    if args.stale_grace_seconds < 0 or args.stale_grace_seconds > 300:
        raise SystemExit("--stale-grace-seconds must be between 0 and 300")

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
    otherwise_healthy = (
        required
        and authoritative_pid is not None
        and runtime_live
        and gateway_state == "running"
        and all(item["healthy"] for item in platform_states)
    )
    grace_applied, stale_age_seconds = _stale_grace(
        pid=authoritative_pid,
        runtime_stale=runtime_stale,
        otherwise_healthy=otherwise_healthy,
        grace_seconds=args.stale_grace_seconds,
    )
    healthy = otherwise_healthy and (not runtime_stale or grace_applied)

    payload = {
        "schemaVersion": 2,
        "required": required,
        "enabledPlatforms": enabled,
        "pid": authoritative_pid,
        "running": authoritative_pid is not None,
        "healthy": healthy,
        "state": gateway_state if required else "disabled",
        "runtimeLive": runtime_live,
        "runtimeStale": runtime_stale,
        "runtimeStaleGraceApplied": grace_applied,
        "runtimeStaleAgeSeconds": stale_age_seconds,
        "runtimeStaleGraceSeconds": args.stale_grace_seconds,
        "platforms": platform_states,
        "logicalPids": logical_pids,
        "duplicateLogicalRoots": len(logical_pids) > 1,
    }
    print(json.dumps(payload, separators=(",", ":"), sort_keys=True))


if __name__ == "__main__":
    main()
