from pathlib import Path

LIB = Path("crates/hermes-ui/src/lib.rs")
CHAT = Path("crates/hermes-ui/src/chat.rs")

text = LIB.read_text(encoding="utf-8")


def extract_between(source: str, start: str, end: str) -> tuple[str, str]:
    start_index = source.find(start)
    if start_index < 0:
        raise SystemExit(f"missing start marker: {start!r}")
    if source.find(start, start_index + 1) >= 0:
        raise SystemExit(f"ambiguous start marker: {start!r}")
    end_index = source.find(end, start_index + len(start))
    if end_index < 0:
        raise SystemExit(f"missing end marker after {start!r}: {end!r}")
    block = source[start_index:end_index]
    return source[:start_index] + source[end_index:], block


text, chat_block = extract_between(
    text,
    "#[component]\nfn Chat() -> Element {",
    "#[component]\nfn Overview() -> Element {",
)
text, session_block = extract_between(
    text,
    "#[component]\nfn Session(id: String) -> Element {",
    "#[component]\nfn Project(id: String) -> Element {",
)

if "mod chat;\n" in text or "use chat::{Chat, Session};\n" in text:
    raise SystemExit("chat module already wired")

insert_after = "//! Dioxus presentation layer. This crate has no filesystem, process, or OS authority.\n\n"
if insert_after not in text:
    raise SystemExit("missing lib module-doc marker")
text = text.replace(
    insert_after,
    insert_after + "mod chat;\n\nuse chat::{Chat, Session};\n",
    1,
)

chat_block = chat_block.replace("fn Chat() -> Element {", "pub(super) fn Chat() -> Element {", 1)
session_block = session_block.replace(
    "fn Session(id: String) -> Element {",
    "pub(super) fn Session(id: String) -> Element {",
    1,
)

module = """//! Chat/session presentation surfaces extracted from the shell so the A4 chat\n//! migration can evolve behind the existing typed service boundary.\n\nuse dioxus::prelude::*;\nuse futures_util::StreamExt;\nuse hermes_core::{AppServices, SessionTranscript};\nuse hermes_protocol::{MessageRole, SessionCreateRequest};\n\nuse super::{Codicon, ProjectPicker, ProjectUiState, Route};\n\n"""
module += chat_block.rstrip() + "\n\n" + session_block.rstrip() + "\n"

LIB.write_text(text, encoding="utf-8")
CHAT.write_text(module, encoding="utf-8")
