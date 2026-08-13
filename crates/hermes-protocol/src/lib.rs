//! Platform-neutral Hermes Local wire and domain contracts.
//!
//! Unknown fields are deliberately retained or ignored by Serde so a newer
//! Hermes Agent can extend messages without breaking an older Desktop client.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const JSON_RPC_VERSION: &str = "2.0";

#[allow(clippy::option_option)] // Three states are required: omitted, explicit null, and a port.
fn deserialize_present_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(untagged)]
pub enum RpcId {
    Number(i64),
    String(String),
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RpcErrorObject {
    #[serde(default)]
    pub code: Option<i64>,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub data: Option<Value>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct JsonRpcFrame {
    #[serde(default = "json_rpc_version")]
    pub jsonrpc: String,
    #[serde(default)]
    pub id: Option<RpcId>,
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub params: Option<Value>,
    #[serde(default)]
    pub result: Option<Value>,
    #[serde(default)]
    pub error: Option<RpcErrorObject>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

fn json_rpc_version() -> String {
    JSON_RPC_VERSION.to_owned()
}

impl JsonRpcFrame {
    pub fn request(id: RpcId, method: impl Into<String>, params: Value) -> Self {
        Self {
            jsonrpc: json_rpc_version(),
            id: Some(id),
            method: Some(method.into()),
            params: Some(params),
            result: None,
            error: None,
            extra: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GatewayEvent {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default)]
    pub payload: Value,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl GatewayEvent {
    pub fn from_frame(frame: &JsonRpcFrame) -> Option<Self> {
        if frame.method.as_deref() != Some("event") {
            return None;
        }
        serde_json::from_value(frame.params.clone()?).ok()
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    #[default]
    Idle,
    Connecting,
    Open,
    Closed,
    Error,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionMode {
    #[default]
    Local,
    Remote,
    Cloud,
    Ssh,
}

impl ConnectionMode {
    pub const fn is_remote_like(self) -> bool {
        matches!(self, Self::Remote | Self::Cloud)
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RemoteAuthMode {
    Oauth,
    #[default]
    Token,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProbeAuthMode {
    Oauth,
    Token,
    #[default]
    Unknown,
}

/// Sanitized Gateway settings exposed to the UI. The actual remote token is
/// deliberately absent: desktop authority returns only a preview and set flag.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionConfig {
    #[serde(default)]
    pub env_override: bool,
    #[serde(default)]
    pub mode: ConnectionMode,
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default)]
    pub remote_auth_mode: RemoteAuthMode,
    #[serde(default)]
    pub remote_oauth_connected: bool,
    #[serde(default)]
    pub remote_token_preview: Option<String>,
    #[serde(default)]
    pub remote_token_set: bool,
    #[serde(default)]
    pub remote_url: String,
    #[serde(default)]
    pub cloud_org: String,
    #[serde(default)]
    pub ssh_host: String,
    #[serde(default)]
    pub ssh_user: String,
    #[serde(default)]
    pub ssh_port: Option<u16>,
    #[serde(default)]
    pub ssh_key_path: String,
    #[serde(default)]
    pub ssh_remote_hermes_path: String,
    #[serde(default)]
    pub ssh_remote_profile: String,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            env_override: false,
            mode: ConnectionMode::Local,
            profile: None,
            remote_auth_mode: RemoteAuthMode::Token,
            remote_oauth_connected: false,
            remote_token_preview: None,
            remote_token_set: false,
            remote_url: String::new(),
            cloud_org: String::new(),
            ssh_host: String::new(),
            ssh_user: String::new(),
            ssh_port: None,
            ssh_key_path: String::new(),
            ssh_remote_hermes_path: String::new(),
            ssh_remote_profile: String::new(),
        }
    }
}

/// Renderer-to-desktop Gateway mutation. A missing token means "preserve the
/// existing secret"; an empty token is never persisted.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionConfigInput {
    #[serde(default)]
    pub mode: ConnectionMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_auth_mode: Option<RemoteAuthMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cloud_org: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_user: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_present_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub ssh_port: Option<Option<u16>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_key_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_remote_hermes_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_remote_profile: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SshErrorKind {
    AuthFailed,
    HermesNotFound,
    HostKeyChanged,
    Timeout,
    Unreachable,
    UnsupportedPlatform,
    UpdateRequired,
    Unknown,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionTestResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ok: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reachable: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_error: Option<SshErrorKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_hermes_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_hermes_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_platform: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthProvider {
    pub name: String,
    pub display_name: String,
    #[serde(default)]
    pub supports_password: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionProbeResult {
    pub base_url: String,
    #[serde(default)]
    pub reachable: bool,
    #[serde(default)]
    pub auth_mode: ProbeAuthMode,
    #[serde(default)]
    pub providers: Vec<AuthProvider>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionOauthLoginResult {
    #[serde(default)]
    pub ok: bool,
    pub base_url: String,
    #[serde(default)]
    pub connected: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionOauthLogoutResult {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub connected: bool,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    Assistant,
    System,
    Tool,
    User,
    #[default]
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct ChatMessage {
    #[serde(default, deserialize_with = "deserialize_stringish")]
    pub id: String,
    pub role: MessageRole,
    #[serde(default, deserialize_with = "deserialize_textish")]
    pub text: String,
    #[serde(
        default,
        rename = "content",
        deserialize_with = "deserialize_textish",
        skip_serializing_if = "String::is_empty"
    )]
    pub content_text: String,
    #[serde(default)]
    pub timestamp: Option<f64>,
    #[serde(default)]
    pub streaming: bool,
    #[serde(default)]
    pub tool_name: Option<String>,
    #[serde(default)]
    pub tool_call_id: Option<String>,
    #[serde(default)]
    pub reasoning: Option<String>,
    #[serde(default)]
    pub metadata: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct MessageReaction {
    #[serde(default)]
    pub emoji: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub at: Option<f64>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct SessionReactionResult {
    #[serde(default, deserialize_with = "deserialize_stringish")]
    pub row_id: String,
    #[serde(default)]
    pub reactions: Vec<MessageReaction>,
}

fn deserialize_stringish<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(match value {
        Value::String(value) => value,
        Value::Number(value) => value.to_string(),
        _ => String::new(),
    })
}

fn deserialize_textish<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    fn collect(value: &Value, output: &mut String) {
        match value {
            Value::String(value) => output.push_str(value),
            Value::Array(values) => {
                for value in values {
                    collect(value, output);
                }
            }
            Value::Object(value) => {
                if let Some(text) = value.get("text").or_else(|| value.get("content")) {
                    collect(text, output);
                }
            }
            _ => {}
        }
    }
    let value = Value::deserialize(deserializer)?;
    let mut output = String::new();
    collect(&value, &mut output);
    Ok(output)
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AttachmentKind {
    #[default]
    File,
    Image,
}

/// Opaque, user-selected Desktop attachment. `id` is a capability token held by
/// Desktop authority; the shared UI never receives an arbitrary filesystem path.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct SelectedAttachment {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub kind: AttachmentKind,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub preview_data_url: Option<String>,
    #[serde(default)]
    pub attached_session_id: Option<String>,
    #[serde(default)]
    pub ref_text: Option<String>,
    #[serde(default)]
    pub staged_path: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct SessionAttachmentResult {
    #[serde(default)]
    pub attached: bool,
    #[serde(default)]
    pub kind: AttachmentKind,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub ref_text: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct SessionSummary {
    pub id: String,
    #[serde(default, alias = "session_id")]
    pub runtime_id: Option<String>,
    #[serde(default)]
    pub lineage_root: Option<String>,
    #[serde(default, deserialize_with = "null_default")]
    pub title: String,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default, alias = "last_active")]
    pub updated_at: Option<f64>,
    #[serde(default, alias = "is_active")]
    pub running: bool,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub archived: bool,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct SessionCreateRequest {
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct SessionCreateResponse {
    pub session_id: String,
    #[serde(default)]
    pub session_key: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct SessionResumeResponse {
    #[serde(default, alias = "resumed")]
    pub stored_session_id: Option<String>,
    pub session_id: String,
    #[serde(default)]
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub running: bool,
    #[serde(default)]
    pub message_count: usize,
    #[serde(default)]
    pub messages_omitted: bool,
    #[serde(default)]
    pub inflight: Option<Value>,
    #[serde(default)]
    pub queued: Option<Value>,
    #[serde(default)]
    pub info: Option<Value>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct SessionMessagesResponse {
    #[serde(default)]
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub session_id: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
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

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct ProjectFolder {
    pub path: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub is_primary: bool,
    #[serde(default)]
    pub path_state: Option<String>,
    #[serde(default)]
    pub repository_id: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct ProjectSummary {
    pub id: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub primary_path: Option<String>,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub path_state: Option<String>,
    #[serde(default)]
    pub folders: Vec<ProjectFolder>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct ProjectsSnapshot {
    #[serde(default)]
    pub projects: Vec<ProjectSummary>,
    #[serde(default)]
    pub active_id: Option<String>,
    #[serde(default)]
    pub pinned_ids: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProjectFilesDeleteResult {
    pub snapshot: ProjectsSnapshot,
    pub deleted_paths: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    Dark,
    Light,
    #[default]
    System,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NativeNotificationKind {
    Approval,
    Input,
    TurnDone,
    TurnError,
    BackgroundDone,
    Credits,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[allow(clippy::struct_excessive_bools)] // Mirrors the OG per-kind preference record.
pub struct NativeNotificationKinds {
    #[serde(default = "default_true")]
    pub approval: bool,
    #[serde(default = "default_true")]
    pub input: bool,
    #[serde(default = "default_true")]
    pub turn_done: bool,
    #[serde(default = "default_true")]
    pub turn_error: bool,
    #[serde(default = "default_true")]
    pub background_done: bool,
    #[serde(default = "default_true")]
    pub credits: bool,
}

impl Default for NativeNotificationKinds {
    fn default() -> Self {
        Self {
            approval: true,
            input: true,
            turn_done: true,
            turn_error: true,
            background_done: true,
            credits: true,
        }
    }
}

impl NativeNotificationKinds {
    #[must_use]
    pub const fn enabled(&self, kind: NativeNotificationKind) -> bool {
        match kind {
            NativeNotificationKind::Approval => self.approval,
            NativeNotificationKind::Input => self.input,
            NativeNotificationKind::TurnDone => self.turn_done,
            NativeNotificationKind::TurnError => self.turn_error,
            NativeNotificationKind::BackgroundDone => self.background_done,
            NativeNotificationKind::Credits => self.credits,
        }
    }

    pub const fn set(&mut self, kind: NativeNotificationKind, enabled: bool) {
        match kind {
            NativeNotificationKind::Approval => self.approval = enabled,
            NativeNotificationKind::Input => self.input = enabled,
            NativeNotificationKind::TurnDone => self.turn_done = enabled,
            NativeNotificationKind::TurnError => self.turn_error = enabled,
            NativeNotificationKind::BackgroundDone => self.background_done = enabled,
            NativeNotificationKind::Credits => self.credits = enabled,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AppSettings {
    #[serde(default)]
    pub theme: ThemeMode,
    #[serde(default)]
    pub theme_name: Option<String>,
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default)]
    pub gateway_url: Option<String>,
    #[serde(default)]
    pub default_project_dir: Option<String>,
    #[serde(default = "default_true")]
    pub notifications: bool,
    #[serde(default)]
    pub notification_kinds: NativeNotificationKinds,
    #[serde(default = "default_completion_sound_variant_id")]
    pub completion_sound_variant_id: u8,
    #[serde(default)]
    pub keep_awake: bool,
    #[serde(default)]
    pub launch_at_login: bool,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: ThemeMode::System,
            theme_name: None,
            profile: None,
            gateway_url: None,
            default_project_dir: None,
            notifications: true,
            notification_kinds: NativeNotificationKinds::default(),
            completion_sound_variant_id: default_completion_sound_variant_id(),
            keep_awake: false,
            launch_at_login: false,
            extra: BTreeMap::new(),
        }
    }
}

/// A single field from the Agent's declared `/api/config/schema` response.
/// Unknown schema metadata is retained so newer Agent versions remain
/// forwards-compatible with this desktop client.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct ConfigFieldSchema {
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub options: Vec<Value>,
    #[serde(default)]
    pub searchable: bool,
    #[serde(default)]
    pub clearable: bool,
    #[serde(default, rename = "type")]
    pub field_type: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct ConfigSchemaResponse {
    #[serde(default)]
    pub category_order: Vec<String>,
    #[serde(default)]
    pub fields: BTreeMap<String, ConfigFieldSchema>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// The exact whole-record config surface used by the OG desktop client.
/// Saves replace the Agent record through `PUT /api/config`; callers must
/// preserve keys they do not edit.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AgentConfigSnapshot {
    pub config: BTreeMap<String, Value>,
    pub defaults: BTreeMap<String, Value>,
    pub schema: ConfigSchemaResponse,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct OAuthProviderStatus {
    #[serde(default)]
    pub logged_in: bool,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub source_label: Option<String>,
    #[serde(default)]
    pub token_preview: Option<String>,
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub has_refresh_token: bool,
    #[serde(default)]
    pub last_refresh: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct OAuthProvider {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub flow: String,
    #[serde(default)]
    pub cli_command: String,
    #[serde(default)]
    pub docs_url: String,
    #[serde(default)]
    pub disconnect_command: Option<String>,
    #[serde(default)]
    pub disconnect_hint: Option<String>,
    #[serde(default)]
    pub disconnectable: Option<bool>,
    #[serde(default)]
    pub status: OAuthProviderStatus,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "flow", rename_all = "snake_case")]
pub enum OAuthStart {
    Pkce {
        auth_url: String,
        expires_in: u64,
        session_id: String,
    },
    DeviceCode {
        expires_in: u64,
        poll_interval: u64,
        session_id: String,
        user_code: String,
        verification_url: String,
    },
}

impl OAuthStart {
    pub fn session_id(&self) -> &str {
        match self {
            Self::Pkce { session_id, .. } | Self::DeviceCode { session_id, .. } => session_id,
        }
    }

    pub fn browser_url(&self) -> &str {
        match self {
            Self::Pkce { auth_url, .. } => auth_url,
            Self::DeviceCode {
                verification_url, ..
            } => verification_url,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct OAuthSubmit {
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub status: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct OAuthPoll {
    #[serde(default)]
    pub error_message: Option<String>,
    #[serde(default)]
    pub expires_at: Option<u64>,
    pub session_id: String,
    #[serde(default)]
    pub status: String,
}

// These independent flags are the Agent's exact `/api/env` wire shape, not a
// state machine that can be collapsed into a single enum.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct EnvVarInfo {
    #[serde(default)]
    pub advanced: bool,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub channel_managed: bool,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub is_password: bool,
    #[serde(default)]
    pub is_set: bool,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub provider_label: Option<String>,
    #[serde(default)]
    pub redacted_value: Option<String>,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct CustomEndpoint {
    #[serde(default)]
    pub api_key_preview: Option<String>,
    pub base_url: String,
    #[serde(default)]
    pub context_length: Option<u64>,
    #[serde(default = "default_true")]
    pub discover_models: bool,
    #[serde(default)]
    pub has_api_key: bool,
    pub id: String,
    #[serde(default)]
    pub is_current: bool,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct CustomEndpointCurrent {
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub provider: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct CustomEndpointsResponse {
    #[serde(default)]
    pub current: CustomEndpointCurrent,
    #[serde(default)]
    pub endpoints: Vec<CustomEndpoint>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub ok: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct CustomEndpointUpdate {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    pub base_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_length: Option<u64>,
    #[serde(default)]
    pub discover_models: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default)]
    pub make_default: bool,
    #[serde(default)]
    pub model: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub models: Vec<String>,
    pub name: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct CustomEndpointValidation {
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub reachable: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct ProviderActivation {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub model: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct ModelInfo {
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub capabilities: BTreeMap<String, Value>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ModelCapabilities {
    #[serde(default)]
    pub fast: bool,
    #[serde(default)]
    pub reasoning: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct ModelProvider {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default)]
    pub authenticated: Option<bool>,
    #[serde(default)]
    pub auth_type: Option<String>,
    #[serde(default)]
    pub key_env: Option<String>,
    #[serde(default)]
    pub api_url: Option<String>,
    #[serde(default)]
    pub capabilities: BTreeMap<String, ModelCapabilities>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct ModelOptions {
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub providers: Vec<ModelProvider>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct AuxiliaryModelAssignment {
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub task: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct AuxiliaryModels {
    #[serde(default)]
    pub main: ModelInfo,
    #[serde(default)]
    pub tasks: Vec<AuxiliaryModelAssignment>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModelSettingsSnapshot {
    pub info: ModelInfo,
    pub options: ModelOptions,
    pub auxiliary: AuxiliaryModels,
    pub moa: Option<MoaConfig>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct MoaModelSlot {
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub reasoning_effort: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct MoaPreset {
    #[serde(default)]
    pub aggregator: MoaModelSlot,
    #[serde(default)]
    pub aggregator_temperature: f64,
    #[serde(default)]
    pub degraded_reference_policy: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub max_tokens: u64,
    #[serde(default)]
    pub reference_models: Vec<MoaModelSlot>,
    #[serde(default)]
    pub reference_temperature: f64,
    #[serde(default)]
    pub reference_max_tokens: Option<u64>,
    #[serde(default)]
    pub fanout: Option<String>,
    #[serde(default)]
    pub reference_timeout: Option<f64>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct MoaConfig {
    #[serde(default)]
    pub default_preset: String,
    #[serde(default)]
    pub active_preset: String,
    #[serde(default)]
    pub presets: BTreeMap<String, MoaPreset>,
    #[serde(default)]
    pub aggregator: MoaModelSlot,
    #[serde(default)]
    pub aggregator_temperature: f64,
    #[serde(default)]
    pub degraded_reference_policy: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub max_tokens: u64,
    #[serde(default)]
    pub reference_models: Vec<MoaModelSlot>,
    #[serde(default)]
    pub reference_temperature: f64,
    #[serde(default)]
    pub reference_timeout: Option<f64>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct ModelAssignmentRequest {
    pub model: String,
    pub provider: String,
    pub scope: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct ModelAssignmentResponse {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub tasks: Vec<String>,
    #[serde(default)]
    pub stale_aux: Vec<AuxiliaryModelAssignment>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

const fn default_true() -> bool {
    true
}

const fn default_completion_sound_variant_id() -> u8 {
    1
}

fn null_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Option::<T>::deserialize(deserializer).map(Option::unwrap_or_default)
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct RuntimeStatus {
    #[serde(default)]
    pub phase: String,
    #[serde(default)]
    pub agent_version: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub gateway: ConnectionState,
    #[serde(default)]
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct TaskSummary {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub progress: Option<f64>,
    #[serde(default)]
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct TrustSnapshot {
    #[serde(default)]
    pub policy: String,
    #[serde(default)]
    pub skills: Vec<TrustItem>,
    #[serde(default)]
    pub mcp_servers: Vec<TrustItem>,
    #[serde(default)]
    pub delegations: Vec<TrustItem>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct TrustItem {
    pub id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct FileEntry {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    #[serde(default)]
    pub size: Option<u64>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct GitChange {
    pub path: String,
    #[serde(default)]
    pub index_status: String,
    #[serde(default)]
    pub worktree_status: String,
    #[serde(default)]
    pub staged: bool,
    #[serde(default)]
    pub unstaged: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct GitStatus {
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub ahead: u32,
    #[serde(default)]
    pub behind: u32,
    #[serde(default)]
    pub changed: Vec<String>,
    #[serde(default)]
    pub entries: Vec<GitChange>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_unknown_gateway_event_fields() {
        let event: GatewayEvent = serde_json::from_str(
            r#"{"type":"future.event","session_id":"s1","payload":{"x":1},"future":true}"#,
        )
        .expect("valid event");
        assert_eq!(event.kind, "future.event");
        assert_eq!(event.extra.get("future"), Some(&Value::Bool(true)));
    }

    #[test]
    fn tolerates_numeric_ids_and_structured_message_content() {
        let message: ChatMessage = serde_json::from_value(serde_json::json!({
            "id": 42,
            "role": "assistant",
            "content": [
                { "type": "text", "text": "hello " },
                { "type": "text", "text": "world" }
            ]
        }))
        .expect("message");
        assert_eq!(message.id, "42");
        assert_eq!(message.content_text, "hello world");
    }

    #[test]
    fn parses_string_and_numeric_rpc_ids() {
        let string_id: JsonRpcFrame =
            serde_json::from_str(r#"{"jsonrpc":"2.0","id":"r1","result":{}}"#).expect("string id");
        let number_id: JsonRpcFrame =
            serde_json::from_str(r#"{"jsonrpc":"2.0","id":7,"result":{}}"#).expect("numeric id");
        assert_eq!(string_id.id, Some(RpcId::String("r1".into())));
        assert_eq!(number_id.id, Some(RpcId::Number(7)));
    }

    #[test]
    fn extracts_only_event_frames() {
        let frame: JsonRpcFrame = serde_json::from_str(
            r#"{"jsonrpc":"2.0","method":"event","params":{"type":"message.delta","payload":{"text":"hi"}}}"#,
        )
        .expect("event frame");
        assert_eq!(
            GatewayEvent::from_frame(&frame).expect("event").kind,
            "message.delta"
        );
    }

    #[test]
    fn notification_preferences_default_on_and_round_trip_per_kind() {
        let mut settings: AppSettings =
            serde_json::from_value(serde_json::json!({})).expect("legacy settings record");
        assert!(settings.notifications);
        assert_eq!(settings.completion_sound_variant_id, 1);
        for kind in [
            NativeNotificationKind::Approval,
            NativeNotificationKind::Input,
            NativeNotificationKind::TurnDone,
            NativeNotificationKind::TurnError,
            NativeNotificationKind::BackgroundDone,
            NativeNotificationKind::Credits,
        ] {
            assert!(settings.notification_kinds.enabled(kind));
        }
        settings
            .notification_kinds
            .set(NativeNotificationKind::TurnError, false);
        let round_trip: AppSettings = serde_json::from_value(
            serde_json::to_value(settings).expect("serialize notification preferences"),
        )
        .expect("deserialize notification preferences");
        assert!(
            !round_trip
                .notification_kinds
                .enabled(NativeNotificationKind::TurnError)
        );
        assert!(
            round_trip
                .notification_kinds
                .enabled(NativeNotificationKind::Approval)
        );
    }

    #[test]
    fn gateway_config_uses_the_og_renderer_shape_without_a_raw_token() {
        let config = ConnectionConfig {
            mode: ConnectionMode::Cloud,
            profile: Some("work".into()),
            remote_auth_mode: RemoteAuthMode::Oauth,
            remote_oauth_connected: true,
            remote_token_preview: Some("...secret".into()),
            remote_token_set: true,
            remote_url: "https://gateway.example".into(),
            cloud_org: "nous".into(),
            ssh_port: Some(22),
            ..ConnectionConfig::default()
        };
        let value = serde_json::to_value(config).expect("serialize Gateway config");
        assert_eq!(value["mode"], "cloud");
        assert_eq!(value["profile"], "work");
        assert_eq!(value["remoteAuthMode"], "oauth");
        assert_eq!(value["remoteOauthConnected"], true);
        assert_eq!(value["remoteTokenPreview"], "...secret");
        assert_eq!(value["remoteTokenSet"], true);
        assert_eq!(value["sshPort"], 22);
        assert!(value.get("remoteToken").is_none());
    }

    #[test]
    fn gateway_input_distinguishes_an_absent_ssh_port_from_explicit_null() {
        let absent: ConnectionConfigInput =
            serde_json::from_value(serde_json::json!({ "mode": "ssh" })).expect("absent port");
        let cleared: ConnectionConfigInput = serde_json::from_value(serde_json::json!({
            "mode": "ssh",
            "sshPort": null
        }))
        .expect("cleared port");
        assert_eq!(absent.ssh_port, None);
        assert_eq!(cleared.ssh_port, Some(None));
        assert_eq!(
            serde_json::to_value(cleared).expect("serialize cleared port")["sshPort"],
            Value::Null
        );
    }

    #[test]
    fn gateway_oauth_results_keep_the_og_ipc_casing() {
        let login = serde_json::to_value(ConnectionOauthLoginResult {
            ok: true,
            base_url: "https://gateway.example/hermes".into(),
            connected: true,
        })
        .expect("serialize OAuth login result");
        assert_eq!(login["baseUrl"], "https://gateway.example/hermes");
        assert_eq!(login["connected"], true);
        assert!(login.get("base_url").is_none());
    }
}

// AG-01: forward-compatible Skills/Hub REST DTOs shared by native authority and Dioxus.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SkillSummary {
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub enabled: bool,
    pub name: String,
    #[serde(default)]
    pub usage: Option<u64>,
    #[serde(default)]
    pub provenance: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SkillHubSource {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub available: Option<bool>,
    #[serde(default)]
    pub rate_limited: Option<bool>,
    #[serde(default)]
    pub searchable: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SkillHubResult {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub source: String,
    pub identifier: String,
    #[serde(default)]
    pub trust_level: String,
    #[serde(default)]
    pub repo: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SkillHubInstalledEntry {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub trust_level: Option<String>,
    #[serde(default)]
    pub scan_verdict: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SkillHubSourcesResponse {
    #[serde(default)]
    pub sources: Vec<SkillHubSource>,
    #[serde(default)]
    pub index_available: bool,
    #[serde(default)]
    pub featured: Vec<SkillHubResult>,
    #[serde(default)]
    pub installed: BTreeMap<String, SkillHubInstalledEntry>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SkillHubSearchResponse {
    #[serde(default)]
    pub results: Vec<SkillHubResult>,
    #[serde(default)]
    pub source_counts: BTreeMap<String, u64>,
    #[serde(default)]
    pub timed_out: Vec<String>,
    #[serde(default)]
    pub installed: BTreeMap<String, SkillHubInstalledEntry>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SkillHubPreview {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub source: String,
    pub identifier: String,
    #[serde(default)]
    pub trust_level: String,
    #[serde(default)]
    pub repo: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub skill_md: String,
    #[serde(default)]
    pub files: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SkillHubScanFinding {
    #[serde(default)]
    pub severity: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub file: String,
    #[serde(default)]
    pub line: Option<u64>,
    #[serde(default)]
    pub description: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SkillHubScanResult {
    pub name: String,
    pub identifier: String,
    pub source: String,
    #[serde(default)]
    pub trust_level: String,
    #[serde(default)]
    pub verdict: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub policy: String,
    #[serde(default)]
    pub policy_reason: Option<String>,
    #[serde(default)]
    pub findings: Vec<SkillHubScanFinding>,
    #[serde(default)]
    pub severity_counts: BTreeMap<String, u64>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SkillActionStart {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub pid: u64,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SkillActionStatus {
    #[serde(default)]
    pub exit_code: Option<i64>,
    #[serde(default)]
    pub lines: Vec<String>,
    pub name: String,
    #[serde(default)]
    pub pid: Option<u64>,
    #[serde(default)]
    pub running: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SkillToggleResult {
    #[serde(default)]
    pub ok: bool,
    pub name: String,
    #[serde(default)]
    pub enabled: bool,
}
