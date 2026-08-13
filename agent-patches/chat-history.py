# Retry after a verification-only failure; source transforms are unchanged.
from pathlib import Path

PROTOCOL = Path("crates/hermes-protocol/src/lib.rs")
CORE = Path("crates/hermes-core/src/lib.rs")
DESKTOP = Path("crates/hermes-desktop/src/lib.rs")


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one match, found {count}")
    return text.replace(old, new, 1)


# Protocol: typed REST transcript response.
protocol = PROTOCOL.read_text(encoding="utf-8")
if "pub struct SessionMessagesResponse" in protocol:
    raise SystemExit("SessionMessagesResponse already exists")
protocol_marker = "#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]\npub struct ProjectFolder {"
protocol_insert = """#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct SessionMessagesResponse {
    #[serde(default)]
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub session_id: String,
}

""" + protocol_marker
protocol = replace_once(protocol, protocol_marker, protocol_insert, "protocol response insertion")
PROTOCOL.write_text(protocol, encoding="utf-8")

# Core: lazy full-history service plus transcript reconciliation metadata.
core = CORE.read_text(encoding="utf-8")
resume_decl = "    fn resume(&self, session_id: &str) -> ServiceFuture<'_, SessionResumeResponse>;\n"
history_decl = resume_decl + """    fn history(&self, _session_id: &str) -> ServiceFuture<'_, Vec<ChatMessage>> {
        Box::pin(async move {
            Err(ServiceError::Unavailable(
                "full session history is unavailable on this host".into(),
            ))
        })
    }
"""
core = replace_once(core, resume_decl, history_decl, "SessionService history method")

core = replace_once(
    core,
    "    pub messages: Vec<ChatMessage>,\n    pub busy: bool,\n",
    "    pub messages: Vec<ChatMessage>,\n    pub message_count: usize,\n    pub messages_omitted: bool,\n    pub busy: bool,\n",
    "SessionTranscript metadata fields",
)

load_anchor = "        let inflight = response.inflight.as_ref();\n        let mut messages = response.messages;\n"
load_replacement = """        let message_count = response.message_count.max(response.messages.len());
        let messages_omitted = response.messages_omitted || message_count > response.messages.len();
        let inflight = response.inflight.as_ref();
        let mut messages = response.messages;
"""
core = replace_once(core, load_anchor, load_replacement, "SessionTranscript load metadata")

self_anchor = "            runtime_id: response.session_id,\n            messages,\n            busy,\n"
self_replacement = """            runtime_id: response.session_id,
            messages,
            message_count,
            messages_omitted,
            busy,
"""
core = replace_once(core, self_anchor, self_replacement, "SessionTranscript load fields")

push_anchor = "    pub fn push_user(&mut self, id: String, text: String) {\n"
merge_method = """    pub fn merge_history(&mut self, mut history: Vec<ChatMessage>) {
        for (index, message) in history.iter_mut().enumerate() {
            if message.id.is_empty() {
                message.id = format!("history-{index}");
            }
            if message.text.is_empty() {
                message.text.clone_from(&message.content_text);
            }
            if let Some(reasoning) = message.reasoning.take().filter(|value| !value.is_empty()) {
                message
                    .metadata
                    .insert("reasoning".into(), Value::String(reasoning));
            }
            if let Some(tool_call_id) = &message.tool_call_id {
                message
                    .metadata
                    .insert("tool_id".into(), Value::String(tool_call_id.clone()));
            }
        }

        for message in std::mem::take(&mut self.messages) {
            let duplicate = history.iter().any(|candidate| {
                candidate.role == message.role
                    && candidate.text == message.text
                    && candidate.tool_call_id == message.tool_call_id
                    && candidate.tool_name == message.tool_name
            });
            if message.streaming || !duplicate {
                history.push(message);
            }
        }
        self.message_count = self.message_count.max(history.len());
        self.messages_omitted = false;
        self.messages = history;
    }

""" + push_anchor
core = replace_once(core, push_anchor, merge_method, "SessionTranscript merge_history")
CORE.write_text(core, encoding="utf-8")

# Desktop: official /api/sessions/<id>/messages contract.
desktop = DESKTOP.read_text(encoding="utf-8")
desktop = replace_once(
    desktop,
    "    RuntimeStatus, SessionCreateRequest, SessionCreateResponse, SessionResumeResponse,\n    SessionSummary, SkillActionStart, SkillActionStatus, SkillHubPreview, SkillHubScanResult,\n",
    "    RuntimeStatus, SessionCreateRequest, SessionCreateResponse, SessionMessagesResponse,\n    SessionResumeResponse, SessionSummary, SkillActionStart, SkillActionStatus, SkillHubPreview,\n    SkillHubScanResult,\n",
    "desktop protocol import",
)

resume_impl = """    fn resume(&self, session_id: &str) -> ServiceFuture<'_, SessionResumeResponse> {
        let session_id = session_id.to_owned();
        Box::pin(async move {
            validate_identifier(&session_id, "session")?;
            self.client()?
                .request("session.resume", json!({ "session_id": session_id }))
                .await
                .map_err(transport)
        })
    }

"""
history_impl = resume_impl + """    fn history(&self, session_id: &str) -> ServiceFuture<'_, Vec<hermes_protocol::ChatMessage>> {
        let session_id = session_id.to_owned();
        Box::pin(async move {
            validate_identifier(&session_id, "session")?;
            let value = self
                .rest()?
                .request(
                    Method::GET,
                    &format!("/api/sessions/{session_id}/messages"),
                    None,
                )
                .await?;
            let response: SessionMessagesResponse = serde_json::from_value(value).map_err(protocol)?;
            Ok(response.messages)
        })
    }

"""
desktop = replace_once(desktop, resume_impl, history_impl, "desktop history implementation")
DESKTOP.write_text(desktop, encoding="utf-8")
