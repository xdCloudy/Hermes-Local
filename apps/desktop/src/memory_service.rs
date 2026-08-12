#![allow(dead_code)] // AG-04 service foundation; Dioxus Memory/Starmap integration is a later stage.

use std::{collections::BTreeMap, time::Duration};

use reqwest::{Client, Method, Request};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use url::Url;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_TOKEN_BYTES: usize = 16 * 1024;
const MAX_PROFILE_BYTES: usize = 256;
const MAX_PROVIDER_BYTES: usize = 256;
const MAX_CONFIG_ENTRIES: usize = 256;
const MAX_CONFIG_KEY_BYTES: usize = 256;
const MAX_CONFIG_VALUE_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct MemoryProviderSummary {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub configured: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct MemoryBuiltinFiles {
    #[serde(default)]
    pub memory: u64,
    #[serde(default)]
    pub user: u64,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct MemoryStatusResponse {
    #[serde(default)]
    pub active: String,
    #[serde(default)]
    pub providers: Vec<MemoryProviderSummary>,
    #[serde(default)]
    pub builtin_files: MemoryBuiltinFiles,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct MemoryProviderFieldOption {
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub value: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct MemoryProviderField {
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub group: String,
    #[serde(default)]
    pub info: Option<String>,
    #[serde(default)]
    pub inline: bool,
    #[serde(default)]
    pub is_set: bool,
    #[serde(default)]
    pub key: String,
    /// Keep this open so an older Desktop build can read future Agent field kinds.
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub options: Vec<MemoryProviderFieldOption>,
    #[serde(default)]
    pub placeholder: String,
    #[serde(default)]
    pub value: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct MemoryProviderConfig {
    #[serde(default)]
    pub docs_url: String,
    #[serde(default)]
    pub fields: Vec<MemoryProviderField>,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub name: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct MemoryProviderOAuthStatus {
    #[serde(default)]
    pub auth: Option<String>,
    #[serde(default)]
    pub connected: bool,
    #[serde(default)]
    pub detail: String,
    /// Keep this open for forward-compatible Agent states.
    #[serde(default)]
    pub state: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct MemoryMutationResponse {
    #[serde(default)]
    pub ok: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct MemoryResetResponse {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub deleted: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct CuratorStatusResponse {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub paused: bool,
    #[serde(default)]
    pub interval_hours: Option<f64>,
    #[serde(default)]
    pub last_run_at: Option<String>,
    #[serde(default)]
    pub min_idle_hours: Option<f64>,
    #[serde(default)]
    pub stale_after_days: Option<f64>,
    #[serde(default)]
    pub archive_after_days: Option<f64>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct CuratorPauseResponse {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub paused: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ActionResponse {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub pid: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryResetTarget {
    All,
    Memory,
    User,
}

impl MemoryResetTarget {
    const fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Memory => "memory",
            Self::User => "user",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MemoryMethod {
    Get,
    Post,
    Put,
}

#[derive(Clone, Debug, PartialEq)]
struct MemoryRequest {
    method: MemoryMethod,
    path: String,
    query: Vec<(String, String)>,
    routing_profile: Option<String>,
    body: Option<Value>,
}

#[derive(Clone)]
pub struct NativeMemoryClient {
    client: Client,
    base_url: Url,
    session_token: Option<String>,
    routing_profile: Option<String>,
}

impl NativeMemoryClient {
    /// Bind the client to the already-selected Desktop backend pool. The legacy
    /// bridge's `profile` field selects a backend process; it is not converted
    /// into an arbitrary Memory API query parameter.
    pub fn new(
        base_url: &str,
        session_token: Option<&str>,
        routing_profile: Option<&str>,
    ) -> Result<Self, String> {
        let base_url = validate_base_url(base_url)?;
        let session_token = session_token
            .map(validate_session_token)
            .transpose()?
            .map(str::to_owned);
        let routing_profile = routing_profile
            .map(validate_profile)
            .transpose()?
            .map(str::to_owned);
        let client = Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|error| format!("Could not build Memory HTTP client: {error}"))?;
        Ok(Self {
            client,
            base_url,
            session_token,
            routing_profile,
        })
    }

    pub async fn status(
        &self,
        routing_profile: Option<&str>,
    ) -> Result<MemoryStatusResponse, String> {
        self.execute(MemoryRequest::memory_status(routing_profile)?).await
    }

    pub async fn reset(
        &self,
        routing_profile: Option<&str>,
        target: MemoryResetTarget,
    ) -> Result<MemoryResetResponse, String> {
        self.execute(MemoryRequest::reset(routing_profile, target)?).await
    }

    pub async fn provider_config(
        &self,
        routing_profile: Option<&str>,
        provider: &str,
    ) -> Result<MemoryProviderConfig, String> {
        self.execute(MemoryRequest::provider_config(
            routing_profile,
            provider,
            MemoryMethod::Get,
            None,
        )?)
        .await
    }

    pub async fn save_provider_config(
        &self,
        routing_profile: Option<&str>,
        provider: &str,
        values: &BTreeMap<String, String>,
    ) -> Result<MemoryMutationResponse, String> {
        validate_provider_values(values)?;
        self.execute(MemoryRequest::provider_config(
            routing_profile,
            provider,
            MemoryMethod::Put,
            Some(json!({ "values": values })),
        )?)
        .await
    }

    pub async fn start_provider_oauth(
        &self,
        routing_profile: Option<&str>,
        provider: &str,
    ) -> Result<MemoryProviderOAuthStatus, String> {
        self.execute(MemoryRequest::provider_oauth(
            routing_profile,
            provider,
            "start",
            MemoryMethod::Post,
        )?)
        .await
    }

    pub async fn provider_oauth_status(
        &self,
        routing_profile: Option<&str>,
        provider: &str,
    ) -> Result<MemoryProviderOAuthStatus, String> {
        self.execute(MemoryRequest::provider_oauth(
            routing_profile,
            provider,
            "status",
            MemoryMethod::Get,
        )?)
        .await
    }

    pub async fn curator_status(
        &self,
        routing_profile: Option<&str>,
    ) -> Result<CuratorStatusResponse, String> {
        self.execute(MemoryRequest::curator_status(routing_profile)?).await
    }

    pub async fn set_curator_paused(
        &self,
        routing_profile: Option<&str>,
        paused: bool,
    ) -> Result<CuratorPauseResponse, String> {
        self.execute(MemoryRequest::set_curator_paused(
            routing_profile,
            paused,
        )?)
        .await
    }

    pub async fn run_curator(
        &self,
        routing_profile: Option<&str>,
    ) -> Result<ActionResponse, String> {
        self.execute(MemoryRequest::run_curator(routing_profile)?).await
    }

    async fn execute<T: DeserializeOwned>(&self, spec: MemoryRequest) -> Result<T, String> {
        self.assert_routing_profile(spec.routing_profile.as_deref())?;
        let request = self.build_request(&spec)?;
        let response = self
            .client
            .execute(request)
            .await
            .map_err(|error| format!("Memory request failed: {error}"))?;
        let status = response.status();
        if !status.is_success() {
            return Err(format!(
                "Memory endpoint returned HTTP {}.",
                status.as_u16()
            ));
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_RESPONSE_BYTES)
        {
            return Err("Memory response exceeded the 4 MiB safety limit.".into());
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|error| format!("Could not read Memory response: {error}"))?;
        if bytes.len() as u64 > MAX_RESPONSE_BYTES {
            return Err("Memory response exceeded the 4 MiB safety limit.".into());
        }
        serde_json::from_slice(&bytes)
            .map_err(|error| format!("Invalid Memory response: {error}"))
    }

    fn build_request(&self, spec: &MemoryRequest) -> Result<Request, String> {
        let mut url = self
            .base_url
            .join(spec.path.trim_start_matches('/'))
            .map_err(|error| format!("Could not construct Memory endpoint: {error}"))?;
        if !spec.query.is_empty() {
            let mut query = url.query_pairs_mut();
            for (key, value) in &spec.query {
                query.append_pair(key, value);
            }
        }
        let method = match spec.method {
            MemoryMethod::Get => Method::GET,
            MemoryMethod::Post => Method::POST,
            MemoryMethod::Put => Method::PUT,
        };
        let mut request = self.client.request(method, url);
        if let Some(token) = self.session_token.as_deref() {
            request = request.header("x-hermes-session-token", token);
        }
        if let Some(body) = spec.body.as_ref() {
            request = request.json(body);
        }
        request
            .build()
            .map_err(|error| format!("Could not build Memory request: {error}"))
    }

    fn assert_routing_profile(&self, requested: Option<&str>) -> Result<(), String> {
        let requested = requested.map(validate_profile).transpose()?;
        if requested == self.routing_profile.as_deref() {
            Ok(())
        } else {
            Err("Memory request targets a different backend profile than this client.".into())
        }
    }
}

impl MemoryRequest {
    fn memory_status(routing_profile: Option<&str>) -> Result<Self, String> {
        request(MemoryMethod::Get, "/api/memory", routing_profile, None)
    }

    fn reset(
        routing_profile: Option<&str>,
        target: MemoryResetTarget,
    ) -> Result<Self, String> {
        request(
            MemoryMethod::Post,
            "/api/memory/reset",
            routing_profile,
            Some(json!({ "target": target.as_str() })),
        )
    }

    fn provider_config(
        routing_profile: Option<&str>,
        provider: &str,
        method: MemoryMethod,
        body: Option<Value>,
    ) -> Result<Self, String> {
        let provider = encoded_provider(provider)?;
        let mut spec = request(
            method,
            &format!("/api/memory/providers/{provider}/config"),
            routing_profile,
            body,
        )?;
        spec.query.push(("surface".into(), "declared".into()));
        Ok(spec)
    }

    fn provider_oauth(
        routing_profile: Option<&str>,
        provider: &str,
        action: &str,
        method: MemoryMethod,
    ) -> Result<Self, String> {
        let provider = encoded_provider(provider)?;
        request(
            method,
            &format!("/api/memory/providers/{provider}/oauth/{action}"),
            routing_profile,
            None,
        )
    }

    fn curator_status(routing_profile: Option<&str>) -> Result<Self, String> {
        request(MemoryMethod::Get, "/api/curator", routing_profile, None)
    }

    fn set_curator_paused(
        routing_profile: Option<&str>,
        paused: bool,
    ) -> Result<Self, String> {
        request(
            MemoryMethod::Put,
            "/api/curator/paused",
            routing_profile,
            Some(json!({ "paused": paused })),
        )
    }

    fn run_curator(routing_profile: Option<&str>) -> Result<Self, String> {
        request(
            MemoryMethod::Post,
            "/api/curator/run",
            routing_profile,
            Some(json!({})),
        )
    }
}

fn request(
    method: MemoryMethod,
    path: &str,
    routing_profile: Option<&str>,
    body: Option<Value>,
) -> Result<MemoryRequest, String> {
    Ok(MemoryRequest {
        method,
        path: path.to_owned(),
        query: Vec::new(),
        routing_profile: routing_profile
            .map(validate_profile)
            .transpose()?
            .map(str::to_owned),
        body,
    })
}

fn validate_base_url(value: &str) -> Result<Url, String> {
    let mut url =
        Url::parse(value.trim()).map_err(|error| format!("Invalid Memory base URL: {error}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("Memory base URL must use HTTP or HTTPS.".into());
    }
    if url.host_str().is_none() {
        return Err("Memory base URL requires a host.".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("Memory base URL cannot contain embedded credentials.".into());
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err("Memory base URL cannot contain query or fragment data.".into());
    }
    let path = format!("{}/", url.path().trim_end_matches('/'));
    url.set_path(&path);
    Ok(url)
}

fn validate_session_token(value: &str) -> Result<&str, String> {
    if value.is_empty() || value.len() > MAX_TOKEN_BYTES || value.chars().any(char::is_control) {
        return Err("Invalid Hermes session token.".into());
    }
    Ok(value)
}

fn validate_profile(value: &str) -> Result<&str, String> {
    if value.trim().is_empty()
        || value.len() > MAX_PROFILE_BYTES
        || value.chars().any(char::is_control)
    {
        return Err("Invalid Memory routing profile.".into());
    }
    Ok(value)
}

fn validate_provider(value: &str) -> Result<&str, String> {
    if value.trim().is_empty()
        || value.len() > MAX_PROVIDER_BYTES
        || value.chars().any(char::is_control)
    {
        return Err("Invalid memory provider.".into());
    }
    Ok(value)
}

fn encoded_provider(value: &str) -> Result<String, String> {
    let value = validate_provider(value)?;
    let mut url = Url::parse("https://hermes.invalid/")
        .map_err(|error| format!("Could not encode memory provider: {error}"))?;
    url.path_segments_mut()
        .map_err(|()| "Could not encode memory provider.".to_owned())?
        .push(value);
    Ok(url.path().trim_start_matches('/').to_owned())
}

fn validate_provider_values(values: &BTreeMap<String, String>) -> Result<(), String> {
    if values.len() > MAX_CONFIG_ENTRIES {
        return Err(format!(
            "Memory provider config cannot contain more than {MAX_CONFIG_ENTRIES} values."
        ));
    }
    for (key, value) in values {
        if key.trim().is_empty()
            || key.len() > MAX_CONFIG_KEY_BYTES
            || key.chars().any(char::is_control)
        {
            return Err("Invalid Memory provider config key.".into());
        }
        if value.len() > MAX_CONFIG_VALUE_BYTES || value.contains('\0') {
            return Err("Memory provider config value exceeds the safety limit.".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_status_preserves_deployment_prefix_and_keeps_profile_out_of_url() {
        let client = NativeMemoryClient::new(
            "https://gateway.example/hermes",
            Some("session-token"),
            Some("work"),
        )
        .expect("client");
        let spec = MemoryRequest::memory_status(Some("work")).expect("spec");
        let request = client.build_request(&spec).expect("request");

        assert_eq!(request.method(), Method::GET);
        assert_eq!(
            request.url().as_str(),
            "https://gateway.example/hermes/api/memory"
        );
        assert_eq!(
            request
                .headers()
                .get("x-hermes-session-token")
                .expect("token")
                .to_str()
                .expect("header text"),
            "session-token"
        );
        assert!(!request.url().as_str().contains("work"));
        assert!(!request.url().as_str().contains("session-token"));
        client
            .assert_routing_profile(spec.routing_profile.as_deref())
            .expect("matching route");
    }

    #[test]
    fn rejects_cross_profile_routing_before_dispatch() {
        let client = NativeMemoryClient::new("http://127.0.0.1:8000", None, Some("work"))
            .expect("client");
        let spec = MemoryRequest::curator_status(Some("personal")).expect("spec");
        let error = client
            .assert_routing_profile(spec.routing_profile.as_deref())
            .expect_err("cross-profile request must fail");
        assert!(error.contains("different backend profile"));
    }

    #[test]
    fn provider_config_uses_declared_surface_and_one_encoded_segment() {
        let client = NativeMemoryClient::new("http://127.0.0.1:8000/root", None, None)
            .expect("client");
        let spec = MemoryRequest::provider_config(
            None,
            "vendor/memory one",
            MemoryMethod::Get,
            None,
        )
        .expect("spec");
        let request = client.build_request(&spec).expect("request");

        assert_eq!(request.method(), Method::GET);
        assert_eq!(
            request.url().as_str(),
            "http://127.0.0.1:8000/root/api/memory/providers/vendor%2Fmemory%20one/config?surface=declared"
        );
    }

    #[test]
    fn mutations_match_legacy_method_path_and_body_contracts() {
        let reset = MemoryRequest::reset(Some("work"), MemoryResetTarget::Memory).expect("reset");
        assert_eq!(reset.method, MemoryMethod::Post);
        assert_eq!(reset.path, "/api/memory/reset");
        assert_eq!(reset.body, Some(json!({ "target": "memory" })));

        let pause = MemoryRequest::set_curator_paused(Some("work"), true).expect("pause");
        assert_eq!(pause.method, MemoryMethod::Put);
        assert_eq!(pause.path, "/api/curator/paused");
        assert_eq!(pause.body, Some(json!({ "paused": true })));

        let run = MemoryRequest::run_curator(Some("work")).expect("run");
        assert_eq!(run.method, MemoryMethod::Post);
        assert_eq!(run.path, "/api/curator/run");
        assert_eq!(run.body, Some(json!({})));
    }

    #[test]
    fn provider_config_write_is_bounded_and_returns_ok_contract() {
        let mut values = BTreeMap::new();
        values.insert("api_key".into(), "super-secret".into());
        validate_provider_values(&values).expect("values");
        let spec = MemoryRequest::provider_config(
            None,
            "mem0",
            MemoryMethod::Put,
            Some(json!({ "values": values })),
        )
        .expect("spec");
        let client = NativeMemoryClient::new("https://gateway.example", None, None).expect("client");
        let request = client.build_request(&spec).expect("request");
        assert_eq!(request.method(), Method::PUT);
        assert!(!request.url().as_str().contains("super-secret"));

        let response: MemoryMutationResponse =
            serde_json::from_value(json!({ "ok": true, "future": 1 })).expect("response");
        assert!(response.ok);
    }

    #[test]
    fn read_contracts_tolerate_future_fields_and_states() {
        let memory: MemoryStatusResponse = serde_json::from_value(json!({
            "active": "builtin",
            "providers": [{
                "name": "builtin",
                "description": "Built in",
                "configured": true,
                "future_provider_field": 7
            }],
            "builtin_files": { "memory": 12, "user": 34 },
            "future_top_level": true
        }))
        .expect("memory status");
        assert_eq!(memory.builtin_files.memory, 12);

        let oauth: MemoryProviderOAuthStatus = serde_json::from_value(json!({
            "auth": "oauth",
            "connected": false,
            "detail": "Waiting",
            "state": "future_pending_state",
            "future": "ignored"
        }))
        .expect("oauth status");
        assert_eq!(oauth.state, "future_pending_state");

        let curator: CuratorStatusResponse = serde_json::from_value(json!({
            "enabled": true,
            "paused": false,
            "interval_hours": 24,
            "last_run_at": null,
            "min_idle_hours": 2,
            "stale_after_days": 14,
            "archive_after_days": 30,
            "future": { "field": true }
        }))
        .expect("curator status");
        assert_eq!(curator.interval_hours, Some(24.0));
    }

    #[test]
    fn rejects_unsafe_bases_tokens_profiles_providers_and_values() {
        assert!(NativeMemoryClient::new("file:///tmp/hermes", None, None).is_err());
        assert!(NativeMemoryClient::new("https://user:pass@example.com", None, None).is_err());
        assert!(NativeMemoryClient::new("https://example.com/?token=x", None, None).is_err());
        assert!(NativeMemoryClient::new("https://example.com", Some("bad\ntoken"), None).is_err());
        assert!(NativeMemoryClient::new("https://example.com", None, Some("bad\nprofile")).is_err());
        assert!(
            MemoryRequest::provider_oauth(None, "bad\nprovider", "start", MemoryMethod::Post)
                .is_err()
        );

        let mut values = BTreeMap::new();
        values.insert("key".into(), "x".repeat(MAX_CONFIG_VALUE_BYTES + 1));
        assert!(validate_provider_values(&values).is_err());
    }
}