#!/usr/bin/env python3
"""Apply deterministic compatibility corrections after the issue #50 prototype diff."""
from __future__ import annotations

import sys
from pathlib import Path


def replace_required(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    if old not in text:
        raise RuntimeError(f"Expected issue #50 compatibility anchor missing in {path}: {old!r}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8", newline="")


def main() -> int:
    if len(sys.argv) != 2:
        raise SystemExit("usage: issue50_fix.py HERMES_AGENT_ROOT")

    root = Path(sys.argv[1]).resolve()
    types = root / "apps/desktop/src/types/hermes.ts"
    tests = root / "apps/desktop/src/store/projects.test.ts"

    for field in (
        "path_key: null | string",
        "repository_id: null | string",
        "path_state: 'available' | 'inaccessible' | 'missing' | 'moved'",
        "last_checked_at: null | number",
        "last_opened_at: null | number",
        "last_active_at: null | number",
        "path_aliases: ProjectPathAlias[]",
    ):
        replace_required(types, field, field.replace(":", "?:", 1))

    replace_required(
        tests,
        "  $activeProjectId,\n  $projectScope,\n  $projects,",
        "  $activeProjectId,\n  $projects,\n  $projectScope,",
    )
    replace_required(
        tests,
        "it('inherits the focused session workspace when not drilled into a project', () => {",
        "it('does not inherit the focused session workspace for a bare new chat', () => {",
    )
    replace_required(
        tests,
        "    expect(resolveNewSessionCwd()).toBe('/Users/me/www/hermes-agent')",
        "    expect(resolveNewSessionCwd()).toBe('/home/user/configured')",
    )
    replace_required(
        tests,
        "    // expect the new draft to stay in that project without sidebar drill-in.",
        "    // issue #49 keeps a bare new chat detached unless project scope is explicit.",
    )

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
