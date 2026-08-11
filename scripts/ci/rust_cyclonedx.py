#!/usr/bin/env python3
"""Generate a CycloneDX SBOM for the resolved Hermes Local Rust client graph."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
from pathlib import Path
import re
import tomllib
from typing import Any
from urllib.parse import quote
import uuid

SHA256_RE = re.compile(r"^[0-9a-fA-F]{64}$")


class SbomError(RuntimeError):
    """Raised when Cargo metadata cannot be converted safely."""


def _read_json(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise SbomError(f"Cargo metadata root must be an object: {path}")
    return value


def _load_lock_checksums(path: Path) -> dict[tuple[str, str, str | None], str]:
    with path.open("rb") as handle:
        lock = tomllib.load(handle)
    packages = lock.get("package")
    if not isinstance(packages, list):
        raise SbomError("Cargo.lock does not contain package records")
    checksums: dict[tuple[str, str, str | None], str] = {}
    for package in packages:
        if not isinstance(package, dict):
            continue
        name = package.get("name")
        version = package.get("version")
        source = package.get("source")
        checksum = package.get("checksum")
        if (
            isinstance(name, str)
            and isinstance(version, str)
            and (source is None or isinstance(source, str))
            and isinstance(checksum, str)
            and SHA256_RE.fullmatch(checksum)
        ):
            checksums[(name, version, source)] = checksum.lower()
    return checksums


def _bom_ref(package_id: str) -> str:
    digest = hashlib.sha256(package_id.encode("utf-8")).hexdigest()
    return f"urn:hermes:cargo:{digest}"


def _purl(package: dict[str, Any]) -> str | None:
    source = package.get("source")
    if not isinstance(source, str) or not source.startswith("registry+"):
        return None
    name = str(package["name"])
    version = str(package["version"])
    return f"pkg:cargo/{quote(name, safe='')}@{quote(version, safe='')}"


def _component(
    package: dict[str, Any],
    *,
    root_id: str,
    checksums: dict[tuple[str, str, str | None], str],
) -> dict[str, Any]:
    package_id = str(package["id"])
    name = str(package["name"])
    version = str(package["version"])
    source = package.get("source")
    source_value = source if isinstance(source, str) else None
    component: dict[str, Any] = {
        "type": "application" if package_id == root_id else "library",
        "bom-ref": _bom_ref(package_id),
        "name": name,
        "version": version,
    }
    purl = _purl(package)
    if purl:
        component["purl"] = purl

    checksum = checksums.get((name, version, source_value))
    if checksum:
        component["hashes"] = [{"alg": "SHA-256", "content": checksum}]

    license_expression = package.get("license")
    if isinstance(license_expression, str) and license_expression.strip():
        component["licenses"] = [{"expression": license_expression.strip()}]

    repository = package.get("repository")
    if isinstance(repository, str) and repository.strip():
        component["externalReferences"] = [
            {"type": "vcs", "url": repository.strip()}
        ]

    if source_value:
        component["properties"] = [
            {"name": "hermes:cargo:source", "value": source_value}
        ]
    return component


def _resolve_graph(
    metadata: dict[str, Any], root_name: str
) -> tuple[str, set[str], dict[str, list[str]]]:
    packages = metadata.get("packages")
    resolve = metadata.get("resolve")
    if not isinstance(packages, list) or not isinstance(resolve, dict):
        raise SbomError("Cargo metadata must include packages and resolve")

    roots = [
        package
        for package in packages
        if isinstance(package, dict) and package.get("name") == root_name
    ]
    if len(roots) != 1:
        raise SbomError(
            f"Expected exactly one Cargo package named {root_name!r}, found {len(roots)}"
        )
    root_id = str(roots[0]["id"])

    raw_nodes = resolve.get("nodes")
    if not isinstance(raw_nodes, list):
        raise SbomError("Cargo metadata resolve.nodes is missing")
    graph: dict[str, list[str]] = {}
    for node in raw_nodes:
        if not isinstance(node, dict) or not isinstance(node.get("id"), str):
            continue
        deps = node.get("deps")
        dep_ids: list[str] = []
        if isinstance(deps, list):
            for dep in deps:
                if not isinstance(dep, dict) or not isinstance(dep.get("pkg"), str):
                    continue
                dep_kinds = dep.get("dep_kinds")
                if isinstance(dep_kinds, list) and dep_kinds:
                    kinds = {
                        entry.get("kind")
                        for entry in dep_kinds
                        if isinstance(entry, dict)
                    }
                    if kinds and kinds <= {"dev"}:
                        continue
                dep_ids.append(dep["pkg"])
        graph[node["id"]] = sorted(set(dep_ids))

    reachable: set[str] = set()
    pending = [root_id]
    while pending:
        package_id = pending.pop()
        if package_id in reachable:
            continue
        reachable.add(package_id)
        pending.extend(graph.get(package_id, []))
    return root_id, reachable, graph


def build_sbom(
    metadata: dict[str, Any],
    lock_path: Path,
    root_name: str,
    timestamp: str,
) -> dict[str, Any]:
    packages = metadata.get("packages")
    if not isinstance(packages, list):
        raise SbomError("Cargo metadata packages is missing")

    root_id, reachable, graph = _resolve_graph(metadata, root_name)
    by_id = {
        str(package["id"]): package
        for package in packages
        if isinstance(package, dict) and isinstance(package.get("id"), str)
    }
    missing = sorted(reachable - by_id.keys())
    if missing:
        raise SbomError(f"Resolved Cargo packages are missing metadata: {missing[:3]}")

    checksums = _load_lock_checksums(lock_path)
    root_package = by_id[root_id]
    root_component = _component(root_package, root_id=root_id, checksums=checksums)

    component_ids = sorted(package_id for package_id in reachable if package_id != root_id)
    components = [
        _component(by_id[package_id], root_id=root_id, checksums=checksums)
        for package_id in component_ids
    ]
    dependencies = [
        {
            "ref": _bom_ref(package_id),
            "dependsOn": [
                _bom_ref(dep_id)
                for dep_id in graph.get(package_id, [])
                if dep_id in reachable
            ],
        }
        for package_id in sorted(reachable)
    ]

    identity_seed = "\n".join(sorted(reachable))
    serial = uuid.uuid5(uuid.NAMESPACE_URL, f"hermes-local:{root_name}:{identity_seed}")
    return {
        "bomFormat": "CycloneDX",
        "specVersion": "1.6",
        "serialNumber": f"urn:uuid:{serial}",
        "version": 1,
        "metadata": {
            "timestamp": timestamp,
            "component": root_component,
        },
        "components": components,
        "dependencies": dependencies,
    }


def _timestamp(value: str | None) -> str:
    if value:
        parsed = dt.datetime.fromisoformat(value.replace("Z", "+00:00"))
        if parsed.tzinfo is None:
            raise SbomError("--timestamp must include a timezone")
        return parsed.astimezone(dt.timezone.utc).isoformat().replace("+00:00", "Z")
    return dt.datetime.now(dt.timezone.utc).isoformat().replace("+00:00", "Z")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--metadata", required=True, help="cargo metadata JSON path")
    parser.add_argument("--lock", required=True, help="Cargo.lock path")
    parser.add_argument("--root-package", default="hermes-local")
    parser.add_argument("--output", required=True)
    parser.add_argument("--timestamp")
    args = parser.parse_args(argv)

    metadata_path = Path(args.metadata).resolve()
    lock_path = Path(args.lock).resolve()
    output = Path(args.output).resolve()
    try:
        sbom = build_sbom(
            _read_json(metadata_path),
            lock_path,
            args.root_package,
            _timestamp(args.timestamp),
        )
    except (OSError, json.JSONDecodeError, tomllib.TOMLDecodeError, ValueError, SbomError) as exc:
        print(f"rust-cyclonedx: {exc}", file=__import__("sys").stderr)
        return 2

    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(
        json.dumps(sbom, indent=2) + "\n", encoding="utf-8", newline="\n"
    )
    print(output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
