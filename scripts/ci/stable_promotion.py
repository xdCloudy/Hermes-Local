#!/usr/bin/env python3
"""Validate compatibility evidence and emit a Stable promotion manifest."""
from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import re
import sys
from pathlib import Path
from typing import Any, Sequence

SCHEMA_VERSION = 1
PROMOTABLE_STATUSES = {"compatible", "compatible-with-warnings"}
PROMOTABLE_LIFECYCLE_STATUSES = {"passed", "passed-with-warnings"}
REQUIRED_COMPONENTS = ("hermes-agent", "llama-cpp-cpu", "llama-cpp-gpu")
HEX_SHA = re.compile(r"^[0-9a-fA-F]{40}$")
HEX_DIGEST = re.compile(r"^[0-9a-fA-F]{64}$")


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
    temporary.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")
    temporary.replace(path)


def canonical_digest(value: Any) -> str:
    payload = json.dumps(
        value, sort_keys=True, separators=(",", ":"), ensure_ascii=False
    ).encode("utf-8")
    return hashlib.sha256(payload).hexdigest()


def indexed_components(report: dict[str, Any]) -> tuple[dict[str, dict[str, Any]], list[str]]:
    components: dict[str, dict[str, Any]] = {}
    errors: list[str] = []
    raw_components = report.get("components", [])
    if not isinstance(raw_components, list):
        return {}, ["components must be an array"]

    for item in raw_components:
        if not isinstance(item, dict):
            errors.append("every component report must be an object")
            continue
        name = item.get("component")
        if not isinstance(name, str) or not name:
            errors.append("every component report must have a non-empty component name")
            continue
        if name in components:
            errors.append(f"duplicate component report: {name}")
            continue
        components[name] = item
    return components, errors


def validate_report(report: dict[str, Any], expected_run_id: str) -> list[str]:
    errors: list[str] = []
    if report.get("schemaVersion") != SCHEMA_VERSION:
        errors.append(f"schemaVersion must be {SCHEMA_VERSION}")
    if report.get("component") != "hermes-local-upstream-compatibility":
        errors.append("report is not a Hermes Local aggregate compatibility report")
    if report.get("status") not in PROMOTABLE_STATUSES:
        errors.append(f"aggregate status {report.get('status')!r} is not promotable")

    components, component_errors = indexed_components(report)
    errors.extend(component_errors)
    missing = [name for name in REQUIRED_COMPONENTS if name not in components]
    if missing:
        errors.append(f"missing required component reports: {', '.join(missing)}")

    for name in REQUIRED_COMPONENTS:
        component = components.get(name)
        if component is None:
            continue
        if component.get("status") not in PROMOTABLE_STATUSES:
            errors.append(f"component {name} has non-promotable status {component.get('status')!r}")
        candidate = component.get("candidate")
        if not isinstance(candidate, str) or not HEX_SHA.fullmatch(candidate):
            errors.append(f"component {name} does not identify a full candidate commit")
        metadata = component.get("metadata")
        if not isinstance(metadata, dict):
            errors.append(f"component {name} is missing metadata")
            continue
        workflow_run_id = metadata.get("workflowRunId")
        if str(workflow_run_id) != expected_run_id:
            errors.append(
                f"component {name} belongs to workflow run {workflow_run_id!r}, "
                f"expected {expected_run_id!r}"
            )

    gpu = components.get("llama-cpp-gpu")
    if gpu is not None:
        metadata = gpu.get("metadata")
        acceleration = metadata.get("acceleration") if isinstance(metadata, dict) else None
        if acceleration != "cuda":
            errors.append("llama-cpp-gpu report was not produced by a CUDA build")
        platforms = gpu.get("testedPlatforms")
        if not isinstance(platforms, list) or not any(
            isinstance(platform, dict) and platform.get("os") == "Windows"
            for platform in platforms
        ):
            errors.append("llama-cpp-gpu report does not contain Windows test evidence")

    return errors


def validate_lifecycle_report(
    report: dict[str, Any], expected_run_id: str, expected_candidate: str,
    expected_matrix_digest: str | None = None,
) -> list[str]:
    errors: list[str] = []
    if report.get("schemaVersion") != SCHEMA_VERSION:
        errors.append(f"lifecycle schemaVersion must be {SCHEMA_VERSION}")
    if report.get("component") != "windows-lifecycle":
        errors.append("report is not Windows lifecycle evidence")
    if report.get("status") not in PROMOTABLE_LIFECYCLE_STATUSES:
        errors.append(f"lifecycle status {report.get('status')!r} is not promotable")
    if report.get("stableEvaluation") is not True:
        errors.append("lifecycle report did not enforce the Stable scenario inventory")
    if report.get("candidate") != expected_candidate or not HEX_SHA.fullmatch(str(report.get("candidate") or "")):
        errors.append("lifecycle candidate does not match the selected Hermes Local revision")
    if not HEX_DIGEST.fullmatch(str(report.get("matrixSha256") or "")):
        errors.append("lifecycle report does not identify its matrix digest")
    elif expected_matrix_digest and report.get("matrixSha256") != expected_matrix_digest:
        errors.append("lifecycle report matrix digest does not match the trusted matrix")
    metadata = report.get("metadata")
    workflow_run_id = metadata.get("workflowRunId") if isinstance(metadata, dict) else None
    if str(workflow_run_id) != expected_run_id:
        errors.append(
            f"lifecycle report belongs to workflow run {workflow_run_id!r}, "
            f"expected {expected_run_id!r}"
        )
    if report.get("failures"):
        errors.append("lifecycle report contains blocking failures")

    scenarios: dict[str, dict[str, Any]] = {}
    raw_scenarios = report.get("scenarios")
    if not isinstance(raw_scenarios, list):
        errors.append("lifecycle scenarios must be an array")
        raw_scenarios = []
    for item in raw_scenarios:
        if isinstance(item, dict) and isinstance(item.get("scenarioId"), str):
            scenarios[item["scenarioId"]] = item
    for scenario_id, runner_class in (
        ("physical-cpu", "physical-cpu"),
        ("physical-nvidia", "physical-nvidia"),
    ):
        scenario = scenarios.get(scenario_id)
        if scenario is None:
            errors.append(f"lifecycle report is missing {scenario_id} evidence")
            continue
        if scenario.get("status") != "passed":
            errors.append(f"lifecycle scenario {scenario_id} did not pass")
        environment = scenario.get("environment")
        if not isinstance(environment, dict) or environment.get("runnerClass") != runner_class:
            errors.append(f"lifecycle scenario {scenario_id} has the wrong runner class")
    return errors


def build_manifest(
    report: dict[str, Any],
    lifecycle_report: dict[str, Any],
    *,
    compatibility_run_id: str,
    lifecycle_run_id: str,
    report_path: Path,
    lifecycle_report_path: Path,
) -> dict[str, Any]:
    components, _ = indexed_components(report)
    return {
        "schemaVersion": SCHEMA_VERSION,
        "channel": "stable",
        "status": "approved",
        "approvedAt": now(),
        "compatibility": {
            "workflowRunId": compatibility_run_id,
            "generatedAt": report.get("generatedAt"),
            "status": report.get("status"),
            "reportSha256": hashlib.sha256(report_path.read_bytes()).hexdigest(),
        },
        "lifecycle": {
            "workflowRunId": lifecycle_run_id,
            "generatedAt": lifecycle_report.get("generatedAt"),
            "status": lifecycle_report.get("status"),
            "candidate": lifecycle_report.get("candidate"),
            "matrixSha256": lifecycle_report.get("matrixSha256"),
            "summary": lifecycle_report.get("summary", {}),
            "reportSha256": hashlib.sha256(lifecycle_report_path.read_bytes()).hexdigest(),
        },
        "components": [
            {
                "component": name,
                "candidate": components[name].get("candidate"),
                "base": components[name].get("base"),
                "status": components[name].get("status"),
                "testedPlatforms": components[name].get("testedPlatforms", []),
                "metadata": components[name].get("metadata", {}),
            }
            for name in REQUIRED_COMPONENTS
        ],
        "warnings": [
            *report.get("warnings", []),
            *lifecycle_report.get("warnings", []),
        ],
    }


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("--report", required=True, help="Aggregate compatibility report")
    result.add_argument("--compatibility-run-id", required=True)
    result.add_argument("--lifecycle-report", required=True, help="Aggregate Windows lifecycle report")
    result.add_argument("--lifecycle-run-id", required=True)
    result.add_argument("--candidate-sha", required=True, help="Hermes Local revision tested by both workflows")
    result.add_argument("--matrix", required=True, help="Trusted Windows lifecycle matrix")
    result.add_argument("--manifest", required=True, help="Stable approval manifest output")
    return result


def main(argv: Sequence[str] | None = None) -> int:
    args = parser().parse_args(argv)
    report_path = Path(args.report).resolve()
    lifecycle_report_path = Path(args.lifecycle_report).resolve()
    matrix_path = Path(args.matrix).resolve()
    manifest_path = Path(args.manifest).resolve()
    try:
        report = read_json(report_path)
        lifecycle_report = read_json(lifecycle_report_path)
        matrix = read_json(matrix_path)
    except (OSError, ValueError, json.JSONDecodeError) as exc:
        print(f"ERROR: unable to read compatibility report: {exc}", file=sys.stderr)
        return 1

    expected_run_id = str(args.compatibility_run_id)
    expected_lifecycle_run_id = str(args.lifecycle_run_id)
    expected_candidate = str(args.candidate_sha)
    if not HEX_SHA.fullmatch(expected_candidate):
        print("ERROR: candidate-sha must be a full commit", file=sys.stderr)
        return 1
    errors = validate_report(report, expected_run_id)
    errors.extend(
        validate_lifecycle_report(
            lifecycle_report, expected_lifecycle_run_id, expected_candidate,
            canonical_digest(matrix),
        )
    )
    if errors:
        for error in errors:
            print(f"ERROR: {error}", file=sys.stderr)
        return 1

    manifest = build_manifest(
        report,
        lifecycle_report,
        compatibility_run_id=expected_run_id,
        lifecycle_run_id=expected_lifecycle_run_id,
        report_path=report_path,
        lifecycle_report_path=lifecycle_report_path,
    )
    write_json(manifest_path, manifest)
    print(
        f"Stable promotion approved from compatibility run {expected_run_id}: "
        f"{manifest_path}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
