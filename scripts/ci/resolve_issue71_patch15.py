#!/usr/bin/env python3
"""Temporary resolver used to rebase Hermes Local patch 15."""
from __future__ import annotations

import sys
from pathlib import Path

END_MARKER = ">>>>>>> fix(desktop): keep packaged chats projectless\n"


def resolve_coding_test(source: Path) -> None:
    path = source / "apps/desktop/src/app/chat/composer/status-stack/coding-row.test.tsx"
    text = path.read_text(encoding="utf-8")
    start = text.index("<<<<<<< HEAD\n")
    middle = text.index("=======\n", start)
    end = text.index(END_MARKER, middle)
    ours = text[start + len("<<<<<<< HEAD\n") : middle]
    theirs = text[middle + len("=======\n") : end]
    text = text[:start] + ours + "  })\n\n" + theirs + text[end + len(END_MARKER) :]
    if "<<<<<<<" in text or ">>>>>>>" in text:
        raise RuntimeError("coding-row test conflict markers remain")
    path.write_text(text, encoding="utf-8")


def resolve_projects(source: Path) -> None:
    path = source / "apps/desktop/src/store/projects.ts"
    text = path.read_text(encoding="utf-8")
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
        end = text.index(END_MARKER, middle)
        theirs = text[middle + len("=======\n") : end]
        text = text[:start] + theirs + text[end + len(END_MARKER) :]
    helper = "/** Live workspace of the session the user is looking at (tile or primary). */\n"
    start = text.index(helper)
    end = text.index("const underPath =", start)
    text = text[:start] + text[end:]
    forbidden = ("$currentCwd", "idsShareLineage", "$focusedSessionState", "$focusedStoredSessionId")
    if any(token in text for token in forbidden):
        raise RuntimeError("removed focused-session inheritance is still referenced")
    if "<<<<<<<" in text or ">>>>>>>" in text:
        raise RuntimeError("projects.ts conflict markers remain")
    path.write_text(text, encoding="utf-8")


def main() -> int:
    source = Path(sys.argv[1]).resolve()
    resolve_coding_test(source)
    resolve_projects(source)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
