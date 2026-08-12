use std::time::Duration;

use reqwest::Method;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use thiserror::Error;
use url::Url;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_NAME_BYTES: usize = 256;
const MAX_PROFILE_BYTES: usize = 256;
const MAX_TOKEN_BYTES: usize = 16 * 1024;

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct WebhookRoute {
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub deliver: String,
    #[serde(default)]
    pub deliver_only: bool,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub events: Vec<String>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub secret_set: bool,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub url: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct WebhooksResponse {
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub subscriptions: Vec<WebhookRoute>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct WebhookCreatePayload {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deliver: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deliver_chat_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deliver_only: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub events: Option<Vec<String>>,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skills: Option<Vec<String>>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct WebhookCreateResponse {
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub deliver: String,
    #[serde(default)]
    pub deliver_only: bool,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub events: Vec<String>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub secret_set: bool,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub url: String,
    pub secret: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct WebhookEnableResponse {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub needs_restart: bool,
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub platform: String,
    #[serde(default)]
    pub restart_action: Option<String>,
    #[serde(default)]
    pub restart_error: Option<String>,
    #[serde(default)]
    pub restart_pid: Option<u64>,
    #[serde(default)]
    pub restart_started: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct WebhookEnabledResponse {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub ok: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct WebhookMutationResponse {
    #[serde(default)]
    pub ok: bool,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum WebhookError {
    #[error("invalid webhook base URL: {0}")]
    InvalidUrl(String),
    #[error("invalid webhook input: {0}")]
    InvalidInput(String),
    #[error("webhook transport error: {0}")]
    Transport(String),
    #[error("webhook endpoint returned HTTP {0}")]
    HttpStatus(u16),
    #[error("webhook response exceeded {MAX_RESPONSE_BYTES} bytes")]
    ResponseTooLarge,
    #[error("invalid webhook response: {0}")]
    Protocol(String),
}

#[derive(Clone)]
pub struct WebhookClient {
    client: reqwest::Client,
    base_url: Url,
    session_token: Option<String>,
}

impl WebhookClient {
    /// Build a bounded Hermes Agent webhook REST client.
    ///
    /// `base_url` is the selected Agent HTTP base, including any deployment
    /// prefix such as `/hermes`. Authentication is kept in the request header,
    /// never in the URL.
    pub fn new(base_url: &str, session_token: Option<&str>) -> Result<Self, WebhookError> {
        let base_url = validate_base_url(base_url)?;
        let session_token = session_token
            .map(validate_session_token)
            .transpose()?
            .map(str::to_owned);
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|error| WebhookError::Transport(error.to_string()))?;
        Ok(Self {
            client,
            base_url,
            session_token,
        })
    }

    pub async fn list(&self, profile: Option<&str>) -> Result<WebhooksResponse, WebhookError> {
        self.request(Method::GET, &[], profile, None).await
    }

    pub async fn enable(
        &self,
        profile: Option<&str>,
    ) -> Result<WebhookEnableResponse, WebhookError> {
        self.request(Method::POST, &["enable"], profile, None).await
    }

    pub async fn create(
        &self,
        profile: Option<&str>,
        payload: &WebhookCreatePayload,
    ) -> Result<WebhookCreateResponse, WebhookError> {
        validate_webhook_name(&payload.name)?;
        let body = serde_json::to_value(payload)
            .map_err(|error| WebhookError::Protocol(error.to_string()))?;
        self.request(Method::POST, &[], profile, Some(body)).await
    }

    pub async fn delete(
        &self,
        profile: Option<&str>,
        name: &str,
    ) -> Result<WebhookMutationResponse, WebhookError> {
        validate_webhook_name(name)?;
        self.request(Method::DELETE, &[name], profile, None).await
    }

    pub async fn set_enabled(
        &self,
        profile: Option<&str>,
        name: &str,
        enabled: bool,
    ) -> Result<WebhookEnabledResponse, WebhookError> {
        validate_webhook_name(name)?;
        self.request(
            Method::PUT,
            &[name, "enabled"],
            profile,
            Some(json!({ "enabled": enabled })),
        )
        .await
    }

    async fn request<T: DeserializeOwned>(
        &self,
        method: Method,
        suffix: &[&str],
        profile: Option<&str>,
        body: Option<Value>,
    ) -> Result<T, WebhookError> {
        let url = self.endpoint(suffix, profile)?;
        let mut request = self.client.request(method, url);
        if let Some(token) = self.session_token.as_deref() {
            request = request.header("x-hermes-session-token", token);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request
            .send()
            .await
            .map_err(|error| WebhookError::Transport(error.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(WebhookError::HttpStatus(status.as_u16()));
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_RESPONSE_BYTES)
        {
            return Err(WebhookError::ResponseTooLarge);
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|error| WebhookError::Transport(error.to_string()))?;
        if bytes.len() as u64 > MAX_RESPONSE_BYTES {
            return Err(WebhookError::ResponseTooLarge);
        }
        serde_json::from_slice(&bytes).map_err(|error| WebhookError::Protocol(error.to_string()))
    }

    fn endpoint(&self, suffix: &[&str], profile: Option<&str>) -> Result<Url, WebhookError> {
        let profile = profile.map(validate_profile).transpose()?;
        let mut url = self.base_url.clone();
        {
            let mut segments = url.path_segments_mut().map_err(|()| {
                WebhookError::InvalidUrl("URL cannot contain path segments".into())
            })?;
            segments.pop_if_empty();
            segments.push("api").push("webhooks");
            for segment in suffix {
                segments.push(segment);
            }
        }
        if let Some(profile) = profile {
            url.query_pairs_mut().append_pair("profile", profile);
        }
        Ok(url)
    }
}

fn validate_base_url(value: &str) -> Result<Url, WebhookError> {
    let value = value.trim();
    let mut url = Url::parse(value).map_err(|error| WebhookError::InvalidUrl(error.to_string()))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(WebhookError::InvalidUrl(
            "scheme must be http or https".into(),
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(WebhookError::InvalidUrl(
            "embedded credentials are not allowed".into(),
        ));
    }
    if url.host_str().is_none() {
        return Err(WebhookError::InvalidUrl("host is required".into()));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(WebhookError::InvalidUrl(
            "base URL cannot contain query or fragment data".into(),
        ));
    }
    url.set_query(None);
    url.set_fragment(None);
    Ok(url)
}

fn validate_session_token(value: &str) -> Result<&str, WebhookError> {
    if value.is_empty() || value.len() > MAX_TOKEN_BYTES || value.chars().any(char::is_control) {
        return Err(WebhookError::InvalidInput(
            "invalid Hermes session token".into(),
        ));
    }
    Ok(value)
}

fn validate_webhook_name(value: &str) -> Result<(), WebhookError> {
    if value.trim().is_empty()
        || value.len() > MAX_NAME_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(WebhookError::InvalidInput("invalid webhook name".into()));
    }
    Ok(())
}

fn validate_profile(value: &str) -> Result<&str, WebhookError> {
    if value.trim().is_empty()
        || value.len() > MAX_PROFILE_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(WebhookError::InvalidInput("invalid profile".into()));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
    };

    use super::*;

    async fn read_request(stream: &mut TcpStream) -> String {
        let mut bytes = Vec::new();
        let mut buffer = [0_u8; 2048];
        let header_end = loop {
            let read = stream.read(&mut buffer).await.expect("read request");
            assert!(read > 0, "connection closed before request headers");
            bytes.extend_from_slice(&buffer[..read]);
            if let Some(index) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
                break index + 4;
            }
            assert!(
                bytes.len() < 64 * 1024,
                "request headers are unexpectedly large"
            );
        };
        let headers = String::from_utf8_lossy(&bytes[..header_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                line.split_once(':').and_then(|(name, value)| {
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
            })
            .unwrap_or(0);
        while bytes.len() < header_end + content_length {
            let read = stream.read(&mut buffer).await.expect("read request body");
            assert!(read > 0, "connection closed before request body");
            bytes.extend_from_slice(&buffer[..read]);
        }
        String::from_utf8(bytes).expect("request is utf-8")
    }

    async fn spawn_server(
        responses: Vec<&'static str>,
    ) -> (String, tokio::task::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind server");
        let address = listener.local_addr().expect("local address");
        let task = tokio::spawn(async move {
            let mut requests = Vec::new();
            for body in responses {
                let (mut stream, _) = listener.accept().await.expect("accept request");
                requests.push(read_request(&mut stream).await);
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream
                    .write_all(response.as_bytes())
                    .await
                    .expect("write response");
            }
            requests
        });
        (format!("http://{address}/hermes"), task)
    }

    #[test]
    fn validates_base_urls_tokens_names_and_profiles() {
        assert!(WebhookClient::new("http://127.0.0.1:8000/hermes", Some("token")).is_ok());
        assert!(WebhookClient::new("file:///tmp/hermes", Some("token")).is_err());
        assert!(WebhookClient::new("https://user:pass@example.com", Some("token")).is_err());
        assert!(WebhookClient::new("https://example.com?token=bad", Some("token")).is_err());
        assert!(WebhookClient::new("https://example.com", Some("bad\ntoken")).is_err());
        assert!(validate_webhook_name("my hook/one").is_ok());
        assert!(validate_webhook_name("bad\nname").is_err());
        assert!(validate_profile("work profile").is_ok());
        assert!(validate_profile("bad\nprofile").is_err());
    }

    #[test]
    fn list_contract_cannot_serialize_a_webhook_secret() {
        let route: WebhookRoute = serde_json::from_value(json!({
            "name": "github-push",
            "secret": "must-not-cross-read-boundary",
            "secret_set": true
        }))
        .expect("decode route");
        let encoded = serde_json::to_string(&route).expect("encode route");
        assert!(route.secret_set);
        assert!(!encoded.contains("must-not-cross-read-boundary"));
        assert!(!encoded.contains("\"secret\""));
    }

    #[tokio::test]
    async fn mirrors_webhook_rest_contract_and_profile_scope() {
        let (base_url, server) = spawn_server(vec![
            r#"{"base_url":"http://127.0.0.1:9000","enabled":true,"subscriptions":[]}"#,
            r#"{"enabled":true,"needs_restart":true,"ok":true,"platform":"webhook"}"#,
            r#"{"name":"github-push","url":"http://127.0.0.1:9000/github-push","secret":"one-time-secret","secret_set":true}"#,
            r#"{"enabled":false,"name":"my hook/one","ok":true}"#,
            r#"{"ok":true}"#,
        ])
        .await;
        let client = WebhookClient::new(&base_url, Some("session-token")).expect("client");
        let profile = Some("work profile");

        let listed = client.list(profile).await.expect("list webhooks");
        assert!(listed.enabled);
        let enabled = client.enable(profile).await.expect("enable webhooks");
        assert!(enabled.ok);
        let created = client
            .create(
                profile,
                &WebhookCreatePayload {
                    deliver: Some("telegram".into()),
                    deliver_only: Some(true),
                    description: Some("push events".into()),
                    events: Some(vec!["push".into()]),
                    name: "github-push".into(),
                    prompt: Some("summarize the push".into()),
                    ..WebhookCreatePayload::default()
                },
            )
            .await
            .expect("create webhook");
        assert_eq!(created.secret, "one-time-secret");
        let toggled = client
            .set_enabled(profile, "my hook/one", false)
            .await
            .expect("toggle webhook");
        assert!(!toggled.enabled);
        assert!(
            client
                .delete(profile, "my hook/one")
                .await
                .expect("delete webhook")
                .ok
        );

        let requests = server.await.expect("server task");
        assert_eq!(requests.len(), 5);
        for request in &requests {
            assert!(request.contains("x-hermes-session-token: session-token"));
        }
        assert!(requests[0].starts_with("GET /hermes/api/webhooks?profile=work+profile HTTP/1.1"));
        assert!(
            requests[1]
                .starts_with("POST /hermes/api/webhooks/enable?profile=work+profile HTTP/1.1")
        );
        assert!(requests[2].starts_with("POST /hermes/api/webhooks?profile=work+profile HTTP/1.1"));
        let create_body = requests[2].split_once("\r\n\r\n").expect("create body").1;
        let create_json: Value = serde_json::from_str(create_body).expect("create json");
        assert_eq!(create_json["name"], "github-push");
        assert_eq!(create_json["events"], json!(["push"]));
        assert_eq!(create_json["deliver_only"], true);
        assert!(requests[3].starts_with(
            "PUT /hermes/api/webhooks/my%20hook%2Fone/enabled?profile=work+profile HTTP/1.1"
        ));
        assert!(requests[3].ends_with("{\"enabled\":false}"));
        assert!(requests[4].starts_with(
            "DELETE /hermes/api/webhooks/my%20hook%2Fone?profile=work+profile HTTP/1.1"
        ));
    }
}
