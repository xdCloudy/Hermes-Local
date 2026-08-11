#!/usr/bin/env python3
"""Measure and enforce conservative Rust desktop artifact footprint budgets."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

MIB = 1024 * 1024
DEFAULT_MAX_BINARY_MIB = 80.0
DEFAULT_MAX_PORTABLE_MIB = 100.0


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


def evaluate(binary: Path, portable: Path | None, max_binary_mib: float, max_portable_mib: float) -> dict:
    binary_record = size_record(binary)
    checks = [
        {
            "name": "optimized-binary-size",
            "limitMiB": max_binary_mib,
            "actualMiB": binary_record["sizeMiB"],
            "passed": binary_record["sizeBytes"] <= int(max_binary_mib * MIB),
        }
    ]
    report: dict = {
        "schemaVersion": 1,
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
    report["status"] = "passed" if all(check["passed"] for check in checks) else "failed"
    return report


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--portable", type=Path)
    parser.add_argument("--max-binary-mib", type=float, default=DEFAULT_MAX_BINARY_MIB)
    parser.add_argument("--max-portable-mib", type=float, default=DEFAULT_MAX_PORTABLE_MIB)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args(argv)
    try:
        if args.max_binary_mib <= 0 or args.max_portable_mib <= 0:
            raise BudgetError("size budgets must be positive")
        report = evaluate(
            args.binary,
            args.portable,
            args.max_binary_mib,
            args.max_portable_mib,
        )
    except BudgetError as exc:
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
