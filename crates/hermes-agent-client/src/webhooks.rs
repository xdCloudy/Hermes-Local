use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use url::Url;

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
pub enum WebhookMethod {
    Get,
    Post,
    Put,
    Delete,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WebhookRequest {
    pub method: WebhookMethod,
    pub path: String,
    pub profile: Option<String>,
    pub body: Option<Value>,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum WebhookContractError {
    #[error("invalid webhook input: {0}")]
    InvalidInput(String),
    #[error("could not construct webhook endpoint")]
    InvalidEndpoint,
    #[error("could not serialize webhook request: {0}")]
    Serialize(String),
}

impl WebhookRequest {
    pub fn list(profile: Option<&str>) -> Result<Self, WebhookContractError> {
        build_request(WebhookMethod::Get, &[], profile, None)
    }

    pub fn enable(profile: Option<&str>) -> Result<Self, WebhookContractError> {
        build_request(WebhookMethod::Post, &["enable"], profile, None)
    }

    pub fn create(
        profile: Option<&str>,
        payload: &WebhookCreatePayload,
    ) -> Result<Self, WebhookContractError> {
        validate_webhook_name(&payload.name)?;
        let body = serde_json::to_value(payload)
            .map_err(|error| WebhookContractError::Serialize(error.to_string()))?;
        build_request(WebhookMethod::Post, &[], profile, Some(body))
    }

    pub fn delete(profile: Option<&str>, name: &str) -> Result<Self, WebhookContractError> {
        validate_webhook_name(name)?;
        build_request(WebhookMethod::Delete, &[name], profile, None)
    }

    pub fn set_enabled(
        profile: Option<&str>,
        name: &str,
        enabled: bool,
    ) -> Result<Self, WebhookContractError> {
        validate_webhook_name(name)?;
        build_request(
            WebhookMethod::Put,
            &[name, "enabled"],
            profile,
            Some(serde_json::json!({ "enabled": enabled })),
        )
    }
}

fn build_request(
    method: WebhookMethod,
    suffix: &[&str],
    profile: Option<&str>,
    body: Option<Value>,
) -> Result<WebhookRequest, WebhookContractError> {
    let profile = profile.map(validate_profile).transpose()?.map(str::to_owned);
    let mut url = Url::parse("https://hermes.invalid/api/webhooks")
        .map_err(|_| WebhookContractError::InvalidEndpoint)?;
    {
        let mut segments = url
            .path_segments_mut()
            .map_err(|()| WebhookContractError::InvalidEndpoint)?;
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

fn validate_webhook_name(value: &str) -> Result<(), WebhookContractError> {
    if value.trim().is_empty()
        || value.len() > MAX_NAME_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(WebhookContractError::InvalidInput(
            "invalid webhook name".into(),
        ));
    }
    Ok(())
}

fn validate_profile(value: &str) -> Result<&str, WebhookContractError> {
    if value.trim().is_empty()
        || value.len() > MAX_PROFILE_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(WebhookContractError::InvalidInput(
            "invalid profile".into(),
        ));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mirrors_webhook_methods_paths_profile_scope_and_payloads() {
        let profile = Some("work profile");
        let list = WebhookRequest::list(profile).expect("list request");
        assert_eq!(list.method, WebhookMethod::Get);
        assert_eq!(list.path, "/api/webhooks");
        assert_eq!(list.profile.as_deref(), profile);

        let enable = WebhookRequest::enable(profile).expect("enable request");
        assert_eq!(enable.method, WebhookMethod::Post);
        assert_eq!(enable.path, "/api/webhooks/enable");

        let create = WebhookRequest::create(
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
        .expect("create request");
        assert_eq!(create.method, WebhookMethod::Post);
        assert_eq!(create.path, "/api/webhooks");
        let body = create.body.expect("create body");
        assert_eq!(body["name"], "github-push");
        assert_eq!(body["events"], serde_json::json!(["push"]));
        assert_eq!(body["deliver_only"], true);

        let toggle = WebhookRequest::set_enabled(profile, "my hook/one", false)
            .expect("toggle request");
        assert_eq!(toggle.method, WebhookMethod::Put);
        assert_eq!(toggle.path, "/api/webhooks/my%20hook%2Fone/enabled");
        assert_eq!(toggle.body, Some(serde_json::json!({ "enabled": false })));

        let delete = WebhookRequest::delete(profile, "my hook/one").expect("delete request");
        assert_eq!(delete.method, WebhookMethod::Delete);
        assert_eq!(delete.path, "/api/webhooks/my%20hook%2Fone");
    }

    #[test]
    fn read_contract_cannot_serialize_a_webhook_secret() {
        let route: WebhookRoute = serde_json::from_value(serde_json::json!({
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
    fn rejects_empty_or_control_character_names_and_profiles() {
        assert!(WebhookRequest::delete(None, "").is_err());
        assert!(WebhookRequest::delete(None, "bad\nname").is_err());
        assert!(WebhookRequest::list(Some("bad\nprofile")).is_err());
        assert!(WebhookRequest::delete(None, "my hook/one").is_ok());
    }
}
