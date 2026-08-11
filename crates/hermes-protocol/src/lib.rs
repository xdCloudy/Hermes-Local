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
    #[serde(default)]
    pub id: String,
    pub role: MessageRole,
    #[serde(default, alias = "content")]
    pub text: String,
    #[serde(default)]
    pub timestamp: Option<f64>,
    #[serde(default)]
    pub streaming: bool,
    #[serde(default)]
    pub tool_name: Option<String>,
    #[serde(default)]
    pub metadata: BTreeMap<String, Value>,
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
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    Dark,
    Light,
    System,
}

impl Default for ThemeMode {
    fn default() -> Self {
        Self::System
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct AppSettings {
    #[serde(default)]
    pub theme: ThemeMode,
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
