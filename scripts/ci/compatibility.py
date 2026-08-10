#!/usr/bin/env python3
"""Generate Hermes Local upstream compatibility reports (stdlib only)."""
from __future__ import annotations

import argparse
import datetime as dt
import glob
import hashlib
import json
import os
import platform
import re
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Any, Iterable, Sequence

SCHEMA_VERSION = 1
STAGES = ("patches", "dependencies", "tests", "build", "package", "health")
BLOCKED_BY_STAGE = {
    "patches": "blocked-patch-conflict",
    "dependencies": "blocked-dependency",
    "tests": "blocked-tests",
    "build": "blocked-build",
    "package": "blocked-packaging",
    "health": "blocked-runtime-health",
}
STATUS_PRIORITY = {
    "compatible": 0,
    "compatible-with-warnings": 1,
    "blocked-runtime-health": 2,
    "blocked-packaging": 3,
    "blocked-tests": 4,
    "blocked-build": 5,
    "blocked-dependency": 6,
    "blocked-patch-conflict": 7,
    "infrastructure-failure": 8,
}
HEX_SHA = re.compile(r"^[0-9a-fA-F]{40}$")
LLAMA_CPP_TEST_PYTHON_REQUIREMENTS = ("jinja2==3.1.6",)
HERMES_AGENT_NPM_VERSION = "12.0.0"
HERMES_AGENT_UV_FALLBACK = "uv==0.11.32"


class CommandError(RuntimeError):
    def __init__(self, command: Sequence[str], code: int, output: str):
        self.command, self.returncode, self.output = list(command), code, output
        super().__init__(f"Command failed ({code}): {' '.join(command)}")


def now() -> str:
    return dt.datetime.now(dt.timezone.utc).isoformat().replace("+00:00", "Z")


def read_json(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"Expected JSON object: {path}")
    return value


def write_json(path: Path, value: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")
    tmp.replace(path)


def run(
    command: Sequence[str], *, cwd: Path, log: Path, timeout: int = 1800,
    allow_failure: bool = False, env: dict[str, str] | None = None,
) -> str:
    log.parent.mkdir(parents=True, exist_ok=True)
    command = [str(x) for x in command]
    merged = os.environ.copy()
    if env:
        merged.update(env)
    try:
        result = subprocess.run(
            command, cwd=cwd, env=merged, stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT, text=True, encoding="utf-8",
            errors="replace", timeout=timeout, check=False,
        )
        output, code = result.stdout or "", result.returncode
    except FileNotFoundError as exc:
        output, code = str(exc), 127
    except subprocess.TimeoutExpired as exc:
        output, code = (exc.stdout or "") + f"\nTimed out after {timeout}s", 124
    with log.open("a", encoding="utf-8") as handle:
        handle.write(f"$ {' '.join(command)}\n{output}")
        if output and not output.endswith("\n"):
            handle.write("\n")
        handle.write(f"[exit {code}]\n\n")
    if code and not allow_failure:
        raise CommandError(command, code, output)
    return output.strip()


def base_report(component: str, base: str, candidate: str | None, log_dir: Path) -> dict[str, Any]:
    return {
        "schemaVersion": SCHEMA_VERSION,
        "component": component,
        "candidate": candidate,
        "base": base,
        "status": "infrastructure-failure",
        "generatedAt": now(),
        "testedPlatforms": [{
            "os": platform.system(), "release": platform.release(),
            "architecture": platform.machine(), "python": platform.python_version(),
        }],
        "stages": {stage: {"status": "not-run"} for stage in STAGES},
        "artifacts": [], "failures": [], "warnings": [],
        "metadata": {
            "workflowRunId": os.getenv("GITHUB_RUN_ID"),
            "workflowRunAttempt": os.getenv("GITHUB_RUN_ATTEMPT"),
            "repository": os.getenv("GITHUB_REPOSITORY"),
            "logDirectory": str(log_dir),
        },
    }


def stage_pass(report: dict[str, Any], stage: str, **details: Any) -> None:
    report["stages"][stage] = {"status": "passed", **details}


def stage_warning(report: dict[str, Any], stage: str, message: str, **details: Any) -> None:
    report["stages"][stage] = {"status": "warning", "message": message, **details}
    report["warnings"].append({"stage": stage, "message": message})


def fail_report(
    report: dict[str, Any], *, stage: str, message: str,
    error: Exception | None = None, infrastructure: bool = False,
    details: dict[str, Any] | None = None,
) -> None:
    status = "infrastructure-failure" if infrastructure else BLOCKED_BY_STAGE[stage]
    item: dict[str, Any] = {"stage": stage, "status": status, "message": message}
    if isinstance(error, CommandError):
        item.update(command=error.command, exitCode=error.returncode, outputTail=error.output[-8000:])
    elif error:
        item["exception"] = f"{type(error).__name__}: {error}"
    if details:
        item.update(details)
    report["failures"].append(item)
    report["stages"][stage] = {"status": "failed", "message": message}
    report["status"] = status


def finalize_success(report: dict[str, Any]) -> None:
    report["status"] = "compatible-with-warnings" if report["warnings"] else "compatible"
    report["generatedAt"] = now()


def finish(report: dict[str, Any], path: Path, code: int) -> int:
    report["generatedAt"] = now()
    write_json(path, report)
    print(json.dumps(report, indent=2))
    return code


def resolve_candidate(repository: str, reference: str, cwd: Path, log: Path) -> str:
    if HEX_SHA.fullmatch(reference):
        return reference.lower()
    refs = [reference] if reference.startswith("refs/") else [reference, f"refs/heads/{reference}", f"refs/tags/{reference}"]
    for ref in refs:
        for line in run(["git", "ls-remote", repository, ref], cwd=cwd, log=log, allow_failure=True, timeout=180).splitlines():
            sha = line.split()[0] if line.split() else ""
            if HEX_SHA.fullmatch(sha):
                return sha.lower()
    raise RuntimeError(f"No upstream commit found for {reference!r}")


def prepare_hermes_agent_checkout(
    repository: str,
    source: Path,
    *,
    base: str,
    candidate: str,
    work: Path,
    log: Path,
) -> None:
    """Clone and fetch every upstream revision needed by compatibility checks."""
    run(["git", "clone", "--no-checkout", repository, source], cwd=work, log=log, timeout=1800)
    for revision in dict.fromkeys((base, candidate)):
        run(["git", "fetch", "origin", revision], cwd=source, log=log, timeout=1800)
    run(["git", "checkout", "--detach", base], cwd=source, log=log)


def seed_patch_preimages(
    source: Path,
    patches: Sequence[Path],
    *,
    expected_tree: str | None,
    log: Path,
) -> str:
    """Reconstruct the pinned integration so three-way preimage blobs exist.

    Mail patches refer to blobs produced by earlier patches in the series. Those
    objects are not part of the upstream repository, so fetching the base commit
    alone is insufficient. Replaying the known-good series once on its pinned
    base creates every intermediate object before the candidate replay begins.
    """
    run(["git", "switch", "-c", "hermes-local-preimage-seed"], cwd=source, log=log)
    run(
        ["git", "am", "--3way", "--committer-date-is-author-date", *patches],
        cwd=source,
        log=log,
        timeout=1800,
    )
    tree = run(["git", "rev-parse", "HEAD^{tree}"], cwd=source, log=log)
    if expected_tree and tree.lower() != expected_tree.lower():
        raise RuntimeError(
            f"Pinned patch reconstruction produced tree {tree}, expected {expected_tree}."
        )
    return tree


def npm() -> str:
    return "npx.cmd" if os.name == "nt" else "npx"


def npm_command(*arguments: str) -> list[str]:
    return [npm(), "--yes", f"npm@{HERMES_AGENT_NPM_VERSION}", *arguments]


def recover_npm_lockfile_conflict(
    source: Path,
    conflicts: Sequence[str],
    *,
    log: Path,
) -> bool:
    """Resolve a lockfile-only ``git am`` conflict deterministically.

    Upstream periodically regenerates ``package-lock.json`` while the package
    manifests remain mergeable. In that narrow case, keep the candidate's
    lockfile as the base, regenerate it from the already-merged manifests with
    the exact supported npm CLI, stage it, and continue the original mail
    patch. Any additional conflict or unexpected unstaged file remains a hard
    failure so source conflicts are never hidden.
    """
    normalized = [path.strip().replace("\\", "/") for path in conflicts if path.strip()]
    if normalized != ["package-lock.json"]:
        return False
    if not (source / "package.json").is_file() or not (source / "package-lock.json").is_file():
        return False

    run(
        ["git", "checkout", "--ours", "--", "package-lock.json"],
        cwd=source,
        log=log,
    )
    run(
        npm_command(
            "install",
            "--package-lock-only",
            "--ignore-scripts",
            "--no-audit",
            "--fund=false",
        ),
        cwd=source,
        log=log,
        timeout=1800,
    )

    unstaged = [
        path.strip().replace("\\", "/")
        for path in run(["git", "diff", "--name-only"], cwd=source, log=log).splitlines()
        if path.strip()
    ]
    unexpected = [path for path in unstaged if path != "package-lock.json"]
    if unexpected:
        raise RuntimeError(
            "npm lockfile regeneration modified unexpected files: " + ", ".join(unexpected)
        )

    run(["git", "add", "--", "package-lock.json"], cwd=source, log=log)
    remaining = [
        path.strip()
        for path in run(
            ["git", "diff", "--name-only", "--diff-filter=U"],
            cwd=source,
            log=log,
            allow_failure=True,
        ).splitlines()
        if path.strip()
    ]
    if remaining:
        raise RuntimeError(
            "Lockfile regeneration left unresolved paths: " + ", ".join(remaining)
        )

    run(
        ["git", "am", "--continue"],
        cwd=source,
        log=log,
        timeout=600,
    )
    return True


def uv() -> str:
    local = Path(sys.executable).resolve().parent / ("uv.exe" if os.name == "nt" else "uv")
    found = str(local) if local.exists() else shutil.which("uv")
    if not found:
        raise RuntimeError("uv executable unavailable after installation")
    return found


def install_python_requirements(requirements: Sequence[str], *, cwd: Path, log: Path) -> None:
    run(
        [sys.executable, "-m", "pip", "install", "--disable-pip-version-check", *requirements],
        cwd=cwd,
        log=log,
        timeout=900,
    )


def ensure_uv(*, cwd: Path, log: Path) -> str:
    try:
        return uv()
    except RuntimeError:
        install_python_requirements((HERMES_AGENT_UV_FALLBACK,), cwd=cwd, log=log)
        return uv()


def uv_sync_command(executable: str, source: Path) -> list[str]:
    command = [executable, "sync", "--extra", "all", "--extra", "dev"]
    if (source / "uv.lock").exists():
        command.append("--frozen")
    return command


def package_script(path: Path) -> str | None:
    scripts = read_json(path).get("scripts", {}) if path.exists() else {}
    for name in ("package:win", "dist:win", "package", "dist"):
        if name in scripts:
            return name
    return None


def executables(root: Path, names: Iterable[str]) -> list[Path]:
    wanted = {x.lower() for x in names}
    return sorted(p for p in root.rglob("*") if p.is_file() and p.name.lower() in wanted)


def digest(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def hermes_agent(args: argparse.Namespace) -> int:
    root, work, logs, output = map(lambda p: Path(p).resolve(), (args.repository_root, args.work_dir, args.log_dir, args.report))
    meta = read_json(root / args.manifest)["sources"]["hermesAgent"]
    repository, base = str(meta["repository"]), str(meta["commit"]).lower()
    reference = args.candidate_ref or str(meta["branch"])
    shutil.rmtree(work, ignore_errors=True); work.mkdir(parents=True); logs.mkdir(parents=True, exist_ok=True)
    report = base_report("hermes-agent", base, None, logs)
    source = work / "hermes-agent"
    patches = sorted((root / str(meta["patchSeries"])).glob("*.patch"))
    if not patches:
        fail_report(report, stage="patches", message="Ordered Hermes Agent patch series is missing.")
        return finish(report, output, 1)
    try:
        try:
            candidate = resolve_candidate(repository, reference, root, logs / "resolve.log")
            report["candidate"] = candidate
            prepare_hermes_agent_checkout(
                repository,
                source,
                base=base,
                candidate=candidate,
                work=work,
                log=logs / "patches.log",
            )
            run(["git", "config", "user.name", "Hermes Local Compatibility CI"], cwd=source, log=logs / "patches.log")
            run(["git", "config", "user.email", "hermes-local-ci@localhost"], cwd=source, log=logs / "patches.log")
        except Exception as exc:
            fail_report(report, stage="patches", message="Unable to resolve or clone Hermes Agent candidate.", error=exc, infrastructure=True)
            return finish(report, output, 1)

        try:
            seed_tree = seed_patch_preimages(
                source,
                patches,
                expected_tree=str(meta.get("harnessTree") or "") or None,
                log=logs / "patches.log",
            )
        except Exception as exc:
            run(["git", "am", "--abort"], cwd=source, log=logs / "patches.log", allow_failure=True)
            fail_report(
                report,
                stage="patches",
                message="Pinned Hermes Agent patch series could not reconstruct the recorded integration.",
                error=exc,
                details={"phase": "preimage-seed"},
            )
            return finish(report, output, 1)

        try:
            run(["git", "checkout", "--detach", candidate], cwd=source, log=logs / "patches.log")
            run(["git", "switch", "-c", str(meta.get("harnessBranch", "hermes-local-harness"))], cwd=source, log=logs / "patches.log")
            report["metadata"]["preimageSeedTree"] = seed_tree
        except Exception as exc:
            fail_report(report, stage="patches", message="Unable to prepare the candidate harness branch.", error=exc, infrastructure=True)
            return finish(report, output, 1)

        applied: list[dict[str, Any]] = []
        for index, patch in enumerate(patches, 1):
            try:
                text = run(["git", "am", "--3way", "--committer-date-is-author-date", patch], cwd=source, log=logs / "patches.log", timeout=600)
                mode = "three-way" if "3-way merge" in text.lower() else "clean"
                applied.append({"order": index, "patch": patch.name, "status": "passed", "application": mode})
            except CommandError as exc:
                conflicts = run(["git", "diff", "--name-only", "--diff-filter=U"], cwd=source, log=logs / "patches.log", allow_failure=True).splitlines()
                recovery_error: Exception | None = None
                try:
                    recovered = recover_npm_lockfile_conflict(
                        source,
                        conflicts,
                        log=logs / "patches.log",
                    )
                except Exception as lock_exc:
                    recovered = False
                    recovery_error = lock_exc
                if recovered:
                    applied.append({
                        "order": index,
                        "patch": patch.name,
                        "status": "passed",
                        "application": "three-way-lockfile-regenerated",
                    })
                    continue

                current = run(["git", "am", "--show-current-patch=diff"], cwd=source, log=logs / "patches.log", allow_failure=True)
                run(["git", "am", "--abort"], cwd=source, log=logs / "patches.log", allow_failure=True)
                details = {
                    "patch": patch.name, "patchOrder": index, "conflictedFiles": conflicts,
                    "currentPatchTail": current[-8000:], "laterPatchesSkipped": [p.name for p in patches[index:]],
                }
                if recovery_error:
                    details["lockfileRecoveryError"] = f"{type(recovery_error).__name__}: {recovery_error}"
                fail_report(report, stage="patches", message=f"Patch {patch.name} failed against {candidate}.", error=exc, details=details)
                report["stages"]["patches"]["patches"] = applied
                return finish(report, output, 1)
        try:
            dirty = run(["git", "status", "--porcelain"], cwd=source, log=logs / "patches.log")
            if dirty:
                raise RuntimeError(f"Integrated tree is dirty: {dirty}")
            stage_pass(report, "patches", patchCount=len(patches), patches=applied,
                       harnessCommit=run(["git", "rev-parse", "HEAD"], cwd=source, log=logs / "patches.log"),
                       harnessTree=run(["git", "rev-parse", "HEAD^{tree}"], cwd=source, log=logs / "patches.log"))
        except Exception as exc:
            fail_report(report, stage="patches", message="Integrated Hermes Agent tree validation failed.", error=exc)
            return finish(report, output, 1)

        desktop = (root / "apps/desktop/package.json").exists()
        python = (source / "pyproject.toml").exists()
        try:
            done = []
            if args.run_desktop_checks and desktop:
                run(npm_command("ci", "--no-audit", "--fund=false"), cwd=root, log=logs / "dependencies.log", timeout=3600); done.append("node:hermes-local-client")
            if args.run_python_checks and python:
                uv_executable = ensure_uv(cwd=source, log=logs / "dependencies.log")
                cmd = uv_sync_command(uv_executable, source)
                run(cmd, cwd=source, log=logs / "dependencies.log", timeout=3600); done.append("python")
            stage_pass(report, "dependencies", ecosystems=done) if done else stage_warning(report, "dependencies", "No dependency installation requested or supported.")
        except Exception as exc:
            fail_report(report, stage="dependencies", message="Candidate dependency installation failed.", error=exc)
            return finish(report, output, 1)

        try:
            done = []
            if args.run_python_checks and python:
                selected = [
                    "tests/hermes_state/test_session_md_export.py", "tests/tools/test_browser_hardening.py",
                    "tests/acp_adapter/test_acp_commands.py", "tests/tools/test_write_approval.py",
                    "tests/run_agent/test_streaming_tool_call_repair.py", "tests/tools/test_read_extract.py",
                    "tests/run_agent/test_dropped_tool_call_recovery.py", "tests/tools/test_browser_ssrf_local.py",
                    "tests/tools/test_cronjob_tools.py", "tests/tools/test_interrupt.py", "tests/tools/test_delegate.py",
                    "tests/agent/test_skill_commands.py", "tests/run_agent/test_compression_persistence.py",
                    "tests/hermes_cli/test_projects_db.py", "tests/tui_gateway/test_project_tree.py",
                    "tests/tui_gateway/test_projects_rpc.py",
                ]
                selected = [x for x in selected if (source / x).exists()]
                if not selected:
                    raise RuntimeError("Windows-critical Python test selection is absent")
                run([uv(), "run", "python", "-m", "pytest", *selected, "-q"], cwd=source, log=logs / "tests.log", timeout=3600)
                done.append(f"python:{len(selected)} files")
            if args.run_desktop_checks and desktop:
                run(npm_command("run", "typecheck"), cwd=root, log=logs / "tests.log")
                run(npm_command("run", "lint"), cwd=root, log=logs / "tests.log")
                vitest = root / "node_modules/.bin" / ("vitest.cmd" if os.name == "nt" else "vitest")
                if vitest.exists():
                    run([vitest, "run", "--project", "electron", "electron/hermes-local-control.test.ts"], cwd=root / "apps/desktop", log=logs / "tests.log")
                    run([vitest, "run", "--project", "electron", "electron/hermes-local-update.test.ts"], cwd=root / "apps/desktop", log=logs / "tests.log")
                    run([vitest, "run", "--project", "electron", "electron/hermes-local-security-progress.test.ts"], cwd=root / "apps/desktop", log=logs / "tests.log")
                    run([vitest, "run", "src/app/local-workstation/task-centre.test.tsx", "src/app/local-workstation/security-task-view.test.tsx"], cwd=root / "apps/desktop", log=logs / "tests.log")
                    run([vitest, "run", "src/store/projects.test.ts"], cwd=root / "apps/desktop", log=logs / "tests.log")
                done.append("desktop:typecheck,lint,focused-electron,security-task-ui,project-registry")
            stage_pass(report, "tests", checks=done) if done else stage_warning(report, "tests", "No test suites requested.")
        except Exception as exc:
            fail_report(report, stage="tests", message="Candidate regression tests failed.", error=exc)
            return finish(report, output, 1)

        try:
            done = []
            if args.run_desktop_checks and desktop:
                run(npm_command("run", "build"), cwd=root, log=logs / "build.log", timeout=3600); done.append("desktop:hermes-local-client")
            if args.run_python_checks and python and (source / "hermes_cli").exists():
                run([uv(), "run", "python", "-m", "compileall", "-q", "hermes_cli"], cwd=source, log=logs / "build.log"); done.append("python-bytecode")
            stage_pass(report, "build", checks=done) if done else stage_warning(report, "build", "No candidate build requested.")
        except Exception as exc:
            fail_report(report, stage="build", message="Candidate build failed.", error=exc)
            return finish(report, output, 1)

        script = package_script(root / "apps/desktop/package.json") if desktop else None
        if args.run_package_checks and script:
            try:
                run(npm_command("run", script, "--workspace", "apps/desktop"), cwd=root, log=logs / "package.log", timeout=5400)
                stage_pass(report, "package", script=script)
            except Exception as exc:
                fail_report(report, stage="package", message=f"Desktop packaging script {script!r} failed.", error=exc)
                return finish(report, output, 1)
        else:
            stage_warning(report, "package", "Packaged launcher validation was not requested or no recognized script exists.")
        stage_warning(report, "health", "Hosted CI did not launch the packaged workstation with a real local model.")
        report["metadata"].update(candidateReference=reference, upstreamRepository=repository)
        finalize_success(report)
        return finish(report, output, 0)
    finally:
        if not args.keep_workspace:
            shutil.rmtree(work, ignore_errors=True)


def llama_cpp(args: argparse.Namespace) -> int:
    root, work, logs, output = map(lambda p: Path(p).resolve(), (args.repository_root, args.work_dir, args.log_dir, args.report))
    meta = read_json(root / args.manifest)["sources"]["llamaCpp"]
    repository, base = str(meta["repository"]), str(meta["commit"]).lower()
    reference = args.candidate_ref or str(meta["branch"])
    shutil.rmtree(work, ignore_errors=True); work.mkdir(parents=True); logs.mkdir(parents=True, exist_ok=True)
    report = base_report("llama-cpp-gpu" if args.cuda else "llama-cpp-cpu", base, None, logs)
    source, build = work / "llama.cpp", work / "build"
    try:
        try:
            candidate = resolve_candidate(repository, reference, root, logs / "resolve.log"); report["candidate"] = candidate
            run(["git", "clone", "--filter=blob:none", "--no-checkout", repository, source], cwd=work, log=logs / "source.log", timeout=1800)
            run(["git", "checkout", "--detach", candidate], cwd=source, log=logs / "source.log")
            stage_pass(report, "patches", mode="not-applicable", sourceCommit=candidate)
        except Exception as exc:
            fail_report(report, stage="patches", message="Unable to resolve or clone llama.cpp candidate.", error=exc, infrastructure=True)
            return finish(report, output, 1)
        try:
            if not shutil.which("cmake") or (args.cuda and not shutil.which("nvcc")):
                raise RuntimeError("Required CMake/CUDA build tools are unavailable")
            dependency_log = logs / "dependencies.log"
            cmake_version = run(["cmake", "--version"], cwd=source, log=dependency_log).splitlines()[0]
            install_python_requirements(
                LLAMA_CPP_TEST_PYTHON_REQUIREMENTS,
                cwd=source,
                log=dependency_log,
            )
            stage_pass(
                report,
                "dependencies",
                cmake=cmake_version,
                pythonRequirements=list(LLAMA_CPP_TEST_PYTHON_REQUIREMENTS),
            )
        except Exception as exc:
            fail_report(report, stage="dependencies", message="llama.cpp build dependencies unavailable.", error=exc, infrastructure=True)
            return finish(report, output, 1)
        try:
            run(["cmake", "-S", source, "-B", build, f"-DGGML_CUDA={'ON' if args.cuda else 'OFF'}", "-DLLAMA_CURL=OFF", "-DLLAMA_BUILD_TESTS=ON", "-DLLAMA_BUILD_EXAMPLES=ON", "-DBUILD_SHARED_LIBS=OFF"], cwd=work, log=logs / "build.log")
            run(["cmake", "--build", build, "--config", "Release", "--parallel", str(args.parallel)], cwd=work, log=logs / "build.log", timeout=5400)
            stage_pass(report, "build", acceleration="cuda" if args.cuda else "cpu")
        except Exception as exc:
            fail_report(report, stage="build", message="llama.cpp candidate build failed.", error=exc)
            return finish(report, output, 1)
        try:
            run(["ctest", "--test-dir", build, "-C", "Release", "--output-on-failure"], cwd=work, log=logs / "tests.log", timeout=3600)
            stage_pass(report, "tests", command="ctest")
        except Exception as exc:
            fail_report(report, stage="tests", message="llama.cpp tests failed.", error=exc)
            return finish(report, output, 1)
        bins = executables(build, ("llama-cli.exe", "llama-cli", "llama-server.exe", "llama-server"))
        records = [{"name": p.name, "path": str(p.relative_to(work)), "sizeBytes": p.stat().st_size, "sha256": digest(p)} for p in bins]
        if not records:
            fail_report(report, stage="package", message="Build produced no llama-cli or llama-server binaries.")
            return finish(report, output, 1)
        report["artifacts"].extend(records); stage_pass(report, "package", binaries=records)
        try:
            smoke = [{"binary": p.name, "output": run([p, "--version"], cwd=work, log=logs / "health.log", timeout=120)[-1000:]} for p in bins]
            stage_warning(report, "health", "Binary smoke passed; a licensed tiny-model API/auth check is still required before Stable promotion.", smokeTests=smoke)
        except Exception as exc:
            fail_report(report, stage="health", message="llama.cpp binary smoke failed.", error=exc)
            return finish(report, output, 1)
        report["metadata"].update(candidateReference=reference, upstreamRepository=repository, acceleration="cuda" if args.cuda else "cpu")
        finalize_success(report)
        return finish(report, output, 0)
    finally:
        if not args.keep_workspace:
            shutil.rmtree(work, ignore_errors=True)


def aggregate_status(reports: Sequence[dict[str, Any]]) -> str:
    if not reports:
        return "infrastructure-failure"
    return max((str(r.get("status", "infrastructure-failure")) for r in reports), key=lambda x: STATUS_PRIORITY.get(x, 8))


def aggregate(args: argparse.Namespace) -> int:
    paths: list[Path] = []
    for pattern in args.reports:
        found = [Path(x) for x in glob.glob(pattern, recursive=True)]
        paths.extend(found if found else [Path(pattern)])
    paths = sorted({p.resolve() for p in paths if p.is_file()})
    reports = [read_json(p) for p in paths]
    value = {
        "schemaVersion": SCHEMA_VERSION, "component": "hermes-local-upstream-compatibility",
        "generatedAt": now(), "status": aggregate_status(reports), "components": reports,
        "artifacts": [str(p) for p in paths],
        "failures": [{"component": r.get("component"), **x} for r in reports for x in r.get("failures", [])],
        "warnings": [{"component": r.get("component"), **x} for r in reports for x in r.get("warnings", [])],
    }
    write_json(Path(args.report).resolve(), value); print(json.dumps(value, indent=2))
    return int(args.fail_on_blocked and value["status"] not in {"compatible", "compatible-with-warnings"})


def verify(args: argparse.Namespace) -> int:
    report = read_json(Path(args.report).resolve()); errors = []
    if report.get("schemaVersion") != SCHEMA_VERSION:
        errors.append(f"schemaVersion must be {SCHEMA_VERSION}")
    if report.get("status") not in set(args.allowed_status):
        errors.append(f"status {report.get('status')!r} is not promotable")
    present = {x.get("component") for x in report.get("components", []) if isinstance(x, dict)}
    missing = sorted(set(args.require_component) - present)
    if missing:
        errors.append(f"missing required component reports: {', '.join(missing)}")
    if errors:
        for error in errors:
            print(f"ERROR: {error}", file=sys.stderr)
        return 1
    print(f"Compatibility report is promotable: {report.get('status')}")
    return 0


def parser() -> argparse.ArgumentParser:
    root = argparse.ArgumentParser(description=__doc__); subs = root.add_subparsers(dest="command", required=True)
    common = argparse.ArgumentParser(add_help=False)
    for name, default in (("repository-root", "."), ("manifest", "VERSION.json"), ("candidate-ref", "")):
        common.add_argument(f"--{name}", default=default)
    for name in ("work-dir", "log-dir", "report"):
        common.add_argument(f"--{name}", required=True)
    common.add_argument("--keep-workspace", action="store_true")
    agent = subs.add_parser("hermes-agent", parents=[common])
    for name in ("run-desktop-checks", "run-python-checks", "run-package-checks"):
        agent.add_argument(f"--{name}", action="store_true")
    agent.set_defaults(handler=hermes_agent)
    runtime = subs.add_parser("llama-cpp", parents=[common]); runtime.add_argument("--cuda", action="store_true"); runtime.add_argument("--parallel", type=int, default=2); runtime.set_defaults(handler=llama_cpp)
    combined = subs.add_parser("aggregate"); combined.add_argument("--reports", nargs="+", required=True); combined.add_argument("--report", required=True); combined.add_argument("--fail-on-blocked", action="store_true"); combined.set_defaults(handler=aggregate)
    promotion = subs.add_parser("verify"); promotion.add_argument("--report", required=True); promotion.add_argument("--allowed-status", action="append", default=["compatible", "compatible-with-warnings"]); promotion.add_argument("--require-component", action="append", default=[]); promotion.set_defaults(handler=verify)
    return root


def main(argv: Sequence[str] | None = None) -> int:
    args = parser().parse_args(argv)
    return int(args.handler(args))


if __name__ == "__main__":
    raise SystemExit(main())
