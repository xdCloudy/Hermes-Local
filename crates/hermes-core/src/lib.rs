//! Hermes Local product boundary consumed by the Dioxus UI.

use std::{
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
    AgentConfigSnapshot, AppSettings, ChatMessage, ConnectionConfig, ConnectionConfigInput,
    ConnectionOauthLoginResult, ConnectionOauthLogoutResult, ConnectionProbeResult,
    ConnectionState, ConnectionTestResult, CustomEndpointUpdate, CustomEndpointValidation,
    CustomEndpointsResponse, EnvVarInfo, FileEntry, GatewayEvent, GitStatus, MessageRole,
    MoaConfig, ModelAssignmentRequest, ModelAssignmentResponse, ModelSettingsSnapshot, OAuthPoll,
    OAuthProvider, OAuthStart, OAuthSubmit, ProjectFilesDeleteResult, ProjectSummary,
    ProjectsSnapshot, ProviderActivation, RuntimeStatus, SessionCreateRequest,
    SessionResumeResponse, SessionSummary, TaskSummary, TrustSnapshot,
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
            busy,
            needs_input: false,
            error,
        }
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

pub trait TerminalService: Send + Sync {
    fn start(&self, cwd: &Path, cols: u16, rows: u16) -> ServiceFuture<'_, String>;
    fn write(&self, id: &str, data: &[u8]) -> ServiceFuture<'_, ()>;
    fn read(&self, id: &str) -> ServiceFuture<'_, Vec<u8>>;
    fn resize(&self, id: &str, cols: u16, rows: u16) -> ServiceFuture<'_, ()>;
    fn dispose(&self, id: &str) -> ServiceFuture<'_, ()>;
}

pub trait UpdateService: Send + Sync {
    fn check(&self) -> ServiceFuture<'_, Value>;
    fn apply(&self, options: Value) -> ServiceFuture<'_, ()>;
}

pub trait PlatformService: Send + Sync {
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
