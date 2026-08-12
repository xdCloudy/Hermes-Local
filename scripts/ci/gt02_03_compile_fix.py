from pathlib import Path

path = Path("crates/hermes-ui/src/source_control.rs")
text = path.read_text(encoding="utf-8")

if text.count('summary: "') != 2:
    raise SystemExit(f"expected two Surface summary props, got {text.count('summary: \"')}")
text = text.replace('summary: "', 'subtitle: "')

old = '''                            let target = tree.clone();
                            rsx! {'''
new = '''                            let target = tree.clone();
                            let branch_label = tree
                                .branch
                                .clone()
                                .unwrap_or_else(|| "detached".to_owned());
                            rsx! {'''
if text.count(old) != 1:
    raise SystemExit(f"expected one worktree row marker, got {text.count(old)}")
text = text.replace(old, new)

old = '                                                strong { "{tree.branch.as_deref().unwrap_or(\"detached\")}" }'
new = '                                                strong { "{branch_label}" }'
if text.count(old) != 1:
    raise SystemExit(f"expected one nested branch label expression, got {text.count(old)}")
text = text.replace(old, new)

path.write_text(text, encoding="utf-8")
