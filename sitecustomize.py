"""Temporary PR-only integration source capture for issue #29 development."""

from __future__ import annotations

import base64
import json
import os
from pathlib import Path
import subprocess
import tempfile


def _run() -> None:
    if os.environ.get("GITHUB_ACTIONS") != "true" or os.environ.get("GITHUB_EVENT_NAME") != "pull_request":
        return

    root = Path.cwd()
    marker = root / ".issue-29-source-snapshot-complete"
    if marker.exists() or not (root / "VERSION.json").is_file():
        return

    marker.write_text("started", encoding="utf-8")
    try:
        version = json.loads((root / "VERSION.json").read_text(encoding="utf-8"))
        base = version["sources"]["hermesAgent"]["commit"]
        with tempfile.TemporaryDirectory(prefix="hermes-issue-29-") as temp_dir:
            temp = Path(temp_dir)
            source = temp / "source"
            archive = temp / "issue-29-source.tar.gz"
            subprocess.run(
                ["git", "clone", "--filter=blob:none", "https://github.com/NousResearch/hermes-agent.git", str(source)],
                check=True,
            )
            subprocess.run(["git", "-C", str(source), "checkout", "--detach", base], check=True)
            subprocess.run(["git", "-C", str(source), "config", "user.name", "Hermes Local Snapshot"], check=True)
            subprocess.run(
                ["git", "-C", str(source), "config", "user.email", "hermes-local-snapshot@localhost"],
                check=True,
            )
            subprocess.run(["git", "-C", str(source), "switch", "-c", "hermes-local-integration"], check=True)
            patches = sorted((root / "source" / "hermes-launcher" / "patches").glob("*.patch"))
            subprocess.run(
                [
                    "git",
                    "-C",
                    str(source),
                    "am",
                    "--3way",
                    "--committer-date-is-author-date",
                    *[str(patch) for patch in patches],
                ],
                check=True,
            )
            subprocess.run(
                [
                    "git",
                    "-C",
                    str(source),
                    "archive",
                    "--format=tar.gz",
                    f"--output={archive}",
                    "HEAD",
                    "apps/desktop/electron",
                    "apps/desktop/e2e",
                    "apps/desktop/package.json",
                    "apps/desktop/src/app/local-workstation",
                    "apps/desktop/src/app/routes.ts",
                    "apps/desktop/src/global.d.ts",
                ],
                check=True,
            )
            encoded = base64.b64encode(archive.read_bytes()).decode("ascii")
            print("ISSUE29_SOURCE_SNAPSHOT_BEGIN")
            for offset in range(0, len(encoded), 120):
                print(encoded[offset : offset + 120])
            print("ISSUE29_SOURCE_SNAPSHOT_END")
            print(f"ISSUE29_SOURCE_BASE={base}")
            print(
                "ISSUE29_SOURCE_TREE="
                + subprocess.check_output(
                    ["git", "-C", str(source), "rev-parse", "HEAD^{tree}"], text=True
                ).strip()
            )
    except Exception as error:  # pragma: no cover - diagnostic-only helper
        print(f"ISSUE29_SOURCE_SNAPSHOT_ERROR={type(error).__name__}: {error}")


_run()
