from pathlib import Path

path = Path("crates/hermes-ui/src/review.rs")
text = path.read_text(encoding="utf-8")
old = "                                for change in rows {"
new = "                                for change in rows.clone() {"
count = text.count(old)
if count != 1:
    raise SystemExit(f"expected one Review rows loop, got {count}")
path.write_text(text.replace(old, new), encoding="utf-8")
