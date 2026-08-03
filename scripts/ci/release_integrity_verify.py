"""Fail-closed release manifest and provenance verification."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import re
import shutil
import subprocess
from typing import Any

from release_integrity_common import (
    IntegrityError,
    _authenticode_status,
    _read_json,
    _resolve_within,
    _safe_relative_path,
    _validate_cyclonedx,
    _validate_manifest_structure,
    _write_json,
    _utc_now,
    sha256_file,
)

def _verify_attestation(
    path: Path,
    repository: str,
    workflow: str,
    source_commit: str,
    bundle: Path | None,
    trusted_root: Path | None,
) -> dict[str, Any]:
    gh = shutil.which("gh")
    if not gh:
        raise IntegrityError("GitHub CLI is required for provenance verification")
    command = [
        gh,
        "attestation",
        "verify",
        str(path),
        "--repo",
        repository,
        "--signer-workflow",
        workflow,
        "--source-digest",
        source_commit,
        "--deny-self-hosted-runners",
        "--format",
        "json",
    ]
    if bundle:
        command.extend(["--bundle", str(bundle)])
    if trusted_root:
        command.extend(["--custom-trusted-root", str(trusted_root)])
    process = subprocess.run(command, text=True, capture_output=True, check=False)
    if process.returncode != 0:
        detail = (process.stderr or process.stdout).strip()
        raise IntegrityError(f"Provenance verification failed for {path.name}: {detail}")
    try:
        result = json.loads(process.stdout)
    except json.JSONDecodeError as exc:
        raise IntegrityError(f"GitHub CLI returned invalid attestation JSON for {path}") from exc
    if not isinstance(result, list) or not result:
        raise IntegrityError(f"No valid provenance attestation was found for {path}")
    return {"verified": True, "attestationCount": len(result)}


def _attestation_bundle_candidates(bundle_dir: Path, digest: str) -> list[Path]:
    preferred = [
        bundle_dir / f"sha256-{digest}.jsonl",
        bundle_dir / f"sha256:{digest}.jsonl",
        bundle_dir / f"{digest}.jsonl",
        bundle_dir / f"sha256-{digest}.json",
        bundle_dir / f"{digest}.json",
    ]
    remaining = sorted(bundle_dir.glob("*.jsonl")) + sorted(bundle_dir.glob("*.json"))
    result: list[Path] = []
    seen: set[Path] = set()
    for candidate in preferred + remaining:
        resolved = candidate.resolve()
        if candidate.is_file() and resolved not in seen:
            seen.add(resolved)
            result.append(resolved)
    return result


def verify_manifest(args: argparse.Namespace) -> int:
    manifest_path = Path(args.manifest).resolve()
    root = Path(args.artifact_root).resolve()
    manifest = _read_json(manifest_path)
    _validate_manifest_structure(manifest)

    provenance = manifest["provenance"]
    report: dict[str, Any] = {
        "schemaVersion": 1,
        "verifiedAt": _utc_now(),
        "manifest": str(manifest_path),
        "artifactRoot": str(root),
        "release": manifest["release"],
        "status": "running",
        "checks": {"manifest": "valid", "artifacts": [], "sboms": [], "provenance": []},
    }

    sbom_scopes: set[str] = set()
    for index, record in enumerate(manifest["sboms"]):
        path = _resolve_within(root, record["path"], field=f"sboms[{index}].path")
        if not path.is_file():
            raise IntegrityError(f"SBOM is missing: {record['path']}")
        if path.stat().st_size != record["sizeBytes"]:
            raise IntegrityError(f"SBOM size mismatch: {record['path']}")
        actual = sha256_file(path)
        if actual != record["sha256"].lower():
            raise IntegrityError(f"SBOM SHA-256 mismatch: {record['path']}")
        _validate_cyclonedx(path)
        scope = str(record.get("scope") or "")
        if not scope or scope in sbom_scopes:
            raise IntegrityError(f"SBOM scope is empty or duplicated: {scope!r}")
        sbom_scopes.add(scope)
        report["checks"]["sboms"].append(
            {"scope": scope, "path": record["path"], "sha256": actual, "status": "verified"}
        )

    attestation_targets: list[Path] = [manifest_path]
    for index, record in enumerate(manifest["artifacts"]):
        path = _resolve_within(root, record["name"], field=f"artifacts[{index}].name")
        if not path.is_file():
            raise IntegrityError(f"Release artifact is missing: {record['name']}")
        actual_size = path.stat().st_size
        if actual_size != record["sizeBytes"]:
            raise IntegrityError(f"Release artifact size mismatch: {record['name']}")
        actual_hash = sha256_file(path)
        if actual_hash != record["sha256"].lower():
            raise IntegrityError(f"Release artifact SHA-256 mismatch: {record['name']}")
        unknown_scopes = sorted(set(record.get("sboms") or []) - sbom_scopes)
        if unknown_scopes:
            raise IntegrityError(
                f"Artifact {record['name']} references unknown SBOM scopes: {unknown_scopes}"
            )
        auth = _authenticode_status(path) if record.get("authenticodeRequired") else {
            "status": "not-required",
            "valid": True,
        }
        if record.get("authenticodeRequired") and not auth.get("valid"):
            raise IntegrityError(
                f"Required Authenticode verification failed for {record['name']}: {auth.get('status')}"
            )
        if record.get("authenticodeRequired"):
            expected_signature = record["authenticode"]
            if auth.get("thumbprint") != expected_signature.get("thumbprint"):
                raise IntegrityError(
                    f"Authenticode publisher thumbprint mismatch for {record['name']}"
                )
            if auth.get("subject") != expected_signature.get("subject"):
                raise IntegrityError(
                    f"Authenticode publisher subject mismatch for {record['name']}"
                )
            if not auth.get("timestampSubject"):
                raise IntegrityError(
                    f"Required Authenticode timestamp is missing for {record['name']}"
                )
        report["checks"]["artifacts"].append(
            {
                "name": record["name"],
                "sizeBytes": actual_size,
                "sha256": actual_hash,
                "authenticode": auth,
                "status": "verified",
            }
        )
        attestation_targets.append(path)

    checksum_path = _resolve_within(root, manifest["checksums"], field="checksums")
    if not checksum_path.is_file():
        raise IntegrityError(f"Checksum file is missing: {manifest['checksums']}")
    checksum_entries: dict[str, str] = {}
    for line_number, line in enumerate(checksum_path.read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip():
            continue
        match = re.fullmatch(r"([0-9a-fA-F]{64})\s+[ *](.+)", line)
        if not match:
            raise IntegrityError(f"Invalid checksum line {line_number} in {checksum_path.name}")
        digest, name = match.groups()
        _safe_relative_path(name, field=f"checksum line {line_number}")
        checksum_entries[name.casefold()] = digest.lower()
    for record in list(manifest["artifacts"]) + [
        {"name": item["path"], "sha256": item["sha256"]} for item in manifest["sboms"]
    ]:
        name = record["name"]
        if checksum_entries.get(name.casefold()) != record["sha256"].lower():
            raise IntegrityError(f"Checksum file does not match manifest entry: {name}")

    require_attestation = bool(args.require_attestation or provenance.get("required"))
    if require_attestation:
        bundle_dir = Path(args.attestation_bundle_dir).resolve() if args.attestation_bundle_dir else None
        trusted_root = Path(args.trusted_root).resolve() if args.trusted_root else None
        for path in attestation_targets:
            if bundle_dir:
                candidates = _attestation_bundle_candidates(bundle_dir, sha256_file(path))
                if not candidates:
                    raise IntegrityError(f"Offline attestation bundle is missing for {path.name}")
                failures: list[str] = []
                result = None
                for bundle in candidates:
                    try:
                        result = _verify_attestation(
                            path,
                            str(provenance["repository"]),
                            str(provenance["workflow"]),
                            str(provenance["sourceCommit"]),
                            bundle,
                            trusted_root,
                        )
                        break
                    except IntegrityError as exc:
                        failures.append(str(exc))
                if result is None:
                    raise IntegrityError(
                        f"No offline attestation bundle verified {path.name}: "
                        + "; ".join(failures[-3:])
                    )
            else:
                result = _verify_attestation(
                    path,
                    str(provenance["repository"]),
                    str(provenance["workflow"]),
                    str(provenance["sourceCommit"]),
                    None,
                    trusted_root,
                )
            report["checks"]["provenance"].append({"name": path.name, **result})

    report["status"] = "verified"
    if args.report:
        _write_json(Path(args.report).resolve(), report)
    print(json.dumps(report, indent=2))
    return 0

