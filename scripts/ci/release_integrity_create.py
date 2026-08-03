"""Release manifest and checksum creation."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path, PurePosixPath
import platform
from typing import Any

from release_integrity_common import (
    CHANNELS,
    SCHEMA_VERSION,
    IntegrityError,
    _artifact_record,
    _expand_artifacts,
    _parse_pair,
    _read_json,
    _relative_to_root,
    _sources_from_version,
    _utc_now,
    _validate_cyclonedx,
    _validate_manifest_structure,
    _write_json,
    sha256_file,
)

def create_manifest(args: argparse.Namespace) -> int:
    root = Path(args.root).resolve()
    output = Path(args.output).resolve()
    version_path = Path(args.version_manifest).resolve()
    version_manifest = _read_json(version_path)
    product = version_manifest.get("product")
    if not isinstance(product, dict):
        raise IntegrityError("VERSION.json does not contain a product object")
    release_version = args.release or str(product.get("version") or "")
    if not release_version:
        raise IntegrityError("A release version is required")
    if args.channel not in CHANNELS:
        raise IntegrityError(f"Unsupported release channel: {args.channel}")

    artifacts = _expand_artifacts(args.artifact, root)
    output_relative = _relative_to_root(output, root, field="manifest output")
    artifacts = [
        path for path in artifacts
        if output_relative is None
        or _relative_to_root(path, root, field="artifact") != output_relative
        or path.resolve() != output
    ]
    if not artifacts:
        raise IntegrityError("The release manifest cannot be the only artifact")

    sboms: list[dict[str, Any]] = []
    sbom_scopes: list[str] = []
    for raw in args.sbom:
        scope, raw_path = _parse_pair(raw, field="--sbom")
        sbom_path = Path(raw_path)
        if not sbom_path.is_absolute():
            sbom_path = root / sbom_path
        sbom_path = sbom_path.resolve()
        _validate_cyclonedx(sbom_path)
        relative = _relative_to_root(sbom_path, root, field="SBOM")
        if scope in sbom_scopes:
            raise IntegrityError(f"Duplicate SBOM scope: {scope}")
        sbom_scopes.append(scope)
        sboms.append(
            {
                "scope": scope,
                "path": relative,
                "format": "CycloneDX",
                "sizeBytes": sbom_path.stat().st_size,
                "sha256": sha256_file(sbom_path),
            }
        )

    dependency_locks: list[dict[str, Any]] = []
    for raw in args.dependency_lock:
        name, raw_path = _parse_pair(raw, field="--dependency-lock")
        path = Path(raw_path).resolve()
        if not path.is_file():
            raise IntegrityError(f"Dependency lock is missing: {path}")
        dependency_locks.append(
            {
                "name": name,
                "path": path.name,
                "sha256": sha256_file(path),
            }
        )

    toolchains: dict[str, str] = {}
    for raw in args.toolchain:
        name, value = _parse_pair(raw, field="--toolchain")
        toolchains[name] = value

    required_patterns = [value.casefold() for value in args.authenticode_required]
    artifact_records: list[dict[str, Any]] = []
    for path in artifacts:
        relative = _relative_to_root(path, root, field="artifact")
        required = any(
            PurePosixPath(relative.casefold()).match(pattern)
            or Path(relative).name.casefold() == pattern
            for pattern in required_patterns
        )
        artifact_records.append(_artifact_record(path, root, required, sbom_scopes))

    manifest: dict[str, Any] = {
        "schemaVersion": SCHEMA_VERSION,
        "release": {
            "version": release_version,
            "channel": args.channel,
            "createdAt": _utc_now(),
        },
        "sources": _sources_from_version(version_manifest, args.repository, args.source_commit),
        "dependencyLocks": dependency_locks,
        "build": {
            "workflow": args.workflow,
            "runId": args.run_id,
            "runner": {
                "os": os.environ.get("RUNNER_OS") or platform.system(),
                "architecture": os.environ.get("RUNNER_ARCH") or platform.machine(),
                "name": os.environ.get("RUNNER_NAME"),
            },
            "commands": list(args.build_command),
            "toolchains": toolchains,
        },
        "provenance": {
            "provider": "github-artifact-attestations",
            "repository": args.repository,
            "workflow": args.workflow,
            "runId": args.run_id,
            "sourceCommit": args.source_commit.lower(),
            "required": True,
            "denySelfHostedRunners": True,
        },
        "signing": {
            "manifest": "github-artifact-attestation",
            "authenticode": {
                "requiredFor": [
                    item["name"] for item in artifact_records if item["authenticodeRequired"]
                ],
                "timestampRequired": True,
                "certificateProvisioned": bool(
                    any(item["authenticodeRequired"] for item in artifact_records)
                ),
            },
        },
        "sboms": sboms,
        "artifacts": artifact_records,
        "checksums": args.checksums_name,
    }

    _validate_manifest_structure(manifest)
    _write_json(output, manifest)

    checksum_path = root / args.checksums_name
    checksum_lines = [
        f"{record['sha256']}  {record['name']}" for record in artifact_records
    ] + [f"{record['sha256']}  {record['path']}" for record in sboms]
    checksum_path.write_text("\n".join(checksum_lines) + "\n", encoding="utf-8", newline="\n")
    print(json.dumps({"manifest": str(output), "checksums": str(checksum_path)}, indent=2))
    return 0

