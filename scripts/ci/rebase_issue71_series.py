#!/usr/bin/env python3
"""Rebase the Hermes Local Hermes Agent patch series onto issue #71's candidate.

Temporary maintenance utility. It always reads the authoritative pre-rebase series
from origin/main, seeds its intermediate Git objects on the recorded base, and then
replays it onto the target candidate with narrowly-scoped deterministic conflict
resolvers. Unknown conflicts are captured as artifacts and remain hard failures.
"""
from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Sequence

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
if hasattr(sys.stderr, "reconfigure"):
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

NPM_VERSION = "12.0.0"
PATCH_ROOT = "source/hermes-launcher/patches"
PROJECTLESS_SUBJECT = "fix(desktop): keep packaged chats projectless"


class CommandError(RuntimeError):
    def __init__(self, command: Sequence[str], code: int, output: str):
        self.command = [str(x) for x in command]
        self.returncode = code
        self.output = output
        super().__init__(f"Command failed ({code}): {' '.join(self.command)}")


def run(
    command: Sequence[object],
    *,
    cwd: Path,
    allow_failure: bool = False,
    timeout: int = 1800,
    env: dict[str, str] | None = None,
) -> str:
    cmd = [str(x) for x in command]
    merged = os.environ.copy()
    if env:
        merged.update(env)
    result = subprocess.run(
        cmd,
        cwd=cwd,
        env=merged,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        encoding="utf-8",
        errors="replace",
        timeout=timeout,
        check=False,
    )
    output = result.stdout or ""
    print(f"$ {' '.join(cmd)}")
    if output:
        print(output, end="" if output.endswith("\n") else "\n")
    if result.returncode and not allow_failure:
        raise CommandError(cmd, result.returncode, output)
    return output.strip()


def git_show(repo: Path, spec: str) -> str:
    return run(["git", "show", spec], cwd=repo)


def normalize_paths(lines: str) -> list[str]:
    return [line.strip().replace("\\", "/") for line in lines.splitlines() if line.strip()]


def npm_command(*args: str) -> list[str]:
    executable = "npx.cmd" if os.name == "nt" else "npx"
    return [executable, "--yes", f"npm@{NPM_VERSION}", *args]


def materialize_main_series(repo: Path, destination: Path) -> list[Path]:
    destination.mkdir(parents=True, exist_ok=True)
    listing = run(["git", "ls-tree", "-r", "--name-only", "origin/main", "--", PATCH_ROOT], cwd=repo)
    paths = sorted(path for path in listing.splitlines() if path.endswith(".patch"))
    if not paths:
        raise RuntimeError("origin/main contains no Hermes Agent patch series")
    for path in paths:
        target = destination / Path(path).name
        target.write_bytes(subprocess.check_output(["git", "show", f"origin/main:{path}"], cwd=repo))
    return sorted(destination.glob("*.patch"))


def read_main_manifest(repo: Path) -> dict:
    return json.loads(git_show(repo, "origin/main:VERSION.json"))


def configure_git(source: Path) -> None:
    run(["git", "config", "user.name", "Hermes Local Compatibility CI"], cwd=source)
    run(["git", "config", "user.email", "hermes-local-ci@localhost"], cwd=source)


def seed_preimages(source: Path, base: str, patches: list[Path], expected_tree: str) -> None:
    run(["git", "checkout", "--detach", base], cwd=source)
    run(["git", "switch", "-c", "issue-71-preimage-seed"], cwd=source)
    run(
        ["git", "am", "--3way", "--committer-date-is-author-date", *patches],
        cwd=source,
        timeout=3600,
    )
    tree = run(["git", "rev-parse", "HEAD^{tree}"], cwd=source)
    if expected_tree and tree.lower() != expected_tree.lower():
        raise RuntimeError(f"Seed tree {tree} did not match recorded tree {expected_tree}")


def recover_lockfile(source: Path, conflicts: list[str]) -> bool:
    if conflicts != ["package-lock.json"]:
        return False
    if not (source / "package.json").is_file() or not (source / "package-lock.json").is_file():
        return False
    run(["git", "checkout", "--ours", "--", "package-lock.json"], cwd=source)
    run(
        npm_command(
            "install",
            "--package-lock-only",
            "--ignore-scripts",
            "--no-audit",
            "--fund=false",
        ),
        cwd=source,
        timeout=1800,
    )
    unstaged = normalize_paths(run(["git", "diff", "--name-only"], cwd=source))
    unexpected = [path for path in unstaged if path != "package-lock.json"]
    if unexpected:
        raise RuntimeError("npm lockfile regeneration modified unexpected files: " + ", ".join(unexpected))
    run(["git", "add", "--", "package-lock.json"], cwd=source)
    remaining = normalize_paths(
        run(["git", "diff", "--name-only", "--diff-filter=U"], cwd=source, allow_failure=True)
    )
    if remaining:
        raise RuntimeError("Lockfile regeneration left unresolved paths: " + ", ".join(remaining))
    run(["git", "am", "--continue"], cwd=source, timeout=600)
    return True


def resolve_projectless_patch(source: Path, conflicts: list[str]) -> bool:
    expected = [
        "apps/desktop/src/app/chat/composer/status-stack/coding-row.test.tsx",
        "apps/desktop/src/store/projects.ts",
    ]
    if conflicts != expected:
        return False

    end_marker = f">>>>>>> {PROJECTLESS_SUBJECT}\n"

    coding = source / expected[0]
    text = coding.read_text(encoding="utf-8")
    start = text.index("<<<<<<< HEAD\n")
    middle = text.index("=======\n", start)
    end = text.index(end_marker, middle)
    ours = text[start + len("<<<<<<< HEAD\n") : middle]
    theirs = text[middle + len("=======\n") : end]
    text = text[:start] + ours + "  })\n\n" + theirs + text[end + len(end_marker) :]
    if "<<<<<<<" in text or ">>>>>>>" in text:
        raise RuntimeError("coding-row test conflict markers remain")
    coding.write_text(text, encoding="utf-8")

    projects = source / expected[1]
    text = projects.read_text(encoding="utf-8")
    old_imports = """import {
  $currentCwd,
  $selectedStoredSessionId,
  $sessions,
  idsShareLineage,
  sessionMatchesStoredId,
  workspaceCwdForNewSession
} from '@/store/session'
import { $focusedSessionState, $focusedStoredSessionId } from '@/store/session-states'
"""
    new_imports = (
        "import { $selectedStoredSessionId, $sessions, sessionMatchesStoredId, "
        "workspaceCwdForNewSession } from '@/store/session'\n"
    )
    if old_imports not in text:
        raise RuntimeError("expected projects.ts import block was not found")
    text = text.replace(old_imports, new_imports, 1)
    while "<<<<<<< HEAD\n" in text:
        start = text.index("<<<<<<< HEAD\n")
        middle = text.index("=======\n", start)
        end = text.index(end_marker, middle)
        theirs = text[middle + len("=======\n") : end]
        text = text[:start] + theirs + text[end + len(end_marker) :]
    helper = "/** Live workspace of the session the user is looking at (tile or primary). */\n"
    start = text.index(helper)
    end = text.index("const underPath =", start)
    text = text[:start] + text[end:]
    forbidden = ("$currentCwd", "idsShareLineage", "$focusedSessionState", "$focusedStoredSessionId")
    if any(token in text for token in forbidden):
        raise RuntimeError("removed focused-session inheritance is still referenced")
    if "<<<<<<<" in text or ">>>>>>>" in text:
        raise RuntimeError("projects.ts conflict markers remain")
    projects.write_text(text, encoding="utf-8")

    run(["git", "add", "--", *expected], cwd=source)
    run(["git", "am", "--continue"], cwd=source, timeout=600)
    return True


def capture_conflict(source: Path, output: Path, patch: Path, order: int, conflicts: list[str]) -> None:
    conflict_dir = output / f"conflict-{order:02d}-{patch.stem}"
    conflict_dir.mkdir(parents=True, exist_ok=True)
    (conflict_dir / "status.txt").write_text(
        run(["git", "status", "--short"], cwd=source, allow_failure=True) + "\n",
        encoding="utf-8",
    )
    (conflict_dir / "current.patch").write_text(
        run(["git", "am", "--show-current-patch=diff"], cwd=source, allow_failure=True) + "\n",
        encoding="utf-8",
    )
    for path in conflicts:
        safe = path.replace("/", "__").replace("\\", "__")
        working = source / path
        if working.exists():
            shutil.copy2(working, conflict_dir / f"{safe}.conflicted")
        for stage, name in ((1, "base"), (2, "ours"), (3, "theirs")):
            content = run(["git", "show", f":{stage}:{path}"], cwd=source, allow_failure=True)
            (conflict_dir / f"{safe}.{name}").write_text(content, encoding="utf-8")
    (conflict_dir / "metadata.json").write_text(
        json.dumps({"order": order, "patch": patch.name, "conflicts": conflicts}, indent=2) + "\n",
        encoding="utf-8",
    )


def apply_candidate_series(source: Path, candidate: str, patches: list[Path], output: Path) -> None:
    run(["git", "checkout", "--detach", candidate], cwd=source)
    run(["git", "switch", "-c", "hermes-local-integration"], cwd=source)
    applied: list[dict[str, object]] = []
    for order, patch in enumerate(patches, 1):
        try:
            text = run(
                ["git", "am", "--3way", "--committer-date-is-author-date", patch],
                cwd=source,
                timeout=600,
            )
            applied.append({"order": order, "patch": patch.name, "application": "three-way" if "3-way" in text else "clean"})
            continue
        except CommandError:
            conflicts = normalize_paths(
                run(["git", "diff", "--name-only", "--diff-filter=U"], cwd=source, allow_failure=True)
            )
            recovered = False
            if recover_lockfile(source, conflicts):
                applied.append({"order": order, "patch": patch.name, "application": "three-way-lockfile-regenerated"})
                recovered = True
            elif resolve_projectless_patch(source, conflicts):
                applied.append({"order": order, "patch": patch.name, "application": "three-way-projectless-rebased"})
                recovered = True
            if recovered:
                continue
            capture_conflict(source, output, patch, order, conflicts)
            (output / "applied.json").write_text(json.dumps(applied, indent=2) + "\n", encoding="utf-8")
            raise RuntimeError(f"Unresolved patch {order}: {patch.name}: {', '.join(conflicts)}")

    (output / "applied.json").write_text(json.dumps(applied, indent=2) + "\n", encoding="utf-8")


def export_result(repo: Path, source: Path, candidate: str, output: Path, manifest: dict) -> None:
    patch_out = output / "patches"
    shutil.rmtree(patch_out, ignore_errors=True)
    patch_out.mkdir(parents=True)
    run(
        [
            "git",
            "format-patch",
            "--zero-commit",
            "--no-signature",
            "--output-directory",
            patch_out,
            f"{candidate}..HEAD",
        ],
        cwd=source,
    )
    generated = sorted(patch_out.glob("*.patch"))
    if len(generated) != 30:
        raise RuntimeError(f"Expected 30 rebased patches, found {len(generated)}")
    integration_commit = run(["git", "rev-parse", "HEAD"], cwd=source)
    integration_tree = run(["git", "rev-parse", "HEAD^{tree}"], cwd=source)
    data = manifest
    agent = data["sources"]["hermesAgent"]
    agent["commit"] = candidate
    agent["integrationCommit"] = integration_commit
    agent["integrationTree"] = integration_tree
    from datetime import datetime, timezone
    data["recordedAt"] = datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")
    (output / "VERSION.json").write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
    (output / "result.json").write_text(
        json.dumps(
            {
                "candidate": candidate,
                "integrationCommit": integration_commit,
                "integrationTree": integration_tree,
                "patchCount": len(generated),
            },
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repository-root", required=True)
    parser.add_argument("--candidate", required=True)
    parser.add_argument("--work-dir", required=True)
    parser.add_argument("--output-dir", required=True)
    args = parser.parse_args()

    repo = Path(args.repository_root).resolve()
    work = Path(args.work_dir).resolve()
    output = Path(args.output_dir).resolve()
    candidate = args.candidate.lower()
    shutil.rmtree(work, ignore_errors=True)
    shutil.rmtree(output, ignore_errors=True)
    work.mkdir(parents=True)
    output.mkdir(parents=True)

    run(["git", "fetch", "origin", "main"], cwd=repo)
    manifest = read_main_manifest(repo)
    agent = manifest["sources"]["hermesAgent"]
    base = str(agent["commit"]).lower()
    expected_tree = str(agent.get("integrationTree") or "")
    repository = str(agent["repository"])

    patches = materialize_main_series(repo, work / "original-patches")
    source = work / "hermes-agent"
    run(["git", "clone", "--no-checkout", repository, source], cwd=work, timeout=1800)
    for revision in dict.fromkeys((base, candidate)):
        run(["git", "fetch", "origin", revision], cwd=source, timeout=1800)
    configure_git(source)
    seed_preimages(source, base, patches, expected_tree)
    apply_candidate_series(source, candidate, patches, output)
    export_result(repo, source, candidate, output, manifest)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
