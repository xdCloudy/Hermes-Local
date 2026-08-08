from pathlib import Path
import sys

root = Path(sys.argv[1])


def read(path):
    return (root / path).read_text(encoding="utf-8")


def write(path, text):
    (root / path).write_text(text, encoding="utf-8", newline="")


def one(path, old, new):
    text = read(path)
    n = text.count(old)
    if n != 1:
        raise RuntimeError(f"{path}: expected 1 match, got {n}: {old[:100]!r}")
    write(path, text.replace(old, new, 1))


one(
    "hermes_state_common.py",
    "    git_repo_root TEXT,\n    billing_provider TEXT,",
    "    git_repo_root TEXT,\n    project_id TEXT,\n    billing_provider TEXT,",
)

one(
    "hermes_state.py",
    '                "UPDATE sessions SET cwd = NULL, git_branch = NULL, git_repo_root = NULL WHERE id = ?",',
    '                "UPDATE sessions SET cwd = NULL, git_branch = NULL, git_repo_root = NULL, "\n'
    '                "project_id = NULL WHERE id = ?",',
)
one(
    "hermes_state.py",
    "        self._execute_write(_do)\n\n    def backfill_repo_roots(self, cwd_to_root: Dict[str, str]) -> None:",
    '''        self._execute_write(_do)\n\n    def update_session_project(self, session_id: str, project_id: Optional[str]) -> None:\n        """Persist the stable Project owning a session, or detach it."""\n        if not session_id:\n            return\n        value = str(project_id or "").strip() or None\n\n        def _do(conn):\n            conn.execute(\n                "UPDATE sessions SET project_id = ? WHERE id = ?",\n                (value, session_id),\n            )\n\n        self._execute_write(_do)\n\n    def backfill_repo_roots(self, cwd_to_root: Dict[str, str]) -> None:''',
)

one(
    "tui_gateway/project_tree.py",
    '''    folder_index = _FolderIndex(active_projects)\n\n    by_project: dict[str, list[dict]] = {}\n    unowned: list[dict] = []\n    for session in sessions:\n        owner = _project_for_session(session, folder_index, resolve)\n        if owner:\n            by_project.setdefault(owner["id"], []).append(session)\n        else:\n            unowned.append(session)\n''',
    '''    folder_index = _FolderIndex(active_projects)\n    project_by_id = {\n        str(project.get("id")): project\n        for project in active_projects\n        if str(project.get("id") or "").strip()\n    }\n\n    by_project: dict[str, list[dict]] = {}\n    unowned: list[dict] = []\n    identity_orphans: list[dict] = []\n    for session in sessions:\n        selected_project_id = str(session.get("project_id") or "").strip()\n        owner = (\n            project_by_id.get(selected_project_id)\n            if selected_project_id\n            else _project_for_session(session, folder_index, resolve)\n        )\n        if owner:\n            by_project.setdefault(owner["id"], []).append(session)\n        elif selected_project_id:\n            identity_orphans.append(session)\n        else:\n            unowned.append(session)\n''',
)
one(
    "tui_gateway/project_tree.py",
    "    # Every session no tier could place. These are the Home bucket's rows.\n    homeless: list[dict] = []",
    "    # Explicit-but-missing project ids never fall back to cwd/repo heuristics.\n"
    "    # Keep those rows visible under Home until the user reattaches them.\n"
    "    homeless: list[dict] = list(identity_orphans)",
)

one(
    "tui_gateway/server.py",
    '    session["cwd_from_settle"] = False\n    _register_session_cwd(session)',
    '    session["cwd_from_settle"] = False\n    session.pop("project_id", None)\n    _register_session_cwd(session)',
)
one(
    "tui_gateway/server.py",
    '    session["cwd"] = neutral\n    session["explicit_cwd"] = False\n    _register_session_cwd(session)',
    '    session["cwd"] = neutral\n    session["explicit_cwd"] = False\n    session["project_id"] = None\n    _register_session_cwd(session)',
)

helpers = r'''
def _projects_db_path_for_session(session: dict | None) -> Path | None:
    profile_home = str((session or {}).get("profile_home") or "").strip()
    return Path(profile_home) / "projects.db" if profile_home else None


def _project_payload(project) -> dict | None:
    if project is None:
        return None
    return {
        "id": project.id,
        "slug": project.slug,
        "name": project.name,
        "primary_path": project.primary_path,
    }


def _project_info_for_id(project_id: str | None, session: dict | None = None) -> dict | None:
    selected = str(project_id or "").strip()
    if not selected:
        return None
    try:
        from hermes_cli import projects_db as pdb

        with pdb.connect_closing(_projects_db_path_for_session(session)) as conn:
            project = pdb.get_project(conn, selected)
        if project is None or getattr(project, "archived", False):
            return None
        return _project_payload(project)
    except Exception:
        logger.debug("failed to resolve project id", exc_info=True)
        return None


def _session_project_identity(
    session: dict | None, cwd: str | None = None
) -> tuple[str | None, dict | None]:
    """Return/backfill the authoritative stable Project id for a session."""
    if not isinstance(session, dict):
        return None, None

    if "project_id" in session:
        selected = str(session.get("project_id") or "").strip() or None
        return selected, _project_info_for_id(selected, session) if selected else None

    key = str(session.get("session_key") or "").strip()
    if key:
        try:
            with _session_db(session) as db:
                row = db.get_session(key) if db is not None else None
            selected = str((row or {}).get("project_id") or "").strip() or None
            if selected:
                session["project_id"] = selected
                return selected, _project_info_for_id(selected, session)
        except Exception:
            logger.debug("failed to read persisted session project", exc_info=True)

    display_cwd = _display_session_cwd(session) if cwd is None else str(cwd or "")
    if display_cwd:
        project = (
            _project_info_for_cwd(display_cwd, session)
            if session.get("profile_home")
            else _project_info_for_cwd(display_cwd)
        )
        selected = str((project or {}).get("id") or "").strip() or None
        if selected:
            session["project_id"] = selected
            if key:
                try:
                    with _session_db(session) as db:
                        if db is not None:
                            db.update_session_project(key, selected)
                except Exception:
                    logger.debug("failed to backfill session project", exc_info=True)
            return selected, project

    session["project_id"] = None
    return None, None


'''
one(
    "tui_gateway/server.py",
    "\n# ── Config I/O ────────────────────────────────────────────────────────\n",
    "\n" + helpers + "# ── Config I/O ────────────────────────────────────────────────────────\n",
)

one(
    "tui_gateway/server.py",
    "# v7: session.cwd.set accepts an empty cwd to explicitly detach a chat.\nDESKTOP_BACKEND_CONTRACT = 7",
    "# v7: session.cwd.set accepts an empty cwd to explicitly detach a chat.\n"
    "# v8: sessions persist and report an authoritative stable Project id.\n"
    "DESKTOP_BACKEND_CONTRACT = 8",
)
one(
    "tui_gateway/server.py",
    "def _project_info_for_cwd(cwd: str) -> dict | None:",
    "def _project_info_for_cwd(cwd: str, session: dict | None = None) -> dict | None:",
)
one(
    "tui_gateway/server.py",
    "        with pdb.connect_closing() as conn:\n            project = pdb.project_for_path(conn, cwd)",
    "        with pdb.connect_closing(_projects_db_path_for_session(session)) as conn:\n            project = pdb.project_for_path(conn, cwd)",
)
one(
    "tui_gateway/server.py",
    '''        if project is None:\n            return None\n        return {\n            "id": project.id,\n            "slug": project.slug,\n            "name": project.name,\n            "primary_path": project.primary_path,\n        }''',
    "        return _project_payload(project)",
)
one(
    "tui_gateway/server.py",
    "    mirror = _metadata_mirror(session)\n    cwd = _display_session_cwd(session)\n    session_key = str(",
    "    mirror = _metadata_mirror(session)\n    cwd = _display_session_cwd(session)\n"
    "    project_id, project = _session_project_identity(session, cwd)\n    session_key = str(",
)
one(
    "tui_gateway/server.py",
    '        "cwd": cwd,\n        "branch": _git_branch_for_cwd(cwd),\n        "project": _project_info_for_cwd(cwd),',
    '        "cwd": cwd,\n        "branch": _git_branch_for_cwd(cwd),\n        "project_id": project_id,\n        "project": project,',
)
one(
    "tui_gateway/server.py",
    '''            profile_name=Path(profile_home).name if profile_home else None,\n        )\n    except Exception as exc:''',
    '''            profile_name=Path(profile_home).name if profile_home else None,\n        )\n        project_id = str(session.get("project_id") or "").strip()\n        if project_id:\n            db.update_session_project(key, project_id)\n    except Exception as exc:''',
)
one(
    "tui_gateway/server.py",
    '''def _fallback_session_info(session: dict) -> dict:\n    agent = session.get("agent")\n    if agent is not None:\n        return _session_info(agent)\n    cwd = _display_session_cwd(session)\n    return {\n        "branch": _git_branch_for_cwd(cwd),\n        "cwd": cwd,\n        "desktop_contract": DESKTOP_BACKEND_CONTRACT,\n        "project": _project_info_for_cwd(cwd),''',
    '''def _fallback_session_info(session: dict) -> dict:\n    agent = session.get("agent")\n    if agent is not None:\n        return _session_info(agent, session)\n    cwd = _display_session_cwd(session)\n    project_id, project = _session_project_identity(session, cwd)\n    return {\n        "branch": _git_branch_for_cwd(cwd),\n        "cwd": cwd,\n        "desktop_contract": DESKTOP_BACKEND_CONTRACT,\n        "project_id": project_id,\n        "project": project,''',
)

one(
    "tui_gateway/methods_session.py",
    "    display_cwd = _display_session_cwd(_sessions[sid])\n\n    return _ok(",
    "    display_cwd = _display_session_cwd(_sessions[sid])\n"
    "    display_project = _project_info_for_cwd(display_cwd, _sessions[sid]) if display_cwd else None\n"
    "    display_project_id = str((display_project or {}).get(\"id\") or \"\").strip() or None\n"
    "    _sessions[sid][\"project_id\"] = display_project_id\n\n    return _ok(",
)
one(
    "tui_gateway/methods_session.py",
    '                "branch": _git_branch_for_cwd(display_cwd),\n                "project": _project_info_for_cwd(display_cwd),',
    '                "branch": _git_branch_for_cwd(display_cwd),\n                "project_id": display_project_id,\n                "project": display_project,',
)
one(
    "tui_gateway/methods_session.py",
    '''    agent = session.get("agent")\n    info = (\n        _session_info(agent, session)\n        if agent is not None\n        else {\n            "cwd": cwd,\n            "branch": _git_branch_for_cwd(cwd),\n            "project": _project_info_for_cwd(cwd),\n            "lazy": True,\n            "desktop_contract": DESKTOP_BACKEND_CONTRACT,\n        }\n    )''',
    '''    project_id, project = _session_project_identity(session, cwd)\n    agent = session.get("agent")\n    info = (\n        _session_info(agent, session)\n        if agent is not None\n        else {\n            "cwd": cwd,\n            "branch": _git_branch_for_cwd(cwd),\n            "project_id": project_id,\n            "project": project,\n            "lazy": True,\n            "desktop_contract": DESKTOP_BACKEND_CONTRACT,\n        }\n    )''',
)
one(
    "tui_gateway/methods_session.py",
    '''    stored_cwd = str(found.get("cwd") or "").strip()\n    profile_resume_cwd = stored_cwd or _profile_configured_cwd(''',
    '''    stored_cwd = str(found.get("cwd") or "").strip()\n    stored_project_id = str(found.get("project_id") or "").strip() or None\n    if stored_cwd and not stored_project_id:\n        migration_session = {\n            "cwd": stored_cwd,\n            "explicit_cwd": True,\n            "profile_home": str(profile_home) if profile_home is not None else None,\n            "session_key": target,\n            "source": found.get("source") or "desktop",\n        }\n        stored_project_id, _ = _session_project_identity(migration_session, stored_cwd)\n        if stored_project_id:\n            found["project_id"] = stored_project_id\n    profile_resume_cwd = stored_cwd or _profile_configured_cwd(''',
)
text = read("tui_gateway/methods_session.py")
needle = "        if (live := _claim_or_reuse_live(sid, target, record, lease)) is not None:"
count = text.count(needle)
if count < 2:
    raise RuntimeError(f"resume live-claim sites: {count}")
text = text.replace(needle, '        record["project_id"] = stored_project_id\n' + needle)
write("tui_gateway/methods_session.py", text)

# Project-tree regressions.
path = "tests/tui_gateway/test_project_tree.py"
text = read(path)
text += r'''\n\n\ndef test_stable_project_id_beats_cwd_heuristics():\n    projects = [\n        {"id": "p_one", "name": "One", "folders": [{"path": "/one"}]},\n        {"id": "p_two", "name": "Two", "folders": [{"path": "/two"}]},\n    ]\n    session = {\n        "id": "chat", "project_id": "p_two", "cwd": "/one/src",\n        "git_repo_root": "", "git_branch": "", "started_at": 1, "last_active": 2,\n    }\n    tree = pt.build_tree(projects, [session], [], resolve=lambda _cwd: None, hydrate=True)\n    one_project = next(node for node in tree["projects"] if node["id"] == "p_one")\n    two_project = next(node for node in tree["projects"] if node["id"] == "p_two")\n    assert one_project["sessionCount"] == 0\n    assert two_project["sessionCount"] == 1\n\n\ndef test_stale_stable_project_id_does_not_path_fallback():\n    projects = [{"id": "p_one", "name": "One", "folders": [{"path": "/one"}]}]\n    session = {\n        "id": "chat", "project_id": "deleted-project", "cwd": "/one/src",\n        "git_repo_root": "", "git_branch": "", "started_at": 1, "last_active": 2,\n    }\n    tree = pt.build_tree(projects, [session], [], resolve=lambda _cwd: None, hydrate=True)\n    one_project = next(node for node in tree["projects"] if node["id"] == "p_one")\n    assert one_project["sessionCount"] == 0\n    assert "chat" not in tree["scoped_session_ids"]\n'''
write(path, text)
