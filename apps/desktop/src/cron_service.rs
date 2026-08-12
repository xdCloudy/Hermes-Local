#![allow(dead_code)] // AG-05 service foundation; Dioxus Cron overlay is a later stage.

use std::time::Duration;

use reqwest::{Client, Method, Request};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use url::Url;

const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const LIST_REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_TOKEN_BYTES: usize = 16 * 1024;
const MAX_JOB_ID_BYTES: usize = 256;
const MAX_PROFILE_BYTES: usize = 256;
const MAX_PROMPT_BYTES: usize = 1024 * 1024;
const MAX_SCHEDULE_BYTES: usize = 4096;
const MAX_RUN_LIMIT: u32 = 1000;

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct CronJobSchedule {
    #[serde(default)]
    pub display: Option<String>,
    #[serde(default)]
    pub expr: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct CronJob {
    #[serde(default)]
    pub deliver: Option<String>,
    pub enabled: bool,
    pub id: String,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub last_run_at: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub next_run_at: Option<String>,
    #[serde(default)]
    pub no_agent: bool,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub schedule: Option<CronJobSchedule>,
    #[serde(default)]
    pub schedule_display: Option<String>,
    #[serde(default)]
    pub script: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct CronJobCreatePayload {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deliver: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    pub schedule: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct CronJobUpdates {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deliver: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schedule: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct CronRunsResponse {
    #[serde(default)]
    pub runs: Vec<Value>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct CronDeleteResponse {
    #[serde(default)]
    pub ok: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CronMethod {
    Get,
    Post,
    Put,
    Delete,
}

#[derive(Clone, Debug, PartialEq)]
struct CronRequest {
    method: CronMethod,
    path: String,
    query: Vec<(String, String)>,
    routing_profile: Option<String>,
    body: Option<Value>,
    timeout: Duration,
}

#[derive(Clone)]
pub struct NativeCronClient {
    client: Client,
    base_url: Url,
    session_token: Option<String>,
    routing_profile: Option<String>,
}

impl NativeCronClient {
    /// Bind this client to the already-selected Electron/Rust backend pool.
    /// `routing_profile` selects the backend process and is deliberately
    /// distinct from Cron's `/api/cron/jobs?profile=` list filter.
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
            .timeout(DEFAULT_REQUEST_TIMEOUT)
            .build()
            .map_err(|error| format!("Could not build Cron HTTP client: {error}"))?;
        Ok(Self {
            client,
            base_url,
            session_token,
            routing_profile,
        })
    }

    pub async fn list(
        &self,
        routing_profile: Option<&str>,
        filter_profile: Option<&str>,
    ) -> Result<Vec<CronJob>, String> {
        self.execute(CronRequest::list(routing_profile, filter_profile)?)
            .await
    }

    pub async fn get(
        &self,
        routing_profile: Option<&str>,
        job_id: &str,
    ) -> Result<CronJob, String> {
        self.execute(CronRequest::job(
            CronMethod::Get,
            routing_profile,
            job_id,
            None,
            None,
        )?)
        .await
    }

    pub async fn runs(
        &self,
        routing_profile: Option<&str>,
        job_id: &str,
        limit: u32,
    ) -> Result<Vec<Value>, String> {
        let response: CronRunsResponse = self
            .execute(CronRequest::runs(routing_profile, job_id, limit)?)
            .await?;
        Ok(response.runs)
    }

    pub async fn create(
        &self,
        routing_profile: Option<&str>,
        payload: &CronJobCreatePayload,
    ) -> Result<CronJob, String> {
        validate_create(payload)?;
        let body = serde_json::to_value(payload)
            .map_err(|error| format!("Could not serialize Cron job: {error}"))?;
        self.execute(CronRequest::root(
            CronMethod::Post,
            routing_profile,
            Some(body),
        )?)
        .await
    }

    pub async fn update(
        &self,
        routing_profile: Option<&str>,
        job_id: &str,
        updates: &CronJobUpdates,
    ) -> Result<CronJob, String> {
        validate_updates(updates)?;
        let body = json!({ "updates": updates });
        self.execute(CronRequest::job(
            CronMethod::Put,
            routing_profile,
            job_id,
            None,
            Some(body),
        )?)
        .await
    }

    pub async fn pause(
        &self,
        routing_profile: Option<&str>,
        job_id: &str,
    ) -> Result<CronJob, String> {
        self.action(routing_profile, job_id, "pause").await
    }

    pub async fn resume(
        &self,
        routing_profile: Option<&str>,
        job_id: &str,
    ) -> Result<CronJob, String> {
        self.action(routing_profile, job_id, "resume").await
    }

    pub async fn trigger(
        &self,
        routing_profile: Option<&str>,
        job_id: &str,
    ) -> Result<CronJob, String> {
        self.action(routing_profile, job_id, "trigger").await
    }

    pub async fn delete(
        &self,
        routing_profile: Option<&str>,
        job_id: &str,
    ) -> Result<CronDeleteResponse, String> {
        self.execute(CronRequest::job(
            CronMethod::Delete,
            routing_profile,
            job_id,
            None,
            None,
        )?)
        .await
    }

    async fn action(
        &self,
        routing_profile: Option<&str>,
        job_id: &str,
        action: &str,
    ) -> Result<CronJob, String> {
        self.execute(CronRequest::job(
            CronMethod::Post,
            routing_profile,
            job_id,
            Some(action),
            None,
        )?)
        .await
    }

    async fn execute<T: DeserializeOwned>(&self, spec: CronRequest) -> Result<T, String> {
        self.assert_routing_profile(spec.routing_profile.as_deref())?;
        let request = self.build_request(&spec)?;
        let response = self
            .client
            .execute(request)
            .await
            .map_err(|error| format!("Cron request failed: {error}"))?;
        let status = response.status();
        if !status.is_success() {
            return Err(format!("Cron endpoint returned HTTP {}.", status.as_u16()));
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_RESPONSE_BYTES)
        {
            return Err("Cron response exceeded the 4 MiB safety limit.".into());
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|error| format!("Could not read Cron response: {error}"))?;
        if bytes.len() as u64 > MAX_RESPONSE_BYTES {
            return Err("Cron response exceeded the 4 MiB safety limit.".into());
        }
        serde_json::from_slice(&bytes).map_err(|error| format!("Invalid Cron response: {error}"))
    }

    fn build_request(&self, spec: &CronRequest) -> Result<Request, String> {
        let mut url = self
            .base_url
            .join(spec.path.trim_start_matches('/'))
            .map_err(|error| format!("Could not construct Cron endpoint: {error}"))?;
        if !spec.query.is_empty() {
            let mut query = url.query_pairs_mut();
            for (key, value) in &spec.query {
                query.append_pair(key, value);
            }
        }
        let method = match spec.method {
            CronMethod::Get => Method::GET,
            CronMethod::Post => Method::POST,
            CronMethod::Put => Method::PUT,
            CronMethod::Delete => Method::DELETE,
        };
        let mut request = self.client.request(method, url).timeout(spec.timeout);
        if let Some(token) = self.session_token.as_deref() {
            request = request.header("x-hermes-session-token", token);
        }
        if let Some(body) = spec.body.as_ref() {
            request = request.json(body);
        }
        request
            .build()
            .map_err(|error| format!("Could not build Cron request: {error}"))
    }

    fn assert_routing_profile(&self, requested: Option<&str>) -> Result<(), String> {
        let requested = requested.map(validate_profile).transpose()?;
        if requested == self.routing_profile.as_deref() {
            Ok(())
        } else {
            Err("Cron request targets a different backend profile than this client.".into())
        }
    }
}

impl CronRequest {
    fn list(routing_profile: Option<&str>, filter_profile: Option<&str>) -> Result<Self, String> {
        let mut query = Vec::new();
        if let Some(filter) = filter_profile {
            query.push(("profile".into(), validate_profile(filter)?.to_owned()));
        }
        Ok(Self {
            method: CronMethod::Get,
            path: "/api/cron/jobs".into(),
            query,
            routing_profile: routing_profile
                .map(validate_profile)
                .transpose()?
                .map(str::to_owned),
            body: None,
            timeout: LIST_REQUEST_TIMEOUT,
        })
    }

    fn root(
        method: CronMethod,
        routing_profile: Option<&str>,
        body: Option<Value>,
    ) -> Result<Self, String> {
        Ok(Self {
            method,
            path: "/api/cron/jobs".into(),
            query: Vec::new(),
            routing_profile: routing_profile
                .map(validate_profile)
                .transpose()?
                .map(str::to_owned),
            body,
            timeout: DEFAULT_REQUEST_TIMEOUT,
        })
    }

    fn job(
        method: CronMethod,
        routing_profile: Option<&str>,
        job_id: &str,
        suffix: Option<&str>,
        body: Option<Value>,
    ) -> Result<Self, String> {
        let job_id = encoded_segment(job_id, "Cron job id")?;
        let path = suffix.map_or_else(
            || format!("/api/cron/jobs/{job_id}"),
            |suffix| format!("/api/cron/jobs/{job_id}/{suffix}"),
        );
        Ok(Self {
            method,
            path,
            query: Vec::new(),
            routing_profile: routing_profile
                .map(validate_profile)
                .transpose()?
                .map(str::to_owned),
            body,
            timeout: DEFAULT_REQUEST_TIMEOUT,
        })
    }

    fn runs(routing_profile: Option<&str>, job_id: &str, limit: u32) -> Result<Self, String> {
        if limit == 0 || limit > MAX_RUN_LIMIT {
            return Err(format!(
                "Cron run limit must be between 1 and {MAX_RUN_LIMIT}."
            ));
        }
        let mut request = Self::job(CronMethod::Get, routing_profile, job_id, Some("runs"), None)?;
        request.query.push(("limit".into(), limit.to_string()));
        Ok(request)
    }
}

fn validate_base_url(value: &str) -> Result<Url, String> {
    let mut url =
        Url::parse(value.trim()).map_err(|error| format!("Invalid Cron base URL: {error}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("Cron base URL must use HTTP or HTTPS.".into());
    }
    if url.host_str().is_none() {
        return Err("Cron base URL requires a host.".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("Cron base URL cannot contain embedded credentials.".into());
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err("Cron base URL cannot contain query or fragment data.".into());
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
        return Err("Invalid Cron profile.".into());
    }
    Ok(value)
}

fn encoded_segment(value: &str, field: &str) -> Result<String, String> {
    if value.trim().is_empty()
        || value.len() > MAX_JOB_ID_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(format!("Invalid {field}."));
    }
    let mut url = Url::parse("https://hermes.invalid/")
        .map_err(|error| format!("Could not encode {field}: {error}"))?;
    url.path_segments_mut()
        .map_err(|()| format!("Could not encode {field}."))?
        .push(value);
    Ok(url.path().trim_start_matches('/').to_owned())
}

fn validate_create(payload: &CronJobCreatePayload) -> Result<(), String> {
    if payload.prompt.trim().is_empty() || payload.prompt.len() > MAX_PROMPT_BYTES {
        return Err("Cron prompt is empty or exceeds the 1 MiB safety limit.".into());
    }
    if payload.schedule.trim().is_empty() || payload.schedule.len() > MAX_SCHEDULE_BYTES {
        return Err("Cron schedule is empty or exceeds the safety limit.".into());
    }
    Ok(())
}

fn validate_updates(updates: &CronJobUpdates) -> Result<(), String> {
    if updates
        .prompt
        .as_ref()
        .is_some_and(|prompt| prompt.len() > MAX_PROMPT_BYTES)
    {
        return Err("Cron prompt exceeds the 1 MiB safety limit.".into());
    }
    if updates
        .schedule
        .as_ref()
        .is_some_and(|schedule| schedule.trim().is_empty() || schedule.len() > MAX_SCHEDULE_BYTES)
    {
        return Err("Cron schedule is empty or exceeds the safety limit.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client() -> NativeCronClient {
        NativeCronClient::new(
            "https://gateway.example/hermes",
            Some("session-token"),
            Some("work profile"),
        )
        .expect("client")
    }

    #[test]
    fn list_keeps_backend_routing_separate_from_endpoint_filter() {
        let client = client();
        let spec = CronRequest::list(Some("work profile"), Some("all")).expect("list spec");
        client
            .assert_routing_profile(spec.routing_profile.as_deref())
            .expect("matching route");
        let request = client.build_request(&spec).expect("request");
        assert_eq!(request.method(), Method::GET);
        assert_eq!(
            request.url().as_str(),
            "https://gateway.example/hermes/api/cron/jobs?profile=all"
        );
        assert_eq!(spec.routing_profile.as_deref(), Some("work profile"));
        assert_eq!(spec.timeout, LIST_REQUEST_TIMEOUT);
        assert!(!request.url().as_str().contains("work+profile"));
    }

    #[test]
    fn rejects_cross_backend_profile_routing() {
        let client = client();
        assert!(
            client
                .assert_routing_profile(Some("other profile"))
                .is_err()
        );
        assert!(client.assert_routing_profile(None).is_err());
    }

    #[test]
    fn job_paths_encode_ids_and_mutations_match_electron_contract() {
        let client = client();
        let update = CronJobUpdates {
            enabled: Some(false),
            model: Some(None),
            provider: Some(Some("nous".into())),
            ..CronJobUpdates::default()
        };
        let spec = CronRequest::job(
            CronMethod::Put,
            Some("work profile"),
            "nightly/job",
            None,
            Some(json!({ "updates": update })),
        )
        .expect("update spec");
        let request = client.build_request(&spec).expect("request");
        assert_eq!(request.method(), Method::PUT);
        assert_eq!(
            request.url().as_str(),
            "https://gateway.example/hermes/api/cron/jobs/nightly%2Fjob"
        );
        assert_eq!(
            spec.body.as_ref().expect("body")["updates"]["enabled"],
            false
        );
        assert!(spec.body.as_ref().expect("body")["updates"]["model"].is_null());

        for action in ["pause", "resume", "trigger"] {
            let action_spec = CronRequest::job(
                CronMethod::Post,
                Some("work profile"),
                "nightly/job",
                Some(action),
                None,
            )
            .expect("action spec");
            let action_request = client.build_request(&action_spec).expect("action request");
            assert_eq!(
                action_request.url().path(),
                format!("/hermes/api/cron/jobs/nightly%2Fjob/{action}")
            );
        }
    }

    #[test]
    fn run_history_is_bounded_and_uses_separate_limit_query() {
        let client = client();
        let spec = CronRequest::runs(Some("work profile"), "job-1", 20).expect("runs spec");
        let request = client.build_request(&spec).expect("request");
        assert_eq!(
            request.url().as_str(),
            "https://gateway.example/hermes/api/cron/jobs/job-1/runs?limit=20"
        );
        assert!(CronRequest::runs(Some("work profile"), "job-1", 0).is_err());
        assert!(CronRequest::runs(Some("work profile"), "job-1", MAX_RUN_LIMIT + 1).is_err());
    }

    #[test]
    fn create_and_update_inputs_are_bounded_without_narrowing_valid_agent_fields() {
        assert!(
            validate_create(&CronJobCreatePayload {
                prompt: "summarize the day".into(),
                schedule: "0 9 * * *".into(),
                model: Some("custom/model".into()),
                provider: Some("custom-provider".into()),
                ..CronJobCreatePayload::default()
            })
            .is_ok()
        );
        assert!(
            validate_create(&CronJobCreatePayload {
                prompt: String::new(),
                schedule: "0 9 * * *".into(),
                ..CronJobCreatePayload::default()
            })
            .is_err()
        );
        assert!(
            validate_updates(&CronJobUpdates {
                schedule: Some(String::new()),
                ..CronJobUpdates::default()
            })
            .is_err()
        );
    }

    #[test]
    fn read_contract_tolerates_future_fields_and_base_url_keeps_secrets_out() {
        let job: CronJob = serde_json::from_value(json!({
            "id": "job-1",
            "enabled": true,
            "future_scheduler_field": { "value": 42 }
        }))
        .expect("decode job");
        assert_eq!(job.id, "job-1");
        assert!(job.enabled);
        assert!(NativeCronClient::new("https://user:pass@example.com", None, None).is_err());
        assert!(NativeCronClient::new("file:///tmp/hermes", None, None).is_err());
        let client = client();
        let spec = CronRequest::list(Some("work profile"), None).expect("list spec");
        let request = client.build_request(&spec).expect("request");
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
}
