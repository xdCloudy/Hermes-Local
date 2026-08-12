from pathlib import Path
import re

path = Path("crates/hermes-ui/src/source_control.rs")
text = path.read_text(encoding="utf-8")

if text.count('summary: "') != 2:
    raise SystemExit(f"expected two Surface summary props, got {text.count('summary: \"')}")
text = text.replace('summary: "', 'subtitle: "')

pattern = re.compile(r'(?m)^(?P<indent>[ \t]*)let target = tree\.clone\(\);\n(?P=indent)rsx! \{')
replacement = '''\g<indent>let target = tree.clone();
\g<indent>let branch_label = tree
\g<indent>    .branch
\g<indent>    .clone()
\g<indent>    .unwrap_or_else(|| "detached".to_owned());
\g<indent>rsx! {'''
text, count = pattern.subn(replacement, text, count=1)
if count != 1:
    raise SystemExit(f"expected one worktree row marker, got {count}")

old = 'strong { "{tree.branch.as_deref().unwrap_or(\"detached\")}" }'
new = 'strong { "{branch_label}" }'
if text.count(old) != 1:
    raise SystemExit(f"expected one nested branch label expression, got {text.count(old)}")
text = text.replace(old, new)

old = "                        for tree in rows {"
new = "                        for tree in rows.clone() {"
if text.count(old) != 1:
    raise SystemExit(f"expected one worktree rows loop, got {text.count(old)}")
text = text.replace(old, new)

path.write_text(text, encoding="utf-8")
