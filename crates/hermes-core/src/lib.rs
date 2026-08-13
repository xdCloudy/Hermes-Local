//! Hermes Local product boundary consumed by the Dioxus UI.

use std::{
    collections::{BTreeMap, VecDeque},
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use futures_core::Stream;
use hermes_protocol::{
    AgentConfigSnapshot, AppSettings, AttachmentKind, ChatMessage, ConnectionConfig,
    ConnectionConfigInput, ConnectionOauthLoginResult, ConnectionOauthLogoutResult,
    ConnectionProbeResult, ConnectionState, ConnectionTestResult, CustomEndpointUpdate,
    CustomEndpointValidation, CustomEndpointsResponse, EnvVarInfo, FileEntry, GatewayEvent,
    GitStatus, MessageRole, MoaConfig, ModelAssignmentRequest, ModelAssignmentResponse,
    ModelSettingsSnapshot, OAuthPoll, OAuthProvider, OAuthStart, OAuthSubmit,
    ProjectFilesDeleteResult, ProjectSummary, ProjectsSnapshot, ProviderActivation, RuntimeStatus,
    SelectedAttachment, SessionAttachmentResult, SessionCreateRequest, SessionDirectiveResult,
    SessionResumeResponse, SessionSummary, SkillActionStart, SkillActionStatus, SkillHubPreview,
    SkillHubScanResult, SkillHubSearchResponse, SkillHubSourcesResponse, SkillSummary,
    SkillToggleResult, TaskSummary, TrustSnapshot,
};
use serde_json::Value;
use thiserror::Error;

pub type ServiceFuture<'a, T> = Pin<Box<dyn Future<Output = ServiceResult<T>> + Send + 'a>>;
pub type EventStream = Pin<Box<dyn Stream<Item = GatewayEvent> + Send>>;
pub type FileWatchStream = Pin<Box<dyn Stream<Item = FileWatchEvent> + Send>>;
pub type ServiceResult<T> = Result<T, ServiceError>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileWatchEvent {
    pub path: PathBuf,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ServiceError {
    #[error("{0}")]
    InvalidInput(String),
    #[error("operation is not available: {0}")]
    Unavailable(String),
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("transport error: {0}")]
    Transport(String),
    #[error("operation timed out: {0}")]
    Timeout(String),
    #[error("platform error: {0}")]
    Platform(String),
}

pub trait SessionService: Send + Sync {
    fn list(&self) -> ServiceFuture<'_, Vec<SessionSummary>>;
    fn create(&self, request: SessionCreateRequest) -> ServiceFuture<'_, SessionSummary>;
    fn resume(&self, session_id: &str) -> ServiceFuture<'_, SessionResumeResponse>;
    fn history(&self, _session_id: &str) -> ServiceFuture<'_, Vec<ChatMessage>> {
        Box::pin(async move {
            Err(ServiceError::Unavailable(
                "full session history is unavailable on this host".into(),
            ))
        })
    }
    fn execute_directive(
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
    fn attach(
        &self,
        _session_id: &str,
        _attachment: &SelectedAttachment,
    ) -> ServiceFuture<'_, SessionAttachmentResult> {
        Box::pin(async move {
            Err(ServiceError::Unavailable(
                "session attachments are unavailable on this host".into(),
            ))
        })
    }
    fn detach_image(&self, _session_id: &str, _path: &str) -> ServiceFuture<'_, ()> {
        Box::pin(async move {
            Err(ServiceError::Unavailable(
                "image detach is unavailable on this host".into(),
            ))
        })
    }
    fn submit(&self, session_id: &str, text: &str) -> ServiceFuture<'_, ()>;
    fn interrupt(&self, session_id: &str) -> ServiceFuture<'_, ()>;
    fn set_pinned(&self, session_id: &str, pinned: bool) -> ServiceFuture<'_, ()>;
    fn set_archived(&self, session_id: &str, archived: bool) -> ServiceFuture<'_, ()>;
    fn rename(
        &self,
        session_id: &str,
        runtime_id: Option<&str>,
        title: &str,
    ) -> ServiceFuture<'_, ()>;
    fn delete(&self, session_id: &str) -> ServiceFuture<'_, ()>;
    fn events(&self) -> ServiceResult<EventStream>;
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SessionTranscript {
    pub stored_id: String,
    pub runtime_id: String,
    pub messages: Vec<ChatMessage>,
    pub message_count: usize,
    pub messages_omitted: bool,
    pub busy: bool,
    pub needs_input: bool,
    pub error: Option<String>,
}

impl SessionTranscript {
    pub fn load(stored_id: String, response: SessionResumeResponse) -> Self {
        let busy = response.running
            || response
                .info
                .as_ref()
                .and_then(|info| info.get("running"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
        let message_count = response.message_count.max(response.messages.len());
        let messages_omitted = response.messages_omitted || message_count > response.messages.len();
        let inflight = response.inflight.as_ref();
        let mut messages = response.messages;
        for (index, message) in messages.iter_mut().enumerate() {
            if message.id.is_empty() {
                message.id = format!("stored-{index}");
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
        if let Some(user) = inflight
            .and_then(|value| value.get("user"))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            && !messages
                .last()
                .is_some_and(|message| message.role == MessageRole::User && message.text == user)
        {
            messages.push(ChatMessage {
                id: "inflight-user".into(),
                role: MessageRole::User,
                text: user.into(),
                ..ChatMessage::default()
            });
        }
        let assistant = inflight
            .and_then(|value| value.get("assistant"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        if busy || !assistant.is_empty() {
            messages.push(ChatMessage {
                id: "inflight-assistant".into(),
                role: MessageRole::Assistant,
                text: assistant.into(),
                streaming: busy,
                ..ChatMessage::default()
            });
        }
        let error = inflight
            .and_then(|value| value.get("error"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        Self {
            stored_id,
            runtime_id: response.session_id,
            messages,
            message_count,
            messages_omitted,
            busy,
            needs_input: false,
            error,
        }
    }

    pub fn merge_history(&mut self, mut history: Vec<ChatMessage>) {
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

    pub fn push_system(&mut self, text: String) {
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

    pub fn push_user(&mut self, id: String, text: String) {
        self.messages.push(ChatMessage {
            id,
            role: MessageRole::User,
            text,
            ..ChatMessage::default()
        });
        self.busy = true;
        self.error = None;
    }

    pub fn apply_event(&mut self, event: &GatewayEvent) -> bool {
        if event.session_id.as_deref() != Some(self.runtime_id.as_str()) {
            return false;
        }
        let text = event
            .payload
            .get("text")
            .or_else(|| event.payload.get("rendered"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        match event.kind.as_str() {
            "message.start" => {
                self.busy = true;
                self.error = None;
            }
            "message.delta" => {
                let message = self.streaming_assistant();
                message.text.push_str(text);
                self.busy = true;
            }
            "reasoning.delta" | "reasoning.available" => {
                let message = self.streaming_assistant();
                let previous = message
                    .metadata
                    .get("reasoning")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let reasoning = if event.kind == "reasoning.available" {
                    text.to_owned()
                } else {
                    format!("{previous}{text}")
                };
                message
                    .metadata
                    .insert("reasoning".into(), Value::String(reasoning));
                self.busy = true;
            }
            "message.interim" => {
                let message = self.streaming_assistant();
                if !text.is_empty() {
                    text.clone_into(&mut message.text);
                }
                message.streaming = false;
                message.metadata.insert("interim".into(), Value::Bool(true));
            }
            "message.complete" => {
                let message = self.assistant_for_completion();
                if !text.is_empty() {
                    text.clone_into(&mut message.text);
                }
                message.streaming = false;
                message.metadata.remove("interim");
                if event.payload.get("status").and_then(Value::as_str) == Some("error") {
                    self.error = event
                        .payload
                        .get("error")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                        .or_else(|| (!text.is_empty()).then(|| text.to_owned()));
                }
                self.busy = false;
                self.needs_input = false;
            }
            "tool.start" | "tool.progress" | "tool.complete" => {
                self.upsert_tool(event, text);
            }
            "clarify.request" | "approval.request" | "sudo.request" => {
                self.needs_input = true;
            }
            "session.info" => {
                if event.payload.get("running").and_then(Value::as_bool) == Some(false) {
                    self.busy = false;
                    for message in &mut self.messages {
                        message.streaming = false;
                    }
                }
            }
            _ => return false,
        }
        true
    }

    fn streaming_assistant(&mut self) -> &mut ChatMessage {
        let reuse = self
            .messages
            .last()
            .is_some_and(|message| message.role == MessageRole::Assistant && message.streaming);
        if !reuse {
            self.messages.push(ChatMessage {
                id: format!("assistant-stream-{}", self.messages.len()),
                role: MessageRole::Assistant,
                streaming: true,
                ..ChatMessage::default()
            });
        }
        self.messages.last_mut().expect("assistant message exists")
    }

    fn assistant_for_completion(&mut self) -> &mut ChatMessage {
        let index = self.messages.iter().rposition(|message| {
            message.role == MessageRole::Assistant
                && (message.streaming
                    || message.metadata.get("interim").and_then(Value::as_bool) == Some(true))
        });
        if let Some(index) = index {
            return &mut self.messages[index];
        }
        self.messages.push(ChatMessage {
            id: format!("assistant-complete-{}", self.messages.len()),
            role: MessageRole::Assistant,
            ..ChatMessage::default()
        });
        self.messages.last_mut().expect("assistant message exists")
    }

    fn upsert_tool(&mut self, event: &GatewayEvent, text: &str) {
        let tool_id = event
            .payload
            .get("tool_id")
            .or_else(|| event.payload.get("tool_call_id"))
            .or_else(|| event.payload.get("id"))
            .and_then(Value::as_str)
            .unwrap_or("tool");
        let tool_name = event
            .payload
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("tool");
        let complete = event.kind == "tool.complete";
        let preview = event
            .payload
            .get("preview")
            .or_else(|| event.payload.get("result"))
            .and_then(Value::as_str)
            .unwrap_or(text);
        if let Some(message) = self.messages.iter_mut().find(|message| {
            message.role == MessageRole::Tool
                && message.metadata.get("tool_id").and_then(Value::as_str) == Some(tool_id)
        }) {
            if !preview.is_empty() {
                preview.clone_into(&mut message.text);
            }
            message.streaming = !complete;
            return;
        }
        let mut metadata = std::collections::BTreeMap::new();
        metadata.insert("tool_id".into(), Value::String(tool_id.to_owned()));
        self.messages.push(ChatMessage {
            id: format!("tool-{tool_id}"),
            role: MessageRole::Tool,
            text: preview.to_owned(),
            streaming: !complete,
            tool_name: Some(tool_name.to_owned()),
            metadata,
            ..ChatMessage::default()
        });
    }
}

pub fn attachment_context_text(
    visible_text: &str,
    attachments: &[SessionAttachmentResult],
) -> String {
    let mut parts = attachments
        .iter()
        .filter_map(|attachment| attachment.ref_text.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let visible_text = visible_text.trim();
    if !visible_text.is_empty() {
        parts.push(visible_text.to_owned());
    }
    if parts.is_empty()
        && attachments
            .iter()
            .any(|attachment| attachment.kind == AttachmentKind::Image && attachment.attached)
    {
        return "What do you see in this image?".into();
    }
    parts.join("\n\n")
}

const COMPOSER_UNDO_LIMIT: usize = 64;

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
        draft
            .redo
            .push_back(std::mem::replace(&mut draft.text, previous));
        Some(draft.text.clone())
    }

    pub fn redo(&mut self, key: &str) -> Option<String> {
        let draft = self.drafts.get_mut(key)?;
        let next = draft.redo.pop_back()?;
        draft
            .undo
            .push_back(std::mem::replace(&mut draft.text, next));
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

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PromptQueueCoordinator {
    sessions: BTreeMap<String, PromptQueueSession>,
    runtime_to_stored: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct QueuedPrompt {
    pub text: String,
    pub attachments: Vec<SelectedAttachment>,
}

impl QueuedPrompt {
    pub fn label(&self) -> String {
        if !self.text.trim().is_empty() {
            return self.text.clone();
        }
        self.attachments
            .iter()
            .map(|attachment| attachment.label.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct PromptQueueSession {
    runtime_id: String,
    queued: VecDeque<QueuedPrompt>,
    parked: bool,
    busy: bool,
    error: Option<String>,
}

impl PromptQueueCoordinator {
    pub fn bind(&mut self, stored_id: &str, runtime_id: &str, busy: bool) {
        if stored_id.is_empty() || runtime_id.is_empty() {
            return;
        }
        let session = self.sessions.entry(stored_id.to_owned()).or_default();
        if !session.runtime_id.is_empty() && session.runtime_id != runtime_id {
            self.runtime_to_stored.remove(&session.runtime_id);
        }
        runtime_id.clone_into(&mut session.runtime_id);
        session.busy = busy;
        self.runtime_to_stored
            .insert(runtime_id.to_owned(), stored_id.to_owned());
    }

    pub fn enqueue(&mut self, stored_id: &str, text: String) -> usize {
        self.enqueue_prompt(
            stored_id,
            QueuedPrompt {
                text,
                attachments: Vec::new(),
            },
        )
    }

    pub fn enqueue_prompt(&mut self, stored_id: &str, mut prompt: QueuedPrompt) -> usize {
        prompt.text = prompt.text.trim().to_owned();
        if stored_id.is_empty() || (prompt.text.is_empty() && prompt.attachments.is_empty()) {
            return self.count(stored_id);
        }
        let session = self.sessions.entry(stored_id.to_owned()).or_default();
        session.queued.push_back(prompt);
        session.error = None;
        session.queued.len()
    }

    pub fn items(&self, stored_id: &str) -> Vec<String> {
        self.sessions
            .get(stored_id)
            .map(|session| session.queued.iter().map(QueuedPrompt::label).collect())
            .unwrap_or_default()
    }

    pub fn prompts(&self, stored_id: &str) -> Vec<QueuedPrompt> {
        self.sessions
            .get(stored_id)
            .map(|session| session.queued.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn count(&self, stored_id: &str) -> usize {
        self.sessions
            .get(stored_id)
            .map_or(0, |session| session.queued.len())
    }

    pub fn remove(&mut self, stored_id: &str, index: usize) -> Option<String> {
        self.remove_prompt(stored_id, index)
            .map(|prompt| prompt.text)
    }

    pub fn remove_prompt(&mut self, stored_id: &str, index: usize) -> Option<QueuedPrompt> {
        self.sessions
            .get_mut(stored_id)
            .and_then(|session| session.queued.remove(index))
    }

    pub fn clear(&mut self, stored_id: &str) -> usize {
        let Some(session) = self.sessions.get_mut(stored_id) else {
            return 0;
        };
        let removed = session.queued.len();
        session.queued.clear();
        session.error = None;
        removed
    }

    pub fn park(&mut self, stored_id: &str) {
        if let Some(session) = self.sessions.get_mut(stored_id) {
            session.parked = true;
        }
    }

    pub fn resume(&mut self, stored_id: &str) {
        if let Some(session) = self.sessions.get_mut(stored_id) {
            session.parked = false;
            session.error = None;
        }
    }

    pub fn is_parked(&self, stored_id: &str) -> bool {
        self.sessions
            .get(stored_id)
            .is_some_and(|session| session.parked)
    }

    pub fn error(&self, stored_id: &str) -> Option<String> {
        self.sessions
            .get(stored_id)
            .and_then(|session| session.error.clone())
    }

    pub fn mark_busy(&mut self, stored_id: &str, busy: bool) {
        if let Some(session) = self.sessions.get_mut(stored_id) {
            session.busy = busy;
            if busy {
                session.error = None;
            }
        }
    }

    pub fn mark_runtime_busy(&mut self, runtime_id: &str) {
        let Some(stored_id) = self.runtime_to_stored.get(runtime_id).cloned() else {
            return;
        };
        self.mark_busy(&stored_id, true);
    }

    pub fn next_after_completion(&mut self, runtime_id: &str) -> Option<(String, String, String)> {
        self.next_prompt_after_completion(runtime_id)
            .map(|(stored_id, runtime_id, prompt)| (stored_id, runtime_id, prompt.text))
    }

    pub fn next_prompt_after_completion(
        &mut self,
        runtime_id: &str,
    ) -> Option<(String, String, QueuedPrompt)> {
        let stored_id = self.runtime_to_stored.get(runtime_id)?.clone();
        let session = self.sessions.get_mut(&stored_id)?;
        session.busy = false;
        if session.parked {
            return None;
        }
        let prompt = session.queued.pop_front()?;
        session.busy = true;
        session.error = None;
        Some((stored_id, runtime_id.to_owned(), prompt))
    }

    pub fn next_if_idle(&mut self, stored_id: &str) -> Option<(String, String)> {
        self.next_prompt_if_idle(stored_id)
            .map(|(runtime_id, prompt)| (runtime_id, prompt.text))
    }

    pub fn next_prompt_if_idle(&mut self, stored_id: &str) -> Option<(String, QueuedPrompt)> {
        let session = self.sessions.get_mut(stored_id)?;
        if session.busy || session.parked || session.runtime_id.is_empty() {
            return None;
        }
        let prompt = session.queued.pop_front()?;
        session.busy = true;
        session.error = None;
        Some((session.runtime_id.clone(), prompt))
    }

    pub fn mark_submit_failed(&mut self, stored_id: &str, text: String, error: String) {
        self.mark_prompt_failed(
            stored_id,
            QueuedPrompt {
                text,
                attachments: Vec::new(),
            },
            error,
        );
    }

    pub fn mark_prompt_failed(&mut self, stored_id: &str, prompt: QueuedPrompt, error: String) {
        let session = self.sessions.entry(stored_id.to_owned()).or_default();
        session.busy = false;
        session.queued.push_front(prompt);
        session.error = Some(error);
    }
}

#[cfg(test)]
mod attachment_queue_tests {
    use super::{PromptQueueCoordinator, QueuedPrompt};
    use hermes_protocol::{AttachmentKind, SelectedAttachment};

    #[test]
    fn queued_attachment_payload_survives_handoff_and_failure() {
        let mut queue = PromptQueueCoordinator::default();
        queue.bind("stored", "runtime", true);
        let prompt = QueuedPrompt {
            text: "inspect".into(),
            attachments: vec![SelectedAttachment {
                id: "capability".into(),
                kind: AttachmentKind::Image,
                label: "shot.png".into(),
                ..SelectedAttachment::default()
            }],
        };
        queue.enqueue_prompt("stored", prompt.clone());
        let (_, _, dequeued) = queue
            .next_prompt_after_completion("runtime")
            .expect("queued attachment prompt");
        assert_eq!(dequeued, prompt);
        queue.mark_prompt_failed("stored", dequeued, "offline".into());
        assert_eq!(queue.prompts("stored"), vec![prompt]);
    }
}

#[cfg(test)]
mod prompt_queue_tests {
    use super::PromptQueueCoordinator;

    #[test]
    fn queues_are_fifo_and_isolated_across_background_sessions() {
        let mut queue = PromptQueueCoordinator::default();
        queue.bind("stored-a", "runtime-a", true);
        queue.bind("stored-b", "runtime-b", true);
        queue.enqueue("stored-a", "a1".into());
        queue.enqueue("stored-a", "a2".into());
        queue.enqueue("stored-b", "b1".into());

        assert_eq!(
            queue.next_after_completion("runtime-b"),
            Some(("stored-b".into(), "runtime-b".into(), "b1".into()))
        );
        assert_eq!(
            queue.next_after_completion("runtime-a"),
            Some(("stored-a".into(), "runtime-a".into(), "a1".into()))
        );
        assert_eq!(queue.count("stored-a"), 1);
        assert_eq!(queue.count("stored-b"), 0);
        assert_eq!(
            queue.next_after_completion("runtime-a"),
            Some(("stored-a".into(), "runtime-a".into(), "a2".into()))
        );
    }

    #[test]
    fn stop_parks_queue_until_explicit_resume() {
        let mut queue = PromptQueueCoordinator::default();
        queue.bind("stored", "runtime", true);
        queue.enqueue("stored", "later".into());
        queue.park("stored");

        assert_eq!(queue.next_after_completion("runtime"), None);
        assert!(queue.is_parked("stored"));
        assert_eq!(queue.count("stored"), 1);

        queue.resume("stored");
        assert_eq!(
            queue.next_if_idle("stored"),
            Some(("runtime".into(), "later".into()))
        );
    }

    #[test]
    fn queued_prompts_can_be_cancelled_without_touching_other_sessions() {
        let mut queue = PromptQueueCoordinator::default();
        queue.bind("a", "ra", true);
        queue.bind("b", "rb", true);
        queue.enqueue("a", "one".into());
        queue.enqueue("a", "two".into());
        queue.enqueue("b", "other".into());

        assert_eq!(queue.remove("a", 0), Some("one".into()));
        assert_eq!(queue.items("a"), vec!["two".to_owned()]);
        assert_eq!(queue.clear("a"), 1);
        assert!(queue.items("a").is_empty());
        assert_eq!(queue.items("b"), vec!["other".to_owned()]);
    }

    #[test]
    fn failed_background_submission_returns_prompt_to_front() {
        let mut queue = PromptQueueCoordinator::default();
        queue.bind("stored", "runtime", false);
        queue.enqueue("stored", "first".into());
        queue.enqueue("stored", "second".into());
        let (_, first) = queue.next_if_idle("stored").expect("first queued prompt");
        queue.mark_submit_failed("stored", first, "offline".into());

        assert_eq!(queue.items("stored"), vec!["first", "second"]);
        assert_eq!(queue.error("stored").as_deref(), Some("offline"));
    }
}

pub trait ConnectionService: Send + Sync {
    fn initialize(&self) -> ServiceFuture<'_, ConnectionState>;
    fn connect(&self, websocket_url: &str) -> ServiceFuture<'_, ConnectionState>;
    fn disconnect(&self) -> ServiceFuture<'_, ()>;
    fn state(&self) -> ServiceResult<ConnectionState>;
    fn config(&self, profile: Option<&str>) -> ServiceFuture<'_, ConnectionConfig>;
    fn save_config(&self, input: &ConnectionConfigInput) -> ServiceFuture<'_, ConnectionConfig>;
    fn apply_config(&self, input: &ConnectionConfigInput) -> ServiceFuture<'_, ConnectionConfig>;
    fn test_config(&self, input: &ConnectionConfigInput)
    -> ServiceFuture<'_, ConnectionTestResult>;
    fn probe_config(&self, remote_url: &str) -> ServiceFuture<'_, ConnectionProbeResult>;
    fn oauth_login(&self, remote_url: &str) -> ServiceFuture<'_, ConnectionOauthLoginResult>;
    fn oauth_logout(&self, remote_url: &str) -> ServiceFuture<'_, ConnectionOauthLogoutResult>;
    fn list_ssh_hosts(&self) -> ServiceFuture<'_, Vec<String>> {
        Box::pin(async move { Ok(Vec::new()) })
    }
}

pub trait ProjectService: Send + Sync {
    fn snapshot(&self) -> ServiceFuture<'_, ProjectsSnapshot>;
    fn create(&self, name: &str, folders: &[String]) -> ServiceFuture<'_, ProjectSummary>;
    fn clone_repository(
        &self,
        name: &str,
        repository_url: &str,
        parent_path: &str,
    ) -> ServiceFuture<'_, ProjectSummary>;
    fn set_active(&self, id: Option<&str>) -> ServiceFuture<'_, ()>;
    fn set_pinned(&self, id: &str, pinned: bool) -> ServiceFuture<'_, ProjectsSnapshot>;
    fn set_archived(&self, id: &str, archived: bool) -> ServiceFuture<'_, ProjectsSnapshot>;
    fn recover_path(
        &self,
        id: &str,
        old_path: &str,
        new_path: &str,
        repository_id: Option<&str>,
    ) -> ServiceFuture<'_, ProjectSummary>;
    fn remove(&self, id: &str) -> ServiceFuture<'_, ()>;
    fn delete_files(
        &self,
        id: &str,
        confirmation: &str,
    ) -> ServiceFuture<'_, ProjectFilesDeleteResult>;
}

pub trait SettingsService: Send + Sync {
    fn load(&self) -> ServiceFuture<'_, AppSettings>;
    fn save(&self, settings: &AppSettings) -> ServiceFuture<'_, ()>;
}

/// Profile-scoped Hermes Agent configuration. This intentionally exposes only
/// the official config endpoints instead of a generic REST or RPC escape hatch.
pub trait AgentConfigService: Send + Sync {
    fn load(&self, profile: Option<&str>) -> ServiceFuture<'_, AgentConfigSnapshot>;
    fn save(
        &self,
        profile: Option<&str>,
        config: &std::collections::BTreeMap<String, Value>,
    ) -> ServiceFuture<'_, ()>;
}

pub trait ModelService: Send + Sync {
    fn load(&self, profile: Option<&str>) -> ServiceFuture<'_, ModelSettingsSnapshot>;
    fn assign(
        &self,
        profile: Option<&str>,
        request: &ModelAssignmentRequest,
    ) -> ServiceFuture<'_, ModelAssignmentResponse>;
    fn save_moa(&self, profile: Option<&str>, config: &MoaConfig) -> ServiceFuture<'_, MoaConfig>;
}

pub trait ProviderService: Send + Sync {
    fn list_oauth(&self, profile: Option<&str>) -> ServiceFuture<'_, Vec<OAuthProvider>>;
    fn start_oauth(
        &self,
        profile: Option<&str>,
        provider_id: &str,
    ) -> ServiceFuture<'_, OAuthStart>;
    fn submit_oauth(
        &self,
        profile: Option<&str>,
        provider_id: &str,
        session_id: &str,
        code: &str,
    ) -> ServiceFuture<'_, OAuthSubmit>;
    fn poll_oauth(
        &self,
        profile: Option<&str>,
        provider_id: &str,
        session_id: &str,
    ) -> ServiceFuture<'_, OAuthPoll>;
    fn cancel_oauth(&self, profile: Option<&str>, session_id: &str) -> ServiceFuture<'_, ()>;
    fn disconnect_oauth(&self, profile: Option<&str>, provider_id: &str) -> ServiceFuture<'_, ()>;
    fn env(
        &self,
        profile: Option<&str>,
    ) -> ServiceFuture<'_, std::collections::BTreeMap<String, EnvVarInfo>>;
    fn set_env(&self, profile: Option<&str>, key: &str, value: &str) -> ServiceFuture<'_, ()>;
    fn delete_env(&self, profile: Option<&str>, key: &str) -> ServiceFuture<'_, ()>;
    fn reveal_env(&self, profile: Option<&str>, key: &str) -> ServiceFuture<'_, String>;
    fn custom_endpoints(&self) -> ServiceFuture<'_, CustomEndpointsResponse>;
    fn save_custom_endpoint(
        &self,
        endpoint: &CustomEndpointUpdate,
    ) -> ServiceFuture<'_, CustomEndpointsResponse>;
    fn validate_custom_endpoint(
        &self,
        endpoint: &CustomEndpointUpdate,
    ) -> ServiceFuture<'_, CustomEndpointValidation>;
    fn activate_custom_endpoint(&self, id: &str) -> ServiceFuture<'_, ProviderActivation>;
    fn delete_custom_endpoint(&self, id: &str) -> ServiceFuture<'_, CustomEndpointsResponse>;
}

pub trait RuntimeService: Send + Sync {
    fn status(&self) -> ServiceFuture<'_, RuntimeStatus>;
    fn actions(&self) -> ServiceFuture<'_, Vec<TaskSummary>>;
    fn start_action(&self, action: &str, input: Value) -> ServiceFuture<'_, TaskSummary>;
    fn cancel_action(&self, id: &str) -> ServiceFuture<'_, ()>;
}

pub trait TrustService: Send + Sync {
    fn snapshot(&self) -> ServiceFuture<'_, TrustSnapshot>;
    fn set_policy(&self, policy: &str) -> ServiceFuture<'_, TrustSnapshot>;
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PreviewDocumentKind {
    Url,
    Html,
    Image,
    Binary,
    #[default]
    Text,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PreviewDocument {
    pub kind: PreviewDocumentKind,
    pub label: String,
    pub source: String,
    pub url: String,
    pub mime_type: Option<String>,
    pub language: Option<String>,
    pub byte_size: Option<u64>,
    pub large: bool,
    pub text: Option<String>,
}

pub trait PreviewService: Send + Sync {
    fn load(
        &self,
        raw_target: &str,
        base_dir: Option<&Path>,
    ) -> ServiceFuture<'_, Option<PreviewDocument>>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailablePreviewService;

impl PreviewService for UnavailablePreviewService {
    fn load(
        &self,
        _raw_target: &str,
        _base_dir: Option<&Path>,
    ) -> ServiceFuture<'_, Option<PreviewDocument>> {
        Box::pin(async move {
            Err(ServiceError::Unavailable(
                "safe preview is unavailable on this platform".into(),
            ))
        })
    }
}

pub trait FileService: Send + Sync {
    fn read_dir(&self, root: &Path, relative: &Path) -> ServiceFuture<'_, Vec<FileEntry>>;
    fn read_text(&self, root: &Path, relative: &Path) -> ServiceFuture<'_, String>;
    fn write_text(&self, root: &Path, relative: &Path, content: &str) -> ServiceFuture<'_, ()>;
    fn rename(&self, root: &Path, relative: &Path, new_name: &str) -> ServiceFuture<'_, String>;
    fn reveal(&self, root: &Path, relative: &Path) -> ServiceFuture<'_, ()>;
    fn open(&self, root: &Path, relative: &Path) -> ServiceFuture<'_, ()>;
    fn trash(&self, root: &Path, relative: &Path) -> ServiceFuture<'_, ()>;
    fn watch_directory(&self, _root: &Path, _relative: &Path) -> ServiceResult<FileWatchStream> {
        Err(ServiceError::Unavailable(
            "directory watching is unavailable on this platform".into(),
        ))
    }
}

pub trait GitService: Send + Sync {
    fn status(&self, repository: &Path) -> ServiceFuture<'_, GitStatus>;
    fn diff(&self, repository: &Path, relative: &Path) -> ServiceFuture<'_, String>;
    fn diff_staged(&self, _repository: &Path, _relative: &Path) -> ServiceFuture<'_, String> {
        Box::pin(async move {
            Err(ServiceError::Unavailable(
                "staged Git diff is unavailable on this platform".into(),
            ))
        })
    }
    fn stage(&self, repository: &Path, relative: &Path) -> ServiceFuture<'_, ()>;
    fn unstage(&self, repository: &Path, relative: &Path) -> ServiceFuture<'_, ()>;
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GitBranchInfo {
    pub name: String,
    pub checked_out: bool,
    pub is_default: bool,
    pub worktree_path: Option<String>,
}

pub trait GitBranchService: Send + Sync {
    fn list(&self, repository: &Path) -> ServiceFuture<'_, Vec<GitBranchInfo>>;
    fn switch(&self, repository: &Path, branch: &str) -> ServiceFuture<'_, String>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableGitBranchService;

impl GitBranchService for UnavailableGitBranchService {
    fn list(&self, _repository: &Path) -> ServiceFuture<'_, Vec<GitBranchInfo>> {
        Box::pin(async move {
            Err(ServiceError::Unavailable(
                "Git branch management is unavailable on this platform".into(),
            ))
        })
    }

    fn switch(&self, _repository: &Path, _branch: &str) -> ServiceFuture<'_, String> {
        Box::pin(async move {
            Err(ServiceError::Unavailable(
                "Git branch management is unavailable on this platform".into(),
            ))
        })
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GitWorktreeInfo {
    pub path: String,
    pub branch: Option<String>,
    pub is_main: bool,
    pub detached: bool,
    pub locked: bool,
}

pub trait GitWorktreeService: Send + Sync {
    fn list(&self, repository: &Path) -> ServiceFuture<'_, Vec<GitWorktreeInfo>>;
    fn add_new(
        &self,
        repository: &Path,
        display_name: &str,
        branch: &str,
        base: Option<&str>,
    ) -> ServiceFuture<'_, GitWorktreeInfo>;
    fn add_existing(&self, repository: &Path, branch: &str) -> ServiceFuture<'_, GitWorktreeInfo>;
    fn remove(
        &self,
        repository: &Path,
        worktree_path: &Path,
        force: bool,
    ) -> ServiceFuture<'_, String>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableGitWorktreeService;

impl GitWorktreeService for UnavailableGitWorktreeService {
    fn list(&self, _repository: &Path) -> ServiceFuture<'_, Vec<GitWorktreeInfo>> {
        Box::pin(async move {
            Err(ServiceError::Unavailable(
                "Git worktree management is unavailable on this platform".into(),
            ))
        })
    }

    fn add_new(
        &self,
        _repository: &Path,
        _display_name: &str,
        _branch: &str,
        _base: Option<&str>,
    ) -> ServiceFuture<'_, GitWorktreeInfo> {
        Box::pin(async move {
            Err(ServiceError::Unavailable(
                "Git worktree management is unavailable on this platform".into(),
            ))
        })
    }

    fn add_existing(
        &self,
        _repository: &Path,
        _branch: &str,
    ) -> ServiceFuture<'_, GitWorktreeInfo> {
        Box::pin(async move {
            Err(ServiceError::Unavailable(
                "Git worktree management is unavailable on this platform".into(),
            ))
        })
    }

    fn remove(
        &self,
        _repository: &Path,
        _worktree_path: &Path,
        _force: bool,
    ) -> ServiceFuture<'_, String> {
        Box::pin(async move {
            Err(ServiceError::Unavailable(
                "Git worktree management is unavailable on this platform".into(),
            ))
        })
    }
}

pub trait GitDiscardService: Send + Sync {
    fn discard_path(&self, repository: &Path, relative: &Path) -> ServiceFuture<'_, ()>;
    fn discard_all(&self, repository: &Path) -> ServiceFuture<'_, ()>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableGitDiscardService;

impl GitDiscardService for UnavailableGitDiscardService {
    fn discard_path(&self, _repository: &Path, _relative: &Path) -> ServiceFuture<'_, ()> {
        Box::pin(async move {
            Err(ServiceError::Unavailable(
                "Git discard is unavailable on this platform".into(),
            ))
        })
    }

    fn discard_all(&self, _repository: &Path) -> ServiceFuture<'_, ()> {
        Box::pin(async move {
            Err(ServiceError::Unavailable(
                "Git discard is unavailable on this platform".into(),
            ))
        })
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GitPullRequestInfo {
    pub url: String,
    pub state: String,
    pub number: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GitShipInfo {
    pub gh_ready: bool,
    pub pull_request: Option<GitPullRequestInfo>,
}

pub trait GitShipService: Send + Sync {
    fn info(&self, repository: &Path) -> ServiceFuture<'_, GitShipInfo>;
    fn commit(
        &self,
        repository: &Path,
        message: &str,
        push_after_commit: bool,
    ) -> ServiceFuture<'_, ()>;
    fn push(&self, repository: &Path) -> ServiceFuture<'_, ()>;
    fn create_pull_request(&self, repository: &Path) -> ServiceFuture<'_, String>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableGitShipService;

impl GitShipService for UnavailableGitShipService {
    fn info(&self, _repository: &Path) -> ServiceFuture<'_, GitShipInfo> {
        Box::pin(async move {
            Err(ServiceError::Unavailable(
                "Git ship actions are unavailable on this platform".into(),
            ))
        })
    }

    fn commit(
        &self,
        _repository: &Path,
        _message: &str,
        _push_after_commit: bool,
    ) -> ServiceFuture<'_, ()> {
        Box::pin(async move {
            Err(ServiceError::Unavailable(
                "Git ship actions are unavailable on this platform".into(),
            ))
        })
    }

    fn push(&self, _repository: &Path) -> ServiceFuture<'_, ()> {
        Box::pin(async move {
            Err(ServiceError::Unavailable(
                "Git ship actions are unavailable on this platform".into(),
            ))
        })
    }

    fn create_pull_request(&self, _repository: &Path) -> ServiceFuture<'_, String> {
        Box::pin(async move {
            Err(ServiceError::Unavailable(
                "Git ship actions are unavailable on this platform".into(),
            ))
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct RepoScanCancellation {
    cancelled: Arc<AtomicBool>,
}

impl RepoScanCancellation {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DiscoveredGitRepository {
    pub root: PathBuf,
    pub label: String,
}

pub trait GitRepoScanService: Send + Sync {
    fn scan(
        &self,
        roots: &[PathBuf],
        exclude_paths: &[PathBuf],
        enabled: bool,
        cancellation: RepoScanCancellation,
    ) -> ServiceFuture<'_, Vec<DiscoveredGitRepository>>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableGitRepoScanService;

impl GitRepoScanService for UnavailableGitRepoScanService {
    fn scan(
        &self,
        _roots: &[PathBuf],
        _exclude_paths: &[PathBuf],
        _enabled: bool,
        _cancellation: RepoScanCancellation,
    ) -> ServiceFuture<'_, Vec<DiscoveredGitRepository>> {
        Box::pin(async move {
            Err(ServiceError::Unavailable(
                "Git repository discovery is unavailable on this platform".into(),
            ))
        })
    }
}

pub trait SkillsService: Send + Sync {
    fn list(&self, profile: Option<&str>) -> ServiceFuture<'_, Vec<SkillSummary>>;
    fn set_enabled(
        &self,
        profile: Option<&str>,
        name: &str,
        enabled: bool,
    ) -> ServiceFuture<'_, SkillToggleResult>;
    fn hub_sources(&self, profile: Option<&str>) -> ServiceFuture<'_, SkillHubSourcesResponse>;
    fn hub_search(
        &self,
        profile: Option<&str>,
        query: &str,
        source: &str,
        limit: u32,
    ) -> ServiceFuture<'_, SkillHubSearchResponse>;
    fn hub_preview(
        &self,
        profile: Option<&str>,
        identifier: &str,
    ) -> ServiceFuture<'_, SkillHubPreview>;
    fn hub_scan(
        &self,
        profile: Option<&str>,
        identifier: &str,
    ) -> ServiceFuture<'_, SkillHubScanResult>;
    fn hub_install(
        &self,
        profile: Option<&str>,
        identifier: &str,
    ) -> ServiceFuture<'_, SkillActionStart>;
    fn hub_uninstall(
        &self,
        profile: Option<&str>,
        name: &str,
    ) -> ServiceFuture<'_, SkillActionStart>;
    fn hub_update(&self, profile: Option<&str>) -> ServiceFuture<'_, SkillActionStart>;
    fn action_status(
        &self,
        profile: Option<&str>,
        name: &str,
        lines: u32,
    ) -> ServiceFuture<'_, SkillActionStatus>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableSkillsService;

impl SkillsService for UnavailableSkillsService {
    fn list(&self, _profile: Option<&str>) -> ServiceFuture<'_, Vec<SkillSummary>> {
        Box::pin(async {
            Err(ServiceError::Unavailable(
                "Skills are unavailable on this platform".into(),
            ))
        })
    }

    fn set_enabled(
        &self,
        _profile: Option<&str>,
        _name: &str,
        _enabled: bool,
    ) -> ServiceFuture<'_, SkillToggleResult> {
        Box::pin(async {
            Err(ServiceError::Unavailable(
                "Skills are unavailable on this platform".into(),
            ))
        })
    }

    fn hub_sources(&self, _profile: Option<&str>) -> ServiceFuture<'_, SkillHubSourcesResponse> {
        Box::pin(async {
            Err(ServiceError::Unavailable(
                "Skills Hub is unavailable on this platform".into(),
            ))
        })
    }

    fn hub_search(
        &self,
        _profile: Option<&str>,
        _query: &str,
        _source: &str,
        _limit: u32,
    ) -> ServiceFuture<'_, SkillHubSearchResponse> {
        Box::pin(async {
            Err(ServiceError::Unavailable(
                "Skills Hub is unavailable on this platform".into(),
            ))
        })
    }

    fn hub_preview(
        &self,
        _profile: Option<&str>,
        _identifier: &str,
    ) -> ServiceFuture<'_, SkillHubPreview> {
        Box::pin(async {
            Err(ServiceError::Unavailable(
                "Skills Hub is unavailable on this platform".into(),
            ))
        })
    }

    fn hub_scan(
        &self,
        _profile: Option<&str>,
        _identifier: &str,
    ) -> ServiceFuture<'_, SkillHubScanResult> {
        Box::pin(async {
            Err(ServiceError::Unavailable(
                "Skills Hub is unavailable on this platform".into(),
            ))
        })
    }

    fn hub_install(
        &self,
        _profile: Option<&str>,
        _identifier: &str,
    ) -> ServiceFuture<'_, SkillActionStart> {
        Box::pin(async {
            Err(ServiceError::Unavailable(
                "Skills Hub is unavailable on this platform".into(),
            ))
        })
    }

    fn hub_uninstall(
        &self,
        _profile: Option<&str>,
        _name: &str,
    ) -> ServiceFuture<'_, SkillActionStart> {
        Box::pin(async {
            Err(ServiceError::Unavailable(
                "Skills Hub is unavailable on this platform".into(),
            ))
        })
    }

    fn hub_update(&self, _profile: Option<&str>) -> ServiceFuture<'_, SkillActionStart> {
        Box::pin(async {
            Err(ServiceError::Unavailable(
                "Skills Hub is unavailable on this platform".into(),
            ))
        })
    }

    fn action_status(
        &self,
        _profile: Option<&str>,
        _name: &str,
        _lines: u32,
    ) -> ServiceFuture<'_, SkillActionStatus> {
        Box::pin(async {
            Err(ServiceError::Unavailable(
                "Skills Hub action status is unavailable on this platform".into(),
            ))
        })
    }
}

pub trait TerminalService: Send + Sync {
    fn start(&self, cwd: &Path, cols: u16, rows: u16) -> ServiceFuture<'_, String>;
    fn write(&self, id: &str, data: &[u8]) -> ServiceFuture<'_, ()>;
    fn read(&self, id: &str) -> ServiceFuture<'_, Vec<u8>>;
    fn resize(&self, id: &str, cols: u16, rows: u16) -> ServiceFuture<'_, ()>;
    fn dispose(&self, id: &str) -> ServiceFuture<'_, ()>;
    fn dispose_now(&self, _id: &str) -> ServiceResult<()> {
        Err(ServiceError::Unavailable(
            "synchronous terminal disposal is unavailable on this platform".into(),
        ))
    }
}

pub trait UpdateService: Send + Sync {
    fn check(&self) -> ServiceFuture<'_, Value>;
    fn apply(&self, options: Value) -> ServiceFuture<'_, ()>;
}

pub trait PlatformService: Send + Sync {
    fn pick_attachments(
        &self,
        _title: &str,
        _starting_directory: Option<&Path>,
        _images_only: bool,
    ) -> ServiceFuture<'_, Vec<SelectedAttachment>> {
        Box::pin(async move {
            Err(ServiceError::Unavailable(
                "native attachment selection is unavailable on this host".into(),
            ))
        })
    }
    fn pick_folder(
        &self,
        title: &str,
        starting_directory: Option<&Path>,
    ) -> ServiceFuture<'_, Option<PathBuf>>;
    fn open_external(&self, url: &str) -> ServiceFuture<'_, ()>;
    fn notify(&self, title: &str, body: &str) -> ServiceFuture<'_, bool>;
    fn version(&self) -> ServiceFuture<'_, String>;
}

#[derive(Clone)]
pub struct AppServices {
    pub connection: Arc<dyn ConnectionService>,
    pub sessions: Arc<dyn SessionService>,
    pub projects: Arc<dyn ProjectService>,
    pub settings: Arc<dyn SettingsService>,
    pub agent_config: Arc<dyn AgentConfigService>,
    pub models: Arc<dyn ModelService>,
    pub providers: Arc<dyn ProviderService>,
    pub runtime: Arc<dyn RuntimeService>,
    pub trust: Arc<dyn TrustService>,
    pub preview: Arc<dyn PreviewService>,
    pub files: Arc<dyn FileService>,
    pub git: Arc<dyn GitService>,
    pub git_branches: Arc<dyn GitBranchService>,
    pub git_worktrees: Arc<dyn GitWorktreeService>,
    pub git_discard: Arc<dyn GitDiscardService>,
    pub git_ship: Arc<dyn GitShipService>,
    pub git_repo_scan: Arc<dyn GitRepoScanService>,
    pub skills: Arc<dyn SkillsService>,
    pub terminal: Arc<dyn TerminalService>,
    pub updates: Arc<dyn UpdateService>,
    pub platform: Arc<dyn PlatformService>,
}

pub fn validate_identifier(value: &str, field: &str) -> ServiceResult<()> {
    if value.is_empty()
        || value.len() > 256
        || value
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\' | '?' | '#'))
    {
        return Err(ServiceError::InvalidInput(format!("invalid {field}")));
    }
    Ok(())
}

pub fn validate_relative_path(path: &Path) -> ServiceResult<()> {
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(ServiceError::InvalidInput(
            "path must stay relative to the selected root".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn rejects_parent_and_absolute_paths() {
        assert!(validate_relative_path(Path::new("../secret")).is_err());
        assert!(validate_relative_path(Path::new("C:\\secret")).is_err());
        assert!(validate_relative_path(Path::new("src/main.rs")).is_ok());
    }

    #[test]
    fn rejects_control_characters_in_identifiers() {
        assert!(validate_identifier("session-1", "session").is_ok());
        assert!(validate_identifier("session\n2", "session").is_err());
        assert!(validate_identifier("../profiles", "session").is_err());
        assert!(validate_identifier("session?profile=other", "session").is_err());
    }

    fn event(kind: &str, session_id: &str, payload: Value) -> GatewayEvent {
        GatewayEvent {
            kind: kind.into(),
            session_id: Some(session_id.into()),
            profile: None,
            payload,
            extra: Default::default(),
        }
    }

    #[test]
    fn reconciles_stream_deltas_and_completion() {
        let mut state = SessionTranscript {
            runtime_id: "runtime-1".into(),
            ..SessionTranscript::default()
        };
        assert!(state.apply_event(&event(
            "message.delta",
            "runtime-1",
            serde_json::json!({ "text": "hello " })
        )));
        assert!(state.apply_event(&event(
            "message.delta",
            "runtime-1",
            serde_json::json!({ "text": "world" })
        )));
        assert!(state.apply_event(&event(
            "message.complete",
            "runtime-1",
            serde_json::json!({ "text": "hello world" })
        )));
        assert_eq!(state.messages.len(), 1);
        assert_eq!(state.messages[0].text, "hello world");
        assert!(!state.messages[0].streaming);
        assert!(!state.busy);
    }

    #[test]
    fn isolates_events_and_upserts_tool_progress() {
        let mut state = SessionTranscript {
            runtime_id: "runtime-1".into(),
            ..SessionTranscript::default()
        };
        assert!(!state.apply_event(&event(
            "message.delta",
            "runtime-2",
            serde_json::json!({ "text": "wrong" })
        )));
        assert!(state.apply_event(&event(
            "tool.start",
            "runtime-1",
            serde_json::json!({ "name": "terminal", "tool_id": "call-1", "preview": "running" })
        )));
        assert!(state.apply_event(&event(
            "tool.complete",
            "runtime-1",
            serde_json::json!({ "name": "terminal", "tool_id": "call-1", "result": "done" })
        )));
        assert_eq!(state.messages.len(), 1);
        assert_eq!(state.messages[0].text, "done");
        assert!(!state.messages[0].streaming);
    }

    #[test]
    fn settles_an_interim_reply_without_duplicating_it() {
        let mut state = SessionTranscript {
            runtime_id: "runtime-1".into(),
            ..SessionTranscript::default()
        };
        state.apply_event(&event(
            "message.delta",
            "runtime-1",
            serde_json::json!({ "text": "working" }),
        ));
        state.apply_event(&event(
            "message.interim",
            "runtime-1",
            serde_json::json!({ "text": "working" }),
        ));
        state.apply_event(&event(
            "message.complete",
            "runtime-1",
            serde_json::json!({ "text": "working, done" }),
        ));
        assert_eq!(state.messages.len(), 1);
        assert_eq!(state.messages[0].text, "working, done");
    }

    #[test]
    fn projects_an_inflight_turn_from_resume() {
        let state = SessionTranscript::load(
            "stored-1".into(),
            SessionResumeResponse {
                session_id: "runtime-1".into(),
                info: Some(serde_json::json!({ "running": true })),
                inflight: Some(serde_json::json!({
                    "user": "keep going",
                    "assistant": "partial answer"
                })),
                ..SessionResumeResponse::default()
            },
        );
        assert!(state.busy);
        assert_eq!(state.messages.len(), 2);
        assert_eq!(state.messages[0].role, MessageRole::User);
        assert_eq!(state.messages[1].text, "partial answer");
        assert!(state.messages[1].streaming);
    }
}

#[cfg(test)]
mod attachment_context_tests {
    use super::*;

    #[test]
    fn file_refs_precede_visible_text() {
        let attachments = vec![SessionAttachmentResult {
            attached: true,
            kind: AttachmentKind::File,
            ref_text: Some("@file:`notes/a b.txt`".into()),
            ..SessionAttachmentResult::default()
        }];
        assert_eq!(
            attachment_context_text("summarise this", &attachments),
            "@file:`notes/a b.txt`\n\nsummarise this"
        );
    }

    #[test]
    fn image_only_gets_fallback_prompt() {
        let attachments = vec![SessionAttachmentResult {
            attached: true,
            kind: AttachmentKind::Image,
            ..SessionAttachmentResult::default()
        }];
        assert_eq!(
            attachment_context_text("", &attachments),
            "What do you see in this image?"
        );
    }
}
