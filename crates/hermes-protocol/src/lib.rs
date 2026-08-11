//! Platform-neutral Hermes Local wire and domain contracts.
//!
//! Unknown fields are deliberately retained or ignored by Serde so a newer
//! Hermes Agent can extend messages without breaking an older Desktop client.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const JSON_RPC_VERSION: &str = "2.0";

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
pub struct ProjectFolder {
    pub path: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub is_primary: bool,
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

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    Dark,
    Light,
    #[default]
    System,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
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
    pub keep_awake: bool,
    #[serde(default)]
    pub launch_at_login: bool,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
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
}
