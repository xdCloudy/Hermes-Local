#!/usr/bin/env python3
"""Measure and enforce conservative Rust desktop artifact and runtime budgets."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

MIB = 1024 * 1024
DEFAULT_MAX_BINARY_MIB = 80.0
DEFAULT_MAX_PORTABLE_MIB = 100.0
DEFAULT_MAX_STARTUP_SECONDS = 15.0
DEFAULT_MAX_WORKING_SET_MIB = 1024.0
DEFAULT_MAX_CPU_PERCENT = 50.0
DEFAULT_MAX_PROCESS_COUNT = 20


class BudgetError(RuntimeError):
    pass


def size_record(path: Path) -> dict[str, int | float | str]:
    if not path.is_file():
        raise BudgetError(f"artifact not found: {path}")
    size = path.stat().st_size
    return {
        "path": str(path),
        "sizeBytes": size,
        "sizeMiB": round(size / MIB, 2),
    }


def _runtime_number(runtime: dict[str, Any], key: str) -> float:
    value = runtime.get(key)
    if isinstance(value, bool) or not isinstance(value, (int, float)) or value < 0:
        raise BudgetError(f"runtime report field {key!r} must be a non-negative number")
    return float(value)


def runtime_checks(
    runtime: dict[str, Any],
    max_startup_seconds: float,
    max_working_set_mib: float,
    max_cpu_percent: float,
    max_process_count: int,
) -> list[dict[str, Any]]:
    if runtime.get("windowReady") is not True:
        raise BudgetError("runtime report did not observe a ready top-level window")
    startup = _runtime_number(runtime, "startupSeconds")
    working_set = _runtime_number(runtime, "workingSetMiB")
    cpu = _runtime_number(runtime, "cpuPercent")
    process_count = _runtime_number(runtime, "processCount")
    return [
        {
            "name": "window-ready-startup",
            "limitSeconds": max_startup_seconds,
            "actualSeconds": round(startup, 3),
            "passed": startup <= max_startup_seconds,
        },
        {
            "name": "idle-working-set",
            "limitMiB": max_working_set_mib,
            "actualMiB": round(working_set, 2),
            "passed": working_set <= max_working_set_mib,
        },
        {
            "name": "idle-cpu",
            "limitPercent": max_cpu_percent,
            "actualPercent": round(cpu, 2),
            "passed": cpu <= max_cpu_percent,
        },
        {
            "name": "desktop-process-tree",
            "limitProcesses": max_process_count,
            "actualProcesses": int(process_count),
            "passed": process_count <= max_process_count,
        },
    ]


def evaluate(
    binary: Path,
    portable: Path | None,
    max_binary_mib: float,
    max_portable_mib: float,
    *,
    runtime: dict[str, Any] | None = None,
    max_startup_seconds: float = DEFAULT_MAX_STARTUP_SECONDS,
    max_working_set_mib: float = DEFAULT_MAX_WORKING_SET_MIB,
    max_cpu_percent: float = DEFAULT_MAX_CPU_PERCENT,
    max_process_count: int = DEFAULT_MAX_PROCESS_COUNT,
) -> dict[str, Any]:
    binary_record = size_record(binary)
    checks: list[dict[str, Any]] = [
        {
            "name": "optimized-binary-size",
            "limitMiB": max_binary_mib,
            "actualMiB": binary_record["sizeMiB"],
            "passed": binary_record["sizeBytes"] <= int(max_binary_mib * MIB),
        }
    ]
    report: dict[str, Any] = {
        "schemaVersion": 2,
        "binary": binary_record,
        "checks": checks,
    }
    if portable is not None:
        portable_record = size_record(portable)
        checks.append(
            {
                "name": "portable-package-size",
                "limitMiB": max_portable_mib,
                "actualMiB": portable_record["sizeMiB"],
                "passed": portable_record["sizeBytes"] <= int(max_portable_mib * MIB),
            }
        )
        report["portable"] = portable_record
    if runtime is not None:
        checks.extend(
            runtime_checks(
                runtime,
                max_startup_seconds,
                max_working_set_mib,
                max_cpu_percent,
                max_process_count,
            )
        )
        report["runtime"] = runtime
    report["status"] = "passed" if all(check["passed"] for check in checks) else "failed"
    return report


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--portable", type=Path)
    parser.add_argument("--runtime-report", type=Path)
    parser.add_argument("--max-binary-mib", type=float, default=DEFAULT_MAX_BINARY_MIB)
    parser.add_argument("--max-portable-mib", type=float, default=DEFAULT_MAX_PORTABLE_MIB)
    parser.add_argument("--max-startup-seconds", type=float, default=DEFAULT_MAX_STARTUP_SECONDS)
    parser.add_argument("--max-working-set-mib", type=float, default=DEFAULT_MAX_WORKING_SET_MIB)
    parser.add_argument("--max-cpu-percent", type=float, default=DEFAULT_MAX_CPU_PERCENT)
    parser.add_argument("--max-process-count", type=int, default=DEFAULT_MAX_PROCESS_COUNT)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args(argv)
    try:
        numeric_limits = (
            args.max_binary_mib,
            args.max_portable_mib,
            args.max_startup_seconds,
            args.max_working_set_mib,
            args.max_cpu_percent,
        )
        if any(value <= 0 for value in numeric_limits) or args.max_process_count <= 0:
            raise BudgetError("all budgets must be positive")
        runtime = None
        if args.runtime_report is not None:
            if not args.runtime_report.is_file():
                raise BudgetError(f"runtime report not found: {args.runtime_report}")
            decoded = json.loads(args.runtime_report.read_text(encoding="utf-8"))
            if not isinstance(decoded, dict):
                raise BudgetError("runtime report must be a JSON object")
            runtime = decoded
        report = evaluate(
            args.binary,
            args.portable,
            args.max_binary_mib,
            args.max_portable_mib,
            runtime=runtime,
            max_startup_seconds=args.max_startup_seconds,
            max_working_set_mib=args.max_working_set_mib,
            max_cpu_percent=args.max_cpu_percent,
            max_process_count=args.max_process_count,
        )
    except (BudgetError, json.JSONDecodeError) as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2
    payload = json.dumps(report, indent=2, sort_keys=True) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(payload, encoding="utf-8")
    print(payload, end="")
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
