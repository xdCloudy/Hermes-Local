from pathlib import Path
import subprocess
import sys

repo = Path(__file__).resolve().parents[2]
root = sys.argv[1]
out = sys.argv[2]

backend_source = (repo / "scripts/ci/issue52_backend_gen.py").read_text(encoding="utf-8")

cwd_call = '''one(
    "tui_gateway/server.py",
    '    session["cwd_from_settle"] = False\\n    _register_session_cwd(session)',
    '    session["cwd_from_settle"] = False\\n    session.pop("project_id", None)\\n    _register_session_cwd(session)',
)
'''
cwd_replacement = '''text = read("tui_gateway/server.py")
old = '    session["cwd_from_settle"] = False\\n    _register_session_cwd(session)'
new = '    session["cwd_from_settle"] = False\\n    session.pop("project_id", None)\\n    _register_session_cwd(session)'
count = text.count(old)
if count != 2:
    raise RuntimeError(f"tui_gateway/server.py: expected 2 cwd invalidation sites, got {count}")
write("tui_gateway/server.py", text.replace(old, new))
'''
if backend_source.count(cwd_call) != 1:
    raise RuntimeError("backend generator cwd invalidation block changed")
backend_source = backend_source.replace(cwd_call, cwd_replacement)

info_call = '''one(
    "tui_gateway/server.py",
    '        "cwd": cwd,\\n        "branch": _git_branch_for_cwd(cwd),\\n        "project": _project_info_for_cwd(cwd),',
    '        "cwd": cwd,\\n        "branch": _git_branch_for_cwd(cwd),\\n        "project_id": project_id,\\n        "project": project,',
)
'''
info_replacement = '''text = read("tui_gateway/server.py")
old = '        "cwd": cwd,\\n        "branch": _git_branch_for_cwd(cwd),\\n        "project": _project_info_for_cwd(cwd),'
new = '        "cwd": cwd,\\n        "branch": _git_branch_for_cwd(cwd),\\n        "project_id": project_id,\\n        "project": project,'
count = text.count(old)
if count != 2:
    raise RuntimeError(f"tui_gateway/server.py: expected 2 project info payloads, got {count}")
write("tui_gateway/server.py", text.replace(old, new, 1))
'''
if backend_source.count(info_call) != 1:
    raise RuntimeError("backend generator session info payload block changed")
backend_source = backend_source.replace(info_call, info_replacement)

backend_fixed = Path("/tmp/issue52_backend_gen_fixed.py")
backend_fixed.write_text(backend_source, encoding="utf-8", newline="")
subprocess.run([sys.executable, str(backend_fixed), root], check=True)

frontend_source = (repo / "scripts/ci/issue52_frontend_gen.py").read_text(encoding="utf-8")
frontend_source = frontend_source.replace(
    '    "export const setCurrentCwd = (cwd: string) => {",\n'
    '    "export const setCurrentProjectId = (projectId: null | string) => $currentProjectId.set(projectId)\\n\\n"\n'
    '    "export const setCurrentCwd = (cwd: string) => {",',
    '    "export const setCurrentCwd = (next: Updater<string>) => {",\n'
    '    "export const setCurrentProjectId = (projectId: null | string) => $currentProjectId.set(projectId)\\n\\n"\n'
    '    "export const setCurrentCwd = (next: Updater<string>) => {",',
)

# Patch 0016's composer block has evolved since it was authored. Replace the
# generator's brittle whole-block match with a structural rewrite around the
# unique CodingStatusRow instance.
block_start = frontend_source.find(
    'one(\n    "apps/desktop/src/app/chat/composer/index.tsx",\n    \'\'\'                <CodingStatusRow'
)
block_end_marker = '\n\none(\n    "apps/desktop/src/store/updates.ts"'
block_end = frontend_source.find(block_end_marker, block_start)
if block_start < 0 or block_end < 0:
    raise RuntimeError("frontend generator CodingStatusRow rewrite block changed")
structural_rewrite = r'''text = read("apps/desktop/src/app/chat/composer/index.tsx")
anchor = "                <CodingStatusRow\n"
start = text.find(anchor)
if start < 0:
    raise RuntimeError("composer CodingStatusRow not found")
end = text.find("                />", start)
if end < 0:
    raise RuntimeError("composer CodingStatusRow closing tag not found")
end += len("                />")
block = text[start:end]
block = "\n".join(
    line
    for line in block.splitlines()
    if "onChangeProject=" not in line and "onRemoveProject=" not in line
)
replacement = (
    "                <ProjectStatusRow onSelectProject={changeProject} selectedProjectId={selectedProjectId} />\n"
    "                {selectedProjectId && (\n"
    + block
    + "\n                )}"
)
write("apps/desktop/src/app/chat/composer/index.tsx", text[:start] + replacement + text[end:])
'''
frontend_source = frontend_source[:block_start] + structural_rewrite + frontend_source[block_end:]

frontend_fixed = Path("/tmp/issue52_frontend_gen_fixed.py")
frontend_fixed.write_text(frontend_source, encoding="utf-8", newline="")
subprocess.run([sys.executable, str(frontend_fixed), root, out], check=True)
