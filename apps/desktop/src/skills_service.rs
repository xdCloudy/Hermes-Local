#![allow(dead_code)] // AG-01 service foundation; Dioxus Skills/Hub surfaces are later stages.

use std::{collections::BTreeMap, time::Duration};

use reqwest::{Client, Method, Request};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use url::Url;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const HUB_REQUEST_TIMEOUT: Duration = Duration::from_secs(45);
const MAX_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_TOKEN_BYTES: usize = 16 * 1024;
const MAX_PROFILE_BYTES: usize = 256;
const MAX_NAME_BYTES: usize = 512;
const MAX_IDENTIFIER_BYTES: usize = 4096;
const MAX_SEARCH_BYTES: usize = 4096;
const MAX_SOURCE_BYTES: usize = 256;
const MAX_SEARCH_LIMIT: u32 = 1000;
const MAX_ACTION_LINES: u32 = 5000;

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SkillInfo {
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
pub struct ActionResponse {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub pid: u64,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ActionStatusResponse {
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
pub struct SkillToggleResponse {
    #[serde(default)]
    pub ok: bool,
    pub name: String,
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Clone)]
pub struct NativeSkillsClient {
    client: Client,
    base_url: Url,
    session_token: Option<String>,
    profile: Option<String>,
}

impl NativeSkillsClient {
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
            .map_err(|error| format!("Could not build Skills HTTP client: {error}"))?;
        Ok(Self {
            client,
            base_url,
            session_token,
            profile,
        })
    }

    pub async fn list(&self) -> Result<Vec<SkillInfo>, String> {
        self.execute(Method::GET, "/api/skills", &[], None, REQUEST_TIMEOUT)
            .await
    }

    pub async fn set_enabled(
        &self,
        name: &str,
        enabled: bool,
    ) -> Result<SkillToggleResponse, String> {
        let name = validate_name(name, "skill name")?;
        self.execute(
            Method::PUT,
            "/api/skills/toggle",
            &[],
            Some(json!({ "name": name, "enabled": enabled })),
            REQUEST_TIMEOUT,
        )
        .await
    }

    pub async fn hub_sources(&self) -> Result<SkillHubSourcesResponse, String> {
        self.execute(
            Method::GET,
            "/api/skills/hub/sources",
            &[],
            None,
            HUB_REQUEST_TIMEOUT,
        )
        .await
    }

    pub async fn hub_search(
        &self,
        query: &str,
        source: &str,
        limit: u32,
    ) -> Result<SkillHubSearchResponse, String> {
        let query = validate_text(query, MAX_SEARCH_BYTES, "Hub search query", true)?;
        let source = validate_text(source, MAX_SOURCE_BYTES, "Hub source", false)?;
        if limit == 0 || limit > MAX_SEARCH_LIMIT {
            return Err(format!(
                "Hub search limit must be between 1 and {MAX_SEARCH_LIMIT}."
            ));
        }
        let limit_text = limit.to_string();
        self.execute(
            Method::GET,
            "/api/skills/hub/search",
            &[("q", query), ("source", source), ("limit", &limit_text)],
            None,
            HUB_REQUEST_TIMEOUT,
        )
        .await
    }

    pub async fn hub_preview(&self, identifier: &str) -> Result<SkillHubPreview, String> {
        let identifier = validate_identifier(identifier)?;
        self.execute(
            Method::GET,
            "/api/skills/hub/preview",
            &[("identifier", identifier)],
            None,
            HUB_REQUEST_TIMEOUT,
        )
        .await
    }

    pub async fn hub_scan(&self, identifier: &str) -> Result<SkillHubScanResult, String> {
        let identifier = validate_identifier(identifier)?;
        self.execute(
            Method::GET,
            "/api/skills/hub/scan",
            &[("identifier", identifier)],
            None,
            HUB_REQUEST_TIMEOUT,
        )
        .await
    }

    pub async fn hub_install(&self, identifier: &str) -> Result<ActionResponse, String> {
        let identifier = validate_identifier(identifier)?;
        self.execute(
            Method::POST,
            "/api/skills/hub/install",
            &[],
            Some(json!({ "identifier": identifier })),
            REQUEST_TIMEOUT,
        )
        .await
    }

    pub async fn hub_uninstall(&self, name: &str) -> Result<ActionResponse, String> {
        let name = validate_name(name, "skill name")?;
        self.execute(
            Method::POST,
            "/api/skills/hub/uninstall",
            &[],
            Some(json!({ "name": name })),
            REQUEST_TIMEOUT,
        )
        .await
    }

    pub async fn hub_update(&self) -> Result<ActionResponse, String> {
        self.execute(
            Method::POST,
            "/api/skills/hub/update",
            &[],
            Some(json!({})),
            REQUEST_TIMEOUT,
        )
        .await
    }

    pub async fn action_status(
        &self,
        name: &str,
        lines: u32,
    ) -> Result<ActionStatusResponse, String> {
        let name = validate_name(name, "action name")?;
        if lines == 0 || lines > MAX_ACTION_LINES {
            return Err(format!(
                "Action log line count must be between 1 and {MAX_ACTION_LINES}."
            ));
        }
        let mut url = self.dynamic_url("api/actions", name, &["status"])?;
        url.query_pairs_mut()
            .append_pair("lines", &lines.to_string());
        self.execute_url(Method::GET, url, None, REQUEST_TIMEOUT)
            .await
    }

    async fn execute<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, &str)],
        body: Option<Value>,
        timeout: Duration,
    ) -> Result<T, String> {
        let url = self.url(path, query)?;
        self.execute_url(method, url, body, timeout).await
    }

    async fn execute_url<T: DeserializeOwned>(
        &self,
        method: Method,
        url: Url,
        body: Option<Value>,
        timeout: Duration,
    ) -> Result<T, String> {
        let request = self.build_request(method, url, body, timeout)?;
        let response = self
            .client
            .execute(request)
            .await
            .map_err(|error| format!("Skills request failed: {error}"))?;
        let status = response.status();
        if !status.is_success() {
            return Err(format!(
                "Skills endpoint returned HTTP {}.",
                status.as_u16()
            ));
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_RESPONSE_BYTES)
        {
            return Err("Skills response exceeded the 4 MiB safety limit.".into());
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|error| format!("Could not read Skills response: {error}"))?;
        if bytes.len() as u64 > MAX_RESPONSE_BYTES {
            return Err("Skills response exceeded the 4 MiB safety limit.".into());
        }
        serde_json::from_slice(&bytes).map_err(|error| format!("Invalid Skills response: {error}"))
    }

    fn url(&self, path: &str, query: &[(&str, &str)]) -> Result<Url, String> {
        let mut url = self
            .base_url
            .join(path.trim_start_matches('/'))
            .map_err(|error| format!("Could not construct Skills endpoint: {error}"))?;
        {
            let mut pairs = url.query_pairs_mut();
            if let Some(profile) = self.profile.as_deref() {
                pairs.append_pair("profile", profile);
            }
            for (key, value) in query {
                pairs.append_pair(key, value);
            }
        }
        Ok(url)
    }

    fn dynamic_url(&self, prefix: &str, segment: &str, suffix: &[&str]) -> Result<Url, String> {
        let mut url = self
            .base_url
            .join(prefix.trim_start_matches('/'))
            .map_err(|error| format!("Could not construct Skills endpoint: {error}"))?;
        {
            let mut segments = url
                .path_segments_mut()
                .map_err(|_| "Skills base URL cannot hold path segments.".to_owned())?;
            segments.pop_if_empty();
            segments.push(segment);
            for item in suffix {
                segments.push(item);
            }
        }
        if let Some(profile) = self.profile.as_deref() {
            url.query_pairs_mut().append_pair("profile", profile);
        }
        Ok(url)
    }

    fn build_request(
        &self,
        method: Method,
        url: Url,
        body: Option<Value>,
        timeout: Duration,
    ) -> Result<Request, String> {
        let mut request = self.client.request(method, url).timeout(timeout);
        if let Some(token) = self.session_token.as_deref() {
            request = request.header("x-hermes-session-token", token);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        request
            .build()
            .map_err(|error| format!("Could not build Skills request: {error}"))
    }
}

fn validate_base_url(value: &str) -> Result<Url, String> {
    let mut url =
        Url::parse(value.trim()).map_err(|error| format!("Invalid Skills base URL: {error}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("Skills base URL must use HTTP or HTTPS.".into());
    }
    if url.host_str().is_none() {
        return Err("Skills base URL requires a host.".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("Skills base URL must not contain credentials.".into());
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err("Skills base URL must not contain a query or fragment.".into());
    }
    if !url.path().ends_with('/') {
        let path = format!("{}/", url.path());
        url.set_path(&path);
    }
    Ok(url)
}

fn validate_session_token(value: &str) -> Result<&str, String> {
    validate_text(value.trim(), MAX_TOKEN_BYTES, "Skills session token", false)
}

fn validate_profile(value: &str) -> Result<&str, String> {
    let value = validate_text(value.trim(), MAX_PROFILE_BYTES, "Skills profile", false)?;
    if value
        .chars()
        .any(|character| matches!(character, '/' | '\\' | '?' | '#'))
    {
        return Err("Skills profile is invalid.".into());
    }
    Ok(value)
}

fn validate_name<'a>(value: &'a str, field: &str) -> Result<&'a str, String> {
    validate_text(value.trim(), MAX_NAME_BYTES, field, false)
}

fn validate_identifier(value: &str) -> Result<&str, String> {
    validate_text(
        value.trim(),
        MAX_IDENTIFIER_BYTES,
        "Hub skill identifier",
        false,
    )
}

fn validate_text<'a>(
    value: &'a str,
    max_bytes: usize,
    field: &str,
    allow_empty: bool,
) -> Result<&'a str, String> {
    if (!allow_empty && value.is_empty())
        || value.len() > max_bytes
        || value.chars().any(char::is_control)
    {
        return Err(format!("{field} is invalid."));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client(profile: Option<&str>) -> NativeSkillsClient {
        NativeSkillsClient::new(
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
    fn local_skill_contract_is_profile_scoped() {
        let client = client(Some("work profile"));
        let list_url = client.url("/api/skills", &[]).expect("list URL");
        assert_eq!(
            list_url.as_str(),
            "https://gateway.example/hermes/api/skills?profile=work+profile"
        );

        let toggle = client
            .build_request(
                Method::PUT,
                client.url("/api/skills/toggle", &[]).expect("toggle URL"),
                Some(json!({ "name": "coding-agent", "enabled": false })),
                REQUEST_TIMEOUT,
            )
            .expect("toggle request");
        assert_eq!(toggle.method(), Method::PUT);
        assert_eq!(
            toggle
                .headers()
                .get("x-hermes-session-token")
                .and_then(|value| value.to_str().ok()),
            Some("session-secret")
        );
        assert_eq!(
            body(&toggle),
            json!({ "name": "coding-agent", "enabled": false })
        );
    }

    #[test]
    fn hub_browse_contracts_match_electron_queries_and_timeout() {
        let client = client(Some("work profile"));
        let search = client
            .url(
                "/api/skills/hub/search",
                &[("q", "network tools"), ("source", "all"), ("limit", "20")],
            )
            .expect("search URL");
        assert_eq!(search.path(), "/hermes/api/skills/hub/search");
        let pairs = search.query_pairs().collect::<BTreeMap<_, _>>();
        assert_eq!(
            pairs.get("profile").map(|value| value.as_ref()),
            Some("work profile")
        );
        assert_eq!(
            pairs.get("q").map(|value| value.as_ref()),
            Some("network tools")
        );
        assert_eq!(pairs.get("source").map(|value| value.as_ref()), Some("all"));
        assert_eq!(pairs.get("limit").map(|value| value.as_ref()), Some("20"));
        assert_eq!(HUB_REQUEST_TIMEOUT, Duration::from_secs(45));

        let preview = client
            .url(
                "/api/skills/hub/preview",
                &[("identifier", "github:owner/repo")],
            )
            .expect("preview URL");
        assert!(
            preview
                .as_str()
                .contains("identifier=github%3Aowner%2Frepo")
        );
    }

    #[test]
    fn hub_mutations_use_exact_bodies_and_action_path_encoding() {
        let client = client(None);
        let install = client
            .build_request(
                Method::POST,
                client
                    .url("/api/skills/hub/install", &[])
                    .expect("install URL"),
                Some(json!({ "identifier": "github:owner/repo" })),
                REQUEST_TIMEOUT,
            )
            .expect("install request");
        assert_eq!(body(&install), json!({ "identifier": "github:owner/repo" }));

        let uninstall = client
            .build_request(
                Method::POST,
                client
                    .url("/api/skills/hub/uninstall", &[])
                    .expect("uninstall URL"),
                Some(json!({ "name": "repo helper" })),
                REQUEST_TIMEOUT,
            )
            .expect("uninstall request");
        assert_eq!(body(&uninstall), json!({ "name": "repo helper" }));

        let action = client
            .dynamic_url("api/actions", "install owner/repo", &["status"])
            .expect("action URL");
        assert_eq!(
            action.path(),
            "/hermes/api/actions/install%20owner%2Frepo/status"
        );
    }

    #[test]
    fn unsafe_transport_and_unbounded_inputs_fail_closed() {
        assert!(NativeSkillsClient::new("file:///tmp", None, None).is_err());
        assert!(NativeSkillsClient::new("https://u:p@example.test/", None, None).is_err());
        assert!(NativeSkillsClient::new("https://example.test/?secret=x", None, None).is_err());
        assert!(
            NativeSkillsClient::new("https://example.test/", Some("bad\ntoken"), None).is_err()
        );
        assert!(NativeSkillsClient::new("https://example.test/", None, Some("../other")).is_err());
        assert!(validate_identifier("").is_err());
        assert!(validate_name("bad\nname", "skill name").is_err());
    }

    #[test]
    fn hub_reads_ignore_future_agent_fields() {
        let response: SkillHubSourcesResponse = serde_json::from_value(json!({
            "sources": [{ "id": "official", "label": "Official", "future": true }],
            "index_available": true,
            "featured": [{
                "name": "Network helper",
                "description": "",
                "source": "official",
                "identifier": "official:network",
                "trust_level": "trusted",
                "repo": null,
                "tags": [],
                "future_score": 99
            }],
            "installed": {},
            "future_catalog": {}
        }))
        .expect("sources response");
        assert_eq!(response.sources.len(), 1);
        assert_eq!(response.featured[0].identifier, "official:network");
    }
}
