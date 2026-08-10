"""Repository architecture guard for the Hermes Local-owned Desktop."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

HEX40 = re.compile(r"^[0-9a-f]{40}$")
FORBIDDEN_PATCH_PREFIXES = ("apps/desktop/",)


class ArchitectureError(RuntimeError):
    """Raised when a source-ownership invariant is violated."""


def read_json(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def patch_paths(path: Path) -> list[str]:
    result: list[str] = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line.startswith("diff --git a/"):
            continue
        source = line[len("diff --git a/") :].split(" b/", 1)[0]
        result.append(source.replace("\\", "/"))
    return result


def tracked_files(root: Path) -> set[str]:
    process = subprocess.run(
        ["git", "ls-files", "--", "apps/desktop", "packages/hermes-agent-client"],
        cwd=root,
        check=False,
        capture_output=True,
        text=True,
    )
    if process.returncode != 0:
        raise ArchitectureError(f"git ls-files failed: {process.stderr.strip()}")
    return {line.strip().replace("\\", "/") for line in process.stdout.splitlines() if line.strip()}


def validate(root: Path) -> dict:
    root = root.resolve()
    version = read_json(root / "VERSION.json")
    architecture = read_json(root / "config" / "architecture.json")

    errors: list[str] = []
    if version.get("schemaVersion") != 2:
        errors.append("VERSION.json schemaVersion must be 2")

    client = version.get("product", {}).get("client", {})
    if client.get("sourcePath") != "apps/desktop":
        errors.append("product.client.sourcePath must be apps/desktop")
    if client.get("ownership") != "hermes-local":
        errors.append("product.client.ownership must be hermes-local")
    if architecture.get("client", {}).get("source") != client.get("sourcePath"):
        errors.append("config/architecture.json and VERSION.json disagree on the client source")
    if architecture.get("agentHarness", {}).get("source") != "source/hermes-agent":
        errors.append("agent harness source must be source/hermes-agent")
    if architecture.get("agentHarness", {}).get("patchScope") != "harness-only":
        errors.append("agent harness patch scope must be harness-only")

    required = {
        "apps/desktop/package.json",
        "apps/desktop/electron/main.ts",
        "apps/desktop/electron/hermes-local-control.ts",
        "apps/desktop/src/app/chat/sidebar/project-centre-dialog.tsx",
        "apps/desktop/src/app/settings/index.tsx",
        "packages/hermes-agent-client/package.json",
        "packages/hermes-agent-client/src/json-rpc-gateway.ts",
    }
    tracked = tracked_files(root)
    missing = sorted(path for path in required if path not in tracked)
    if missing:
        errors.append("required native client files are not tracked: " + ", ".join(missing))

    root_package = read_json(root / "package.json")
    workspaces = set(root_package.get("workspaces", []))
    if workspaces != {"apps/desktop", "packages/hermes-agent-client"}:
        errors.append("root npm workspaces must contain only the native Desktop and agent client package")

    desktop_package = read_json(root / "apps" / "desktop" / "package.json")
    if desktop_package.get("dependencies", {}).get("@hermes/shared") != "file:../../packages/hermes-agent-client":
        errors.append("Desktop must consume the explicit root-owned Hermes Agent client package")

    agent = version.get("sources", {}).get("hermesAgent", {})
    for field in ("commit", "harnessCommit", "harnessTree"):
        value = str(agent.get(field, "")).lower()
        if not HEX40.fullmatch(value):
            errors.append(f"sources.hermesAgent.{field} must be a 40-character lowercase Git identity")
    if agent.get("patchScope") != "harness-only":
        errors.append("sources.hermesAgent.patchScope must be harness-only")

    patch_dir = root / str(agent.get("patchSeries", ""))
    patches = sorted(patch_dir.glob("*.patch"))
    if not patches:
        errors.append("Hermes Agent harness patch series is empty")
    forbidden: list[str] = []
    for patch in patches:
        for source in patch_paths(patch):
            if source.startswith(FORBIDDEN_PATCH_PREFIXES):
                forbidden.append(f"{patch.name}:{source}")
    if forbidden:
        errors.append("Hermes Agent patches modify root-owned Desktop paths: " + ", ".join(forbidden))

    if (root / "Apply-Hermes-LauncherOverlay.ps1").exists():
        errors.append("legacy Desktop overlay entry point still exists")
    if (root / "source" / "hermes-launcher" / "overlay-src").exists():
        errors.append("legacy Desktop overlay source still exists")

    if errors:
        raise ArchitectureError("\n".join(errors))

    return {
        "clientSource": client["sourcePath"],
        "trackedClientFiles": len([path for path in tracked if path.startswith("apps/desktop/")]),
        "agentClientFiles": len([path for path in tracked if path.startswith("packages/hermes-agent-client/")]),
        "harnessPatchCount": len(patches),
        "harnessTree": agent["harnessTree"],
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository-root", default=".")
    args = parser.parse_args(argv)
    try:
        result = validate(Path(args.repository_root))
    except (ArchitectureError, FileNotFoundError, json.JSONDecodeError) as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 1
    print(json.dumps(result, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

