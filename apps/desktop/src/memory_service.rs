#![allow(dead_code)] // AG-04 service foundation; Dioxus Memory/Curator surfaces are later stages.

use std::time::Duration;

use reqwest::{Client, Method, Request};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use url::Url;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_TOKEN_BYTES: usize = 16 * 1024;
const MAX_PROFILE_BYTES: usize = 256;

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct MemoryProviderInfo {
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
    pub providers: Vec<MemoryProviderInfo>,
    #[serde(default)]
    pub builtin_files: MemoryBuiltinFiles,
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
pub struct MemoryResetResponse {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub deleted: Vec<String>,
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

#[derive(Clone)]
pub struct NativeMemoryClient {
    client: Client,
    base_url: Url,
    session_token: Option<String>,
    profile: Option<String>,
}

impl NativeMemoryClient {
    pub fn new(
        base_url: &str,
        session_token: Option<&str>,
        profile: Option<&str>,
    ) -> Result<Self, String> {
        let base_url = validate_base_url(base_url)?;
        let session_token = session_token
            .map(validate_session_token)
            .transpose()?
            .map(str::to_owned);
        let profile = profile
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
            profile,
        })
    }

    pub async fn status(&self) -> Result<MemoryStatusResponse, String> {
        self.execute(Method::GET, "/api/memory", None).await
    }

    pub async fn reset(&self, target: MemoryResetTarget) -> Result<MemoryResetResponse, String> {
        self.execute(
            Method::POST,
            "/api/memory/reset",
            Some(json!({ "target": target.as_str() })),
        )
        .await
    }

    pub async fn curator_status(&self) -> Result<CuratorStatusResponse, String> {
        self.execute(Method::GET, "/api/curator", None).await
    }

    pub async fn set_curator_paused(&self, paused: bool) -> Result<CuratorPauseResponse, String> {
        self.execute(
            Method::PUT,
            "/api/curator/paused",
            Some(json!({ "paused": paused })),
        )
        .await
    }

    pub async fn run_curator(&self) -> Result<ActionResponse, String> {
        self.execute(Method::POST, "/api/curator/run", Some(json!({})))
            .await
    }

    async fn execute<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<T, String> {
        let request = self.build_request(method, path, body)?;
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

    fn build_request(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<Request, String> {
        let mut url = self
            .base_url
            .join(path.trim_start_matches('/'))
            .map_err(|error| format!("Could not construct Memory endpoint: {error}"))?;
        if let Some(profile) = self.profile.as_deref() {
            url.query_pairs_mut().append_pair("profile", profile);
        }
        let mut request = self.client.request(method, url).timeout(REQUEST_TIMEOUT);
        if let Some(token) = self.session_token.as_deref() {
            request = request.header("x-hermes-session-token", token);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        request
            .build()
            .map_err(|error| format!("Could not build Memory request: {error}"))
    }
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
        return Err("Memory base URL must not contain credentials.".into());
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err("Memory base URL must not contain a query or fragment.".into());
    }
    if !url.path().ends_with('/') {
        let path = format!("{}/", url.path());
        url.set_path(&path);
    }
    Ok(url)
}

fn validate_session_token(value: &str) -> Result<&str, String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > MAX_TOKEN_BYTES
        || value.chars().any(char::is_control)
    {
        return Err("Memory session token is invalid.".into());
    }
    Ok(value)
}

fn validate_profile(value: &str) -> Result<&str, String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > MAX_PROFILE_BYTES
        || value
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\' | '?' | '#'))
    {
        return Err("Memory profile is invalid.".into());
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client(profile: Option<&str>) -> NativeMemoryClient {
        NativeMemoryClient::new(
            "https://gateway.example/hermes/",
            Some("session-secret"),
            profile,
        )
        .expect("client")
    }

    fn body(request: &Request) -> Value {
        serde_json::from_slice(
            request
                .body()
                .and_then(reqwest::Body::as_bytes)
                .expect("JSON request body"),
        )
        .expect("valid JSON")
    }

    #[test]
    fn exact_memory_and_curator_contracts_are_profile_scoped() {
        let client = client(Some("work profile"));
        let status = client
            .build_request(Method::GET, "/api/memory", None)
            .expect("memory status");
        assert_eq!(status.method(), Method::GET);
        assert_eq!(
            status.url().as_str(),
            "https://gateway.example/hermes/api/memory?profile=work+profile"
        );
        assert_eq!(
            status
                .headers()
                .get("x-hermes-session-token")
                .and_then(|value| value.to_str().ok()),
            Some("session-secret")
        );

        let reset = client
            .build_request(
                Method::POST,
                "/api/memory/reset",
                Some(json!({ "target": MemoryResetTarget::All.as_str() })),
            )
            .expect("memory reset");
        assert_eq!(reset.method(), Method::POST);
        assert_eq!(body(&reset), json!({ "target": "all" }));

        let curator = client
            .build_request(Method::GET, "/api/curator", None)
            .expect("curator status");
        assert_eq!(curator.method(), Method::GET);

        let pause = client
            .build_request(
                Method::PUT,
                "/api/curator/paused",
                Some(json!({ "paused": true })),
            )
            .expect("curator pause");
        assert_eq!(pause.method(), Method::PUT);
        assert_eq!(body(&pause), json!({ "paused": true }));

        let run = client
            .build_request(Method::POST, "/api/curator/run", Some(json!({})))
            .expect("curator run");
        assert_eq!(run.method(), Method::POST);
        assert_eq!(body(&run), json!({}));
    }

    #[test]
    fn reset_target_is_closed_over_the_electron_contract() {
        assert_eq!(MemoryResetTarget::All.as_str(), "all");
        assert_eq!(MemoryResetTarget::Memory.as_str(), "memory");
        assert_eq!(MemoryResetTarget::User.as_str(), "user");
    }

    #[test]
    fn base_url_and_auth_validation_fail_closed() {
        assert!(NativeMemoryClient::new("file:///tmp", None, None).is_err());
        assert!(
            NativeMemoryClient::new("https://user:pass@example.test/", None, None).is_err()
        );
        assert!(
            NativeMemoryClient::new("https://example.test/?token=secret", None, None).is_err()
        );
        assert!(NativeMemoryClient::new("https://example.test/", Some("bad\ntoken"), None).is_err());
        assert!(
            NativeMemoryClient::new("https://example.test/", None, Some("../other")).is_err()
        );
    }

    #[test]
    fn read_contracts_ignore_future_agent_fields() {
        let memory: MemoryStatusResponse = serde_json::from_value(json!({
            "active": "builtin",
            "providers": [{
                "name": "builtin",
                "description": "Local memory",
                "configured": true,
                "future": "field"
            }],
            "builtin_files": { "memory": 123, "user": 45 },
            "future_status": { "ok": true }
        }))
        .expect("memory response");
        assert_eq!(memory.active, "builtin");
        assert_eq!(memory.builtin_files.memory, 123);

        let curator: CuratorStatusResponse = serde_json::from_value(json!({
            "enabled": true,
            "paused": false,
            "interval_hours": 12,
            "last_run_at": null,
            "min_idle_hours": 2,
            "stale_after_days": 14,
            "archive_after_days": 30,
            "future_metric": 99
        }))
        .expect("curator response");
        assert!(curator.enabled);
        assert_eq!(curator.interval_hours, Some(12.0));
    }

    #[test]
    fn no_profile_does_not_invent_a_query_scope() {
        let request = client(None)
            .build_request(Method::GET, "/api/memory", None)
            .expect("request");
        assert_eq!(request.url().query(), None);
    }
}
