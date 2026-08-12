#![allow(dead_code)] // AG-07 service foundation; Dioxus integration is a later stage.

use std::time::Duration;

use reqwest::{Client, Method, Request};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use url::Url;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_TOKEN_BYTES: usize = 16 * 1024;
const MAX_NAME_BYTES: usize = 256;
const MAX_PROFILE_BYTES: usize = 256;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WebhookMethod {
    Get,
    Post,
    Put,
    Delete,
}

#[derive(Clone, Debug, PartialEq)]
struct WebhookRequest {
    method: WebhookMethod,
    path: String,
    profile: Option<String>,
    body: Option<Value>,
}

impl WebhookRequest {
    fn list(profile: Option<&str>) -> Result<Self, String> {
        build_spec(WebhookMethod::Get, &[], profile, None)
    }

    fn enable(profile: Option<&str>) -> Result<Self, String> {
        build_spec(WebhookMethod::Post, &["enable"], profile, None)
    }

    fn create(profile: Option<&str>, payload: &WebhookCreatePayload) -> Result<Self, String> {
        validate_webhook_name(&payload.name)?;
        let body = serde_json::to_value(payload)
            .map_err(|error| format!("Could not serialize webhook payload: {error}"))?;
        build_spec(WebhookMethod::Post, &[], profile, Some(body))
    }

    fn delete(profile: Option<&str>, name: &str) -> Result<Self, String> {
        validate_webhook_name(name)?;
        build_spec(WebhookMethod::Delete, &[name], profile, None)
    }

    fn set_enabled(profile: Option<&str>, name: &str, enabled: bool) -> Result<Self, String> {
        validate_webhook_name(name)?;
        build_spec(
            WebhookMethod::Put,
            &[name, "enabled"],
            profile,
            Some(json!({ "enabled": enabled })),
        )
    }
}

#[derive(Clone)]
pub struct NativeWebhookClient {
    client: Client,
    base_url: Url,
    session_token: Option<String>,
}

impl NativeWebhookClient {
    pub fn new(base_url: &str, session_token: Option<&str>) -> Result<Self, String> {
        let base_url = validate_base_url(base_url)?;
        let session_token = session_token
            .map(validate_session_token)
            .transpose()?
            .map(str::to_owned);
        let client = Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|error| format!("Could not build webhook HTTP client: {error}"))?;
        Ok(Self {
            client,
            base_url,
            session_token,
        })
    }

    pub async fn list(&self, profile: Option<&str>) -> Result<WebhooksResponse, String> {
        self.execute(WebhookRequest::list(profile)?).await
    }

    pub async fn enable(&self, profile: Option<&str>) -> Result<WebhookEnableResponse, String> {
        self.execute(WebhookRequest::enable(profile)?).await
    }

    pub async fn create(
        &self,
        profile: Option<&str>,
        payload: &WebhookCreatePayload,
    ) -> Result<WebhookCreateResponse, String> {
        self.execute(WebhookRequest::create(profile, payload)?).await
    }

    pub async fn delete(
        &self,
        profile: Option<&str>,
        name: &str,
    ) -> Result<WebhookMutationResponse, String> {
        self.execute(WebhookRequest::delete(profile, name)?).await
    }

    pub async fn set_enabled(
        &self,
        profile: Option<&str>,
        name: &str,
        enabled: bool,
    ) -> Result<WebhookEnabledResponse, String> {
        self.execute(WebhookRequest::set_enabled(profile, name, enabled)?)
            .await
    }

    async fn execute<T: DeserializeOwned>(&self, spec: WebhookRequest) -> Result<T, String> {
        let request = self.build_request(&spec)?;
        let response = self
            .client
            .execute(request)
            .await
            .map_err(|error| format!("Webhook request failed: {error}"))?;
        let status = response.status();
        if !status.is_success() {
            return Err(format!(
                "Webhook endpoint returned HTTP {}.",
                status.as_u16()
            ));
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_RESPONSE_BYTES)
        {
            return Err("Webhook response exceeded the 4 MiB safety limit.".into());
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|error| format!("Could not read webhook response: {error}"))?;
        if bytes.len() as u64 > MAX_RESPONSE_BYTES {
            return Err("Webhook response exceeded the 4 MiB safety limit.".into());
        }
        serde_json::from_slice(&bytes).map_err(|error| format!("Invalid webhook response: {error}"))
    }

    fn build_request(&self, spec: &WebhookRequest) -> Result<Request, String> {
        let url = endpoint(&self.base_url, spec)?;
        let method = match spec.method {
            WebhookMethod::Get => Method::GET,
            WebhookMethod::Post => Method::POST,
            WebhookMethod::Put => Method::PUT,
            WebhookMethod::Delete => Method::DELETE,
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
            .map_err(|error| format!("Could not build webhook request: {error}"))
    }
}

fn build_spec(
    method: WebhookMethod,
    suffix: &[&str],
    profile: Option<&str>,
    body: Option<Value>,
) -> Result<WebhookRequest, String> {
    let profile = profile
        .map(validate_profile)
        .transpose()?
        .map(str::to_owned);
    let mut url = Url::parse("https://hermes.invalid/api/webhooks")
        .map_err(|error| format!("Could not construct webhook path: {error}"))?;
    {
        let mut segments = url
            .path_segments_mut()
            .map_err(|()| "Could not construct webhook path.".to_owned())?;
        for segment in suffix {
            segments.push(segment);
        }
    }
    Ok(WebhookRequest {
        method,
        path: url.path().to_owned(),
        profile,
        body,
    })
}

fn validate_base_url(value: &str) -> Result<Url, String> {
    let mut url =
        Url::parse(value.trim()).map_err(|error| format!("Invalid webhook base URL: {error}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("Webhook base URL must use HTTP or HTTPS.".into());
    }
    if url.host_str().is_none() {
        return Err("Webhook base URL requires a host.".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("Webhook base URL cannot contain embedded credentials.".into());
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err("Webhook base URL cannot contain query or fragment data.".into());
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

fn validate_webhook_name(value: &str) -> Result<(), String> {
    if value.trim().is_empty()
        || value.len() > MAX_NAME_BYTES
        || value.chars().any(char::is_control)
    {
        return Err("Invalid webhook name.".into());
    }
    Ok(())
}

fn validate_profile(value: &str) -> Result<&str, String> {
    if value.trim().is_empty()
        || value.len() > MAX_PROFILE_BYTES
        || value.chars().any(char::is_control)
    {
        return Err("Invalid webhook profile.".into());
    }
    Ok(value)
}

fn endpoint(base_url: &Url, spec: &WebhookRequest) -> Result<Url, String> {
    let mut url = base_url
        .join(spec.path.trim_start_matches('/'))
        .map_err(|error| format!("Could not construct webhook endpoint: {error}"))?;
    if let Some(profile) = spec.profile.as_deref() {
        url.query_pairs_mut().append_pair("profile", profile);
    }
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_exact_profile_scoped_encoded_request_without_token_in_url() {
        let client =
            NativeWebhookClient::new("https://gateway.example/hermes", Some("session-token"))
                .expect("client");
        let spec = WebhookRequest::set_enabled(Some("work profile"), "my hook/one", false)
            .expect("request spec");
        let request = client.build_request(&spec).expect("request");

        assert_eq!(request.method(), Method::PUT);
        assert_eq!(
            request.url().as_str(),
            "https://gateway.example/hermes/api/webhooks/my%20hook%2Fone/enabled?profile=work+profile"
        );
        assert_eq!(
            request
                .headers()
                .get("x-hermes-session-token")
                .expect("token header")
                .to_str()
                .expect("header text"),
            "session-token"
        );
        assert!(!request.url().as_str().contains("session-token"));
    }

    #[test]
    fn preserves_deployment_prefix_for_list_and_create() {
        let client =
            NativeWebhookClient::new("http://127.0.0.1:8000/hermes", None).expect("client");
        let list = client
            .build_request(&WebhookRequest::list(None).expect("list spec"))
            .expect("list request");
        assert_eq!(
            list.url().as_str(),
            "http://127.0.0.1:8000/hermes/api/webhooks"
        );
        assert_eq!(list.method(), Method::GET);

        let create = client
            .build_request(
                &WebhookRequest::create(
                    None,
                    &WebhookCreatePayload {
                        name: "github-push".into(),
                        ..WebhookCreatePayload::default()
                    },
                )
                .expect("create spec"),
            )
            .expect("create request");
        assert_eq!(
            create.url().as_str(),
            "http://127.0.0.1:8000/hermes/api/webhooks"
        );
        assert_eq!(create.method(), Method::POST);
    }

    #[test]
    fn read_contract_never_contains_a_raw_secret_field() {
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

    #[test]
    fn rejects_credentialed_or_non_http_bases_and_control_inputs() {
        assert!(NativeWebhookClient::new("file:///tmp/hermes", None).is_err());
        assert!(NativeWebhookClient::new("https://user:pass@example.com", None).is_err());
        assert!(NativeWebhookClient::new("https://example.com?token=bad", None).is_err());
        assert!(NativeWebhookClient::new("https://example.com", Some("bad\ntoken")).is_err());
        assert!(WebhookRequest::delete(None, "bad\nname").is_err());
        assert!(WebhookRequest::list(Some("bad\nprofile")).is_err());
    }
}
