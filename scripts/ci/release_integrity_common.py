"""Shared validation primitives for Hermes Local release integrity."""

from __future__ import annotations

import datetime as dt
import glob
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import platform
import re
import shutil
import subprocess
from typing import Any, Iterable

SCHEMA_VERSION = 1
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")
CHANNELS = {"stable", "beta", "nightly", "development"}


class IntegrityError(RuntimeError):
    """Raised when release metadata or an artifact fails validation."""


def _utc_now() -> str:
    return dt.datetime.now(dt.timezone.utc).isoformat().replace("+00:00", "Z")


def _read_json(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError as exc:
        raise IntegrityError(f"JSON file is missing: {path}") from exc
    except json.JSONDecodeError as exc:
        raise IntegrityError(f"JSON file is invalid: {path}: {exc}") from exc
    if not isinstance(value, dict):
        raise IntegrityError(f"JSON root must be an object: {path}")
    return value


def _write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(f".{path.name}.{os.getpid()}.tmp")
    temporary.write_text(
        json.dumps(value, indent=2, sort_keys=False) + "\n",
        encoding="utf-8",
        newline="\n",
    )
    os.replace(temporary, path)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _safe_relative_path(value: str, *, field: str) -> PurePosixPath:
    normalized = value.replace("\\", "/")
    path = PurePosixPath(normalized)
    if not normalized or path.is_absolute() or ".." in path.parts:
        raise IntegrityError(f"{field} contains an unsafe path: {value!r}")
    if any(part in {"", "."} for part in path.parts):
        raise IntegrityError(f"{field} contains an invalid path segment: {value!r}")
    return path


def _resolve_within(root: Path, relative: str, *, field: str) -> Path:
    safe = _safe_relative_path(relative, field=field)
    candidate = (root / Path(*safe.parts)).resolve()
    root_resolved = root.resolve()
    try:
        candidate.relative_to(root_resolved)
    except ValueError as exc:
        raise IntegrityError(f"{field} escapes the artifact root: {relative!r}") from exc
    return candidate


def _relative_to_root(path: Path, root: Path, *, field: str) -> str:
    resolved = path.resolve()
    try:
        relative = resolved.relative_to(root.resolve())
    except ValueError as exc:
        raise IntegrityError(f"{field} is outside the artifact root: {path}") from exc
    return PurePosixPath(relative.as_posix()).as_posix()


def _parse_pair(value: str, *, field: str) -> tuple[str, str]:
    if "=" not in value:
        raise IntegrityError(f"{field} must use NAME=VALUE syntax: {value!r}")
    name, raw_value = value.split("=", 1)
    name = name.strip()
    raw_value = raw_value.strip()
    if not name or not raw_value:
        raise IntegrityError(f"{field} must include a non-empty name and value")
    return name, raw_value


def _expand_artifacts(patterns: Iterable[str], root: Path) -> list[Path]:
    found: dict[str, Path] = {}
    for pattern in patterns:
        candidate_pattern = pattern
        if not Path(pattern).is_absolute():
            candidate_pattern = str(root / pattern)
        matches = [Path(value) for value in glob.glob(candidate_pattern, recursive=True)]
        if not matches and Path(candidate_pattern).is_file():
            matches = [Path(candidate_pattern)]
        for path in matches:
            if path.is_file():
                relative = _relative_to_root(path, root, field="artifact")
                found[relative.casefold()] = path.resolve()
    artifacts = [found[key] for key in sorted(found)]
    if not artifacts:
        raise IntegrityError("No release artifacts matched the supplied --artifact values")
    return artifacts


def _source_record(repository: str, commit: str, **extra: str) -> dict[str, Any]:
    commit = commit.lower()
    if not COMMIT_RE.fullmatch(commit):
        raise IntegrityError(f"Source commit is not a full 40-character SHA: {commit!r}")
    record: dict[str, Any] = {"repository": repository, "commit": commit}
    for key, value in extra.items():
        if value:
            record[key] = value
    return record


def _sources_from_version(
    version_manifest: dict[str, Any], repository: str, source_commit: str
) -> dict[str, Any]:
    sources = version_manifest.get("sources")
    if not isinstance(sources, dict):
        raise IntegrityError("VERSION.json does not contain a sources object")

    agent = sources.get("hermesAgent")
    llama = sources.get("llamaCpp")
    if not isinstance(agent, dict) or not isinstance(llama, dict):
        raise IntegrityError("VERSION.json must identify Hermes Agent and llama.cpp sources")

    agent_commit = str(agent.get("integrationCommit") or agent.get("commit") or "")
    return {
        "hermesLocal": _source_record(repository, source_commit),
        "hermesAgent": _source_record(
            str(agent.get("repository") or ""),
            agent_commit,
            branch=str(agent.get("integrationBranch") or agent.get("branch") or ""),
            upstreamCommit=str(agent.get("commit") or "").lower(),
        ),
        "llamaCpp": _source_record(
            str(llama.get("repository") or ""),
            str(llama.get("commit") or ""),
            branch=str(llama.get("branch") or ""),
        ),
    }


def _validate_cyclonedx(path: Path) -> None:
    value = _read_json(path)
    if value.get("bomFormat") != "CycloneDX":
        raise IntegrityError(f"SBOM is not CycloneDX JSON: {path}")
    if not isinstance(value.get("specVersion"), str):
        raise IntegrityError(f"CycloneDX SBOM has no specVersion: {path}")


def _authenticode_status(path: Path) -> dict[str, Any]:
    if os.name != "nt":
        return {"status": "unsupported-platform", "valid": False}
    shell = shutil.which("pwsh") or shutil.which("powershell")
    if not shell:
        return {"status": "powershell-missing", "valid": False}
    script = (
        "$s=Get-AuthenticodeSignature -LiteralPath $args[0];"
        "[ordered]@{status=[string]$s.Status;"
        "subject=if($s.SignerCertificate){$s.SignerCertificate.Subject}else{$null};"
        "thumbprint=if($s.SignerCertificate){$s.SignerCertificate.Thumbprint}else{$null};"
        "timestampSubject=if($s.TimeStamperCertificate){$s.TimeStamperCertificate.Subject}else{$null}}"
        "|ConvertTo-Json -Compress"
    )
    process = subprocess.run(
        [shell, "-NoLogo", "-NoProfile", "-NonInteractive", "-Command", script, str(path)],
        text=True,
        capture_output=True,
        check=False,
    )
    if process.returncode != 0:
        return {
            "status": "verification-error",
            "valid": False,
            "error": (process.stderr or process.stdout).strip(),
        }
    try:
        record = json.loads(process.stdout.strip())
    except json.JSONDecodeError:
        return {"status": "invalid-verifier-output", "valid": False}
    return {
        "status": str(record.get("status") or "Unknown"),
        "valid": str(record.get("status") or "").casefold() == "valid",
        "subject": record.get("subject"),
        "thumbprint": record.get("thumbprint"),
        "timestampSubject": record.get("timestampSubject"),
    }


def _artifact_record(
    path: Path,
    root: Path,
    authenticode_required: bool,
    sbom_scopes: list[str],
) -> dict[str, Any]:
    record: dict[str, Any] = {
        "name": _relative_to_root(path, root, field="artifact"),
        "sizeBytes": path.stat().st_size,
        "sha256": sha256_file(path),
        "authenticodeRequired": authenticode_required,
        "sboms": sorted(sbom_scopes),
        "provenanceRequired": True,
    }
    if authenticode_required:
        signature = _authenticode_status(path)
        if not signature.get("valid"):
            raise IntegrityError(
                f"Required Authenticode signature is invalid for {path.name}: "
                f"{signature.get('status')}"
            )
        if not signature.get("timestampSubject"):
            raise IntegrityError(
                f"Required Authenticode timestamp is missing for {path.name}"
            )
        record["authenticode"] = {
            "subject": signature.get("subject"),
            "thumbprint": signature.get("thumbprint"),
            "timestampSubject": signature.get("timestampSubject"),
        }
    return record

def _require_mapping(value: Any, field: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise IntegrityError(f"{field} must be an object")
    return value


def _require_list(value: Any, field: str) -> list[Any]:
    if not isinstance(value, list):
        raise IntegrityError(f"{field} must be an array")
    return value


def _validate_manifest_structure(manifest: dict[str, Any]) -> None:
    if manifest.get("schemaVersion") != SCHEMA_VERSION:
        raise IntegrityError(
            f"Unsupported release manifest schemaVersion: {manifest.get('schemaVersion')!r}"
        )
    release = _require_mapping(manifest.get("release"), "release")
    if not isinstance(release.get("version"), str) or not release["version"]:
        raise IntegrityError("release.version is required")
    if release.get("channel") not in CHANNELS:
        raise IntegrityError("release.channel is invalid")
    if not isinstance(release.get("createdAt"), str) or not release["createdAt"]:
        raise IntegrityError("release.createdAt is required")

    sources = _require_mapping(manifest.get("sources"), "sources")
    for name in ("hermesLocal", "hermesAgent", "llamaCpp"):
        source = _require_mapping(sources.get(name), f"sources.{name}")
        if not isinstance(source.get("repository"), str) or not source["repository"]:
            raise IntegrityError(f"sources.{name}.repository is required")
        commit = str(source.get("commit") or "").lower()
        if not COMMIT_RE.fullmatch(commit):
            raise IntegrityError(f"sources.{name}.commit must be a full commit SHA")

    dependency_locks = _require_list(manifest.get("dependencyLocks"), "dependencyLocks")
    for index, raw in enumerate(dependency_locks):
        lock = _require_mapping(raw, f"dependencyLocks[{index}]")
        if not isinstance(lock.get("name"), str) or not lock["name"]:
            raise IntegrityError(f"dependencyLocks[{index}].name is required")
        _safe_relative_path(str(lock.get("path") or ""), field=f"dependencyLocks[{index}].path")
        if not SHA256_RE.fullmatch(str(lock.get("sha256") or "").lower()):
            raise IntegrityError(f"dependencyLocks[{index}].sha256 is invalid")

    build = _require_mapping(manifest.get("build"), "build")
    for field in ("workflow", "runId", "runner", "commands", "toolchains"):
        if build.get(field) is None:
            raise IntegrityError(f"build.{field} is required")

    provenance = _require_mapping(manifest.get("provenance"), "provenance")
    for field in ("provider", "repository", "workflow", "runId", "sourceCommit"):
        if provenance.get(field) in (None, ""):
            raise IntegrityError(f"provenance.{field} is required")
    if provenance.get("provider") != "github-artifact-attestations":
        raise IntegrityError("provenance.provider is invalid")
    if provenance.get("required") is not True:
        raise IntegrityError("provenance.required must be true")
    if provenance.get("denySelfHostedRunners") is not True:
        raise IntegrityError("provenance.denySelfHostedRunners must be true")
    if not COMMIT_RE.fullmatch(str(provenance.get("sourceCommit") or "").lower()):
        raise IntegrityError("provenance.sourceCommit must be a full commit SHA")
    if build.get("workflow") != provenance.get("workflow"):
        raise IntegrityError("build.workflow must match provenance.workflow")
    if str(build.get("runId")) != str(provenance.get("runId")):
        raise IntegrityError("build.runId must match provenance.runId")

    signing = _require_mapping(manifest.get("signing"), "signing")
    if signing.get("manifest") != "github-artifact-attestation":
        raise IntegrityError("signing.manifest must use GitHub artifact attestations")
    signing_authenticode = _require_mapping(
        signing.get("authenticode"), "signing.authenticode"
    )
    required_for = _require_list(
        signing_authenticode.get("requiredFor"), "signing.authenticode.requiredFor"
    )
    if signing_authenticode.get("timestampRequired") is not True:
        raise IntegrityError("signing.authenticode.timestampRequired must be true")
    if not isinstance(signing_authenticode.get("certificateProvisioned"), bool):
        raise IntegrityError("signing.authenticode.certificateProvisioned must be boolean")

    artifacts = _require_list(manifest.get("artifacts"), "artifacts")
    if not artifacts:
        raise IntegrityError("artifacts must not be empty")
    seen: set[str] = set()
    for index, raw in enumerate(artifacts):
        artifact = _require_mapping(raw, f"artifacts[{index}]")
        name = str(artifact.get("name") or "")
        _safe_relative_path(name, field=f"artifacts[{index}].name")
        key = name.casefold()
        if key in seen:
            raise IntegrityError(f"Duplicate artifact path: {name}")
        seen.add(key)
        if not isinstance(artifact.get("sizeBytes"), int) or artifact["sizeBytes"] < 0:
            raise IntegrityError(f"artifacts[{index}].sizeBytes is invalid")
        if not SHA256_RE.fullmatch(str(artifact.get("sha256") or "").lower()):
            raise IntegrityError(f"artifacts[{index}].sha256 is invalid")
        if not isinstance(artifact.get("authenticodeRequired"), bool):
            raise IntegrityError(f"artifacts[{index}].authenticodeRequired must be boolean")
        artifact_sboms = _require_list(artifact.get("sboms"), f"artifacts[{index}].sboms")
        if any(not isinstance(scope, str) or not scope for scope in artifact_sboms):
            raise IntegrityError(f"artifacts[{index}].sboms contains an invalid scope")
        if artifact.get("provenanceRequired") is not True:
            raise IntegrityError(f"artifacts[{index}].provenanceRequired must be true")
        if artifact.get("authenticodeRequired"):
            signature = _require_mapping(
                artifact.get("authenticode"), f"artifacts[{index}].authenticode"
            )
            for field in ("subject", "thumbprint", "timestampSubject"):
                if not isinstance(signature.get(field), str) or not signature[field]:
                    raise IntegrityError(
                        f"artifacts[{index}].authenticode.{field} is required"
                    )

    expected_required = sorted(
        artifact["name"] for artifact in artifacts if artifact["authenticodeRequired"]
    )
    if sorted(str(name) for name in required_for) != expected_required:
        raise IntegrityError(
            "signing.authenticode.requiredFor does not match required artifacts"
        )
    if bool(expected_required) != signing_authenticode["certificateProvisioned"]:
        raise IntegrityError(
            "signing.authenticode.certificateProvisioned is inconsistent"
        )

    seen_scopes: set[str] = set()
    for index, raw in enumerate(_require_list(manifest.get("sboms"), "sboms")):
        sbom = _require_mapping(raw, f"sboms[{index}]")
        scope = str(sbom.get("scope") or "")
        if not scope or scope in seen_scopes:
            raise IntegrityError(f"sboms[{index}].scope is empty or duplicated")
        seen_scopes.add(scope)
        _safe_relative_path(str(sbom.get("path") or ""), field=f"sboms[{index}].path")
        if sbom.get("format") != "CycloneDX":
            raise IntegrityError(f"sboms[{index}].format must be CycloneDX")
        if not isinstance(sbom.get("sizeBytes"), int) or sbom["sizeBytes"] < 0:
            raise IntegrityError(f"sboms[{index}].sizeBytes is invalid")
        if not SHA256_RE.fullmatch(str(sbom.get("sha256") or "").lower()):
            raise IntegrityError(f"sboms[{index}].sha256 is invalid")

    checksums = str(manifest.get("checksums") or "")
    _safe_relative_path(checksums, field="checksums")

