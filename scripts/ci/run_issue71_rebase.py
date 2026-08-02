#!/usr/bin/env python3
"""Run the issue #71 full-series rebase with known source resolutions."""
from __future__ import annotations

import rebase_issue71_series as rebase

DASHBOARD_SUBJECT = "feat(desktop): embed loopback dashboard securely"
_original_projectless_resolver = rebase.resolve_projectless_patch


def resolve_known_source_conflicts(source, conflicts):
    if _original_projectless_resolver(source, conflicts):
        return True

    path = "apps/desktop/electron/main.ts"
    if conflicts != [path]:
        return False

    target = source / path
    text = target.read_text(encoding="utf-8")
    marker = f"""<<<<<<< HEAD
    wakeIndicatorController.close()
=======
    hermesLocalDashboardView.handleWindowClosed(createdMainWindow)
>>>>>>> {DASHBOARD_SUBJECT}
"""
    replacement = """    wakeIndicatorController.close()
    hermesLocalDashboardView.handleWindowClosed(createdMainWindow)
"""
    if marker not in text:
        return False

    text = text.replace(marker, replacement, 1)
    if "<<<<<<<" in text or ">>>>>>>" in text:
        raise RuntimeError("dashboard main-process conflict markers remain")
    target.write_text(text, encoding="utf-8")
    rebase.run(["git", "add", "--", path], cwd=source)
    rebase.run(["git", "am", "--continue"], cwd=source, timeout=600)
    return True


rebase.resolve_projectless_patch = resolve_known_source_conflicts
raise SystemExit(rebase.main())
