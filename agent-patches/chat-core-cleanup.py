from pathlib import Path

LIB = Path("crates/hermes-ui/src/lib.rs")
CHAT = Path("crates/hermes-ui/src/chat.rs")


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one match, found {count}")
    return text.replace(old, new, 1)


lib = LIB.read_text(encoding="utf-8")
lib = replace_once(lib, "use futures_util::StreamExt;\n", "", "unused StreamExt import")
lib = replace_once(lib, "    SessionTranscript,\n", "", "unused SessionTranscript import")
lib = replace_once(lib, "    ConnectionProbeResult, CustomEndpoint, CustomEndpointUpdate, EnvVarInfo, MessageRole,\n", "    ConnectionProbeResult, CustomEndpoint, CustomEndpointUpdate, EnvVarInfo,\n", "unused MessageRole import")
lib = replace_once(lib, "    ProjectsSnapshot, RemoteAuthMode, SessionCreateRequest, SessionSummary, ThemeMode,\n", "    ProjectsSnapshot, RemoteAuthMode, SessionSummary, ThemeMode,\n", "unused SessionCreateRequest import")
LIB.write_text(lib, encoding="utf-8")

chat = CHAT.read_text(encoding="utf-8")
chat = replace_once(
    chat,
    "use super::{Codicon, ProjectPicker, ProjectUiState, Route};",
    "use super::{Codicon, ErrorState, LoadingState, ProjectPicker, ProjectUiState, Route};",
    "chat shared-state imports",
)
CHAT.write_text(chat, encoding="utf-8")
