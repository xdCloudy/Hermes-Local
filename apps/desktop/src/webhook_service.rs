#![allow(dead_code)] // AG-07 service foundation; Dioxus integration is a later stage.

use std::time::Duration;

use hermes_agent_client::webhooks::{
    WebhookCreatePayload, WebhookCreateResponse, WebhookEnableResponse, WebhookEnabledResponse,
    WebhookMethod, WebhookMutationResponse, WebhookRequest, WebhooksResponse,
};
use reqwest::{Client, Method, Request};
use serde::de::DeserializeOwned;
use url::Url;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_TOKEN_BYTES: usize = 16 * 1024;

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
        self.execute(WebhookRequest::list(profile).map_err(contract)?)
            .await
    }

    pub async fn enable(&self, profile: Option<&str>) -> Result<WebhookEnableResponse, String> {
        self.execute(WebhookRequest::enable(profile).map_err(contract)?)
            .await
    }

    pub async fn create(
        &self,
        profile: Option<&str>,
        payload: &WebhookCreatePayload,
    ) -> Result<WebhookCreateResponse, String> {
        self.execute(WebhookRequest::create(profile, payload).map_err(contract)?)
            .await
    }

    pub async fn delete(
        &self,
        profile: Option<&str>,
        name: &str,
    ) -> Result<WebhookMutationResponse, String> {
        self.execute(WebhookRequest::delete(profile, name).map_err(contract)?)
            .await
    }

    pub async fn set_enabled(
        &self,
        profile: Option<&str>,
        name: &str,
        enabled: bool,
    ) -> Result<WebhookEnabledResponse, String> {
        self.execute(WebhookRequest::set_enabled(profile, name, enabled).map_err(contract)?)
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

fn endpoint(base_url: &Url, spec: &WebhookRequest) -> Result<Url, String> {
    let mut url = base_url
        .join(spec.path.trim_start_matches('/'))
        .map_err(|error| format!("Could not construct webhook endpoint: {error}"))?;
    if let Some(profile) = spec.profile.as_deref() {
        url.query_pairs_mut().append_pair("profile", profile);
    }
    Ok(url)
}

fn contract(error: impl std::fmt::Display) -> String {
    error.to_string()
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
    fn rejects_credentialed_or_non_http_bases_and_control_tokens() {
        assert!(NativeWebhookClient::new("file:///tmp/hermes", None).is_err());
        assert!(NativeWebhookClient::new("https://user:pass@example.com", None).is_err());
        assert!(NativeWebhookClient::new("https://example.com?token=bad", None).is_err());
        assert!(NativeWebhookClient::new("https://example.com", Some("bad\ntoken")).is_err());
    }
}
