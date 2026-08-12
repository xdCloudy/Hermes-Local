#![allow(dead_code)] // AG-06 service foundation; Dioxus integration is a later stage.

use std::{collections::BTreeMap, time::Duration};

use reqwest::{Client, Method, Request};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use url::Url;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_TOKEN_BYTES: usize = 16 * 1024;
const MAX_SEGMENT_BYTES: usize = 256;
const MAX_PROFILE_BYTES: usize = 256;
const MAX_ENV_ENTRIES: usize = 128;
const MAX_ENV_VALUE_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct MessagingEnvVarInfo {
    #[serde(default)]
    pub advanced: bool,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub is_password: bool,
    #[serde(default)]
    pub is_set: bool,
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub redacted_value: Option<String>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct MessagingHomeChannel {
    #[serde(default)]
    pub chat_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub platform: String,
    #[serde(default)]
    pub thread_id: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct MessagingPlatformInfo {
    #[serde(default)]
    pub configured: bool,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub docs_url: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub env_vars: Vec<MessagingEnvVarInfo>,
    #[serde(default)]
    pub error_code: Option<String>,
    #[serde(default)]
    pub error_message: Option<String>,
    #[serde(default)]
    pub gateway_running: bool,
    #[serde(default)]
    pub home_channel: Option<MessagingHomeChannel>,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct MessagingPlatformsResponse {
    #[serde(default)]
    pub platforms: Vec<MessagingPlatformInfo>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct MessagingPlatformUpdate {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clear_env: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<BTreeMap<String, String>>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct MessagingPlatformMutationResponse {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub platform: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct MessagingPlatformTestResponse {
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub state: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct PairingUser {
    #[serde(default)]
    pub age_minutes: Option<u64>,
    #[serde(default)]
    pub platform: String,
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub user_name: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct PairingResponse {
    #[serde(default)]
    pub approved: Vec<PairingUser>,
    #[serde(default)]
    pub pending: Vec<PairingUser>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct PairingApproveResponse {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub user: PairingUser,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct PairingMutationResponse {
    #[serde(default)]
    pub ok: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MessagingMethod {
    Get,
    Post,
    Put,
}

#[derive(Clone, Debug, PartialEq)]
struct MessagingRequest {
    method: MessagingMethod,
    path: String,
    query_profile: Option<String>,
    body: Option<Value>,
}

#[derive(Clone)]
pub struct NativeMessagingClient {
    client: Client,
    base_url: Url,
    session_token: Option<String>,
}

impl NativeMessagingClient {
    pub fn new(base_url: &str, session_token: Option<&str>) -> Result<Self, String> {
        let base_url = validate_base_url(base_url)?;
        let session_token = session_token
            .map(validate_session_token)
            .transpose()?
            .map(str::to_owned);
        let client = Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|error| format!("Could not build messaging HTTP client: {error}"))?;
        Ok(Self {
            client,
            base_url,
            session_token,
        })
    }

    pub async fn platforms(&self) -> Result<MessagingPlatformsResponse, String> {
        self.execute(MessagingRequest::platforms()?).await
    }

    pub async fn update_platform(
        &self,
        platform_id: &str,
        update: &MessagingPlatformUpdate,
    ) -> Result<MessagingPlatformMutationResponse, String> {
        validate_platform_update(update)?;
        self.execute(MessagingRequest::update_platform(platform_id, update)?)
            .await
    }

    pub async fn test_platform(
        &self,
        platform_id: &str,
    ) -> Result<MessagingPlatformTestResponse, String> {
        self.execute(MessagingRequest::test_platform(platform_id)?)
            .await
    }

    pub async fn pairing(&self, profile: Option<&str>) -> Result<PairingResponse, String> {
        self.execute(MessagingRequest::pairing(profile)?).await
    }

    pub async fn approve_pairing(
        &self,
        profile: Option<&str>,
        platform: &str,
        request_id: &str,
    ) -> Result<PairingApproveResponse, String> {
        self.execute(MessagingRequest::approve_pairing(
            profile, platform, request_id,
        )?)
        .await
    }

    pub async fn revoke_pairing(
        &self,
        profile: Option<&str>,
        platform: &str,
        user_id: &str,
    ) -> Result<PairingMutationResponse, String> {
        self.execute(MessagingRequest::revoke_pairing(
            profile, platform, user_id,
        )?)
        .await
    }

    async fn execute<T: DeserializeOwned>(&self, spec: MessagingRequest) -> Result<T, String> {
        let request = self.build_request(&spec)?;
        let response = self
            .client
            .execute(request)
            .await
            .map_err(|error| format!("Messaging request failed: {error}"))?;
        let status = response.status();
        if !status.is_success() {
            return Err(format!(
                "Messaging endpoint returned HTTP {}.",
                status.as_u16()
            ));
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_RESPONSE_BYTES)
        {
            return Err("Messaging response exceeded the 4 MiB safety limit.".into());
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|error| format!("Could not read messaging response: {error}"))?;
        if bytes.len() as u64 > MAX_RESPONSE_BYTES {
            return Err("Messaging response exceeded the 4 MiB safety limit.".into());
        }
        serde_json::from_slice(&bytes)
            .map_err(|error| format!("Invalid messaging response: {error}"))
    }

    fn build_request(&self, spec: &MessagingRequest) -> Result<Request, String> {
        let mut url = self
            .base_url
            .join(spec.path.trim_start_matches('/'))
            .map_err(|error| format!("Could not construct messaging endpoint: {error}"))?;
        if let Some(profile) = spec.query_profile.as_deref() {
            url.query_pairs_mut().append_pair("profile", profile);
        }
        let method = match spec.method {
            MessagingMethod::Get => Method::GET,
            MessagingMethod::Post => Method::POST,
            MessagingMethod::Put => Method::PUT,
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
            .map_err(|error| format!("Could not build messaging request: {error}"))
    }
}

impl MessagingRequest {
    fn platforms() -> Result<Self, String> {
        Ok(Self {
            method: MessagingMethod::Get,
            path: "/api/messaging/platforms".into(),
            query_profile: None,
            body: None,
        })
    }

    fn update_platform(
        platform_id: &str,
        update: &MessagingPlatformUpdate,
    ) -> Result<Self, String> {
        let platform_id = encoded_segment(platform_id, "platform")?;
        Ok(Self {
            method: MessagingMethod::Put,
            path: format!("/api/messaging/platforms/{platform_id}"),
            query_profile: None,
            body: Some(
                serde_json::to_value(update)
                    .map_err(|error| format!("Could not serialize messaging update: {error}"))?,
            ),
        })
    }

    fn test_platform(platform_id: &str) -> Result<Self, String> {
        let platform_id = encoded_segment(platform_id, "platform")?;
        Ok(Self {
            method: MessagingMethod::Post,
            path: format!("/api/messaging/platforms/{platform_id}/test"),
            query_profile: None,
            body: None,
        })
    }

    fn pairing(profile: Option<&str>) -> Result<Self, String> {
        Ok(Self {
            method: MessagingMethod::Get,
            path: "/api/pairing".into(),
            query_profile: profile
                .map(validate_profile)
                .transpose()?
                .map(str::to_owned),
            body: None,
        })
    }

    fn approve_pairing(
        profile: Option<&str>,
        platform: &str,
        request_id: &str,
    ) -> Result<Self, String> {
        validate_segment(platform, "platform")?;
        validate_segment(request_id, "pairing request")?;
        let mut body = serde_json::Map::from_iter([
            ("platform".into(), Value::String(platform.to_owned())),
            ("request_id".into(), Value::String(request_id.to_owned())),
        ]);
        if let Some(profile) = profile.map(validate_profile).transpose()? {
            body.insert("profile".into(), Value::String(profile.to_owned()));
        }
        Ok(Self {
            method: MessagingMethod::Post,
            path: "/api/pairing/approve".into(),
            query_profile: profile.map(str::to_owned),
            body: Some(Value::Object(body)),
        })
    }

    fn revoke_pairing(
        profile: Option<&str>,
        platform: &str,
        user_id: &str,
    ) -> Result<Self, String> {
        validate_segment(platform, "platform")?;
        validate_segment(user_id, "pairing user")?;
        let mut body = serde_json::Map::from_iter([
            ("platform".into(), Value::String(platform.to_owned())),
            ("user_id".into(), Value::String(user_id.to_owned())),
        ]);
        if let Some(profile) = profile.map(validate_profile).transpose()? {
            body.insert("profile".into(), Value::String(profile.to_owned()));
        }
        Ok(Self {
            method: MessagingMethod::Post,
            path: "/api/pairing/revoke".into(),
            query_profile: profile.map(str::to_owned),
            body: Some(Value::Object(body)),
        })
    }
}

fn validate_base_url(value: &str) -> Result<Url, String> {
    let mut url =
        Url::parse(value.trim()).map_err(|error| format!("Invalid messaging base URL: {error}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("Messaging base URL must use HTTP or HTTPS.".into());
    }
    if url.host_str().is_none() {
        return Err("Messaging base URL requires a host.".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("Messaging base URL cannot contain embedded credentials.".into());
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err("Messaging base URL cannot contain query or fragment data.".into());
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

fn validate_segment<'a>(value: &'a str, field: &str) -> Result<&'a str, String> {
    if value.trim().is_empty()
        || value.len() > MAX_SEGMENT_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(format!("Invalid messaging {field}."));
    }
    Ok(value)
}

fn encoded_segment(value: &str, field: &str) -> Result<String, String> {
    validate_segment(value, field)?;
    let mut url = Url::parse("https://hermes.invalid/")
        .map_err(|error| format!("Could not encode messaging path: {error}"))?;
    url.path_segments_mut()
        .map_err(|()| "Could not encode messaging path.".to_owned())?
        .push(value);
    Ok(url.path().trim_start_matches('/').to_owned())
}

fn validate_profile(value: &str) -> Result<&str, String> {
    if value.trim().is_empty()
        || value.len() > MAX_PROFILE_BYTES
        || value.chars().any(char::is_control)
    {
        return Err("Invalid messaging profile.".into());
    }
    Ok(value)
}

fn validate_platform_update(update: &MessagingPlatformUpdate) -> Result<(), String> {
    if update
        .env
        .as_ref()
        .is_some_and(|env| env.len() > MAX_ENV_ENTRIES)
    {
        return Err("Messaging environment update contains too many entries.".into());
    }
    if let Some(env) = update.env.as_ref() {
        for (key, value) in env {
            validate_segment(key, "environment key")?;
            if value.len() > MAX_ENV_VALUE_BYTES || value.contains('\0') {
                return Err("Messaging environment value exceeds safety limits.".into());
            }
        }
    }
    if let Some(clear_env) = update.clear_env.as_ref() {
        if clear_env.len() > MAX_ENV_ENTRIES {
            return Err("Messaging clear-env update contains too many entries.".into());
        }
        for key in clear_env {
            validate_segment(key, "environment key")?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client() -> NativeMessagingClient {
        NativeMessagingClient::new("https://gateway.example/hermes", Some("session-token"))
            .expect("client")
    }

    #[test]
    fn platform_contract_matches_electron_paths_and_keeps_secrets_out_of_url() {
        let client = client();
        let update = MessagingPlatformUpdate {
            enabled: Some(true),
            env: Some(BTreeMap::from([(
                "TEAMS_TOKEN".into(),
                "super-secret".into(),
            )])),
            ..MessagingPlatformUpdate::default()
        };
        let spec = MessagingRequest::update_platform("teams/plugin", &update).expect("update spec");
        let request = client.build_request(&spec).expect("update request");
        assert_eq!(request.method(), Method::PUT);
        assert_eq!(
            request.url().as_str(),
            "https://gateway.example/hermes/api/messaging/platforms/teams%2Fplugin"
        );
        assert!(!request.url().as_str().contains("super-secret"));
        assert_eq!(
            request
                .headers()
                .get("x-hermes-session-token")
                .expect("auth header")
                .to_str()
                .expect("header text"),
            "session-token"
        );
        let body = spec.body.expect("update body");
        assert_eq!(body["env"]["TEAMS_TOKEN"], "super-secret");

        let test = client
            .build_request(&MessagingRequest::test_platform("teams/plugin").expect("test spec"))
            .expect("test request");
        assert_eq!(test.method(), Method::POST);
        assert_eq!(
            test.url().as_str(),
            "https://gateway.example/hermes/api/messaging/platforms/teams%2Fplugin/test"
        );
    }

    #[test]
    fn pairing_scope_matches_electron_query_and_body_rules() {
        let client = client();
        let listing = client
            .build_request(&MessagingRequest::pairing(Some("work profile")).expect("pairing spec"))
            .expect("pairing request");
        assert_eq!(
            listing.url().as_str(),
            "https://gateway.example/hermes/api/pairing?profile=work+profile"
        );

        let approve_spec =
            MessagingRequest::approve_pairing(Some("work profile"), "telegram", "a1b2c3d4e5f60718")
                .expect("approve spec");
        let approve = client
            .build_request(&approve_spec)
            .expect("approve request");
        assert_eq!(approve.method(), Method::POST);
        assert_eq!(
            approve.url().as_str(),
            "https://gateway.example/hermes/api/pairing/approve?profile=work+profile"
        );
        let body = approve_spec.body.expect("approve body");
        assert_eq!(body["profile"], "work profile");
        assert_eq!(body["platform"], "telegram");
        assert_eq!(body["request_id"], "a1b2c3d4e5f60718");
        assert!(body.get("code").is_none());

        let revoke_spec = MessagingRequest::revoke_pairing(Some("work profile"), "telegram", "U1")
            .expect("revoke spec");
        let revoke_body = revoke_spec.body.expect("revoke body");
        assert_eq!(revoke_body["profile"], "work profile");
        assert_eq!(revoke_body["user_id"], "U1");
    }

    #[test]
    fn pairing_omits_profile_for_single_profile_users() {
        let client = client();
        let listing = client
            .build_request(&MessagingRequest::pairing(None).expect("pairing spec"))
            .expect("pairing request");
        assert_eq!(
            listing.url().as_str(),
            "https://gateway.example/hermes/api/pairing"
        );

        let approve = MessagingRequest::approve_pairing(None, "telegram", "a1b2c3d4e5f60718")
            .expect("approve spec");
        assert!(approve.query_profile.is_none());
        assert!(approve.body.expect("approve body").get("profile").is_none());
    }

    #[test]
    fn read_contracts_remain_redacted_and_updates_are_bounded() {
        let info: MessagingEnvVarInfo = serde_json::from_value(json!({
            "key": "TEAMS_TOKEN",
            "is_password": true,
            "is_set": true,
            "redacted_value": "***abcd",
            "value": "must-not-survive"
        }))
        .expect("decode env info");
        let encoded = serde_json::to_string(&info).expect("encode env info");
        assert!(!encoded.contains("must-not-survive"));

        let too_many = MessagingPlatformUpdate {
            env: Some(
                (0..=MAX_ENV_ENTRIES)
                    .map(|index| (format!("KEY_{index}"), "x".into()))
                    .collect(),
            ),
            ..MessagingPlatformUpdate::default()
        };
        assert!(validate_platform_update(&too_many).is_err());
        assert!(NativeMessagingClient::new("https://user:pass@example.com", None).is_err());
        assert!(NativeMessagingClient::new("file:///tmp/hermes", None).is_err());
    }
}
