from pathlib import Path
import subprocess
import sys

repo = Path(__file__).resolve().parents[2]
root = sys.argv[1]
out = sys.argv[2]
source_path = repo / "scripts/ci/issue52_backend_gen.py"
source = source_path.read_text(encoding="utf-8")
old_call = '''one(
    "tui_gateway/server.py",
    '    session["cwd_from_settle"] = False\\n    _register_session_cwd(session)',
    '    session["cwd_from_settle"] = False\\n    session.pop("project_id", None)\\n    _register_session_cwd(session)',
)
'''
new_call = '''text = read("tui_gateway/server.py")
old = '    session["cwd_from_settle"] = False\\n    _register_session_cwd(session)'
new = '    session["cwd_from_settle"] = False\\n    session.pop("project_id", None)\\n    _register_session_cwd(session)'
count = text.count(old)
if count != 2:
    raise RuntimeError(f"tui_gateway/server.py: expected 2 cwd invalidation sites, got {count}")
write("tui_gateway/server.py", text.replace(old, new))
'''
if source.count(old_call) != 1:
    raise RuntimeError("backend generator cwd invalidation block changed")
fixed = Path("/tmp/issue52_backend_gen_fixed.py")
fixed.write_text(source.replace(old_call, new_call), encoding="utf-8", newline="")
subprocess.run([sys.executable, str(fixed), root], check=True)
subprocess.run([sys.executable, str(repo / "scripts/ci/issue52_frontend_gen.py"), root, out], check=True)
