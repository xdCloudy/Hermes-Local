from pathlib import Path

PROTOCOL = Path("crates/hermes-protocol/src/lib.rs")
CORE = Path("crates/hermes-core/src/lib.rs")
DESKTOP = Path("crates/hermes-desktop/src/lib.rs")


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one match, found {count}")
    return text.replace(old, new, 1)


protocol = PROTOCOL.read_text(encoding="utf-8")
protocol_marker = "#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]\npub struct ProjectFolder {"
directive = r'''#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct SessionDirectiveResult {
    #[serde(default, rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub notice: Option<String>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub display: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

''' + protocol_marker
protocol = replace_once(protocol, protocol_marker, directive, "directive protocol")
PROTOCOL.write_text(protocol, encoding="utf-8")

core = CORE.read_text(encoding="utf-8")
core = replace_once(
    core,
    "    ProjectsSnapshot, ProviderActivation, RuntimeStatus, SessionCreateRequest,\n    SessionResumeResponse, SessionSummary, SkillActionStart, SkillActionStatus, SkillHubPreview,\n",
    "    ProjectsSnapshot, ProviderActivation, RuntimeStatus, SessionCreateRequest,\n    SessionDirectiveResult, SessionResumeResponse, SessionSummary, SkillActionStart,\n    SkillActionStatus, SkillHubPreview,\n",
    "core directive import",
)

history_decl = '''    fn history(&self, _session_id: &str) -> ServiceFuture<'_, Vec<ChatMessage>> {
        Box::pin(async move {
            Err(ServiceError::Unavailable(
                "full session history is unavailable on this host".into(),
            ))
        })
    }
'''
directive_decl = history_decl + '''    fn execute_directive(
        &self,
        _session_id: &str,
        _command: &str,
    ) -> ServiceFuture<'_, SessionDirectiveResult> {
        Box::pin(async move {
            Err(ServiceError::Unavailable(
                "session directives are unavailable on this host".into(),
            ))
        })
    }
'''
core = replace_once(core, history_decl, directive_decl, "SessionService directive")

push_user = '''    pub fn push_user(&mut self, id: String, text: String) {
'''
push_system = '''    pub fn push_system(&mut self, text: String) {
        if text.trim().is_empty() {
            return;
        }
        self.messages.push(ChatMessage {
            id: format!("system-local-{}", self.messages.len()),
            role: MessageRole::System,
            text,
            ..ChatMessage::default()
        });
    }

''' + push_user
core = replace_once(core, push_user, push_system, "push_system")

queue_marker = "#[derive(Clone, Debug, Default, PartialEq)]\npub struct PromptQueueCoordinator {"
drafts = r'''const COMPOSER_UNDO_LIMIT: usize = 64;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ComposerDraftStore {
    drafts: BTreeMap<String, ComposerDraftState>,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct ComposerDraftState {
    text: String,
    undo: VecDeque<String>,
    redo: VecDeque<String>,
}

impl ComposerDraftStore {
    pub fn hydrate(value: &Value) -> Self {
        let mut store = Self::default();
        let Some(values) = value.as_object() else {
            return store;
        };
        for (key, value) in values {
            let Some(text) = value.as_str() else {
                continue;
            };
            if !key.is_empty() && !text.is_empty() {
                store.drafts.insert(
                    key.clone(),
                    ComposerDraftState {
                        text: text.to_owned(),
                        ..ComposerDraftState::default()
                    },
                );
            }
        }
        store
    }

    pub fn persisted_value(&self) -> Value {
        let mut values = serde_json::Map::new();
        for (key, draft) in &self.drafts {
            if !draft.text.is_empty() {
                values.insert(key.clone(), Value::String(draft.text.clone()));
            }
        }
        Value::Object(values)
    }

    pub fn text(&self, key: &str) -> String {
        self.drafts
            .get(key)
            .map(|draft| draft.text.clone())
            .unwrap_or_default()
    }

    pub fn edit(&mut self, key: &str, text: String) {
        if key.is_empty() {
            return;
        }
        let draft = self.drafts.entry(key.to_owned()).or_default();
        if draft.text == text {
            return;
        }
        draft.undo.push_back(draft.text.clone());
        while draft.undo.len() > COMPOSER_UNDO_LIMIT {
            draft.undo.pop_front();
        }
        draft.redo.clear();
        draft.text = text;
    }

    pub fn replace_without_history(&mut self, key: &str, text: String) {
        if key.is_empty() {
            return;
        }
        let draft = self.drafts.entry(key.to_owned()).or_default();
        draft.text = text;
        draft.undo.clear();
        draft.redo.clear();
    }

    pub fn undo(&mut self, key: &str) -> Option<String> {
        let draft = self.drafts.get_mut(key)?;
        let previous = draft.undo.pop_back()?;
        draft.redo.push_back(std::mem::replace(&mut draft.text, previous));
        Some(draft.text.clone())
    }

    pub fn redo(&mut self, key: &str) -> Option<String> {
        let draft = self.drafts.get_mut(key)?;
        let next = draft.redo.pop_back()?;
        draft.undo.push_back(std::mem::replace(&mut draft.text, next));
        Some(draft.text.clone())
    }

    pub fn clear(&mut self, key: &str) {
        self.drafts.remove(key);
    }
}

#[cfg(test)]
mod composer_draft_tests {
    use super::{COMPOSER_UNDO_LIMIT, ComposerDraftStore};
    use serde_json::json;

    #[test]
    fn drafts_round_trip_and_remain_session_isolated() {
        let mut drafts = ComposerDraftStore::default();
        drafts.edit("one", "alpha".into());
        drafts.edit("two", "beta".into());
        let persisted = drafts.persisted_value();
        let hydrated = ComposerDraftStore::hydrate(&persisted);
        assert_eq!(hydrated.text("one"), "alpha");
        assert_eq!(hydrated.text("two"), "beta");
    }

    #[test]
    fn undo_redo_tracks_programmatic_and_typed_edits() {
        let mut drafts = ComposerDraftStore::default();
        drafts.edit("s", "first".into());
        drafts.edit("s", "second".into());
        assert_eq!(drafts.undo("s").as_deref(), Some("first"));
        assert_eq!(drafts.undo("s").as_deref(), Some(""));
        assert_eq!(drafts.redo("s").as_deref(), Some("first"));
        assert_eq!(drafts.redo("s").as_deref(), Some("second"));
    }

    #[test]
    fn history_is_bounded_and_hydration_ignores_non_strings() {
        let mut drafts = ComposerDraftStore::hydrate(&json!({"ok":"saved","bad":42}));
        for index in 0..(COMPOSER_UNDO_LIMIT + 20) {
            drafts.edit("ok", format!("edit-{index}"));
        }
        for _ in 0..COMPOSER_UNDO_LIMIT {
            assert!(drafts.undo("ok").is_some());
        }
        assert!(drafts.undo("ok").is_none());
        assert_eq!(drafts.text("bad"), "");
    }

    #[test]
    fn clear_removes_persisted_draft() {
        let mut drafts = ComposerDraftStore::default();
        drafts.edit("s", "temporary".into());
        drafts.clear("s");
        assert_eq!(drafts.persisted_value(), json!({}));
    }
}

''' + queue_marker
core = replace_once(core, queue_marker, drafts, "draft store insertion")
CORE.write_text(core, encoding="utf-8")

desktop = DESKTOP.read_text(encoding="utf-8")
desktop = replace_once(
    desktop,
    "    RuntimeStatus, SessionCreateRequest, SessionCreateResponse, SessionMessagesResponse,\n    SessionResumeResponse, SessionSummary, SkillActionStart, SkillActionStatus, SkillHubPreview,\n",
    "    RuntimeStatus, SessionCreateRequest, SessionCreateResponse, SessionDirectiveResult,\n    SessionMessagesResponse, SessionResumeResponse, SessionSummary, SkillActionStart,\n    SkillActionStatus, SkillHubPreview,\n",
    "desktop directive import",
)

history_impl = '''    fn history(&self, session_id: &str) -> ServiceFuture<'_, Vec<hermes_protocol::ChatMessage>> {
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
            let response: SessionMessagesResponse =
                serde_json::from_value(value).map_err(protocol)?;
            Ok(response.messages)
        })
    }

'''
directive_impl = history_impl + r'''    fn execute_directive(
        &self,
        session_id: &str,
        command: &str,
    ) -> ServiceFuture<'_, SessionDirectiveResult> {
        let session_id = session_id.to_owned();
        let command = command.trim().trim_start_matches('/').trim().to_owned();
        Box::pin(async move {
            validate_identifier(&session_id, "session")?;
            if command.is_empty() || command.len() > 32_768 || command.chars().any(|char| char == '\0') {
                return Err(ServiceError::InvalidInput("invalid session directive".into()));
            }
            let mut parts = command.splitn(2, char::is_whitespace);
            let name = parts.next().unwrap_or_default().to_owned();
            let arg = parts.next().unwrap_or_default().trim().to_owned();
            let client = self.client()?;
            let value = match client
                .request::<_, Value>(
                    "slash.exec",
                    json!({ "session_id": session_id, "command": command }),
                )
                .await
            {
                Ok(value) => value,
                Err(_) => client
                    .request::<_, Value>(
                        "command.dispatch",
                        json!({ "session_id": session_id, "name": name, "arg": arg }),
                    )
                    .await
                    .map_err(transport)?,
            };
            let mut result: SessionDirectiveResult =
                serde_json::from_value(value.clone()).map_err(protocol)?;
            if result.output.is_none()
                && let Some(output) = value.as_str()
            {
                result.output = Some(output.to_owned());
            }
            if result.kind.is_empty() {
                result.kind = if result.message.is_some() {
                    "send".into()
                } else {
                    "exec".into()
                };
            }
            Ok(result)
        })
    }

'''
desktop = replace_once(desktop, history_impl, directive_impl, "desktop directive implementation")
DESKTOP.write_text(desktop, encoding="utf-8")
