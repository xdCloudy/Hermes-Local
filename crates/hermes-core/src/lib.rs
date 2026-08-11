//! Hermes Local product boundary consumed by the Dioxus UI.

use std::{future::Future, path::Path, pin::Pin, sync::Arc};

use futures_core::Stream;
use hermes_protocol::{
    AppSettings, ChatMessage, ConnectionState, FileEntry, GatewayEvent, GitStatus, ProjectSummary,
    ProjectsSnapshot, RuntimeStatus, SessionCreateRequest, SessionSummary, TaskSummary,
    TrustSnapshot,
};
use serde_json::Value;
use thiserror::Error;

pub type ServiceFuture<'a, T> = Pin<Box<dyn Future<Output = ServiceResult<T>> + Send + 'a>>;
pub type EventStream = Pin<Box<dyn Stream<Item = GatewayEvent> + Send>>;
pub type ServiceResult<T> = Result<T, ServiceError>;

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
    fn resume(&self, session_id: &str) -> ServiceFuture<'_, Vec<ChatMessage>>;
    fn submit(&self, session_id: &str, text: &str) -> ServiceFuture<'_, ()>;
    fn interrupt(&self, session_id: &str) -> ServiceFuture<'_, ()>;
    fn events(&self) -> ServiceResult<EventStream>;
}

pub trait ConnectionService: Send + Sync {
    fn initialize(&self) -> ServiceFuture<'_, ConnectionState>;
    fn connect(&self, websocket_url: &str) -> ServiceFuture<'_, ConnectionState>;
    fn disconnect(&self) -> ServiceFuture<'_, ()>;
    fn state(&self) -> ServiceResult<ConnectionState>;
}

pub trait ProjectService: Send + Sync {
    fn snapshot(&self) -> ServiceFuture<'_, ProjectsSnapshot>;
    fn create(&self, name: &str, folders: &[String]) -> ServiceFuture<'_, ProjectSummary>;
    fn set_active(&self, id: Option<&str>) -> ServiceFuture<'_, ()>;
    fn remove(&self, id: &str) -> ServiceFuture<'_, ()>;
}

pub trait SettingsService: Send + Sync {
    fn load(&self) -> ServiceFuture<'_, AppSettings>;
    fn save(&self, settings: &AppSettings) -> ServiceFuture<'_, ()>;
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

pub trait FileService: Send + Sync {
    fn read_dir(&self, root: &Path, relative: &Path) -> ServiceFuture<'_, Vec<FileEntry>>;
    fn read_text(&self, root: &Path, relative: &Path) -> ServiceFuture<'_, String>;
    fn write_text(&self, root: &Path, relative: &Path, content: &str) -> ServiceFuture<'_, ()>;
    fn trash(&self, root: &Path, relative: &Path) -> ServiceFuture<'_, ()>;
}

pub trait GitService: Send + Sync {
    fn status(&self, repository: &Path) -> ServiceFuture<'_, GitStatus>;
    fn diff(&self, repository: &Path, relative: &Path) -> ServiceFuture<'_, String>;
    fn stage(&self, repository: &Path, relative: &Path) -> ServiceFuture<'_, ()>;
    fn unstage(&self, repository: &Path, relative: &Path) -> ServiceFuture<'_, ()>;
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
    fn open_external(&self, url: &str) -> ServiceFuture<'_, ()>;
    fn notify(&self, title: &str, body: &str) -> ServiceFuture<'_, ()>;
    fn version(&self) -> ServiceFuture<'_, String>;
}

#[derive(Clone)]
pub struct AppServices {
    pub connection: Arc<dyn ConnectionService>,
    pub sessions: Arc<dyn SessionService>,
    pub projects: Arc<dyn ProjectService>,
    pub settings: Arc<dyn SettingsService>,
    pub runtime: Arc<dyn RuntimeService>,
    pub trust: Arc<dyn TrustService>,
    pub files: Arc<dyn FileService>,
    pub git: Arc<dyn GitService>,
    pub terminal: Arc<dyn TerminalService>,
    pub updates: Arc<dyn UpdateService>,
    pub platform: Arc<dyn PlatformService>,
}

pub fn validate_identifier(value: &str, field: &str) -> ServiceResult<()> {
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
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
    }
}
