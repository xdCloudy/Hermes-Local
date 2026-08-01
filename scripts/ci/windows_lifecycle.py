#!/usr/bin/env python3
"""Validate Windows lifecycle scenarios and aggregate release evidence."""
from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import platform
import re
import sys
from pathlib import Path
from typing import Any, Iterable, Sequence

SCHEMA_VERSION = 1
EVIDENCE_SCHEMA_VERSION = 1
HEX_SHA = re.compile(r"^[0-9a-fA-F]{40}$")
VALID_STATUSES = {"passed", "failed", "skipped"}
VALID_RUNNERS = {"hosted-windows", "physical-cpu", "physical-nvidia"}
VALID_CATEGORIES = {"clean-install", "upgrade", "repair", "rollback", "uninstall", "adverse", "hardware"}
REQUIRED_SCENARIOS = {
    "clean-standard", "clean-portable", "clean-offline", "clean-cpu",
    "clean-path-spaces", "clean-path-unicode", "clean-interrupted-download",
    "upgrade-stable", "upgrade-old-schema", "upgrade-active-service", "upgrade-custom-config",
    "repair-launcher", "repair-python", "repair-runtime", "repair-provider",
    "repair-stale-marker", "repair-patched-source", "rollback-pre-promotion",
    "rollback-atomic-promotion", "rollback-service-restart", "rollback-model-health",
    "rollback-launcher-health", "rollback-last-known-good", "uninstall-preserve-data",
    "reinstall-preserved", "migrate-legacy", "adverse-port-conflict", "adverse-disk-space",
    "adverse-read-only", "adverse-offline", "adverse-interrupted-promotion",
    "physical-cpu", "physical-nvidia",
}
FIXTURE_FILES: dict[str, Any] = {
    "config/models.json": {"schemaVersion": 1, "models": [{"id": "fixture-model", "path": "models/fixture.gguf", "sha256": "f" * 64}]},
    "config/profiles.json": {"schemaVersion": 1, "selected": "Fixture profile", "profiles": [{"name": "Fixture profile", "contextTokens": 8192}]},
    "config/settings.json": {"schemaVersion": 1, "acceleration": "cpu", "installRoot": "D:/Hermes Fixture/日本語"},
    "data/sessions/conversation.jsonl": '{"role":"user","content":"preserve me"}\n{"role":"assistant","content":"fixture reply"}\n',
    "data/memory/memory.json": {"schemaVersion": 1, "entries": [{"scope": "project", "text": "fixture memory"}]},
    "skills/fixture-skill/SKILL.md": "# Fixture skill\n\nDeterministic lifecycle-validation content.\n",
    "cron/jobs.json": {"schemaVersion": 1, "jobs": [{"id": "fixture-job", "schedule": "0 9 * * 1", "enabled": True}]},
    "projects/registry.json": {"schemaVersion": 1, "projects": [{"id": "fixture-project", "path": "projects/workspace"}]},
    "projects/workspace/README.md": "# User workspace\n\nThis file must survive lifecycle operations.\n",
    "backups/history.json": {"schemaVersion": 1, "backups": [{"id": "fixture-backup", "sha256": "b" * 64}]},
    "models/fixture.gguf": "deterministic model registration placeholder\n",
}


def now() -> str:
    return dt.datetime.now(dt.timezone.utc).isoformat().replace("+00:00", "Z")


def read_json(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"Expected JSON object: {path}")
    return value


def write_json(path: Path, value: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    temporary.replace(path)


def canonical_json(value: Any) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode("utf-8")


def matrix_digest(matrix: dict[str, Any]) -> str:
    return hashlib.sha256(canonical_json(matrix)).hexdigest()


def scenario_index(matrix: dict[str, Any]) -> dict[str, dict[str, Any]]:
    result: dict[str, dict[str, Any]] = {}
    for item in matrix.get("scenarios", []):
        if isinstance(item, dict) and isinstance(item.get("id"), str):
            result[item["id"]] = item
    return result


def validate_matrix(matrix: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    if matrix.get("schemaVersion") != SCHEMA_VERSION:
        errors.append(f"schemaVersion must be {SCHEMA_VERSION}")
    if matrix.get("fixtureVersion") != 1:
        errors.append("fixtureVersion must be 1")
    if not isinstance(matrix.get("retentionDays"), int) or matrix["retentionDays"] < 90:
        errors.append("retentionDays must be at least 90")
    if set(matrix.get("requiredCategories", [])) != VALID_CATEGORIES:
        errors.append("requiredCategories must name every lifecycle category exactly once")
    if set(matrix.get("requiredRunnerClasses", [])) != VALID_RUNNERS:
        errors.append("requiredRunnerClasses must include hosted Windows, physical CPU and physical NVIDIA")

    raw = matrix.get("scenarios")
    if not isinstance(raw, list):
        return errors + ["scenarios must be an array"]
    seen: set[str] = set()
    for index, scenario in enumerate(raw):
        label = f"scenarios[{index}]"
        if not isinstance(scenario, dict):
            errors.append(f"{label} must be an object")
            continue
        scenario_id = scenario.get("id")
        if not isinstance(scenario_id, str) or not scenario_id:
            errors.append(f"{label}.id must be non-empty")
        elif scenario_id in seen:
            errors.append(f"duplicate scenario id: {scenario_id}")
        else:
            seen.add(scenario_id)
        if scenario.get("category") not in VALID_CATEGORIES:
            errors.append(f"{label}.category is invalid")
        if scenario.get("runnerClass") not in VALID_RUNNERS:
            errors.append(f"{label}.runnerClass is invalid")
        if scenario.get("tier") not in {"fast", "hosted", "trusted"}:
            errors.append(f"{label}.tier is invalid")
        if scenario.get("automation") not in {"package-shell", "fault-fixture", "physical-smoke", "manual-evidence"}:
            errors.append(f"{label}.automation is invalid")
        for field in ("critical", "preservationRequired", "stableRequired"):
            if not isinstance(scenario.get(field), bool):
                errors.append(f"{label}.{field} must be boolean")
    missing = sorted(REQUIRED_SCENARIOS - seen)
    if missing:
        errors.append(f"missing required scenarios: {', '.join(missing)}")
    return errors


def fixture_payload(value: Any) -> bytes:
    if isinstance(value, str):
        return value.encode("utf-8")
    return json.dumps(value, indent=2, sort_keys=True, ensure_ascii=False).encode("utf-8") + b"\n"


def create_fixture(root: Path) -> dict[str, Any]:
    root.mkdir(parents=True, exist_ok=True)
    for relative, value in FIXTURE_FILES.items():
        target = root / Path(relative)
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(fixture_payload(value))
    snapshot = snapshot_fixture(root)
    manifest = {
        "schemaVersion": EVIDENCE_SCHEMA_VERSION,
        "fixtureVersion": 1,
        "treeHash": snapshot["treeHash"],
        "files": snapshot["files"],
    }
    write_json(root / ".lifecycle-fixture.json", manifest)
    return manifest


def fixture_files(root: Path) -> Iterable[Path]:
    for path in sorted(root.rglob("*"), key=lambda item: item.as_posix().casefold()):
        if path.is_file() and path.name != ".lifecycle-fixture.json":
            yield path


def snapshot_fixture(root: Path) -> dict[str, Any]:
    files: dict[str, str] = {}
    tree = hashlib.sha256()
    if not root.is_dir():
        raise ValueError(f"Fixture root does not exist: {root}")
    for path in fixture_files(root):
        relative = path.relative_to(root).as_posix()
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        files[relative] = digest
        tree.update(relative.encode("utf-8"))
        tree.update(b"\0")
        tree.update(digest.encode("ascii"))
        tree.update(b"\n")
    return {"treeHash": tree.hexdigest(), "files": files}


def compare_fixture(root: Path, expected: dict[str, Any]) -> dict[str, Any]:
    actual = snapshot_fixture(root)
    expected_files = expected.get("files", {}) if isinstance(expected, dict) else {}
    actual_files = actual["files"]
    return {
        "preserved": actual.get("treeHash") == expected.get("treeHash") and actual_files == expected_files,
        "beforeHash": expected.get("treeHash"),
        "afterHash": actual.get("treeHash"),
        "added": sorted(set(actual_files) - set(expected_files)),
        "removed": sorted(set(expected_files) - set(actual_files)),
        "changed": sorted(path for path in set(actual_files) & set(expected_files) if actual_files[path] != expected_files[path]),
    }


def validate_evidence(evidence: dict[str, Any], scenario: dict[str, Any], candidate: str) -> list[str]:
    errors: list[str] = []
    scenario_id = scenario["id"]
    if evidence.get("schemaVersion") != EVIDENCE_SCHEMA_VERSION:
        errors.append(f"{scenario_id}: schemaVersion must be {EVIDENCE_SCHEMA_VERSION}")
    if evidence.get("component") != "windows-lifecycle-scenario":
        errors.append(f"{scenario_id}: invalid component")
    if evidence.get("scenarioId") != scenario_id:
        errors.append(f"{scenario_id}: evidence scenarioId mismatch")
    if evidence.get("candidate") != candidate:
        errors.append(f"{scenario_id}: candidate mismatch")
    status = evidence.get("status")
    if status not in VALID_STATUSES:
        errors.append(f"{scenario_id}: invalid status {status!r}")
    if status == "skipped" and not str(evidence.get("skipReason") or "").strip():
        errors.append(f"{scenario_id}: skipped evidence requires a reason")
    environment = evidence.get("environment")
    if not isinstance(environment, dict) or environment.get("runnerClass") != scenario.get("runnerClass"):
        errors.append(f"{scenario_id}: runner class mismatch")
    if scenario.get("preservationRequired") and status == "passed":
        fixture = evidence.get("fixture")
        if not isinstance(fixture, dict) or fixture.get("preserved") is not True:
            errors.append(f"{scenario_id}: passed result did not preserve the fixture")
        elif fixture.get("beforeHash") != fixture.get("afterHash"):
            errors.append(f"{scenario_id}: before/after fixture hashes differ")
    if scenario.get("runnerClass") == "physical-nvidia" and status == "passed":
        gpu = environment.get("gpu") if isinstance(environment, dict) else None
        if not isinstance(gpu, dict) or not str(gpu.get("name") or "").strip() or not str(gpu.get("driver") or "").strip():
            errors.append(f"{scenario_id}: NVIDIA evidence requires GPU name and driver")
    return errors


def load_evidence(paths: Sequence[Path]) -> list[dict[str, Any]]:
    return [read_json(path) for path in paths]


def aggregate(
    matrix: dict[str, Any], evidence_items: Sequence[dict[str, Any]], *, candidate: str,
    stable: bool, workflow_run_id: str | None,
) -> tuple[dict[str, Any], list[str]]:
    errors = validate_matrix(matrix)
    scenarios = scenario_index(matrix)
    indexed: dict[str, dict[str, Any]] = {}
    warnings: list[str] = []
    for evidence in evidence_items:
        scenario_id = evidence.get("scenarioId")
        if not isinstance(scenario_id, str) or scenario_id not in scenarios:
            errors.append(f"unknown scenario evidence: {scenario_id!r}")
            continue
        if scenario_id in indexed:
            errors.append(f"duplicate evidence for scenario: {scenario_id}")
            continue
        indexed[scenario_id] = evidence
        errors.extend(validate_evidence(evidence, scenarios[scenario_id], candidate))

    required = [scenario for scenario in scenarios.values() if scenario.get("stableRequired")] if stable else []
    if stable:
        for scenario in required:
            evidence = indexed.get(scenario["id"])
            if evidence is None:
                errors.append(f"missing Stable-required scenario: {scenario['id']}")
            elif evidence.get("status") != "passed":
                errors.append(f"Stable-required scenario {scenario['id']} is {evidence.get('status')}")
    for scenario_id, evidence in indexed.items():
        scenario = scenarios[scenario_id]
        if scenario.get("critical") and evidence.get("status") == "failed":
            errors.append(f"critical scenario failed: {scenario_id}")
        elif evidence.get("status") == "skipped":
            warnings.append(f"{scenario_id}: {evidence.get('skipReason')}")

    counts = {status: sum(1 for item in indexed.values() if item.get("status") == status) for status in VALID_STATUSES}
    report = {
        "schemaVersion": EVIDENCE_SCHEMA_VERSION,
        "component": "windows-lifecycle",
        "candidate": candidate,
        "status": "blocked" if errors else ("passed-with-warnings" if warnings else "passed"),
        "generatedAt": now(),
        "matrixSha256": matrix_digest(matrix),
        "stableEvaluation": stable,
        "summary": {"total": len(indexed), **counts},
        "scenarios": [indexed[key] for key in sorted(indexed)],
        "failures": errors,
        "warnings": warnings,
        "metadata": {"workflowRunId": workflow_run_id},
    }
    return report, errors


def record_evidence(args: argparse.Namespace, matrix: dict[str, Any]) -> dict[str, Any]:
    scenarios = scenario_index(matrix)
    scenario = scenarios.get(args.scenario)
    if scenario is None:
        raise ValueError(f"Unknown scenario: {args.scenario}")
    fixture: dict[str, Any] | None = None
    if args.fixture_root and args.fixture_manifest:
        fixture = compare_fixture(Path(args.fixture_root), read_json(Path(args.fixture_manifest)))
    environment: dict[str, Any] = {
        "runnerClass": scenario["runnerClass"],
        "os": platform.system(),
        "release": platform.release(),
        "architecture": platform.machine(),
    }
    if args.gpu_name or args.gpu_driver:
        environment["gpu"] = {"name": args.gpu_name or "", "driver": args.gpu_driver or ""}
    return {
        "schemaVersion": EVIDENCE_SCHEMA_VERSION,
        "component": "windows-lifecycle-scenario",
        "scenarioId": scenario["id"],
        "category": scenario["category"],
        "candidate": args.candidate,
        "status": args.status,
        "startedAt": args.started_at or now(),
        "completedAt": now(),
        "environment": environment,
        "fixture": fixture,
        "checks": args.check or [],
        "logs": args.log or [],
        "failures": args.failure or [],
        "skipReason": args.skip_reason,
    }


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("--matrix", default="config/validation/windows-lifecycle-matrix.json")
    commands = result.add_subparsers(dest="command", required=True)
    commands.add_parser("validate")
    create = commands.add_parser("create-fixture")
    create.add_argument("--root", required=True)
    snapshot = commands.add_parser("snapshot-fixture")
    snapshot.add_argument("--root", required=True)
    snapshot.add_argument("--output", required=True)
    record = commands.add_parser("record")
    record.add_argument("--scenario", required=True)
    record.add_argument("--candidate", required=True)
    record.add_argument("--status", choices=sorted(VALID_STATUSES), required=True)
    record.add_argument("--output", required=True)
    record.add_argument("--fixture-root")
    record.add_argument("--fixture-manifest")
    record.add_argument("--started-at")
    record.add_argument("--check", action="append")
    record.add_argument("--log", action="append")
    record.add_argument("--failure", action="append")
    record.add_argument("--skip-reason")
    record.add_argument("--gpu-name")
    record.add_argument("--gpu-driver")
    aggregate_parser = commands.add_parser("aggregate")
    aggregate_parser.add_argument("--evidence", nargs="+", required=True)
    aggregate_parser.add_argument("--candidate", required=True)
    aggregate_parser.add_argument("--output", required=True)
    aggregate_parser.add_argument("--stable", action="store_true")
    aggregate_parser.add_argument("--workflow-run-id")
    return result


def main(argv: Sequence[str] | None = None) -> int:
    args = parser().parse_args(argv)
    try:
        matrix = read_json(Path(args.matrix))
        matrix_errors = validate_matrix(matrix)
        if args.command == "validate":
            if matrix_errors:
                raise ValueError("; ".join(matrix_errors))
            print(f"Windows lifecycle matrix valid: {len(matrix['scenarios'])} scenarios; {matrix_digest(matrix)}")
            return 0
        if matrix_errors:
            raise ValueError("; ".join(matrix_errors))
        if args.command == "create-fixture":
            manifest = create_fixture(Path(args.root))
            print(json.dumps(manifest, indent=2))
            return 0
        if args.command == "snapshot-fixture":
            write_json(Path(args.output), snapshot_fixture(Path(args.root)))
            return 0
        if args.command == "record":
            if not HEX_SHA.fullmatch(args.candidate):
                raise ValueError("candidate must be a full 40-character commit")
            evidence = record_evidence(args, matrix)
            scenario = scenario_index(matrix)[args.scenario]
            errors = validate_evidence(evidence, scenario, args.candidate)
            if errors:
                raise ValueError("; ".join(errors))
            write_json(Path(args.output), evidence)
            return 0
        if args.command == "aggregate":
            if not HEX_SHA.fullmatch(args.candidate):
                raise ValueError("candidate must be a full 40-character commit")
            paths: list[Path] = []
            for pattern in args.evidence:
                matches = sorted(Path().glob(pattern)) if any(character in pattern for character in "*?[") else [Path(pattern)]
                paths.extend(path for path in matches if path.is_file())
            report, errors = aggregate(
                matrix, load_evidence(paths), candidate=args.candidate,
                stable=args.stable, workflow_run_id=args.workflow_run_id,
            )
            write_json(Path(args.output), report)
            print(json.dumps(report, indent=2))
            return 1 if errors else 0
    except (OSError, ValueError, json.JSONDecodeError) as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 1
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
