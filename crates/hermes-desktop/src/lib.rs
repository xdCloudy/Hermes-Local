//! Windows desktop authority for Hermes Local.

use std::{
    collections::{BTreeMap, HashMap},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex, OnceLock, RwLock},
};

use base64::{
    Engine as _,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use hermes_agent_client::GatewayClient;
use hermes_core::{
    AgentConfigService, AppServices, ConnectionService, EventStream, FileService, GitService,
    ModelService, PlatformService, ProjectService, ProviderService, RuntimeService, ServiceError,
    ServiceFuture, ServiceResult, SessionService, SettingsService, SkillsService, TerminalService,
    TrustService, UnavailableGitBranchService, UnavailableGitDiscardService,
    UnavailableGitRepoScanService, UnavailableGitShipService, UnavailableGitWorktreeService,
    UnavailablePreviewService, UpdateService, validate_identifier, validate_relative_path,
};
use hermes_protocol::{
    AgentConfigSnapshot, AppSettings, AttachmentKind, AuthProvider, AuxiliaryModels,
    ConfigSchemaResponse, ConnectionConfig, ConnectionConfigInput, ConnectionMode,
    ConnectionOauthLoginResult, ConnectionOauthLogoutResult, ConnectionProbeResult,
    ConnectionState, ConnectionTestResult, CustomEndpointUpdate, CustomEndpointValidation,
    CustomEndpointsResponse, EnvVarInfo, FileEntry, GitStatus, MoaConfig, ModelAssignmentRequest,
    ModelAssignmentResponse, ModelInfo, ModelOptions, ModelSettingsSnapshot, OAuthPoll,
    OAuthProvider, OAuthStart, OAuthSubmit, ProbeAuthMode, ProjectFilesDeleteResult,
    ProjectSummary, ProjectsSnapshot, ProviderActivation, RemoteAuthMode, RuntimeStatus,
    SelectedAttachment, SessionAttachmentResult, SessionCreateRequest, SessionCreateResponse,
    SessionDirectiveResult, SessionMessagesResponse, SessionReactionResult, SessionResumeResponse,
    SessionSummary, SkillActionStart, SkillActionStatus, SkillHubPreview, SkillHubScanResult,
    SkillHubSearchResponse, SkillHubSourcesResponse, SkillSummary, SkillToggleResult, TaskSummary,
    TrustSnapshot,
};
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::sync::broadcast;
use uuid::Uuid;

#[derive(Clone)]
struct GatewayServices {
    client: Arc<RwLock<Option<GatewayClient>>>,
    rest: Arc<RwLock<Option<GatewayRest>>>,
    connection_store: Arc<ConnectionConfigStore>,
}

const MAX_ATTACHMENT_SELECTIONS: usize = 256;
const MAX_IMAGE_PREVIEW_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Default)]
struct AttachmentSelectionStore {
    paths: Mutex<HashMap<String, PathBuf>>,
}

static ATTACHMENT_SELECTIONS: OnceLock<AttachmentSelectionStore> = OnceLock::new();

fn attachment_selections() -> &'static AttachmentSelectionStore {
    ATTACHMENT_SELECTIONS.get_or_init(AttachmentSelectionStore::default)
}

impl AttachmentSelectionStore {
    fn register(&self, path: &Path) -> ServiceResult<String> {
        let mut paths = self.paths.lock().map_err(|_| {
            ServiceError::Platform("attachment selection store lock was poisoned".into())
        })?;
        if paths.len() >= MAX_ATTACHMENT_SELECTIONS {
            paths.clear();
        }
        let id = Uuid::new_v4().to_string();
        paths.insert(id.clone(), path.to_owned());
        Ok(id)
    }

    fn resolve(&self, id: &str) -> ServiceResult<PathBuf> {
        if id.is_empty() || id.len() > 128 {
            return Err(ServiceError::InvalidInput(
                "invalid attachment selection".into(),
            ));
        }
        self.paths
            .lock()
            .map_err(|_| {
                ServiceError::Platform("attachment selection store lock was poisoned".into())
            })?
            .get(id)
            .cloned()
            .ok_or_else(|| ServiceError::NotFound("attachment selection expired".into()))
    }
}

fn attachment_image_mime(path: &Path) -> Option<&'static str> {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "bmp" => Some("image/bmp"),
        "tif" | "tiff" => Some("image/tiff"),
        _ => None,
    }
}

fn selected_attachment(path: &Path) -> ServiceResult<SelectedAttachment> {
    let metadata = fs::metadata(path).map_err(platform)?;
    if !metadata.is_file() {
        return Err(ServiceError::InvalidInput(format!(
            "attachment is not a file: {}",
            path.display()
        )));
    }
    let kind = if attachment_image_mime(path).is_some() {
        AttachmentKind::Image
    } else {
        AttachmentKind::File
    };
    let preview_data_url =
        if kind == AttachmentKind::Image && metadata.len() <= MAX_IMAGE_PREVIEW_BYTES {
            let mime = attachment_image_mime(path).unwrap_or("application/octet-stream");
            let bytes = fs::read(path).map_err(platform)?;
            Some(format!("data:{mime};base64,{}", STANDARD.encode(bytes)))
        } else {
            None
        };
    Ok(SelectedAttachment {
        id: attachment_selections().register(path)?,
        kind,
        label: path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("attachment")
            .to_owned(),
        size: metadata.len(),
        preview_data_url,
        ..SelectedAttachment::default()
    })
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredSecret {
    #[serde(default)]
    encoding: String,
    #[serde(default)]
    value: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
struct NativeOauthTokens {
    access_token: String,
    #[serde(default)]
    refresh_token: String,
    #[serde(default)]
    expires_at: u64,
    #[serde(default)]
    provider: String,
    #[serde(default)]
    user_id: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredConnectionBlock {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mode: Option<ConnectionMode>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    url: String,
    #[serde(default)]
    auth_mode: RemoteAuthMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    token: Option<StoredSecret>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    org: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    host: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    user: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    port: Option<u16>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    key_path: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    remote_hermes_path: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    remote_profile: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    saved_ssh: Option<Box<Self>>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct StoredConnectionDocument {
    #[serde(default)]
    mode: ConnectionMode,
    #[serde(default)]
    remote: StoredConnectionBlock,
    #[serde(default)]
    profiles: BTreeMap<String, StoredConnectionBlock>,
}

trait GatewaySecretStore: Send + Sync {
    fn get(&self, account: &str) -> ServiceResult<Option<String>>;
    fn set(&self, account: &str, secret: &str) -> ServiceResult<()>;
    fn delete(&self, account: &str) -> ServiceResult<()>;
}

#[cfg(windows)]
struct NativeGatewaySecretStore;

#[cfg(windows)]
impl NativeGatewaySecretStore {
    fn entry(account: &str) -> ServiceResult<keyring::Entry> {
        keyring::Entry::new("Hermes Local Gateway", account).map_err(platform)
    }
}

#[cfg(windows)]
impl GatewaySecretStore for NativeGatewaySecretStore {
    fn get(&self, account: &str) -> ServiceResult<Option<String>> {
        match Self::entry(account)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(platform(error)),
        }
    }

    fn set(&self, account: &str, secret: &str) -> ServiceResult<()> {
        Self::entry(account)?.set_password(secret).map_err(platform)
    }

    fn delete(&self, account: &str) -> ServiceResult<()> {
        match Self::entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(platform(error)),
        }
    }
}

#[cfg(not(windows))]
struct NativeGatewaySecretStore;

#[cfg(not(windows))]
impl GatewaySecretStore for NativeGatewaySecretStore {
    fn get(&self, _account: &str) -> ServiceResult<Option<String>> {
        Ok(None)
    }

    fn set(&self, _account: &str, _secret: &str) -> ServiceResult<()> {
        Err(ServiceError::Unavailable(
            "native Gateway secret storage is unavailable on this platform".into(),
        ))
    }

    fn delete(&self, _account: &str) -> ServiceResult<()> {
        Ok(())
    }
}

struct ConnectionConfigStore {
    path: PathBuf,
    secrets: Arc<dyn GatewaySecretStore>,
    lock: Mutex<()>,
}

impl ConnectionConfigStore {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            secrets: Arc::new(NativeGatewaySecretStore),
            lock: Mutex::new(()),
        }
    }

    #[cfg(test)]
    fn with_secrets(path: PathBuf, secrets: Arc<dyn GatewaySecretStore>) -> Self {
        Self {
            path,
            secrets,
            lock: Mutex::new(()),
        }
    }

    fn document(&self) -> ServiceResult<StoredConnectionDocument> {
        match fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(protocol),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(StoredConnectionDocument::default())
            }
            Err(error) => Err(platform(error)),
        }
    }

    fn write_document(&self, document: &StoredConnectionDocument) -> ServiceResult<()> {
        let parent = self.path.parent().ok_or_else(|| {
            ServiceError::Platform("connection settings path has no parent".into())
        })?;
        fs::create_dir_all(parent).map_err(platform)?;
        let bytes = serde_json::to_vec_pretty(document).map_err(protocol)?;
        let temporary = self.path.with_extension("json.tmp");
        fs::write(&temporary, bytes).map_err(platform)?;
        fs::rename(&temporary, &self.path).map_err(platform)
    }

    fn load(&self, profile: Option<&str>) -> ServiceResult<ConnectionConfig> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| ServiceError::Platform("connection settings lock was poisoned".into()))?;
        let document = self.document().unwrap_or_default();
        self.sanitize(&document, profile)
    }

    #[allow(clippy::too_many_lines)] // Mirrors the OG mode-transition state machine in one lock.
    fn save(&self, input: &ConnectionConfigInput) -> ServiceResult<ConnectionConfig> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| ServiceError::Platform("connection settings lock was poisoned".into()))?;
        let mut document = self.document().unwrap_or_default();
        let profile = validated_profile(input.profile.as_deref())?;
        let account = secret_account(profile.as_deref());
        let existing = profile.as_ref().map_or_else(
            || document.remote.clone(),
            |profile| document.profiles.get(profile).cloned().unwrap_or_default(),
        );

        let mut block = existing.clone();
        match input.mode {
            ConnectionMode::Remote | ConnectionMode::Cloud => {
                let leaving_cloud = existing.mode == Some(ConnectionMode::Cloud)
                    && input.mode != ConnectionMode::Cloud;
                if leaving_cloud || existing.mode == Some(ConnectionMode::Ssh) {
                    block = StoredConnectionBlock::default();
                }
                block.mode = profile.as_ref().map(|_| input.mode);
                block.url =
                    normalize_remote_url(input.remote_url.as_deref().unwrap_or(&block.url))?;
                block.auth_mode = input.remote_auth_mode.unwrap_or(block.auth_mode);
                block.org = if input.mode == ConnectionMode::Cloud {
                    input
                        .cloud_org
                        .as_deref()
                        .unwrap_or(&block.org)
                        .trim()
                        .to_owned()
                } else {
                    String::new()
                };
                if let Some(token) = input
                    .remote_token
                    .as_deref()
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                {
                    self.secrets.set(&account, token)?;
                    block.token = Some(StoredSecret {
                        encoding: "credentialManager".into(),
                        value: account.clone(),
                    });
                }
                if block.auth_mode == RemoteAuthMode::Token
                    && self.secret(&block, &account)?.is_none()
                {
                    return Err(ServiceError::InvalidInput(
                        "Remote gateway session token is required.".into(),
                    ));
                }
            }
            ConnectionMode::Ssh => {
                let host = input.ssh_host.as_deref().unwrap_or(&block.host).trim();
                if host.is_empty() {
                    return Err(ServiceError::InvalidInput("SSH host is required.".into()));
                }
                if existing.host != host {
                    self.secrets.delete(&account)?;
                    block.token = None;
                }
                block = StoredConnectionBlock {
                    mode: Some(ConnectionMode::Ssh),
                    host: host.to_owned(),
                    user: input
                        .ssh_user
                        .as_deref()
                        .unwrap_or(&block.user)
                        .trim()
                        .to_owned(),
                    port: input.ssh_port.unwrap_or(block.port),
                    key_path: input
                        .ssh_key_path
                        .as_deref()
                        .unwrap_or(&block.key_path)
                        .trim()
                        .to_owned(),
                    remote_hermes_path: input
                        .ssh_remote_hermes_path
                        .as_deref()
                        .unwrap_or(&block.remote_hermes_path)
                        .trim()
                        .to_owned(),
                    remote_profile: input
                        .ssh_remote_profile
                        .as_deref()
                        .unwrap_or(&block.remote_profile)
                        .trim()
                        .to_owned(),
                    token: block.token,
                    ..StoredConnectionBlock::default()
                };
            }
            ConnectionMode::Local => {
                if profile.is_some() {
                    if existing.mode == Some(ConnectionMode::Ssh) {
                        block = StoredConnectionBlock {
                            mode: Some(ConnectionMode::Local),
                            saved_ssh: Some(Box::new(existing)),
                            ..StoredConnectionBlock::default()
                        };
                    } else {
                        block = StoredConnectionBlock::default();
                    }
                }
            }
        }

        if let Some(profile) = profile.as_ref() {
            if input.mode == ConnectionMode::Local && block.saved_ssh.is_none() {
                document.profiles.remove(profile);
            } else {
                document.profiles.insert(profile.clone(), block);
            }
        } else {
            document.mode = input.mode;
            document.remote = block;
        }
        self.write_document(&document)?;
        self.sanitize(&document, profile.as_deref())
    }

    fn sanitize(
        &self,
        document: &StoredConnectionDocument,
        profile: Option<&str>,
    ) -> ServiceResult<ConnectionConfig> {
        let profile = validated_profile(profile)?;
        let scoped = profile.as_ref().and_then(|key| document.profiles.get(key));
        let block = scoped.unwrap_or(&document.remote);
        let saved_mode = if profile.is_some() {
            scoped
                .and_then(|entry| entry.mode)
                .unwrap_or(ConnectionMode::Local)
        } else {
            document.mode
        };
        let env_url = (profile.is_none())
            .then(|| std::env::var("HERMES_DESKTOP_REMOTE_URL").ok())
            .flatten()
            .filter(|value| !value.trim().is_empty());
        let env_override = env_url.is_some();
        let mode = if env_override {
            ConnectionMode::Remote
        } else {
            saved_mode
        };
        let ssh = if mode == ConnectionMode::Ssh {
            Some(block)
        } else if mode == ConnectionMode::Local {
            block
                .saved_ssh
                .as_deref()
                .or((block.mode == Some(ConnectionMode::Ssh)).then_some(block))
        } else {
            None
        };
        let account = secret_account(profile.as_deref());
        let token = self.secret(block, &account)?;
        let remote_url = env_url.unwrap_or_else(|| block.url.clone());
        let remote_oauth_connected = block.auth_mode == RemoteAuthMode::Oauth
            && !remote_url.is_empty()
            && self.oauth_tokens(&remote_url)?.is_some();
        Ok(ConnectionConfig {
            env_override,
            mode,
            profile,
            remote_auth_mode: block.auth_mode,
            remote_oauth_connected,
            remote_token_preview: token_preview(token.as_deref()),
            remote_token_set: token.is_some(),
            remote_url,
            cloud_org: if mode == ConnectionMode::Cloud {
                block.org.clone()
            } else {
                String::new()
            },
            ssh_host: ssh.map_or_else(String::new, |ssh| ssh.host.clone()),
            ssh_user: ssh.map_or_else(String::new, |ssh| ssh.user.clone()),
            ssh_port: ssh.and_then(|ssh| ssh.port),
            ssh_key_path: ssh.map_or_else(String::new, |ssh| ssh.key_path.clone()),
            ssh_remote_hermes_path: ssh
                .map_or_else(String::new, |ssh| ssh.remote_hermes_path.clone()),
            ssh_remote_profile: ssh.map_or_else(String::new, |ssh| ssh.remote_profile.clone()),
        })
    }

    fn secret(
        &self,
        block: &StoredConnectionBlock,
        account: &str,
    ) -> ServiceResult<Option<String>> {
        let Some(secret) = block.token.as_ref() else {
            return Ok(None);
        };
        if secret.encoding == "plain" {
            return Ok((!secret.value.is_empty()).then(|| secret.value.clone()));
        }
        self.secrets.get(account)
    }

    fn remote_secret(&self, profile: Option<&str>) -> ServiceResult<Option<String>> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| ServiceError::Platform("connection settings lock was poisoned".into()))?;
        let profile = validated_profile(profile)?;
        let document = self.document().unwrap_or_default();
        let block = profile
            .as_ref()
            .and_then(|key| document.profiles.get(key))
            .unwrap_or(&document.remote);
        self.secret(block, &secret_account(profile.as_deref()))
    }

    fn oauth_tokens(&self, base_url: &str) -> ServiceResult<Option<NativeOauthTokens>> {
        let Some(encoded) = self.secrets.get(&oauth_account(base_url))? else {
            return Ok(None);
        };
        let tokens = serde_json::from_str(&encoded).map_err(|error| {
            ServiceError::Platform(format!("invalid stored OAuth session: {error}"))
        })?;
        Ok(Some(tokens))
    }

    fn store_oauth_tokens(&self, base_url: &str, tokens: &NativeOauthTokens) -> ServiceResult<()> {
        if tokens.access_token.is_empty() {
            return Err(ServiceError::InvalidInput(
                "Gateway token response missing access_token".into(),
            ));
        }
        let encoded = serde_json::to_string(tokens)
            .map_err(|error| ServiceError::Platform(error.to_string()))?;
        self.secrets.set(&oauth_account(base_url), &encoded)
    }

    fn clear_oauth_tokens(&self, base_url: &str) -> ServiceResult<()> {
        self.secrets.delete(&oauth_account(base_url))
    }
}

#[derive(Clone)]
struct GatewayRest {
    client: reqwest::Client,
    base_url: url::Url,
    session_token: Option<String>,
}

impl GatewayServices {
    fn client(&self) -> ServiceResult<GatewayClient> {
        self.client
            .read()
            .map_err(|_| ServiceError::Platform("gateway lock was poisoned".into()))?
            .clone()
            .ok_or_else(|| ServiceError::Unavailable("Hermes Agent is not connected".into()))
    }

    fn rest(&self) -> ServiceResult<GatewayRest> {
        self.rest
            .read()
            .map_err(|_| ServiceError::Platform("gateway REST lock was poisoned".into()))?
            .clone()
            .ok_or_else(|| {
                ServiceError::Unavailable("Hermes Agent REST API is not connected".into())
            })
    }

    async fn run_native_oauth_login(&self, base_url: &str) -> ServiceResult<NativeOauthTokens> {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .map_err(platform)?;
        let port = listener.local_addr().map_err(platform)?.port();
        let redirect_uri = format!("http://127.0.0.1:{port}/callback");
        let (verifier, challenge, state) = native_pkce_material()?;
        let authorize_url = native_oauth_url(
            base_url,
            "authorize",
            &[
                ("code_challenge", &challenge),
                ("code_challenge_method", "S256"),
                ("redirect_uri", &redirect_uri),
                ("state", &state),
            ],
        )?;
        open::that(&authorize_url).map_err(platform)?;
        let code = receive_oauth_code(&listener, &state).await?;
        let token_url = native_oauth_url(base_url, "token", &[])?;
        let response = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(platform)?
            .post(token_url)
            .json(&json!({ "code": code, "code_verifier": verifier }))
            .send()
            .await
            .map_err(|error| ServiceError::Transport(error.to_string()))?;
        if !response.status().is_success() {
            return Err(ServiceError::PermissionDenied(format!(
                "Gateway rejected native token exchange with HTTP {}",
                response.status()
            )));
        }
        let tokens: NativeOauthTokens = response.json().await.map_err(|error| {
            ServiceError::Transport(format!("invalid Gateway token response: {error}"))
        })?;
        if tokens.access_token.is_empty() {
            return Err(ServiceError::Transport(
                "Gateway token response missing access_token".into(),
            ));
        }
        Ok(tokens)
    }

    async fn ensure_native_access_token(&self, base_url: &str) -> ServiceResult<String> {
        let mut tokens = self
            .connection_store
            .oauth_tokens(base_url)?
            .ok_or_else(|| ServiceError::PermissionDenied("Gateway sign-in is required".into()))?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(platform)?
            .as_secs();
        if tokens.expires_at > now.saturating_add(60) {
            return Ok(tokens.access_token);
        }
        if tokens.refresh_token.is_empty() {
            self.connection_store.clear_oauth_tokens(base_url)?;
            return Err(ServiceError::PermissionDenied(
                "Gateway session expired; sign in again".into(),
            ));
        }
        let refresh_url = native_oauth_url(base_url, "refresh", &[])?;
        let response = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(platform)?
            .post(refresh_url)
            .json(&json!({
                "refresh_token": tokens.refresh_token,
                "provider": tokens.provider,
            }))
            .send()
            .await
            .map_err(|error| ServiceError::Transport(error.to_string()))?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            self.connection_store.clear_oauth_tokens(base_url)?;
            return Err(ServiceError::PermissionDenied(
                "Gateway session expired; sign in again".into(),
            ));
        }
        if !response.status().is_success() {
            return Err(ServiceError::Transport(format!(
                "Gateway token refresh returned HTTP {}",
                response.status()
            )));
        }
        tokens = response.json().await.map_err(|error| {
            ServiceError::Transport(format!("invalid Gateway refresh response: {error}"))
        })?;
        self.connection_store
            .store_oauth_tokens(base_url, &tokens)?;
        Ok(tokens.access_token)
    }

    async fn mint_gateway_ticket(&self, base_url: &str) -> ServiceResult<String> {
        let access_token = self.ensure_native_access_token(base_url).await?;
        let base_url = normalize_remote_url(base_url)?;
        let response = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(8))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(platform)?
            .post(format!("{base_url}/api/auth/ws-ticket"))
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|error| ServiceError::Transport(error.to_string()))?;
        if matches!(response.status().as_u16(), 401 | 403) {
            return Err(ServiceError::PermissionDenied(
                "Gateway rejected the OAuth session; sign in again".into(),
            ));
        }
        if !response.status().is_success() {
            return Err(ServiceError::Transport(format!(
                "Gateway ticket mint returned HTTP {}",
                response.status()
            )));
        }
        let value: Value = response.json().await.map_err(|error| {
            ServiceError::Transport(format!("invalid Gateway ticket response: {error}"))
        })?;
        value
            .get("ticket")
            .and_then(Value::as_str)
            .filter(|ticket| !ticket.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| {
                ServiceError::Transport("Gateway did not return a WebSocket ticket".into())
            })
    }
}

pub struct NativeApp {
    pub services: AppServices,
}

impl NativeApp {
    pub fn new(data_dir: PathBuf) -> Self {
        let gateway = Arc::new(RwLock::new(None));
        let rest = Arc::new(RwLock::new(None));
        let remote = Arc::new(GatewayServices {
            client: gateway.clone(),
            rest: rest.clone(),
            connection_store: Arc::new(ConnectionConfigStore::new(
                data_dir.join("connection.json"),
            )),
        });
        let settings = Arc::new(JsonSettings::new(data_dir.join("settings.json")));
        let platform = Arc::new(DesktopPlatform);
        Self {
            services: AppServices {
                connection: remote.clone(),
                sessions: remote.clone(),
                projects: remote.clone(),
                settings,
                agent_config: remote.clone(),
                models: remote.clone(),
                providers: remote.clone(),
                runtime: remote.clone(),
                trust: remote.clone(),
                skills: remote,
                preview: Arc::new(UnavailablePreviewService),
                files: Arc::new(DesktopFiles),
                git: Arc::new(DesktopGit),
                git_branches: Arc::new(UnavailableGitBranchService),
                git_worktrees: Arc::new(UnavailableGitWorktreeService),
                git_discard: Arc::new(UnavailableGitDiscardService),
                git_ship: Arc::new(UnavailableGitShipService),
                git_repo_scan: Arc::new(UnavailableGitRepoScanService),
                terminal: Arc::new(DesktopTerminals::default()),
                updates: Arc::new(DesktopUpdates { data_dir }),
                platform,
            },
        }
    }
}

impl ConnectionService for GatewayServices {
    fn initialize(&self) -> ServiceFuture<'_, ConnectionState> {
        Box::pin(async move {
            if self.state()? == ConnectionState::Open {
                return Ok(ConnectionState::Open);
            }
            let explicit = std::env::var("HERMES_DESKTOP_GATEWAY_WS_URL").ok();
            let remote = std::env::var("HERMES_DESKTOP_REMOTE_URL").ok();
            let token = std::env::var("HERMES_DESKTOP_REMOTE_TOKEN").ok();
            let websocket_url = match explicit {
                Some(url) if !url.trim().is_empty() => url,
                _ => match (remote, token) {
                    (Some(base), Some(token)) if !base.trim().is_empty() && !token.is_empty() => {
                        websocket_url(&base, &token)?
                    }
                    _ => {
                        let config = self.connection_store.load(None)?;
                        if !config.mode.is_remote_like() {
                            return Err(ServiceError::Unavailable(
                                "no gateway is configured; local Agent bootstrap is pending".into(),
                            ));
                        }
                        match config.remote_auth_mode {
                            RemoteAuthMode::Token => {
                                let token = self.connection_store.remote_secret(None)?.ok_or_else(
                                    || {
                                        ServiceError::Unavailable(
                                            "the configured Gateway token is unavailable".into(),
                                        )
                                    },
                                )?;
                                websocket_url(&config.remote_url, &token)?
                            }
                            RemoteAuthMode::Oauth => {
                                let ticket = self.mint_gateway_ticket(&config.remote_url).await?;
                                websocket_url_with_ticket(&config.remote_url, &ticket)?
                            }
                        }
                    }
                },
            };
            self.connect(&websocket_url).await
        })
    }

    fn connect(&self, websocket_url: &str) -> ServiceFuture<'_, ConnectionState> {
        let websocket_url = websocket_url.to_owned();
        Box::pin(async move {
            let rest = rest_from_websocket_url(&websocket_url)?;
            let client = GatewayClient::connect(&websocket_url, Default::default())
                .await
                .map_err(transport)?;
            let previous = {
                self.client
                    .write()
                    .map_err(|_| ServiceError::Platform("gateway lock was poisoned".into()))?
                    .replace(client)
            };
            if let Some(previous) = previous {
                let _ = previous.close().await;
            }
            *self
                .rest
                .write()
                .map_err(|_| ServiceError::Platform("gateway REST lock was poisoned".into()))? =
                Some(rest);
            Ok(ConnectionState::Open)
        })
    }

    fn disconnect(&self) -> ServiceFuture<'_, ()> {
        Box::pin(async move {
            let previous = {
                self.client
                    .write()
                    .map_err(|_| ServiceError::Platform("gateway lock was poisoned".into()))?
                    .take()
            };
            if let Some(previous) = previous {
                previous.close().await.map_err(transport)?;
            }
            self.rest
                .write()
                .map_err(|_| ServiceError::Platform("gateway REST lock was poisoned".into()))?
                .take();
            Ok(())
        })
    }

    fn state(&self) -> ServiceResult<ConnectionState> {
        let client = self
            .client
            .read()
            .map_err(|_| ServiceError::Platform("gateway lock was poisoned".into()))?;
        Ok(client.as_ref().map_or(ConnectionState::Idle, |client| {
            *client.connection_state().borrow()
        }))
    }

    fn config(&self, profile: Option<&str>) -> ServiceFuture<'_, ConnectionConfig> {
        let profile = profile.map(str::to_owned);
        Box::pin(async move { self.connection_store.load(profile.as_deref()) })
    }

    fn save_config(&self, input: &ConnectionConfigInput) -> ServiceFuture<'_, ConnectionConfig> {
        let input = input.clone();
        Box::pin(async move { self.connection_store.save(&input) })
    }

    fn apply_config(&self, input: &ConnectionConfigInput) -> ServiceFuture<'_, ConnectionConfig> {
        let input = input.clone();
        Box::pin(async move {
            let config = self.connection_store.save(&input)?;
            self.disconnect().await?;
            match config.mode {
                ConnectionMode::Remote if config.remote_auth_mode == RemoteAuthMode::Token => {
                    let token = self
                        .connection_store
                        .remote_secret(config.profile.as_deref())?
                        .ok_or_else(|| {
                            ServiceError::Unavailable(
                                "the configured Gateway token is unavailable".into(),
                            )
                        })?;
                    self.connect(&websocket_url(&config.remote_url, &token)?)
                        .await?;
                }
                ConnectionMode::Remote => {
                    let ticket = self.mint_gateway_ticket(&config.remote_url).await?;
                    self.connect(&websocket_url_with_ticket(&config.remote_url, &ticket)?)
                        .await?;
                }
                ConnectionMode::Cloud => {
                    return Err(ServiceError::Unavailable(
                        "Hermes Cloud agent discovery is pending".into(),
                    ));
                }
                ConnectionMode::Ssh => {
                    return Err(ServiceError::Unavailable(
                        "SSH Gateway bootstrap is pending".into(),
                    ));
                }
                ConnectionMode::Local => {
                    self.initialize().await?;
                }
            }
            Ok(config)
        })
    }

    fn test_config(
        &self,
        input: &ConnectionConfigInput,
    ) -> ServiceFuture<'_, ConnectionTestResult> {
        let input = input.clone();
        Box::pin(async move {
            if input.mode == ConnectionMode::Ssh {
                return Ok(ConnectionTestResult {
                    reachable: Some(false),
                    error: Some("SSH connection testing is not ported yet.".into()),
                    ..ConnectionTestResult::default()
                });
            }
            let current = self.connection_store.load(input.profile.as_deref())?;
            let remote_url = input
                .remote_url
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(&current.remote_url);
            if remote_url.is_empty() {
                return Err(ServiceError::Unavailable(
                    "local Agent bootstrap is pending".into(),
                ));
            }
            let probe = probe_remote_gateway(remote_url).await?;
            Ok(ConnectionTestResult {
                base_url: Some(probe.base_url),
                ok: Some(probe.reachable),
                version: probe.version,
                reachable: Some(probe.reachable),
                error: probe.error,
                ..ConnectionTestResult::default()
            })
        })
    }

    fn probe_config(&self, remote_url: &str) -> ServiceFuture<'_, ConnectionProbeResult> {
        let remote_url = remote_url.to_owned();
        Box::pin(async move { probe_remote_gateway(&remote_url).await })
    }

    fn oauth_login(&self, remote_url: &str) -> ServiceFuture<'_, ConnectionOauthLoginResult> {
        let remote_url = remote_url.to_owned();
        Box::pin(async move {
            let base_url = native_oauth_gateway(&remote_url).await?;
            let tokens = self.run_native_oauth_login(&base_url).await?;
            self.connection_store
                .store_oauth_tokens(&base_url, &tokens)?;
            Ok(ConnectionOauthLoginResult {
                ok: true,
                base_url,
                connected: true,
            })
        })
    }

    fn oauth_logout(&self, remote_url: &str) -> ServiceFuture<'_, ConnectionOauthLogoutResult> {
        let remote_url = remote_url.to_owned();
        Box::pin(async move {
            let base_url = normalize_remote_url(&remote_url)?;
            self.connection_store.clear_oauth_tokens(&base_url)?;
            Ok(ConnectionOauthLogoutResult {
                ok: true,
                connected: false,
            })
        })
    }
}

impl SessionService for GatewayServices {
    fn list(&self) -> ServiceFuture<'_, Vec<SessionSummary>> {
        Box::pin(async move {
            let value = self
                .rest()?
                .request(
                    Method::GET,
                    "/api/sessions?limit=50&offset=0&min_messages=1&archived=exclude&order=recent",
                    None,
                )
                .await?;
            decode_list(value, "sessions")
        })
    }

    fn create(&self, request: SessionCreateRequest) -> ServiceFuture<'_, SessionSummary> {
        Box::pin(async move {
            let created: SessionCreateResponse = self
                .client()?
                .request("session.create", request)
                .await
                .map_err(transport)?;
            Ok(SessionSummary {
                id: created
                    .extra
                    .get("stored_session_id")
                    .and_then(Value::as_str)
                    .unwrap_or(&created.session_id)
                    .to_owned(),
                runtime_id: Some(created.session_id),
                running: true,
                ..SessionSummary::default()
            })
        })
    }

    fn resume(&self, session_id: &str) -> ServiceFuture<'_, SessionResumeResponse> {
        let session_id = session_id.to_owned();
        Box::pin(async move {
            validate_identifier(&session_id, "session")?;
            self.client()?
                .request("session.resume", json!({ "session_id": session_id }))
                .await
                .map_err(transport)
        })
    }

    fn history(&self, session_id: &str) -> ServiceFuture<'_, Vec<hermes_protocol::ChatMessage>> {
        let session_id = session_id.to_owned();
        Box::pin(async move {
            validate_identifier(&session_id, "session")?;
            let value = self
                .rest()?
                .request(
                    Method::GET,
                    &format!("/api/sessions/{session_id}/messages"),
                    None,
                )
                .await?;
            let response: SessionMessagesResponse =
                serde_json::from_value(value).map_err(protocol)?;
            Ok(response.messages)
        })
    }

    fn execute_directive(
        &self,
        session_id: &str,
        command: &str,
    ) -> ServiceFuture<'_, SessionDirectiveResult> {
        let session_id = session_id.to_owned();
        let command = command.trim().trim_start_matches('/').trim().to_owned();
        Box::pin(async move {
            validate_identifier(&session_id, "session")?;
            if command.is_empty()
                || command.len() > 32_768
                || command.chars().any(|char| char == '\0')
            {
                return Err(ServiceError::InvalidInput(
                    "invalid session directive".into(),
                ));
            }
            let mut parts = command.splitn(2, char::is_whitespace);
            let name = parts.next().unwrap_or_default().to_owned();
            let arg = parts.next().unwrap_or_default().trim().to_owned();
            let client = self.client()?;
            let value = match client
                .request::<_, Value>(
                    "slash.exec",
                    json!({ "session_id": session_id, "command": command }),
                )
                .await
            {
                Ok(value) => value,
                Err(_) => client
                    .request::<_, Value>(
                        "command.dispatch",
                        json!({ "session_id": session_id, "name": name, "arg": arg }),
                    )
                    .await
                    .map_err(transport)?,
            };
            let mut result: SessionDirectiveResult =
                serde_json::from_value(value.clone()).map_err(protocol)?;
            if result.output.is_none()
                && let Some(output) = value.as_str()
            {
                result.output = Some(output.to_owned());
            }
            if result.kind.is_empty() {
                result.kind = if result.message.is_some() {
                    "send".into()
                } else {
                    "exec".into()
                };
            }
            Ok(result)
        })
    }

    fn attach(
        &self,
        session_id: &str,
        attachment: &SelectedAttachment,
    ) -> ServiceFuture<'_, SessionAttachmentResult> {
        let session_id = session_id.to_owned();
        let attachment = attachment.clone();
        Box::pin(async move {
            validate_identifier(&session_id, "session")?;
            if attachment.attached_session_id.as_deref() == Some(session_id.as_str()) {
                return Ok(SessionAttachmentResult {
                    attached: true,
                    kind: attachment.kind,
                    path: attachment.staged_path.clone(),
                    ref_text: attachment.ref_text.clone(),
                    message: None,
                });
            }
            let path = attachment_selections().resolve(&attachment.id)?;
            let metadata = fs::metadata(&path).map_err(platform)?;
            if !metadata.is_file() {
                return Err(ServiceError::InvalidInput(
                    "attachment is not a file".into(),
                ));
            }
            let limit = if attachment.kind == AttachmentKind::Image {
                64 * 1024 * 1024
            } else {
                256 * 1024 * 1024
            };
            if metadata.len() > limit {
                return Err(ServiceError::InvalidInput(format!(
                    "{} is too large to attach ({} bytes; limit {} bytes)",
                    attachment.label,
                    metadata.len(),
                    limit
                )));
            }
            let local = self.connection_store.load(None)?.mode == ConnectionMode::Local;
            let value: Value = match (attachment.kind, local) {
                (AttachmentKind::Image, true) => self
                    .client()?
                    .request_with_timeout(
                        "image.attach",
                        json!({ "session_id": session_id, "path": path.to_string_lossy() }),
                        std::time::Duration::from_mins(5),
                    )
                    .await
                    .map_err(transport)?,
                (AttachmentKind::Image, false) => {
                    let bytes = fs::read(&path).map_err(platform)?;
                    self.client()?
                        .request_with_timeout(
                            "image.attach_bytes",
                            json!({
                                "session_id": session_id,
                                "content_base64": STANDARD.encode(bytes),
                                "filename": attachment.label,
                            }),
                            std::time::Duration::from_mins(5),
                        )
                        .await
                        .map_err(transport)?
                }
                (AttachmentKind::File, true) => self
                    .client()?
                    .request_with_timeout(
                        "file.attach",
                        json!({
                            "session_id": session_id,
                            "name": attachment.label,
                            "path": path.to_string_lossy(),
                        }),
                        std::time::Duration::from_mins(5),
                    )
                    .await
                    .map_err(transport)?,
                (AttachmentKind::File, false) => {
                    let bytes = fs::read(&path).map_err(platform)?;
                    self.client()?
                        .request_with_timeout(
                            "file.attach",
                            json!({
                                "session_id": session_id,
                                "name": attachment.label,
                                "path": path.to_string_lossy(),
                                "data_url": format!(
                                    "data:application/octet-stream;base64,{}",
                                    STANDARD.encode(bytes)
                                ),
                            }),
                            std::time::Duration::from_mins(5),
                        )
                        .await
                        .map_err(transport)?
                }
            };
            let result = SessionAttachmentResult {
                attached: value
                    .get("attached")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                kind: attachment.kind,
                path: value.get("path").and_then(Value::as_str).map(str::to_owned),
                ref_text: value
                    .get("ref_text")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                message: value
                    .get("message")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            };
            if !result.attached {
                return Err(ServiceError::Transport(
                    result
                        .message
                        .clone()
                        .unwrap_or_else(|| "attachment rejected".into()),
                ));
            }
            Ok(result)
        })
    }

    fn detach_image(&self, session_id: &str, path: &str) -> ServiceFuture<'_, ()> {
        let session_id = session_id.to_owned();
        let path = path.trim().to_owned();
        Box::pin(async move {
            validate_identifier(&session_id, "session")?;
            if path.is_empty() || path.len() > 32_768 {
                return Err(ServiceError::InvalidInput("invalid image path".into()));
            }
            let _: Value = self
                .client()?
                .request_with_timeout(
                    "image.detach",
                    json!({ "session_id": session_id, "path": path }),
                    std::time::Duration::from_secs(30),
                )
                .await
                .map_err(transport)?;
            Ok(())
        })
    }

    fn react(
        &self,
        session_id: &str,
        row_id: Option<&str>,
        newest_role: hermes_protocol::MessageRole,
        emoji: Option<&str>,
    ) -> ServiceFuture<'_, SessionReactionResult> {
        let session_id = session_id.to_owned();
        let row_id = row_id.map(str::to_owned);
        let emoji = emoji.map(str::to_owned);
        Box::pin(async move {
            validate_identifier(&session_id, "session")?;
            if row_id.as_deref().is_some_and(|value| {
                value.is_empty() || value.len() > 256 || value.chars().any(char::is_control)
            }) {
                return Err(ServiceError::InvalidInput("invalid reaction row id".into()));
            }
            if emoji.as_deref().is_some_and(|value| {
                value.is_empty() || value.len() > 32 || value.chars().any(char::is_control)
            }) {
                return Err(ServiceError::InvalidInput("invalid reaction emoji".into()));
            }
            let role = match newest_role {
                hermes_protocol::MessageRole::Assistant => "assistant",
                hermes_protocol::MessageRole::User => "user",
                _ => {
                    return Err(ServiceError::InvalidInput(
                        "reactions require a user or assistant message".into(),
                    ));
                }
            };
            let mut params = serde_json::Map::from_iter([
                ("session_id".into(), Value::String(session_id)),
                ("author".into(), Value::String("user".into())),
                (
                    "emoji".into(),
                    emoji.map(Value::String).unwrap_or(Value::Null),
                ),
            ]);
            if let Some(row_id) = row_id {
                let value = row_id
                    .parse::<i64>()
                    .map(Value::from)
                    .unwrap_or_else(|_| Value::String(row_id));
                params.insert("row_id".into(), value);
            } else {
                params.insert("newest_role".into(), Value::String(role.into()));
            }
            let result: SessionReactionResult = self
                .client()?
                .request_with_timeout(
                    "message.react",
                    Value::Object(params),
                    std::time::Duration::from_secs(30),
                )
                .await
                .map_err(transport)?;
            if result.row_id.is_empty() {
                return Err(ServiceError::Transport(
                    "reaction response did not identify a message row".into(),
                ));
            }
            Ok(result)
        })
    }

    fn submit(&self, session_id: &str, text: &str) -> ServiceFuture<'_, ()> {
        let session_id = session_id.to_owned();
        let text = text.to_owned();
        Box::pin(async move {
            validate_identifier(&session_id, "session")?;
            if text.trim().is_empty() || text.len() > 1_000_000 {
                return Err(ServiceError::InvalidInput("invalid prompt".into()));
            }
            let _: Value = self
                .client()?
                .request_with_timeout(
                    "prompt.submit",
                    json!({ "session_id": session_id, "text": text }),
                    std::time::Duration::from_mins(30),
                )
                .await
                .map_err(transport)?;
            Ok(())
        })
    }

    fn interrupt(&self, session_id: &str) -> ServiceFuture<'_, ()> {
        let session_id = session_id.to_owned();
        Box::pin(async move {
            validate_identifier(&session_id, "session")?;
            let _: Value = self
                .client()?
                .request("session.interrupt", json!({ "session_id": session_id }))
                .await
                .map_err(transport)?;
            Ok(())
        })
    }

    fn set_pinned(&self, session_id: &str, pinned: bool) -> ServiceFuture<'_, ()> {
        let session_id = session_id.to_owned();
        Box::pin(async move {
            validate_identifier(&session_id, "session")?;
            self.rest()?
                .request(
                    Method::PATCH,
                    &format!("/api/sessions/{session_id}"),
                    Some(json!({ "pinned": pinned })),
                )
                .await?;
            Ok(())
        })
    }

    fn set_archived(&self, session_id: &str, archived: bool) -> ServiceFuture<'_, ()> {
        let session_id = session_id.to_owned();
        Box::pin(async move {
            validate_identifier(&session_id, "session")?;
            self.rest()?
                .request(
                    Method::PATCH,
                    &format!("/api/sessions/{session_id}"),
                    Some(json!({ "archived": archived })),
                )
                .await?;
            Ok(())
        })
    }

    fn rename(
        &self,
        session_id: &str,
        runtime_id: Option<&str>,
        title: &str,
    ) -> ServiceFuture<'_, ()> {
        let session_id = session_id.to_owned();
        let runtime_id = runtime_id.map(str::to_owned);
        let title = title.to_owned();
        Box::pin(async move {
            validate_identifier(&session_id, "session")?;
            if let Some(runtime_id) = &runtime_id {
                validate_identifier(runtime_id, "runtime session")?;
            }
            if title.len() > 512 || title.chars().any(char::is_control) {
                return Err(ServiceError::InvalidInput("invalid session title".into()));
            }

            if !title.is_empty()
                && let Some(runtime_id) = runtime_id
            {
                let runtime_rename = self
                    .client()?
                    .request::<_, Value>(
                        "session.title",
                        json!({ "session_id": runtime_id, "title": title }),
                    )
                    .await;
                if runtime_rename.is_ok() {
                    return Ok(());
                }
            }

            self.rest()?
                .request(
                    Method::PATCH,
                    &format!("/api/sessions/{session_id}"),
                    Some(json!({ "title": title })),
                )
                .await?;
            Ok(())
        })
    }

    fn delete(&self, session_id: &str) -> ServiceFuture<'_, ()> {
        let session_id = session_id.to_owned();
        Box::pin(async move {
            validate_identifier(&session_id, "session")?;
            self.rest()?
                .request(Method::DELETE, &format!("/api/sessions/{session_id}"), None)
                .await?;
            Ok(())
        })
    }

    fn events(&self) -> ServiceResult<EventStream> {
        let mut receiver = self.client()?.subscribe();
        Ok(Box::pin(async_stream::stream! {
            loop {
                match receiver.recv().await {
                    Ok(event) => yield event,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }))
    }
}

impl ProjectService for GatewayServices {
    fn snapshot(&self) -> ServiceFuture<'_, ProjectsSnapshot> {
        Box::pin(async move {
            let value: Value = self
                .client()?
                .request("projects.centre", json!({}))
                .await
                .map_err(transport)?;
            serde_json::from_value(value).map_err(protocol)
        })
    }

    fn create(&self, name: &str, folders: &[String]) -> ServiceFuture<'_, ProjectSummary> {
        let name = name.to_owned();
        let folders = folders.to_vec();
        Box::pin(async move {
            validate_identifier(&name, "project name")?;
            let value: Value = self
                .client()?
                .request(
                    "projects.create",
                    json!({
                        "name": name,
                        "folders": folders,
                        "primary_path": folders.first(),
                        "use": false
                    }),
                )
                .await
                .map_err(transport)?;
            serde_json::from_value(value.get("project").cloned().unwrap_or(value)).map_err(protocol)
        })
    }

    fn clone_repository(
        &self,
        name: &str,
        repository_url: &str,
        parent_path: &str,
    ) -> ServiceFuture<'_, ProjectSummary> {
        let name = name.to_owned();
        let repository_url = repository_url.to_owned();
        let parent_path = parent_path.to_owned();
        Box::pin(async move {
            validate_identifier(&name, "project name")?;
            if repository_url.trim().is_empty() || parent_path.trim().is_empty() {
                return Err(ServiceError::InvalidInput(
                    "repository URL and parent folder are required".into(),
                ));
            }
            let value: Value = self
                .client()?
                .request(
                    "projects.clone",
                    json!({
                        "name": name,
                        "repository_url": repository_url,
                        "parent_path": parent_path,
                        "use": true
                    }),
                )
                .await
                .map_err(transport)?;
            serde_json::from_value(value.get("project").cloned().unwrap_or(value)).map_err(protocol)
        })
    }

    fn set_active(&self, id: Option<&str>) -> ServiceFuture<'_, ()> {
        let id = id.map(str::to_owned);
        Box::pin(async move {
            if let Some(id) = &id {
                validate_identifier(id, "project")?;
            }
            let _: Value = self
                .client()?
                .request("projects.set_active", json!({ "id": id }))
                .await
                .map_err(transport)?;
            Ok(())
        })
    }

    fn set_pinned(&self, id: &str, pinned: bool) -> ServiceFuture<'_, ProjectsSnapshot> {
        let id = id.to_owned();
        Box::pin(async move {
            validate_identifier(&id, "project")?;
            let value: Value = self
                .client()?
                .request("projects.pin", json!({ "id": id, "pinned": pinned }))
                .await
                .map_err(transport)?;
            serde_json::from_value(value).map_err(protocol)
        })
    }

    fn set_archived(&self, id: &str, archived: bool) -> ServiceFuture<'_, ProjectsSnapshot> {
        let id = id.to_owned();
        Box::pin(async move {
            validate_identifier(&id, "project")?;
            let value: Value = self
                .client()?
                .request(
                    "projects.archive",
                    json!({ "id": id, "restore": !archived }),
                )
                .await
                .map_err(transport)?;
            serde_json::from_value(value).map_err(protocol)
        })
    }

    fn recover_path(
        &self,
        id: &str,
        old_path: &str,
        new_path: &str,
        repository_id: Option<&str>,
    ) -> ServiceFuture<'_, ProjectSummary> {
        let id = id.to_owned();
        let old_path = old_path.to_owned();
        let new_path = new_path.to_owned();
        let repository_id = repository_id.map(str::to_owned);
        Box::pin(async move {
            validate_identifier(&id, "project")?;
            if old_path.trim().is_empty() || new_path.trim().is_empty() {
                return Err(ServiceError::InvalidInput(
                    "old and replacement project paths are required".into(),
                ));
            }
            let value: Value = self
                .client()?
                .request(
                    "projects.recover_path",
                    json!({
                        "id": id,
                        "old_path": old_path,
                        "new_path": new_path,
                        "repository_id": repository_id
                    }),
                )
                .await
                .map_err(transport)?;
            let project = value.get("project").cloned().ok_or_else(|| {
                ServiceError::Transport("Project path recovery returned no project".into())
            })?;
            serde_json::from_value(project).map_err(protocol)
        })
    }

    fn remove(&self, id: &str) -> ServiceFuture<'_, ()> {
        let id = id.to_owned();
        Box::pin(async move {
            validate_identifier(&id, "project")?;
            let _: Value = self
                .client()?
                .request("projects.remove", json!({ "id": id }))
                .await
                .map_err(transport)?;
            Ok(())
        })
    }

    fn delete_files(
        &self,
        id: &str,
        confirmation: &str,
    ) -> ServiceFuture<'_, ProjectFilesDeleteResult> {
        let id = id.to_owned();
        let confirmation = confirmation.to_owned();
        Box::pin(async move {
            validate_identifier(&id, "project")?;
            if !confirmation.starts_with("DELETE ") || confirmation.len() > 512 {
                return Err(ServiceError::InvalidInput(
                    "invalid project file-deletion confirmation".into(),
                ));
            }
            let value: Value = self
                .client()?
                .request(
                    "projects.delete_files",
                    json!({ "id": id, "confirmation": confirmation }),
                )
                .await
                .map_err(transport)?;
            let deleted_paths = value
                .get("deleted_paths")
                .cloned()
                .map(serde_json::from_value)
                .transpose()
                .map_err(protocol)?
                .unwrap_or_default();
            let snapshot = serde_json::from_value(value).map_err(protocol)?;
            Ok(ProjectFilesDeleteResult {
                snapshot,
                deleted_paths,
            })
        })
    }
}

impl AgentConfigService for GatewayServices {
    fn load(&self, profile: Option<&str>) -> ServiceFuture<'_, AgentConfigSnapshot> {
        let profile = profile.map(str::to_owned);
        Box::pin(async move {
            let rest = self.rest()?;
            let config = rest
                .request(
                    Method::GET,
                    &profiled_path("/api/config", profile.as_deref()),
                    None,
                )
                .await?;
            let defaults = rest
                .request(
                    Method::GET,
                    &profiled_path("/api/config/defaults", profile.as_deref()),
                    None,
                )
                .await?;
            let schema = rest
                .request(
                    Method::GET,
                    &profiled_path("/api/config/schema", profile.as_deref()),
                    None,
                )
                .await?;
            Ok(AgentConfigSnapshot {
                config: serde_json::from_value(config).map_err(protocol)?,
                defaults: serde_json::from_value(defaults).map_err(protocol)?,
                schema: serde_json::from_value::<ConfigSchemaResponse>(schema).map_err(protocol)?,
            })
        })
    }

    fn save(
        &self,
        profile: Option<&str>,
        config: &std::collections::BTreeMap<String, Value>,
    ) -> ServiceFuture<'_, ()> {
        let profile = profile.map(str::to_owned);
        let config = config.clone();
        Box::pin(async move {
            let response = self
                .rest()?
                .request(
                    Method::PUT,
                    &profiled_path("/api/config", profile.as_deref()),
                    Some(json!({ "config": config })),
                )
                .await?;
            if response.get("ok").and_then(Value::as_bool) != Some(true) {
                return Err(ServiceError::Transport(
                    "Hermes Agent did not confirm the config save".into(),
                ));
            }
            Ok(())
        })
    }
}

impl ModelService for GatewayServices {
    fn load(&self, profile: Option<&str>) -> ServiceFuture<'_, ModelSettingsSnapshot> {
        let profile = profile.map(str::to_owned);
        Box::pin(async move {
            let rest = self.rest()?;
            let info = rest
                .request(
                    Method::GET,
                    &profiled_path("/api/model/info", profile.as_deref()),
                    None,
                )
                .await?;
            let options = rest
                .request(Method::GET, &model_options_path(profile.as_deref()), None)
                .await?;
            let auxiliary = rest
                .request(
                    Method::GET,
                    &profiled_path("/api/model/auxiliary", profile.as_deref()),
                    None,
                )
                .await?;
            let moa = match rest
                .request(
                    Method::GET,
                    &profiled_path("/api/model/moa", profile.as_deref()),
                    None,
                )
                .await
            {
                Ok(value) => serde_json::from_value::<MoaConfig>(value).ok(),
                Err(_) => None,
            };
            Ok(ModelSettingsSnapshot {
                info: serde_json::from_value::<ModelInfo>(info).map_err(protocol)?,
                options: serde_json::from_value::<ModelOptions>(options).map_err(protocol)?,
                auxiliary: serde_json::from_value::<AuxiliaryModels>(auxiliary)
                    .map_err(protocol)?,
                moa,
            })
        })
    }

    fn assign(
        &self,
        profile: Option<&str>,
        request: &ModelAssignmentRequest,
    ) -> ServiceFuture<'_, ModelAssignmentResponse> {
        let profile = profile.map(str::to_owned);
        let request = request.clone();
        Box::pin(async move {
            if !matches!(request.scope.as_str(), "main" | "auxiliary") {
                return Err(ServiceError::InvalidInput("invalid model scope".into()));
            }
            for (field, value) in [
                ("model", request.model.as_str()),
                ("provider", request.provider.as_str()),
            ] {
                if value.trim().is_empty()
                    || value.len() > 1_024
                    || value.chars().any(char::is_control)
                {
                    return Err(ServiceError::InvalidInput(format!("invalid {field}")));
                }
            }
            if let Some(task) = &request.task {
                validate_identifier(task, "model task")?;
            }
            let value = self
                .rest()?
                .request(
                    Method::POST,
                    &profiled_path("/api/model/set", profile.as_deref()),
                    Some(serde_json::to_value(request).map_err(protocol)?),
                )
                .await?;
            let response: ModelAssignmentResponse =
                serde_json::from_value(value).map_err(protocol)?;
            if !response.ok {
                return Err(ServiceError::Transport(
                    "Hermes Agent did not confirm the model assignment".into(),
                ));
            }
            Ok(response)
        })
    }

    fn save_moa(&self, profile: Option<&str>, config: &MoaConfig) -> ServiceFuture<'_, MoaConfig> {
        let profile = profile.map(str::to_owned);
        let config = config.clone();
        Box::pin(async move {
            let value = self
                .rest()?
                .request(
                    Method::PUT,
                    &profiled_path("/api/model/moa", profile.as_deref()),
                    Some(serde_json::to_value(config).map_err(protocol)?),
                )
                .await?;
            if value.get("ok").and_then(Value::as_bool) == Some(false) {
                return Err(ServiceError::Transport(
                    "Hermes Agent did not confirm the MoA save".into(),
                ));
            }
            serde_json::from_value(value).map_err(protocol)
        })
    }
}

impl ProviderService for GatewayServices {
    fn list_oauth(&self, profile: Option<&str>) -> ServiceFuture<'_, Vec<OAuthProvider>> {
        let profile = profile.map(str::to_owned);
        Box::pin(async move {
            let value = self
                .rest()?
                .request(
                    Method::GET,
                    &profiled_path("/api/providers/oauth", profile.as_deref()),
                    None,
                )
                .await?;
            decode_list(value, "providers")
        })
    }

    fn start_oauth(
        &self,
        profile: Option<&str>,
        provider_id: &str,
    ) -> ServiceFuture<'_, OAuthStart> {
        let profile = profile.map(str::to_owned);
        let provider_id = provider_id.to_owned();
        Box::pin(async move {
            validate_path_id(&provider_id, "provider")?;
            let value = self
                .rest()?
                .request(
                    Method::POST,
                    &profiled_path(
                        &format!("/api/providers/oauth/{provider_id}/start"),
                        profile.as_deref(),
                    ),
                    Some(json!({})),
                )
                .await?;
            serde_json::from_value(value).map_err(protocol)
        })
    }

    fn submit_oauth(
        &self,
        profile: Option<&str>,
        provider_id: &str,
        session_id: &str,
        code: &str,
    ) -> ServiceFuture<'_, OAuthSubmit> {
        let profile = profile.map(str::to_owned);
        let provider_id = provider_id.to_owned();
        let session_id = session_id.to_owned();
        let code = code.to_owned();
        Box::pin(async move {
            validate_path_id(&provider_id, "provider")?;
            validate_oauth_session(&session_id)?;
            if code.trim().is_empty() || code.len() > 16_384 || code.chars().any(char::is_control) {
                return Err(ServiceError::InvalidInput(
                    "invalid authorization code".into(),
                ));
            }
            let value = self
                .rest()?
                .request(
                    Method::POST,
                    &profiled_path(
                        &format!("/api/providers/oauth/{provider_id}/submit"),
                        profile.as_deref(),
                    ),
                    Some(json!({ "session_id": session_id, "code": code.trim() })),
                )
                .await?;
            serde_json::from_value(value).map_err(protocol)
        })
    }

    fn poll_oauth(
        &self,
        profile: Option<&str>,
        provider_id: &str,
        session_id: &str,
    ) -> ServiceFuture<'_, OAuthPoll> {
        let profile = profile.map(str::to_owned);
        let provider_id = provider_id.to_owned();
        let session_id = session_id.to_owned();
        Box::pin(async move {
            validate_path_id(&provider_id, "provider")?;
            validate_oauth_session(&session_id)?;
            let value = self
                .rest()?
                .request(
                    Method::GET,
                    &profiled_path(
                        &format!("/api/providers/oauth/{provider_id}/poll/{session_id}"),
                        profile.as_deref(),
                    ),
                    None,
                )
                .await?;
            serde_json::from_value(value).map_err(protocol)
        })
    }

    fn cancel_oauth(&self, profile: Option<&str>, session_id: &str) -> ServiceFuture<'_, ()> {
        let profile = profile.map(str::to_owned);
        let session_id = session_id.to_owned();
        Box::pin(async move {
            validate_oauth_session(&session_id)?;
            let value = self
                .rest()?
                .request(
                    Method::DELETE,
                    &profiled_path(
                        &format!("/api/providers/oauth/sessions/{session_id}"),
                        profile.as_deref(),
                    ),
                    None,
                )
                .await?;
            require_confirmation(&value, "OAuth cancellation")
        })
    }

    fn disconnect_oauth(&self, profile: Option<&str>, provider_id: &str) -> ServiceFuture<'_, ()> {
        let profile = profile.map(str::to_owned);
        let provider_id = provider_id.to_owned();
        Box::pin(async move {
            validate_path_id(&provider_id, "provider")?;
            let value = self
                .rest()?
                .request(
                    Method::DELETE,
                    &profiled_path(
                        &format!("/api/providers/oauth/{provider_id}"),
                        profile.as_deref(),
                    ),
                    None,
                )
                .await?;
            require_confirmation(&value, "OAuth disconnect")
        })
    }

    fn env(
        &self,
        profile: Option<&str>,
    ) -> ServiceFuture<'_, std::collections::BTreeMap<String, EnvVarInfo>> {
        let profile = profile.map(str::to_owned);
        Box::pin(async move {
            let value = self
                .rest()?
                .request(
                    Method::GET,
                    &profiled_path("/api/env", profile.as_deref()),
                    None,
                )
                .await?;
            serde_json::from_value(value).map_err(protocol)
        })
    }

    fn set_env(&self, profile: Option<&str>, key: &str, value: &str) -> ServiceFuture<'_, ()> {
        let profile = profile.map(str::to_owned);
        let key = key.to_owned();
        let value = value.to_owned();
        Box::pin(async move {
            validate_env_key(&key)?;
            if value.trim().is_empty()
                || value.len() > 65_536
                || value.chars().any(char::is_control)
            {
                return Err(ServiceError::InvalidInput(
                    "credential value must be non-empty plain text".into(),
                ));
            }
            let response = self
                .rest()?
                .request(
                    Method::PUT,
                    &profiled_path("/api/env", profile.as_deref()),
                    Some(json!({ "key": key, "value": value })),
                )
                .await?;
            require_confirmation(&response, "credential save")
        })
    }

    fn delete_env(&self, profile: Option<&str>, key: &str) -> ServiceFuture<'_, ()> {
        let profile = profile.map(str::to_owned);
        let key = key.to_owned();
        Box::pin(async move {
            validate_env_key(&key)?;
            let response = self
                .rest()?
                .request(
                    Method::DELETE,
                    &profiled_path("/api/env", profile.as_deref()),
                    Some(json!({ "key": key })),
                )
                .await?;
            require_confirmation(&response, "credential removal")
        })
    }

    fn reveal_env(&self, profile: Option<&str>, key: &str) -> ServiceFuture<'_, String> {
        let profile = profile.map(str::to_owned);
        let key = key.to_owned();
        Box::pin(async move {
            validate_env_key(&key)?;
            let response = self
                .rest()?
                .request(
                    Method::POST,
                    &profiled_path("/api/env/reveal", profile.as_deref()),
                    Some(json!({ "key": key })),
                )
                .await?;
            response
                .get("value")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| protocol_missing("credential reveal value"))
        })
    }

    fn custom_endpoints(&self) -> ServiceFuture<'_, CustomEndpointsResponse> {
        Box::pin(async move {
            let value = self
                .rest()?
                .request(Method::GET, "/api/providers/custom-endpoints", None)
                .await?;
            serde_json::from_value(value).map_err(protocol)
        })
    }

    fn save_custom_endpoint(
        &self,
        endpoint: &CustomEndpointUpdate,
    ) -> ServiceFuture<'_, CustomEndpointsResponse> {
        let endpoint = endpoint.clone();
        Box::pin(async move {
            validate_custom_endpoint(&endpoint)?;
            let value = self
                .rest()?
                .request(
                    Method::POST,
                    "/api/providers/custom-endpoints",
                    Some(serde_json::to_value(endpoint).map_err(protocol)?),
                )
                .await?;
            serde_json::from_value(value).map_err(protocol)
        })
    }

    fn validate_custom_endpoint(
        &self,
        endpoint: &CustomEndpointUpdate,
    ) -> ServiceFuture<'_, CustomEndpointValidation> {
        let endpoint = endpoint.clone();
        Box::pin(async move {
            validate_custom_endpoint(&endpoint)?;
            let value = self
                .rest()?
                .request(
                    Method::POST,
                    "/api/providers/custom-endpoints/validate",
                    Some(serde_json::to_value(endpoint).map_err(protocol)?),
                )
                .await?;
            serde_json::from_value(value).map_err(protocol)
        })
    }

    fn activate_custom_endpoint(&self, id: &str) -> ServiceFuture<'_, ProviderActivation> {
        let id = id.to_owned();
        Box::pin(async move {
            validate_path_id(&id, "custom endpoint")?;
            let value = self
                .rest()?
                .request(
                    Method::POST,
                    &format!("/api/providers/custom-endpoints/{id}/activate"),
                    None,
                )
                .await?;
            let response: ProviderActivation = serde_json::from_value(value).map_err(protocol)?;
            if !response.ok {
                return Err(ServiceError::Transport(
                    "Hermes Agent did not confirm custom endpoint activation".into(),
                ));
            }
            Ok(response)
        })
    }

    fn delete_custom_endpoint(&self, id: &str) -> ServiceFuture<'_, CustomEndpointsResponse> {
        let id = id.to_owned();
        Box::pin(async move {
            validate_path_id(&id, "custom endpoint")?;
            let value = self
                .rest()?
                .request(
                    Method::DELETE,
                    &format!("/api/providers/custom-endpoints/{id}"),
                    None,
                )
                .await?;
            serde_json::from_value(value).map_err(protocol)
        })
    }
}

impl RuntimeService for GatewayServices {
    fn status(&self) -> ServiceFuture<'_, RuntimeStatus> {
        Box::pin(async move {
            self.client()?
                .request("status.get", json!({}))
                .await
                .map_err(transport)
        })
    }

    fn actions(&self) -> ServiceFuture<'_, Vec<TaskSummary>> {
        Box::pin(async move {
            let value: Value = self
                .client()?
                .request("tasks.list", json!({}))
                .await
                .map_err(transport)?;
            decode_list(value, "tasks")
        })
    }

    fn start_action(&self, action: &str, input: Value) -> ServiceFuture<'_, TaskSummary> {
        let action = action.to_owned();
        Box::pin(async move {
            validate_identifier(&action, "action")?;
            self.client()?
                .request("tasks.start", json!({ "action": action, "input": input }))
                .await
                .map_err(transport)
        })
    }

    fn cancel_action(&self, id: &str) -> ServiceFuture<'_, ()> {
        let id = id.to_owned();
        Box::pin(async move {
            validate_identifier(&id, "task")?;
            let _: Value = self
                .client()?
                .request("tasks.cancel", json!({ "task_id": id }))
                .await
                .map_err(transport)?;
            Ok(())
        })
    }
}

const SKILLS_REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
const SKILLS_HUB_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(45);
const MAX_SKILLS_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_SKILL_NAME_BYTES: usize = 512;
const MAX_SKILL_IDENTIFIER_BYTES: usize = 4096;
const MAX_SKILL_SEARCH_BYTES: usize = 4096;
const MAX_SKILL_SOURCE_BYTES: usize = 256;
const MAX_SKILL_SEARCH_LIMIT: u32 = 1000;
const MAX_SKILL_ACTION_LINES: u32 = 5000;

fn checked_skill_text(
    value: &str,
    max_bytes: usize,
    field: &str,
    allow_empty: bool,
) -> ServiceResult<String> {
    let value = value.trim();
    if (!allow_empty && value.is_empty())
        || value.len() > max_bytes
        || value.chars().any(char::is_control)
    {
        return Err(ServiceError::InvalidInput(format!("invalid {field}")));
    }
    Ok(value.to_owned())
}

fn skills_query_path(path: &str, profile: Option<&str>, query: &[(&str, &str)]) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    if let Some(profile) = profile.filter(|value| !value.is_empty()) {
        serializer.append_pair("profile", profile);
    }
    for (key, value) in query {
        serializer.append_pair(key, value);
    }
    let query = serializer.finish();
    if query.is_empty() {
        path.to_owned()
    } else {
        format!("{path}?{query}")
    }
}

fn skill_action_status_path(
    name: &str,
    profile: Option<&str>,
    lines: u32,
) -> ServiceResult<String> {
    let name = checked_skill_text(name, MAX_SKILL_NAME_BYTES, "Skills action name", false)?;
    if lines == 0 || lines > MAX_SKILL_ACTION_LINES {
        return Err(ServiceError::InvalidInput(format!(
            "Skills action log line count must be between 1 and {MAX_SKILL_ACTION_LINES}"
        )));
    }
    let mut url = url::Url::parse("http://skills.invalid/")
        .map_err(|error| ServiceError::Platform(error.to_string()))?;
    {
        let mut segments = url
            .path_segments_mut()
            .map_err(|()| ServiceError::Platform("could not encode Skills action path".into()))?;
        segments.push("api");
        segments.push("actions");
        segments.push(&name);
        segments.push("status");
    }
    if let Some(profile) = profile.filter(|value| !value.is_empty()) {
        url.query_pairs_mut().append_pair("profile", profile);
    }
    url.query_pairs_mut()
        .append_pair("lines", &lines.to_string());
    Ok(match url.query() {
        Some(query) => format!("{}?{query}", url.path()),
        None => url.path().to_owned(),
    })
}

impl SkillsService for GatewayServices {
    fn list(&self, profile: Option<&str>) -> ServiceFuture<'_, Vec<SkillSummary>> {
        let profile = profile.map(str::to_owned);
        Box::pin(async move {
            let profile = validated_profile(profile.as_deref())?;
            let value = self
                .rest()?
                .request_bounded(
                    Method::GET,
                    &skills_query_path("/api/skills", profile.as_deref(), &[]),
                    None,
                    SKILLS_REQUEST_TIMEOUT,
                    MAX_SKILLS_RESPONSE_BYTES,
                )
                .await?;
            serde_json::from_value(value).map_err(protocol)
        })
    }

    fn set_enabled(
        &self,
        profile: Option<&str>,
        name: &str,
        enabled: bool,
    ) -> ServiceFuture<'_, SkillToggleResult> {
        let profile = profile.map(str::to_owned);
        let name = name.to_owned();
        Box::pin(async move {
            let profile = validated_profile(profile.as_deref())?;
            let name = checked_skill_text(&name, MAX_SKILL_NAME_BYTES, "skill name", false)?;
            let value = self
                .rest()?
                .request_bounded(
                    Method::PUT,
                    &skills_query_path("/api/skills/toggle", profile.as_deref(), &[]),
                    Some(json!({ "name": name, "enabled": enabled })),
                    SKILLS_REQUEST_TIMEOUT,
                    MAX_SKILLS_RESPONSE_BYTES,
                )
                .await?;
            serde_json::from_value(value).map_err(protocol)
        })
    }

    fn hub_sources(&self, profile: Option<&str>) -> ServiceFuture<'_, SkillHubSourcesResponse> {
        let profile = profile.map(str::to_owned);
        Box::pin(async move {
            let profile = validated_profile(profile.as_deref())?;
            let value = self
                .rest()?
                .request_bounded(
                    Method::GET,
                    &skills_query_path("/api/skills/hub/sources", profile.as_deref(), &[]),
                    None,
                    SKILLS_HUB_TIMEOUT,
                    MAX_SKILLS_RESPONSE_BYTES,
                )
                .await?;
            serde_json::from_value(value).map_err(protocol)
        })
    }

    fn hub_search(
        &self,
        profile: Option<&str>,
        query: &str,
        source: &str,
        limit: u32,
    ) -> ServiceFuture<'_, SkillHubSearchResponse> {
        let profile = profile.map(str::to_owned);
        let query = query.to_owned();
        let source = source.to_owned();
        Box::pin(async move {
            let profile = validated_profile(profile.as_deref())?;
            let query = checked_skill_text(
                &query,
                MAX_SKILL_SEARCH_BYTES,
                "Skills Hub search query",
                true,
            )?;
            let source =
                checked_skill_text(&source, MAX_SKILL_SOURCE_BYTES, "Skills Hub source", false)?;
            if limit == 0 || limit > MAX_SKILL_SEARCH_LIMIT {
                return Err(ServiceError::InvalidInput(format!(
                    "Skills Hub search limit must be between 1 and {MAX_SKILL_SEARCH_LIMIT}"
                )));
            }
            let limit_text = limit.to_string();
            let value = self
                .rest()?
                .request_bounded(
                    Method::GET,
                    &skills_query_path(
                        "/api/skills/hub/search",
                        profile.as_deref(),
                        &[("q", &query), ("source", &source), ("limit", &limit_text)],
                    ),
                    None,
                    SKILLS_HUB_TIMEOUT,
                    MAX_SKILLS_RESPONSE_BYTES,
                )
                .await?;
            serde_json::from_value(value).map_err(protocol)
        })
    }

    fn hub_preview(
        &self,
        profile: Option<&str>,
        identifier: &str,
    ) -> ServiceFuture<'_, SkillHubPreview> {
        let profile = profile.map(str::to_owned);
        let identifier = identifier.to_owned();
        Box::pin(async move {
            let profile = validated_profile(profile.as_deref())?;
            let identifier = checked_skill_text(
                &identifier,
                MAX_SKILL_IDENTIFIER_BYTES,
                "Skills Hub identifier",
                false,
            )?;
            let value = self
                .rest()?
                .request_bounded(
                    Method::GET,
                    &skills_query_path(
                        "/api/skills/hub/preview",
                        profile.as_deref(),
                        &[("identifier", &identifier)],
                    ),
                    None,
                    SKILLS_HUB_TIMEOUT,
                    MAX_SKILLS_RESPONSE_BYTES,
                )
                .await?;
            serde_json::from_value(value).map_err(protocol)
        })
    }

    fn hub_scan(
        &self,
        profile: Option<&str>,
        identifier: &str,
    ) -> ServiceFuture<'_, SkillHubScanResult> {
        let profile = profile.map(str::to_owned);
        let identifier = identifier.to_owned();
        Box::pin(async move {
            let profile = validated_profile(profile.as_deref())?;
            let identifier = checked_skill_text(
                &identifier,
                MAX_SKILL_IDENTIFIER_BYTES,
                "Skills Hub identifier",
                false,
            )?;
            let value = self
                .rest()?
                .request_bounded(
                    Method::GET,
                    &skills_query_path(
                        "/api/skills/hub/scan",
                        profile.as_deref(),
                        &[("identifier", &identifier)],
                    ),
                    None,
                    SKILLS_HUB_TIMEOUT,
                    MAX_SKILLS_RESPONSE_BYTES,
                )
                .await?;
            serde_json::from_value(value).map_err(protocol)
        })
    }

    fn hub_install(
        &self,
        profile: Option<&str>,
        identifier: &str,
    ) -> ServiceFuture<'_, SkillActionStart> {
        let profile = profile.map(str::to_owned);
        let identifier = identifier.to_owned();
        Box::pin(async move {
            let profile = validated_profile(profile.as_deref())?;
            let identifier = checked_skill_text(
                &identifier,
                MAX_SKILL_IDENTIFIER_BYTES,
                "Skills Hub identifier",
                false,
            )?;
            let value = self
                .rest()?
                .request_bounded(
                    Method::POST,
                    &skills_query_path("/api/skills/hub/install", profile.as_deref(), &[]),
                    Some(json!({ "identifier": identifier })),
                    SKILLS_REQUEST_TIMEOUT,
                    MAX_SKILLS_RESPONSE_BYTES,
                )
                .await?;
            serde_json::from_value(value).map_err(protocol)
        })
    }

    fn hub_uninstall(
        &self,
        profile: Option<&str>,
        name: &str,
    ) -> ServiceFuture<'_, SkillActionStart> {
        let profile = profile.map(str::to_owned);
        let name = name.to_owned();
        Box::pin(async move {
            let profile = validated_profile(profile.as_deref())?;
            let name = checked_skill_text(&name, MAX_SKILL_NAME_BYTES, "skill name", false)?;
            let value = self
                .rest()?
                .request_bounded(
                    Method::POST,
                    &skills_query_path("/api/skills/hub/uninstall", profile.as_deref(), &[]),
                    Some(json!({ "name": name })),
                    SKILLS_REQUEST_TIMEOUT,
                    MAX_SKILLS_RESPONSE_BYTES,
                )
                .await?;
            serde_json::from_value(value).map_err(protocol)
        })
    }

    fn hub_update(&self, profile: Option<&str>) -> ServiceFuture<'_, SkillActionStart> {
        let profile = profile.map(str::to_owned);
        Box::pin(async move {
            let profile = validated_profile(profile.as_deref())?;
            let value = self
                .rest()?
                .request_bounded(
                    Method::POST,
                    &skills_query_path("/api/skills/hub/update", profile.as_deref(), &[]),
                    Some(json!({})),
                    SKILLS_REQUEST_TIMEOUT,
                    MAX_SKILLS_RESPONSE_BYTES,
                )
                .await?;
            serde_json::from_value(value).map_err(protocol)
        })
    }

    fn action_status(
        &self,
        profile: Option<&str>,
        name: &str,
        lines: u32,
    ) -> ServiceFuture<'_, SkillActionStatus> {
        let profile = profile.map(str::to_owned);
        let name = name.to_owned();
        Box::pin(async move {
            let profile = validated_profile(profile.as_deref())?;
            let path = skill_action_status_path(&name, profile.as_deref(), lines)?;
            let value = self
                .rest()?
                .request_bounded(
                    Method::GET,
                    &path,
                    None,
                    SKILLS_REQUEST_TIMEOUT,
                    MAX_SKILLS_RESPONSE_BYTES,
                )
                .await?;
            serde_json::from_value(value).map_err(protocol)
        })
    }
}

impl TrustService for GatewayServices {
    fn snapshot(&self) -> ServiceFuture<'_, TrustSnapshot> {
        Box::pin(async move {
            self.client()?
                .request("trust.get", json!({}))
                .await
                .map_err(transport)
        })
    }

    fn set_policy(&self, policy: &str) -> ServiceFuture<'_, TrustSnapshot> {
        let policy = policy.to_owned();
        Box::pin(async move {
            validate_identifier(&policy, "trust policy")?;
            self.client()?
                .request("trust.set_policy", json!({ "policy": policy }))
                .await
                .map_err(transport)
        })
    }
}

struct JsonSettings {
    path: PathBuf,
}

impl JsonSettings {
    const fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl SettingsService for JsonSettings {
    fn load(&self) -> ServiceFuture<'_, AppSettings> {
        Box::pin(async move {
            match fs::read(&self.path) {
                Ok(bytes) => serde_json::from_slice(&bytes).map_err(protocol),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    Ok(AppSettings::default())
                }
                Err(error) => Err(platform(error)),
            }
        })
    }

    fn save(&self, settings: &AppSettings) -> ServiceFuture<'_, ()> {
        let settings = settings.clone();
        Box::pin(async move {
            let parent = self
                .path
                .parent()
                .ok_or_else(|| ServiceError::Platform("settings path has no parent".into()))?;
            fs::create_dir_all(parent).map_err(platform)?;
            let bytes = serde_json::to_vec_pretty(&settings).map_err(protocol)?;
            let temporary = self.path.with_extension("json.tmp");
            fs::write(&temporary, bytes).map_err(platform)?;
            fs::rename(&temporary, &self.path).map_err(platform)
        })
    }
}

fn reveal_native_path(target: &Path) -> ServiceResult<()> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer.exe")
            .arg(format!("/select,{}", target.display()))
            .spawn()
            .map(|_| ())
            .map_err(platform)
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(target)
            .spawn()
            .map(|_| ())
            .map_err(platform)
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let parent = target.parent().unwrap_or(target);
        open::that(parent).map_err(|error| ServiceError::Platform(error.to_string()))
    }
}

struct DesktopFiles;

impl FileService for DesktopFiles {
    fn read_dir(&self, root: &Path, relative: &Path) -> ServiceFuture<'_, Vec<FileEntry>> {
        let root = root.to_owned();
        let relative = relative.to_owned();
        Box::pin(async move {
            let target = contained_existing(&root, &relative)?;
            let mut entries = fs::read_dir(target)
                .map_err(platform)?
                .map(|entry| {
                    let entry = entry.map_err(platform)?;
                    let metadata = entry.metadata().map_err(platform)?;
                    let path = relative.join(entry.file_name());
                    Ok(FileEntry {
                        path: path.to_string_lossy().replace('\\', "/"),
                        name: entry.file_name().to_string_lossy().into_owned(),
                        is_dir: metadata.is_dir(),
                        size: metadata.is_file().then_some(metadata.len()),
                    })
                })
                .collect::<ServiceResult<Vec<_>>>()?;
            entries.sort_by(|left, right| {
                right
                    .is_dir
                    .cmp(&left.is_dir)
                    .then_with(|| left.name.cmp(&right.name))
            });
            Ok(entries)
        })
    }

    fn read_text(&self, root: &Path, relative: &Path) -> ServiceFuture<'_, String> {
        let root = root.to_owned();
        let relative = relative.to_owned();
        Box::pin(async move {
            let target = contained_existing(&root, &relative)?;
            let metadata = fs::metadata(&target).map_err(platform)?;
            if metadata.len() > 10 * 1024 * 1024 {
                return Err(ServiceError::InvalidInput("file exceeds 10 MiB".into()));
            }
            fs::read_to_string(target).map_err(platform)
        })
    }

    fn write_text(&self, root: &Path, relative: &Path, content: &str) -> ServiceFuture<'_, ()> {
        let root = root.to_owned();
        let relative = relative.to_owned();
        let content = content.to_owned();
        Box::pin(async move {
            let target = contained_for_write(&root, &relative)?;
            fs::write(target, content).map_err(platform)
        })
    }

    fn rename(&self, root: &Path, relative: &Path, new_name: &str) -> ServiceFuture<'_, String> {
        let root = root.to_owned();
        let relative = relative.to_owned();
        let new_name = new_name.trim().to_owned();
        Box::pin(async move {
            if new_name.is_empty()
                || new_name == "."
                || new_name == ".."
                || new_name.contains('/')
                || new_name.contains('\\')
            {
                return Err(ServiceError::InvalidInput("Invalid rename".into()));
            }

            let source = contained_existing(&root, &relative)?;
            let parent = relative.parent().unwrap_or_else(|| Path::new(""));
            let destination_relative = parent.join(&new_name);
            let destination = contained_for_write(&root, &destination_relative)?;
            if destination.exists() {
                return Err(ServiceError::InvalidInput(format!(
                    "A file or folder named '{new_name}' already exists"
                )));
            }

            fs::rename(source, destination).map_err(platform)?;
            Ok(destination_relative.to_string_lossy().replace('\\', "/"))
        })
    }

    fn reveal(&self, root: &Path, relative: &Path) -> ServiceFuture<'_, ()> {
        let root = root.to_owned();
        let relative = relative.to_owned();
        Box::pin(async move {
            let target = contained_existing(&root, &relative)?;
            reveal_native_path(&target)
        })
    }

    fn open(&self, root: &Path, relative: &Path) -> ServiceFuture<'_, ()> {
        let root = root.to_owned();
        let relative = relative.to_owned();
        Box::pin(async move {
            let target = contained_existing(&root, &relative)?;
            open::that(target).map_err(|error| ServiceError::Platform(error.to_string()))
        })
    }

    fn trash(&self, root: &Path, relative: &Path) -> ServiceFuture<'_, ()> {
        let root = root.to_owned();
        let relative = relative.to_owned();
        Box::pin(async move {
            let target = contained_existing(&root, &relative)?;
            trash::delete(target).map_err(|error| ServiceError::Platform(error.to_string()))
        })
    }
}

struct DesktopGit;

impl GitService for DesktopGit {
    fn status(&self, repository: &Path) -> ServiceFuture<'_, GitStatus> {
        let repository = repository.to_owned();
        Box::pin(async move {
            let output = git(&repository, &["status", "--porcelain=v1", "--branch"])?;
            parse_git_status(&output)
        })
    }

    fn diff(&self, repository: &Path, relative: &Path) -> ServiceFuture<'_, String> {
        let repository = repository.to_owned();
        let relative = relative.to_owned();
        Box::pin(async move {
            validate_relative_path(&relative)?;
            git(
                &repository,
                &["diff", "--", relative.to_string_lossy().as_ref()],
            )
        })
    }

    fn diff_staged(&self, repository: &Path, relative: &Path) -> ServiceFuture<'_, String> {
        let repository = repository.to_owned();
        let relative = relative.to_owned();
        Box::pin(async move {
            validate_relative_path(&relative)?;
            git(
                &repository,
                &[
                    "diff",
                    "--cached",
                    "--",
                    relative.to_string_lossy().as_ref(),
                ],
            )
        })
    }

    fn stage(&self, repository: &Path, relative: &Path) -> ServiceFuture<'_, ()> {
        let repository = repository.to_owned();
        let relative = relative.to_owned();
        Box::pin(async move {
            validate_relative_path(&relative)?;
            git(
                &repository,
                &["add", "--", relative.to_string_lossy().as_ref()],
            )?;
            Ok(())
        })
    }

    fn unstage(&self, repository: &Path, relative: &Path) -> ServiceFuture<'_, ()> {
        let repository = repository.to_owned();
        let relative = relative.to_owned();
        Box::pin(async move {
            validate_relative_path(&relative)?;
            git(
                &repository,
                &[
                    "restore",
                    "--staged",
                    "--",
                    relative.to_string_lossy().as_ref(),
                ],
            )?;
            Ok(())
        })
    }
}

struct TerminalProcess {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
    output: Arc<Mutex<Vec<u8>>>,
    control_tail: Vec<u8>,
}

#[derive(Default)]
struct DesktopTerminals {
    processes: Mutex<HashMap<String, TerminalProcess>>,
}

impl DesktopTerminals {
    fn dispose_process(&self, id: &str) -> ServiceResult<()> {
        validate_identifier(id, "terminal")?;
        let mut process = self
            .processes
            .lock()
            .map_err(|_| ServiceError::Platform("terminal lock was poisoned".into()))?
            .remove(id)
            .ok_or_else(|| ServiceError::NotFound("terminal".into()))?;
        process
            .child
            .kill()
            .map_err(|error| ServiceError::Platform(error.to_string()))
    }
}

impl TerminalService for DesktopTerminals {
    fn start(&self, cwd: &Path, cols: u16, rows: u16) -> ServiceFuture<'_, String> {
        let cwd = cwd.to_owned();
        Box::pin(async move {
            if cols == 0 || rows == 0 || !cwd.is_dir() {
                return Err(ServiceError::InvalidInput(
                    "invalid terminal dimensions or cwd".into(),
                ));
            }
            let pair = native_pty_system()
                .openpty(PtySize {
                    rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                })
                .map_err(|error| ServiceError::Platform(error.to_string()))?;
            let shell = std::env::var_os("COMSPEC").unwrap_or_else(|| "cmd.exe".into());
            let mut command = CommandBuilder::new(shell);
            command.cwd(&cwd);
            let child = pair
                .slave
                .spawn_command(command)
                .map_err(|error| ServiceError::Platform(error.to_string()))?;
            let writer = pair
                .master
                .take_writer()
                .map_err(|error| ServiceError::Platform(error.to_string()))?;
            let mut reader = pair
                .master
                .try_clone_reader()
                .map_err(|error| ServiceError::Platform(error.to_string()))?;
            let output = Arc::new(Mutex::new(Vec::new()));
            let reader_output = output.clone();
            std::thread::spawn(move || {
                let mut buffer = [0_u8; 8192];
                while let Ok(count) = reader.read(&mut buffer) {
                    if count == 0 {
                        break;
                    }
                    if let Ok(mut output) = reader_output.lock() {
                        output.extend_from_slice(&buffer[..count]);
                        if output.len() > 2 * 1024 * 1024 {
                            let excess = output.len() - 2 * 1024 * 1024;
                            output.drain(..excess);
                        }
                    }
                }
            });
            let id = Uuid::new_v4().to_string();
            self.processes
                .lock()
                .map_err(|_| ServiceError::Platform("terminal lock was poisoned".into()))?
                .insert(
                    id.clone(),
                    TerminalProcess {
                        master: pair.master,
                        writer,
                        child,
                        output,
                        control_tail: Vec::new(),
                    },
                );
            Ok(id)
        })
    }

    fn write(&self, id: &str, data: &[u8]) -> ServiceFuture<'_, ()> {
        let id = id.to_owned();
        let data = data.to_vec();
        Box::pin(async move {
            validate_identifier(&id, "terminal")?;
            let mut processes = self
                .processes
                .lock()
                .map_err(|_| ServiceError::Platform("terminal lock was poisoned".into()))?;
            let process = processes
                .get_mut(&id)
                .ok_or_else(|| ServiceError::NotFound("terminal".into()))?;
            process.writer.write_all(&data).map_err(platform)?;
            process.writer.flush().map_err(platform)
        })
    }

    fn read(&self, id: &str) -> ServiceFuture<'_, Vec<u8>> {
        let id = id.to_owned();
        Box::pin(async move {
            validate_identifier(&id, "terminal")?;
            let mut processes = self
                .processes
                .lock()
                .map_err(|_| ServiceError::Platform("terminal lock was poisoned".into()))?;
            let process = processes
                .get_mut(&id)
                .ok_or_else(|| ServiceError::NotFound("terminal".into()))?;
            let bytes = {
                let mut output = process.output.lock().map_err(|_| {
                    ServiceError::Platform("terminal output lock was poisoned".into())
                })?;
                std::mem::take(&mut *output)
            };

            let mut control_window = Vec::with_capacity(process.control_tail.len() + bytes.len());
            control_window.extend_from_slice(&process.control_tail);
            control_window.extend_from_slice(&bytes);
            let cursor_queries = control_window
                .windows(4)
                .filter(|window| *window == b"\x1b[6n")
                .count();
            let tail_start = control_window.len().saturating_sub(3);
            process.control_tail.clear();
            process
                .control_tail
                .extend_from_slice(&control_window[tail_start..]);

            if cursor_queries > 0 {
                for _ in 0..cursor_queries {
                    process.writer.write_all(b"\x1b[1;1R").map_err(platform)?;
                }
                process.writer.flush().map_err(platform)?;
            }
            Ok(bytes)
        })
    }

    fn resize(&self, id: &str, cols: u16, rows: u16) -> ServiceFuture<'_, ()> {
        let id = id.to_owned();
        Box::pin(async move {
            validate_identifier(&id, "terminal")?;
            if cols == 0 || rows == 0 {
                return Err(ServiceError::InvalidInput(
                    "invalid terminal dimensions".into(),
                ));
            }
            let processes = self
                .processes
                .lock()
                .map_err(|_| ServiceError::Platform("terminal lock was poisoned".into()))?;
            let process = processes
                .get(&id)
                .ok_or_else(|| ServiceError::NotFound("terminal".into()))?;
            process
                .master
                .resize(PtySize {
                    rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                })
                .map_err(|error| ServiceError::Platform(error.to_string()))
        })
    }

    fn dispose(&self, id: &str) -> ServiceFuture<'_, ()> {
        let id = id.to_owned();
        Box::pin(async move { self.dispose_process(&id) })
    }

    fn dispose_now(&self, id: &str) -> ServiceResult<()> {
        self.dispose_process(id)
    }
}

struct DesktopUpdates {
    data_dir: PathBuf,
}

impl UpdateService for DesktopUpdates {
    fn check(&self) -> ServiceFuture<'_, Value> {
        Box::pin(async move {
            let state = self.data_dir.join("update-state.json");
            match fs::read(state) {
                Ok(bytes) => serde_json::from_slice(&bytes).map_err(protocol),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    Ok(json!({ "status": "idle" }))
                }
                Err(error) => Err(platform(error)),
            }
        })
    }

    fn apply(&self, _options: Value) -> ServiceFuture<'_, ()> {
        Box::pin(async {
            Err(ServiceError::Unavailable(
                "the signed Rust update installer is not configured".into(),
            ))
        })
    }
}

struct DesktopPlatform;

impl PlatformService for DesktopPlatform {
    fn pick_attachments(
        &self,
        title: &str,
        starting_directory: Option<&Path>,
        images_only: bool,
    ) -> ServiceFuture<'_, Vec<SelectedAttachment>> {
        let title = title.to_owned();
        let starting_directory = starting_directory.map(Path::to_owned);
        Box::pin(async move {
            let mut dialog = rfd::AsyncFileDialog::new().set_title(title);
            if let Some(directory) = starting_directory.filter(|path| path.is_dir()) {
                dialog = dialog.set_directory(directory);
            }
            if images_only {
                dialog = dialog.add_filter(
                    "Images",
                    &["png", "jpg", "jpeg", "gif", "webp", "bmp", "tif", "tiff"],
                );
            }
            let handles = dialog.pick_files().await.unwrap_or_default();
            handles
                .into_iter()
                .map(|handle| selected_attachment(handle.path()))
                .collect()
        })
    }

    fn pick_folder(
        &self,
        title: &str,
        starting_directory: Option<&Path>,
    ) -> ServiceFuture<'_, Option<PathBuf>> {
        let title = title.to_owned();
        let starting_directory = starting_directory.map(Path::to_owned);
        Box::pin(async move {
            let mut dialog = rfd::AsyncFileDialog::new().set_title(title);
            if let Some(directory) = starting_directory.filter(|path| path.is_dir()) {
                dialog = dialog.set_directory(directory);
            }
            Ok(dialog
                .pick_folder()
                .await
                .map(|folder| folder.path().to_owned()))
        })
    }

    fn open_external(&self, url: &str) -> ServiceFuture<'_, ()> {
        let url = url.to_owned();
        Box::pin(async move {
            let parsed = validate_external_url(&url)?;
            open::that_detached(parsed.as_str()).map_err(platform)
        })
    }

    fn notify(&self, _title: &str, _body: &str) -> ServiceFuture<'_, bool> {
        Box::pin(async {
            Err(ServiceError::Unavailable(
                "native notifications are not configured".into(),
            ))
        })
    }

    fn version(&self) -> ServiceFuture<'_, String> {
        Box::pin(async { Ok(env!("CARGO_PKG_VERSION").to_owned()) })
    }
}

fn validate_external_url(value: &str) -> ServiceResult<url::Url> {
    let value = value.trim();
    if value.is_empty() || value.len() > 16_384 || value.chars().any(char::is_control) {
        return Err(ServiceError::InvalidInput(
            "external URL is empty, oversized, or contains control characters".into(),
        ));
    }

    let parsed =
        url::Url::parse(value).map_err(|error| ServiceError::InvalidInput(error.to_string()))?;
    match parsed.scheme() {
        "http" | "https" => {
            if parsed.cannot_be_a_base() || parsed.host_str().is_none() {
                return Err(ServiceError::InvalidInput(
                    "external HTTP(S) URL must include a host".into(),
                ));
            }
            if !parsed.username().is_empty() || parsed.password().is_some() {
                return Err(ServiceError::PermissionDenied(
                    "credentialed external URLs are blocked".into(),
                ));
            }
        }
        "mailto" => {
            if parsed.path().is_empty() {
                return Err(ServiceError::InvalidInput(
                    "external mail URL must include a recipient".into(),
                ));
            }
        }
        _ => {
            return Err(ServiceError::PermissionDenied(
                "external URL scheme is blocked".into(),
            ));
        }
    }
    Ok(parsed)
}

fn decode_list<T: serde::de::DeserializeOwned>(value: Value, key: &str) -> ServiceResult<Vec<T>> {
    serde_json::from_value(value.clone())
        .or_else(|_| {
            value
                .get(key)
                .cloned()
                .ok_or_else(|| {
                    serde_json::Error::io(std::io::Error::other(format!("missing {key}")))
                })
                .and_then(serde_json::from_value)
        })
        .map_err(protocol)
}

fn websocket_url(base: &str, token: &str) -> ServiceResult<String> {
    authenticated_websocket_url(base, "token", token)
}

fn websocket_url_with_ticket(base: &str, ticket: &str) -> ServiceResult<String> {
    authenticated_websocket_url(base, "ticket", ticket)
}

fn authenticated_websocket_url(base: &str, key: &str, value: &str) -> ServiceResult<String> {
    let mut url = url::Url::parse(base)
        .map_err(|error| ServiceError::InvalidInput(format!("invalid gateway URL: {error}")))?;
    let websocket_scheme = match url.scheme() {
        "http" => "ws",
        "https" => "wss",
        "ws" => "ws",
        "wss" => "wss",
        _ => {
            return Err(ServiceError::InvalidInput(
                "gateway URL must use http, https, ws, or wss".into(),
            ));
        }
    };
    url.set_scheme(websocket_scheme)
        .map_err(|()| ServiceError::InvalidInput("could not set gateway URL scheme".into()))?;
    if !url.path().ends_with("/api/ws") {
        let path = format!("{}/api/ws", url.path().trim_end_matches('/'));
        url.set_path(&path);
    }
    url.set_query(None);
    url.query_pairs_mut().append_pair(key, value);
    Ok(url.to_string())
}

fn validated_profile(profile: Option<&str>) -> ServiceResult<Option<String>> {
    let Some(profile) = profile.map(str::trim).filter(|profile| !profile.is_empty()) else {
        return Ok(None);
    };
    let valid = profile == "default"
        || (profile.len() <= 64
            && profile.bytes().enumerate().all(|(index, byte)| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || (index > 0 && matches!(byte, b'-' | b'_'))
            }));
    if !valid {
        return Err(ServiceError::InvalidInput(format!(
            "Invalid profile name: {profile}"
        )));
    }
    Ok(Some(profile.to_owned()))
}

fn secret_account(profile: Option<&str>) -> String {
    profile.map_or_else(|| "global".into(), |profile| format!("profile:{profile}"))
}

fn oauth_account(base_url: &str) -> String {
    let digest = Sha256::digest(base_url.as_bytes());
    format!("oauth:{}", URL_SAFE_NO_PAD.encode(digest))
}

fn native_pkce_material() -> ServiceResult<(String, String, String)> {
    let mut verifier_random = [0_u8; 32];
    let mut state_random = [0_u8; 24];
    getrandom::fill(&mut verifier_random).map_err(platform)?;
    getrandom::fill(&mut state_random).map_err(platform)?;
    let verifier = URL_SAFE_NO_PAD.encode(verifier_random);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let state = URL_SAFE_NO_PAD.encode(state_random);
    Ok((verifier, challenge, state))
}

fn native_oauth_url(
    base_url: &str,
    endpoint: &str,
    query: &[(&str, &str)],
) -> ServiceResult<String> {
    let normalized = normalize_remote_url(base_url)?;
    let mut url = url::Url::parse(&normalized)
        .map_err(|error| ServiceError::InvalidInput(format!("invalid Gateway URL: {error}")))?;
    let prefix = url.path().trim_end_matches('/').to_owned();
    url.set_path(&format!("{prefix}/auth/native/{endpoint}"));
    url.set_query(None);
    if !query.is_empty() {
        let mut pairs = url.query_pairs_mut();
        for (key, value) in query {
            pairs.append_pair(key, value);
        }
    }
    Ok(url.to_string())
}

fn parse_loopback_callback(target: &str, expected_state: &str) -> ServiceResult<String> {
    let callback = url::Url::parse(&format!("http://127.0.0.1{target}"))
        .map_err(|error| ServiceError::InvalidInput(format!("invalid OAuth callback: {error}")))?;
    if callback.path() != "/callback" {
        return Err(ServiceError::InvalidInput(
            "OAuth callback used an unexpected path".into(),
        ));
    }
    let parameter = |name: &str| {
        callback
            .query_pairs()
            .find_map(|(key, value)| (key == name).then(|| value.into_owned()))
    };
    if let Some(error) = parameter("error") {
        let description = parameter("error_description").unwrap_or_default();
        return Err(ServiceError::PermissionDenied(format!(
            "Gateway rejected native login: {error}{}",
            if description.is_empty() {
                String::new()
            } else {
                format!(" ({description})")
            }
        )));
    }
    let state = parameter("state");
    if expected_state.is_empty() || state.as_deref() != Some(expected_state) {
        return Err(ServiceError::PermissionDenied(
            "OAuth callback state mismatch (possible CSRF)".into(),
        ));
    }
    parameter("code")
        .filter(|code| !code.is_empty())
        .ok_or_else(|| {
            ServiceError::InvalidInput("OAuth callback missing authorization code".into())
        })
}

async fn receive_oauth_code(
    listener: &tokio::net::TcpListener,
    expected_state: &str,
) -> ServiceResult<String> {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    let receive = async {
        loop {
            let (mut stream, peer) = listener.accept().await.map_err(platform)?;
            if !peer.ip().is_loopback() {
                continue;
            }
            let mut request = Vec::with_capacity(2_048);
            let mut chunk = [0_u8; 1_024];
            while request.len() < 16_384 && !request.windows(4).any(|part| part == b"\r\n\r\n") {
                let read = stream.read(&mut chunk).await.map_err(platform)?;
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&chunk[..read]);
            }
            let first_line = String::from_utf8_lossy(&request)
                .lines()
                .next()
                .unwrap_or_default()
                .to_owned();
            let mut parts = first_line.split_whitespace();
            let method = parts.next().unwrap_or_default();
            let target = parts.next().unwrap_or_default();
            let body = "<!doctype html><meta charset=\"utf-8\"><title>Signed in</title><body style=\"font:15px system-ui;margin:3rem;text-align:center\"><h2>✓ Signed in to Hermes</h2><p>You can close this window and return to the app.</p><script>setTimeout(()=>window.close(),800)</script>";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n{body}",
                body.len()
            );
            stream
                .write_all(response.as_bytes())
                .await
                .map_err(platform)?;
            let _ = stream.shutdown().await;
            if method != "GET" || (!target.contains("?code=") && !target.contains("?error=")) {
                continue;
            }
            return parse_loopback_callback(target, expected_state);
        }
    };
    tokio::time::timeout(std::time::Duration::from_mins(5), receive)
        .await
        .map_err(|_| ServiceError::Timeout("native Gateway sign-in timed out".into()))?
}

fn token_preview(token: Option<&str>) -> Option<String> {
    let token = token.filter(|token| !token.is_empty())?;
    if token.chars().count() <= 8 {
        return Some("set".into());
    }
    let suffix: String = token
        .chars()
        .rev()
        .take(6)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    Some(format!("...{suffix}"))
}

fn normalize_remote_url(raw: &str) -> ServiceResult<String> {
    let mut url = url::Url::parse(raw.trim())
        .map_err(|error| ServiceError::InvalidInput(format!("invalid Gateway URL: {error}")))?;
    if !matches!(url.scheme(), "http" | "https") || url.cannot_be_a_base() {
        return Err(ServiceError::InvalidInput(
            "Gateway URL must use HTTP or HTTPS".into(),
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(ServiceError::InvalidInput(
            "Gateway URL must not contain credentials".into(),
        ));
    }
    let path = url.path().trim_end_matches('/').to_owned();
    url.set_path(&path);
    url.set_query(None);
    url.set_fragment(None);
    Ok(url.as_str().trim_end_matches('/').to_owned())
}

async fn probe_remote_gateway(remote_url: &str) -> ServiceResult<ConnectionProbeResult> {
    let base_url = normalize_remote_url(remote_url)?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(platform)?;
    let status_url = format!("{base_url}/api/status");
    let response = client
        .get(status_url)
        .send()
        .await
        .map_err(|error| ServiceError::Transport(error.to_string()))?;
    if !response.status().is_success() {
        return Ok(ConnectionProbeResult {
            base_url,
            reachable: false,
            error: Some(format!("Gateway returned HTTP {}", response.status())),
            ..ConnectionProbeResult::default()
        });
    }
    let status: Value = response
        .json()
        .await
        .map_err(|error| ServiceError::Transport(format!("invalid Gateway response: {error}")))?;
    let auth_mode = if status
        .get("auth_required")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        ProbeAuthMode::Oauth
    } else {
        ProbeAuthMode::Token
    };
    let mut providers = Vec::new();
    if auth_mode == ProbeAuthMode::Oauth
        && let Ok(response) = client
            .get(format!("{base_url}/api/auth/providers"))
            .send()
            .await
        && response.status().is_success()
        && let Ok(value) = response.json::<Value>().await
    {
        let entries = value
            .get("providers")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        providers = entries
            .into_iter()
            .filter_map(|provider| {
                let name = provider.get("name")?.as_str()?.to_owned();
                Some(AuthProvider {
                    display_name: provider
                        .get("display_name")
                        .and_then(Value::as_str)
                        .unwrap_or(&name)
                        .to_owned(),
                    supports_password: provider
                        .get("supports_password")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    name,
                })
            })
            .collect();
    }
    Ok(ConnectionProbeResult {
        base_url,
        reachable: true,
        auth_mode,
        providers,
        version: status
            .get("version")
            .and_then(Value::as_str)
            .map(str::to_owned),
        error: None,
    })
}

async fn native_oauth_gateway(remote_url: &str) -> ServiceResult<String> {
    let base_url = normalize_remote_url(remote_url)?;
    let response = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(platform)?
        .get(format!("{base_url}/api/status"))
        .send()
        .await
        .map_err(|error| ServiceError::Transport(error.to_string()))?;
    if !response.status().is_success() {
        return Err(ServiceError::Transport(format!(
            "Gateway returned HTTP {}",
            response.status()
        )));
    }
    let status: Value = response
        .json()
        .await
        .map_err(|error| ServiceError::Transport(format!("invalid Gateway response: {error}")))?;
    if !status
        .get("auth_required")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err(ServiceError::InvalidInput(
            "This Gateway does not advertise OAuth authentication".into(),
        ));
    }
    let supports_native_pkce = status
        .get("auth_flows")
        .and_then(Value::as_array)
        .is_some_and(|flows| {
            flows
                .iter()
                .any(|flow| flow.as_str() == Some("native_pkce"))
        });
    if !supports_native_pkce {
        return Err(ServiceError::Unavailable(
            "This Gateway only supports legacy embedded sign-in; update Hermes Gateway to use native PKCE"
                .into(),
        ));
    }
    Ok(base_url)
}

impl GatewayRest {
    async fn request(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> ServiceResult<Value> {
        let path = path.strip_prefix('/').ok_or_else(|| {
            ServiceError::InvalidInput("Hermes REST path must be absolute".into())
        })?;
        let url = self.base_url.join(path).map_err(|error| {
            ServiceError::InvalidInput(format!("invalid Hermes REST path: {error}"))
        })?;
        if !matches!(url.scheme(), "http" | "https") || url.origin() != self.base_url.origin() {
            return Err(ServiceError::PermissionDenied(
                "Hermes REST request escaped the configured gateway".into(),
            ));
        }

        let mut request = self.client.request(method, url);
        if let Some(token) = &self.session_token {
            request = request.header("X-Hermes-Session-Token", token);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request
            .send()
            .await
            .map_err(|error| ServiceError::Transport(error.to_string()))?;
        let status = response.status();
        if response
            .content_length()
            .is_some_and(|length| length > 16 * 1024 * 1024)
        {
            return Err(ServiceError::Transport(
                "Hermes REST response exceeded the 16 MiB limit".into(),
            ));
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|error| ServiceError::Transport(error.to_string()))?;
        if !status.is_success() {
            let detail = String::from_utf8_lossy(&bytes);
            return match status.as_u16() {
                404 => Err(ServiceError::NotFound(detail.trim().to_owned())),
                401 | 403 => Err(ServiceError::PermissionDenied(detail.trim().to_owned())),
                _ => Err(ServiceError::Transport(format!(
                    "{status}: {}",
                    detail.trim()
                ))),
            };
        }
        if bytes.is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_slice(&bytes).map_err(protocol)
    }

    async fn request_bounded(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        timeout: std::time::Duration,
        max_response_bytes: u64,
    ) -> ServiceResult<Value> {
        let path = path.strip_prefix('/').ok_or_else(|| {
            ServiceError::InvalidInput("Hermes REST path must be absolute".into())
        })?;
        let url = self.base_url.join(path).map_err(|error| {
            ServiceError::InvalidInput(format!("invalid Hermes REST path: {error}"))
        })?;
        if !matches!(url.scheme(), "http" | "https") || url.origin() != self.base_url.origin() {
            return Err(ServiceError::PermissionDenied(
                "Hermes REST request escaped the configured gateway".into(),
            ));
        }

        let mut request = self.client.request(method, url).timeout(timeout);
        if let Some(token) = &self.session_token {
            request = request.header("X-Hermes-Session-Token", token);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request
            .send()
            .await
            .map_err(|error| ServiceError::Transport(error.to_string()))?;
        let status = response.status();
        if response
            .content_length()
            .is_some_and(|length| length > max_response_bytes)
        {
            return Err(ServiceError::Transport(format!(
                "Hermes REST response exceeded the {max_response_bytes}-byte limit"
            )));
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|error| ServiceError::Transport(error.to_string()))?;
        if bytes.len() as u64 > max_response_bytes {
            return Err(ServiceError::Transport(format!(
                "Hermes REST response exceeded the {max_response_bytes}-byte limit"
            )));
        }
        if !status.is_success() {
            let detail = String::from_utf8_lossy(&bytes);
            return match status.as_u16() {
                404 => Err(ServiceError::NotFound(detail.trim().to_owned())),
                401 | 403 => Err(ServiceError::PermissionDenied(detail.trim().to_owned())),
                _ => Err(ServiceError::Transport(format!(
                    "{status}: {}",
                    detail.trim()
                ))),
            };
        }
        if bytes.is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_slice(&bytes).map_err(protocol)
    }
}

fn rest_from_websocket_url(websocket_url: &str) -> ServiceResult<GatewayRest> {
    let mut url = url::Url::parse(websocket_url)
        .map_err(|error| ServiceError::InvalidInput(format!("invalid gateway URL: {error}")))?;
    let http_scheme = match url.scheme() {
        "ws" => "http",
        "wss" => "https",
        _ => {
            return Err(ServiceError::InvalidInput(
                "gateway WebSocket URL must use ws or wss".into(),
            ));
        }
    };
    let session_token = url
        .query_pairs()
        .find_map(|(key, value)| (key == "token").then(|| value.into_owned()));
    url.set_scheme(http_scheme)
        .map_err(|()| ServiceError::InvalidInput("could not set gateway REST scheme".into()))?;
    let base_path = url
        .path()
        .strip_suffix("/api/ws")
        .unwrap_or_else(|| url.path())
        .trim_end_matches('/');
    url.set_path(&format!("{base_path}/"));
    url.set_query(None);
    url.set_fragment(None);
    Ok(GatewayRest {
        client: reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| ServiceError::Platform(error.to_string()))?,
        base_url: url,
        session_token,
    })
}

fn profiled_path(path: &str, profile: Option<&str>) -> String {
    let Some(profile) = profile.filter(|profile| !profile.is_empty()) else {
        return path.to_owned();
    };
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("profile", profile)
        .finish();
    format!("{path}?{query}")
}

fn model_options_path(profile: Option<&str>) -> String {
    let mut query = url::form_urlencoded::Serializer::new(String::new());
    query.append_pair("explicit_only", "1");
    if let Some(profile) = profile.filter(|profile| !profile.is_empty()) {
        query.append_pair("profile", profile);
    }
    format!("/api/model/options?{}", query.finish())
}

fn validate_path_id(value: &str, field: &str) -> ServiceResult<()> {
    if value.is_empty()
        || value.len() > 256
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(ServiceError::InvalidInput(format!("invalid {field}")));
    }
    Ok(())
}

fn validate_env_key(key: &str) -> ServiceResult<()> {
    if key.is_empty()
        || key.len() > 128
        || !key
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(ServiceError::InvalidInput(
            "invalid credential environment key".into(),
        ));
    }
    Ok(())
}

fn validate_oauth_session(session_id: &str) -> ServiceResult<()> {
    if session_id.is_empty()
        || session_id.len() > 512
        || !session_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(ServiceError::InvalidInput("invalid OAuth session".into()));
    }
    Ok(())
}

fn validate_custom_endpoint(endpoint: &CustomEndpointUpdate) -> ServiceResult<()> {
    if endpoint.name.trim().is_empty()
        || endpoint.name.len() > 256
        || endpoint.name.chars().any(char::is_control)
    {
        return Err(ServiceError::InvalidInput(
            "custom endpoint name is required".into(),
        ));
    }
    if let Some(id) = endpoint.id.as_deref() {
        validate_path_id(id, "custom endpoint")?;
    }
    let url = url::Url::parse(&endpoint.base_url)
        .map_err(|error| ServiceError::InvalidInput(format!("invalid endpoint URL: {error}")))?;
    if !matches!(url.scheme(), "http" | "https") || url.cannot_be_a_base() {
        return Err(ServiceError::InvalidInput(
            "custom endpoint URL must use HTTP or HTTPS".into(),
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(ServiceError::InvalidInput(
            "custom endpoint URL must not contain credentials".into(),
        ));
    }
    if endpoint.model.len() > 1_024 || endpoint.model.chars().any(char::is_control) {
        return Err(ServiceError::InvalidInput(
            "invalid custom endpoint model".into(),
        ));
    }
    if endpoint.context_length == Some(0) {
        return Err(ServiceError::InvalidInput(
            "context length must be greater than zero".into(),
        ));
    }
    Ok(())
}

fn require_confirmation(value: &Value, operation: &str) -> ServiceResult<()> {
    if value.get("ok").and_then(Value::as_bool) == Some(true) {
        Ok(())
    } else {
        Err(ServiceError::Transport(format!(
            "Hermes Agent did not confirm {operation}"
        )))
    }
}

fn protocol_missing(field: &str) -> ServiceError {
    ServiceError::Transport(format!("invalid Agent response: missing {field}"))
}

fn contained_root(root: &Path) -> ServiceResult<PathBuf> {
    root.canonicalize().map_err(platform)
}

fn contained_existing(root: &Path, relative: &Path) -> ServiceResult<PathBuf> {
    validate_relative_path(relative)?;
    let root = contained_root(root)?;
    let target = root.join(relative).canonicalize().map_err(platform)?;
    if !target.starts_with(&root) {
        return Err(ServiceError::PermissionDenied(
            "path escaped the selected root".into(),
        ));
    }
    Ok(target)
}

fn contained_for_write(root: &Path, relative: &Path) -> ServiceResult<PathBuf> {
    validate_relative_path(relative)?;
    let root = contained_root(root)?;
    let target = root.join(relative);
    let parent = target
        .parent()
        .ok_or_else(|| ServiceError::InvalidInput("file path has no parent".into()))?
        .canonicalize()
        .map_err(platform)?;
    if !parent.starts_with(&root) {
        return Err(ServiceError::PermissionDenied(
            "path escaped the selected root".into(),
        ));
    }
    Ok(target)
}

fn git(repository: &Path, args: &[&str]) -> ServiceResult<String> {
    let repository = repository.canonicalize().map_err(platform)?;
    let output = Command::new("git")
        .arg("-C")
        .arg(&repository)
        .args(args)
        .output()
        .map_err(platform)?;
    if !output.status.success() {
        return Err(ServiceError::Platform(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ));
    }
    String::from_utf8(output.stdout).map_err(|error| ServiceError::Platform(error.to_string()))
}

fn parse_git_status(output: &str) -> ServiceResult<GitStatus> {
    let mut lines = output.lines();
    let header = lines
        .next()
        .unwrap_or_default()
        .strip_prefix("## ")
        .unwrap_or_default();
    let branch = if let Some(unborn) = header.strip_prefix("No commits yet on ") {
        let unborn = unborn.trim();
        (!unborn.is_empty()).then(|| unborn.to_owned())
    } else if header == "HEAD (no branch)" || header.starts_with("HEAD detached ") {
        None
    } else {
        header
            .split(['.', ' '])
            .next()
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    let ahead = parse_counter(header, "ahead ");
    let behind = parse_counter(header, "behind ");
    let entries: Vec<_> = lines.filter_map(parse_git_change).collect();
    let changed = entries.iter().map(|entry| entry.path.clone()).collect();
    Ok(GitStatus {
        branch,
        ahead,
        behind,
        changed,
        entries,
    })
}

fn parse_git_change(line: &str) -> Option<hermes_protocol::GitChange> {
    let status = line.get(..2)?;
    let raw_path = line.get(3..)?;
    let mut status_chars = status.chars();
    let index = status_chars.next()?;
    let worktree = status_chars.next()?;
    let path = raw_path
        .rsplit(" -> ")
        .next()
        .unwrap_or(raw_path)
        .trim_matches('"')
        .to_owned();
    if path.is_empty() {
        return None;
    }
    Some(hermes_protocol::GitChange {
        path,
        index_status: index.to_string(),
        worktree_status: worktree.to_string(),
        staged: index != ' ' && index != '?',
        unstaged: worktree != ' ' || (index == '?' && worktree == '?'),
    })
}

fn parse_counter(header: &str, marker: &str) -> u32 {
    header
        .split(marker)
        .nth(1)
        .and_then(|rest| {
            rest.split(|character: char| !character.is_ascii_digit())
                .next()
        })
        .and_then(|value| value.parse().ok())
        .unwrap_or_default()
}

fn protocol(error: serde_json::Error) -> ServiceError {
    ServiceError::Transport(format!("invalid Agent response: {error}"))
}

fn transport(error: hermes_agent_client::GatewayError) -> ServiceError {
    ServiceError::Transport(error.to_string())
}

fn platform(error: impl std::fmt::Display) -> ServiceError {
    ServiceError::Platform(error.to_string())
}

#[cfg(test)]
mod tests {
    use futures_util::{SinkExt, StreamExt};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio_tungstenite::{accept_async, tungstenite::Message};

    use super::*;

    #[derive(Default)]
    struct MemoryGatewaySecrets(Mutex<BTreeMap<String, String>>);

    impl GatewaySecretStore for MemoryGatewaySecrets {
        fn get(&self, account: &str) -> ServiceResult<Option<String>> {
            Ok(self
                .0
                .lock()
                .map_err(|_| ServiceError::Platform("test secret lock was poisoned".into()))?
                .get(account)
                .cloned())
        }

        fn set(&self, account: &str, secret: &str) -> ServiceResult<()> {
            self.0
                .lock()
                .map_err(|_| ServiceError::Platform("test secret lock was poisoned".into()))?
                .insert(account.into(), secret.into());
            Ok(())
        }

        fn delete(&self, account: &str) -> ServiceResult<()> {
            self.0
                .lock()
                .map_err(|_| ServiceError::Platform("test secret lock was poisoned".into()))?
                .remove(account);
            Ok(())
        }
    }

    fn test_connection_store() -> Arc<ConnectionConfigStore> {
        Arc::new(ConnectionConfigStore::with_secrets(
            std::env::temp_dir().join(format!(
                "unused-hermes-connection-{}.json",
                Uuid::new_v4().simple()
            )),
            Arc::new(MemoryGatewaySecrets::default()),
        ))
    }

    #[test]
    fn parses_porcelain_status() {
        let status = parse_git_status(
            "## main...origin/main [ahead 2, behind 1]\n M src/main.rs\n?? new.txt\n",
        )
        .expect("status");
        assert_eq!(status.branch.as_deref(), Some("main"));
        assert_eq!(status.ahead, 2);
        assert_eq!(status.behind, 1);
        assert_eq!(status.changed, ["src/main.rs", "new.txt"]);
    }

    #[test]
    fn parses_unborn_and_detached_porcelain_headers() {
        let unborn = parse_git_status("## No commits yet on feature/status\n").expect("unborn");
        assert_eq!(unborn.branch.as_deref(), Some("feature/status"));
        assert!(unborn.changed.is_empty());

        let detached = parse_git_status("## HEAD (no branch)\n").expect("detached");
        assert_eq!(detached.branch, None);
        assert!(detached.changed.is_empty());
    }

    #[test]
    fn external_url_policy_allows_web_and_mail_targets() {
        assert_eq!(
            validate_external_url("http://example.com/path")
                .expect("plain HTTP")
                .as_str(),
            "http://example.com/path"
        );
        assert_eq!(
            validate_external_url("https://example.com/path")
                .expect("HTTPS")
                .as_str(),
            "https://example.com/path"
        );
        assert_eq!(
            validate_external_url("mailto:person@example.com")
                .expect("mail target")
                .as_str(),
            "mailto:person@example.com"
        );
    }

    #[test]
    fn external_url_policy_rejects_privileged_or_ambiguous_targets() {
        for blocked in [
            "javascript:alert(1)",
            "file:///C:/Windows/System32/config/SAM",
            "https://user:secret@example.com/path",
            "https://example.com/\nheader: value",
            "mailto:",
        ] {
            assert!(
                validate_external_url(blocked).is_err(),
                "unexpectedly allowed {blocked:?}"
            );
        }
    }

    #[test]
    fn blocks_symlink_escape_for_existing_paths() {
        assert!(validate_relative_path(Path::new("../outside")).is_err());
    }

    #[test]
    fn builds_encoded_websocket_url_without_losing_base_path() {
        let url = websocket_url("https://gateway.example/hermes", "a b&c").expect("URL");
        assert_eq!(url, "wss://gateway.example/hermes/api/ws?token=a+b%26c");
    }

    #[test]
    fn builds_one_time_ticket_websocket_url_without_legacy_credentials() {
        let url = websocket_url_with_ticket(
            "https://gateway.example/hermes?discard=yes",
            "one time&ticket",
        )
        .expect("URL");
        assert_eq!(
            url,
            "wss://gateway.example/hermes/api/ws?ticket=one+time%26ticket"
        );
        assert!(!url.contains("token="));
    }

    #[test]
    fn native_oauth_helpers_match_the_og_pkce_contract() {
        let (verifier, challenge, state) = native_pkce_material().expect("PKCE material");
        assert_eq!(verifier.len(), 43);
        assert_eq!(challenge.len(), 43);
        assert_eq!(
            challenge,
            URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
        );
        assert_eq!(state.len(), 32);

        let url = native_oauth_url(
            "https://gateway.example/hermes/?discard=yes",
            "authorize",
            &[
                ("code_challenge", &challenge),
                ("redirect_uri", "http://127.0.0.1:43210/callback"),
                ("state", &state),
            ],
        )
        .expect("authorize URL");
        let parsed = url::Url::parse(&url).expect("parsed authorize URL");
        assert_eq!(parsed.path(), "/hermes/auth/native/authorize");
        let query = parsed.query_pairs().collect::<BTreeMap<_, _>>();
        assert_eq!(
            query.get("code_challenge").map(std::convert::AsRef::as_ref),
            Some(challenge.as_str())
        );
        assert_eq!(
            query.get("redirect_uri").map(std::convert::AsRef::as_ref),
            Some("http://127.0.0.1:43210/callback")
        );
        assert_eq!(
            query.get("state").map(std::convert::AsRef::as_ref),
            Some(state.as_str())
        );
    }

    #[test]
    fn loopback_oauth_callback_rejects_csrf_and_gateway_errors() {
        assert_eq!(
            parse_loopback_callback("/callback?code=once%26only&state=expected", "expected")
                .expect("callback code"),
            "once&only"
        );
        assert!(matches!(
            parse_loopback_callback("/callback?code=attacker&state=wrong", "expected"),
            Err(ServiceError::PermissionDenied(message)) if message.contains("CSRF")
        ));
        assert!(matches!(
            parse_loopback_callback(
                "/callback?error=access_denied&error_description=Nope&state=expected",
                "expected"
            ),
            Err(ServiceError::PermissionDenied(message)) if message.contains("access_denied") && message.contains("Nope")
        ));
    }

    #[test]
    fn derives_rest_endpoint_and_legacy_token_from_websocket() {
        let rest = rest_from_websocket_url(
            "wss://gateway.example/hermes/api/ws?token=a+b%26c&ignored=yes",
        )
        .expect("REST endpoint");
        assert_eq!(rest.base_url.as_str(), "https://gateway.example/hermes/");
        assert_eq!(rest.session_token.as_deref(), Some("a b&c"));
    }

    #[tokio::test]
    async fn settings_round_trip_theme_mode_and_skin_atomically() {
        let directory =
            std::env::temp_dir().join(format!("hermes-settings-test-{}", Uuid::new_v4().simple()));
        let path = directory.join("settings.json");
        let store = JsonSettings::new(path.clone());
        let expected = AppSettings {
            theme: hermes_protocol::ThemeMode::Dark,
            theme_name: Some("midnight".into()),
            notifications: true,
            ..AppSettings::default()
        };
        SettingsService::save(&store, &expected)
            .await
            .expect("save settings");
        assert!(!path.with_extension("json.tmp").exists());
        let actual = SettingsService::load(&store).await.expect("load settings");
        assert_eq!(actual, expected);
        fs::remove_file(path).expect("remove test settings");
        fs::remove_dir(directory).expect("remove test directory");
    }

    #[test]
    fn gateway_profiles_keep_tokens_out_of_json_and_preserve_global_scope() {
        let directory = std::env::temp_dir().join(format!(
            "hermes-connection-test-{}",
            Uuid::new_v4().simple()
        ));
        let path = directory.join("connection.json");
        let secrets = Arc::new(MemoryGatewaySecrets::default());
        let store = ConnectionConfigStore::with_secrets(path.clone(), secrets.clone());

        let global = store
            .save(&ConnectionConfigInput {
                mode: ConnectionMode::Remote,
                remote_auth_mode: Some(RemoteAuthMode::Token),
                remote_token: Some("abcdefghijklmnop".into()),
                remote_url: Some("https://gateway.example/base/?ignored=1".into()),
                ..ConnectionConfigInput::default()
            })
            .expect("save global Gateway");
        assert_eq!(global.remote_url, "https://gateway.example/base");
        assert_eq!(global.remote_token_preview.as_deref(), Some("...klmnop"));
        assert!(global.remote_token_set);
        let on_disk = fs::read_to_string(&path).expect("connection settings");
        assert!(!on_disk.contains("abcdefghijklmnop"));
        assert!(on_disk.contains("credentialManager"));

        let cloud = store
            .save(&ConnectionConfigInput {
                mode: ConnectionMode::Cloud,
                profile: Some("work_profile".into()),
                remote_auth_mode: Some(RemoteAuthMode::Oauth),
                remote_url: Some("https://cloud.example/agent".into()),
                cloud_org: Some("nous".into()),
                ..ConnectionConfigInput::default()
            })
            .expect("save profile cloud Gateway");
        assert_eq!(cloud.profile.as_deref(), Some("work_profile"));
        assert_eq!(cloud.mode, ConnectionMode::Cloud);
        assert_eq!(cloud.cloud_org, "nous");
        assert_eq!(
            store.load(None).expect("global config").mode,
            ConnectionMode::Remote
        );
        assert_eq!(
            secrets.get("global").expect("secret lookup").as_deref(),
            Some("abcdefghijklmnop")
        );

        fs::remove_file(path).expect("remove connection settings");
        fs::remove_dir(directory).expect("remove connection settings directory");
    }

    #[test]
    fn native_oauth_session_lives_only_in_the_secret_store() {
        let directory = std::env::temp_dir().join(format!(
            "hermes-oauth-connection-test-{}",
            Uuid::new_v4().simple()
        ));
        let path = directory.join("connection.json");
        let secrets = Arc::new(MemoryGatewaySecrets::default());
        let store = ConnectionConfigStore::with_secrets(path.clone(), secrets.clone());
        let base_url = "https://gateway.example/hermes";
        store
            .save(&ConnectionConfigInput {
                mode: ConnectionMode::Remote,
                remote_auth_mode: Some(RemoteAuthMode::Oauth),
                remote_url: Some(base_url.into()),
                ..ConnectionConfigInput::default()
            })
            .expect("save OAuth Gateway");
        let tokens = NativeOauthTokens {
            access_token: "access-secret".into(),
            refresh_token: "refresh-secret".into(),
            expires_at: 4_102_444_800,
            provider: "portal".into(),
            user_id: "user-1".into(),
        };
        store
            .store_oauth_tokens(base_url, &tokens)
            .expect("store OAuth tokens");

        assert!(
            store
                .load(None)
                .expect("load config")
                .remote_oauth_connected
        );
        let on_disk = fs::read_to_string(&path).expect("connection settings");
        assert!(!on_disk.contains("access-secret"));
        assert!(!on_disk.contains("refresh-secret"));
        assert!(
            secrets
                .get(&oauth_account(base_url))
                .expect("OAuth secret")
                .is_some()
        );

        store
            .clear_oauth_tokens(base_url)
            .expect("clear OAuth tokens");
        assert!(
            !store
                .load(None)
                .expect("load signed-out config")
                .remote_oauth_connected
        );

        fs::remove_file(path).expect("remove connection settings");
        fs::remove_dir(directory).expect("remove connection settings directory");
    }

    #[tokio::test]
    async fn native_oauth_refreshes_then_mints_the_official_ws_ticket() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let address = listener.local_addr().expect("address");
        let base_url = format!("http://{address}/hermes");
        let server = tokio::spawn(async move {
            let bodies = [
                json!({
                    "access_token": "rotated-access",
                    "refresh_token": "rotated-refresh",
                    "expires_at": 4_102_444_800_u64,
                    "provider": "portal",
                    "user_id": "user-1"
                })
                .to_string(),
                json!({ "ticket": "single-use-ticket" }).to_string(),
            ];
            let mut requests = Vec::new();
            for body in bodies {
                let (mut stream, _) = listener.accept().await.expect("connection");
                let mut request = Vec::new();
                let mut chunk = [0_u8; 2_048];
                loop {
                    let count = stream.read(&mut chunk).await.expect("request bytes");
                    if count == 0 {
                        break;
                    }
                    request.extend_from_slice(&chunk[..count]);
                    let text = String::from_utf8_lossy(&request);
                    if let Some(headers_end) = text.find("\r\n\r\n") {
                        let content_length = text[..headers_end]
                            .lines()
                            .find_map(|line| line.strip_prefix("content-length: "))
                            .and_then(|value| value.parse::<usize>().ok())
                            .unwrap_or_default();
                        if request.len() >= headers_end + 4 + content_length {
                            break;
                        }
                    }
                }
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream
                    .write_all(response.as_bytes())
                    .await
                    .expect("response");
                requests.push(String::from_utf8(request).expect("UTF-8 request"));
            }
            requests
        });
        let store = test_connection_store();
        store
            .store_oauth_tokens(
                &base_url,
                &NativeOauthTokens {
                    access_token: "expired-access".into(),
                    refresh_token: "refresh-secret".into(),
                    expires_at: 0,
                    provider: "portal".into(),
                    user_id: "user-1".into(),
                },
            )
            .expect("store expired OAuth tokens");
        let services = GatewayServices {
            client: Arc::new(RwLock::new(None)),
            rest: Arc::new(RwLock::new(None)),
            connection_store: store.clone(),
        };

        assert_eq!(
            services
                .mint_gateway_ticket(&base_url)
                .await
                .expect("mint ticket"),
            "single-use-ticket"
        );
        let requests = server.await.expect("server");
        assert!(requests[0].starts_with("POST /hermes/auth/native/refresh HTTP/1.1"));
        assert!(
            requests[0].ends_with("{\"provider\":\"portal\",\"refresh_token\":\"refresh-secret\"}")
        );
        assert!(requests[1].starts_with("POST /hermes/api/auth/ws-ticket HTTP/1.1"));
        assert!(requests[1].contains("authorization: Bearer rotated-access"));
        assert_eq!(
            store
                .oauth_tokens(&base_url)
                .expect("stored rotated tokens")
                .expect("rotated tokens")
                .refresh_token,
            "rotated-refresh"
        );
    }

    #[test]
    fn gateway_profile_validation_and_explicit_ssh_port_clear_match_og_contract() {
        assert_eq!(
            validated_profile(Some("default")).expect("default"),
            Some("default".into())
        );
        assert!(validated_profile(Some("Work Profile")).is_err());

        let directory = std::env::temp_dir().join(format!(
            "hermes-ssh-connection-test-{}",
            Uuid::new_v4().simple()
        ));
        let path = directory.join("connection.json");
        let store = ConnectionConfigStore::with_secrets(
            path.clone(),
            Arc::new(MemoryGatewaySecrets::default()),
        );
        store
            .save(&ConnectionConfigInput {
                mode: ConnectionMode::Ssh,
                ssh_host: Some("devbox".into()),
                ssh_port: Some(Some(2222)),
                ..ConnectionConfigInput::default()
            })
            .expect("save SSH Gateway");
        let cleared = store
            .save(&ConnectionConfigInput {
                mode: ConnectionMode::Ssh,
                ssh_host: Some("devbox".into()),
                ssh_port: Some(None),
                ..ConnectionConfigInput::default()
            })
            .expect("clear SSH port");
        assert_eq!(cleared.ssh_port, None);

        fs::remove_file(path).expect("remove connection settings");
        fs::remove_dir(directory).expect("remove connection settings directory");
    }

    #[tokio::test]
    async fn rest_adapter_preserves_base_path_auth_and_json_body() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let address = listener.local_addr().expect("address");
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("connection");
            let mut request = Vec::new();
            let mut chunk = [0_u8; 2048];
            loop {
                let count = stream.read(&mut chunk).await.expect("request bytes");
                if count == 0 {
                    break;
                }
                request.extend_from_slice(&chunk[..count]);
                let text = String::from_utf8_lossy(&request);
                if let Some(headers_end) = text.find("\r\n\r\n") {
                    let content_length = text[..headers_end]
                        .lines()
                        .find_map(|line| line.strip_prefix("content-length: "))
                        .and_then(|value| value.parse::<usize>().ok())
                        .unwrap_or_default();
                    if request.len() >= headers_end + 4 + content_length {
                        break;
                    }
                }
            }
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}")
                .await
                .expect("response");
            String::from_utf8(request).expect("UTF-8 request")
        });
        let rest = GatewayRest {
            client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("client"),
            base_url: url::Url::parse(&format!("http://{address}/hermes/")).expect("URL"),
            session_token: Some("secret-token".into()),
        };
        let response = rest
            .request(
                Method::PATCH,
                "/api/sessions/session-1",
                Some(json!({ "pinned": true })),
            )
            .await
            .expect("REST response");
        assert_eq!(response, json!({ "ok": true }));
        let request = server.await.expect("server");
        assert!(request.starts_with("PATCH /hermes/api/sessions/session-1 HTTP/1.1"));
        assert!(request.contains("x-hermes-session-token: secret-token"));
        assert!(request.ends_with("{\"pinned\":true}"));
    }

    #[tokio::test]
    async fn agent_config_uses_the_official_profile_scoped_rest_contracts() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let address = listener.local_addr().expect("address");
        let server = tokio::spawn(async move {
            let responses = [
                json!({
                    "display": { "personality": "helpful" },
                    "model_context_length": 0
                }),
                json!({ "model_context_length": 0 }),
                json!({
                    "fields": {
                        "timezone": {
                            "type": "select",
                            "options": ["UTC"],
                            "searchable": true
                        }
                    }
                }),
                json!({ "ok": true }),
            ];
            let mut requests = Vec::new();
            for body in responses {
                let (mut stream, _) = listener.accept().await.expect("connection");
                let mut request = Vec::new();
                let mut chunk = [0_u8; 2048];
                loop {
                    let count = stream.read(&mut chunk).await.expect("request bytes");
                    if count == 0 {
                        break;
                    }
                    request.extend_from_slice(&chunk[..count]);
                    let text = String::from_utf8_lossy(&request);
                    if let Some(headers_end) = text.find("\r\n\r\n") {
                        let content_length = text[..headers_end]
                            .lines()
                            .find_map(|line| line.strip_prefix("content-length: "))
                            .and_then(|value| value.parse::<usize>().ok())
                            .unwrap_or_default();
                        if request.len() >= headers_end + 4 + content_length {
                            break;
                        }
                    }
                }
                let body = body.to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream
                    .write_all(response.as_bytes())
                    .await
                    .expect("response");
                requests.push(String::from_utf8(request).expect("UTF-8 request"));
            }
            requests
        });
        let services = GatewayServices {
            client: Arc::new(RwLock::new(None)),
            rest: Arc::new(RwLock::new(Some(GatewayRest {
                client: reqwest::Client::builder()
                    .redirect(reqwest::redirect::Policy::none())
                    .build()
                    .expect("client"),
                base_url: url::Url::parse(&format!("http://{address}/hermes/")).expect("URL"),
                session_token: Some("config-token".into()),
            }))),
            connection_store: test_connection_store(),
        };
        let loaded = AgentConfigService::load(&services, Some("work profile"))
            .await
            .expect("load config");
        assert_eq!(loaded.config["model_context_length"], json!(0));
        assert!(loaded.schema.fields["timezone"].searchable);
        AgentConfigService::save(&services, Some("work profile"), &loaded.config)
            .await
            .expect("save config");

        let requests = server.await.expect("server");
        for (request, endpoint) in requests.iter().zip([
            "/hermes/api/config?profile=work+profile",
            "/hermes/api/config/defaults?profile=work+profile",
            "/hermes/api/config/schema?profile=work+profile",
            "/hermes/api/config?profile=work+profile",
        ]) {
            assert!(request.contains(endpoint));
            assert!(request.contains("x-hermes-session-token: config-token"));
        }
        assert!(requests[0].starts_with("GET "));
        assert!(requests[1].starts_with("GET "));
        assert!(requests[2].starts_with("GET "));
        assert!(requests[3].starts_with("PUT "));
        let saved_body = requests[3]
            .split_once("\r\n\r\n")
            .map(|(_, body)| body)
            .expect("request body");
        let saved: Value = serde_json::from_str(saved_body).expect("saved JSON");
        assert_eq!(saved, json!({ "config": loaded.config }));
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn provider_service_uses_the_official_accounts_env_and_endpoint_contracts() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let address = listener.local_addr().expect("address");
        let server = tokio::spawn(async move {
            let responses = [
                json!({
                    "providers": [{
                        "id": "nous",
                        "name": "Nous Research",
                        "flow": "device_code",
                        "status": { "logged_in": true }
                    }]
                }),
                json!({ "ok": true, "provider": "nous" }),
                json!({
                    "OPENAI_API_KEY": {
                        "category": "Models",
                        "description": "OpenAI API key",
                        "is_password": true,
                        "is_set": true,
                        "redacted_value": "sk-...",
                        "tools": [],
                        "url": "https://platform.openai.com/api-keys"
                    }
                }),
                json!({ "ok": true }),
                json!({ "ok": true }),
                json!({ "key": "OPENAI_API_KEY", "value": "secret" }),
                json!({
                    "current": { "base_url": "", "model": "", "provider": "" },
                    "endpoints": []
                }),
                json!({
                    "current": { "base_url": "https://local.test/v1", "model": "", "provider": "custom" },
                    "endpoints": [],
                    "ok": true
                }),
                json!({ "message": "Connected", "models": ["model-a"], "ok": true, "reachable": true }),
                json!({ "ok": true, "provider": "custom", "model": "model-a" }),
                json!({
                    "current": { "base_url": "", "model": "", "provider": "" },
                    "endpoints": [],
                    "ok": true
                }),
            ];
            let mut requests = Vec::new();
            for body in responses {
                let (mut stream, _) = listener.accept().await.expect("connection");
                let mut request = Vec::new();
                let mut chunk = [0_u8; 4096];
                loop {
                    let count = stream.read(&mut chunk).await.expect("request bytes");
                    if count == 0 {
                        break;
                    }
                    request.extend_from_slice(&chunk[..count]);
                    let text = String::from_utf8_lossy(&request);
                    if let Some(headers_end) = text.find("\r\n\r\n") {
                        let content_length = text[..headers_end]
                            .lines()
                            .find_map(|line| line.strip_prefix("content-length: "))
                            .and_then(|value| value.parse::<usize>().ok())
                            .unwrap_or_default();
                        if request.len() >= headers_end + 4 + content_length {
                            break;
                        }
                    }
                }
                let body = body.to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream
                    .write_all(response.as_bytes())
                    .await
                    .expect("response");
                requests.push(String::from_utf8(request).expect("UTF-8 request"));
            }
            requests
        });
        let services = GatewayServices {
            client: Arc::new(RwLock::new(None)),
            rest: Arc::new(RwLock::new(Some(GatewayRest {
                client: reqwest::Client::builder()
                    .redirect(reqwest::redirect::Policy::none())
                    .build()
                    .expect("client"),
                base_url: url::Url::parse(&format!("http://{address}/hermes/")).expect("URL"),
                session_token: Some("providers-token".into()),
            }))),
            connection_store: test_connection_store(),
        };

        let providers = ProviderService::list_oauth(&services, Some("work profile"))
            .await
            .expect("OAuth providers");
        assert_eq!(providers[0].id, "nous");
        ProviderService::disconnect_oauth(&services, Some("work profile"), "nous")
            .await
            .expect("disconnect OAuth");
        let env = ProviderService::env(&services, Some("work profile"))
            .await
            .expect("environment variables");
        assert!(env["OPENAI_API_KEY"].is_set);
        ProviderService::set_env(&services, Some("work profile"), "OPENAI_API_KEY", "secret")
            .await
            .expect("set credential");
        ProviderService::delete_env(&services, Some("work profile"), "OPENAI_API_KEY")
            .await
            .expect("delete credential");
        assert_eq!(
            ProviderService::reveal_env(&services, Some("work profile"), "OPENAI_API_KEY")
                .await
                .expect("reveal credential"),
            "secret"
        );
        ProviderService::custom_endpoints(&services)
            .await
            .expect("custom endpoints");
        let endpoint = CustomEndpointUpdate {
            base_url: "https://local.test/v1".into(),
            discover_models: true,
            make_default: true,
            name: "Local gateway".into(),
            ..CustomEndpointUpdate::default()
        };
        ProviderService::save_custom_endpoint(&services, &endpoint)
            .await
            .expect("save custom endpoint");
        let validation = ProviderService::validate_custom_endpoint(&services, &endpoint)
            .await
            .expect("validate custom endpoint");
        assert_eq!(validation.models, ["model-a"]);
        ProviderService::activate_custom_endpoint(&services, "local-1")
            .await
            .expect("activate custom endpoint");
        ProviderService::delete_custom_endpoint(&services, "local-1")
            .await
            .expect("delete custom endpoint");

        let requests = server.await.expect("server");
        let expected = [
            "GET /hermes/api/providers/oauth?profile=work+profile ",
            "DELETE /hermes/api/providers/oauth/nous?profile=work+profile ",
            "GET /hermes/api/env?profile=work+profile ",
            "PUT /hermes/api/env?profile=work+profile ",
            "DELETE /hermes/api/env?profile=work+profile ",
            "POST /hermes/api/env/reveal?profile=work+profile ",
            "GET /hermes/api/providers/custom-endpoints ",
            "POST /hermes/api/providers/custom-endpoints ",
            "POST /hermes/api/providers/custom-endpoints/validate ",
            "POST /hermes/api/providers/custom-endpoints/local-1/activate ",
            "DELETE /hermes/api/providers/custom-endpoints/local-1 ",
        ];
        for (request, expected) in requests.iter().zip(expected) {
            assert!(
                request.starts_with(expected),
                "unexpected request: {request}"
            );
            assert!(request.contains("x-hermes-session-token: providers-token"));
        }
        assert!(requests[3].ends_with("{\"key\":\"OPENAI_API_KEY\",\"value\":\"secret\"}"));
        assert!(requests[4].ends_with("{\"key\":\"OPENAI_API_KEY\"}"));
        assert!(requests[5].ends_with("{\"key\":\"OPENAI_API_KEY\"}"));
        let saved_endpoint: Value = serde_json::from_str(
            requests[7]
                .split_once("\r\n\r\n")
                .map(|(_, body)| body)
                .expect("custom endpoint body"),
        )
        .expect("custom endpoint JSON");
        assert_eq!(saved_endpoint["base_url"], "https://local.test/v1");
        assert_eq!(saved_endpoint["discover_models"], true);
        assert_eq!(saved_endpoint["make_default"], true);
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn provider_oauth_session_uses_the_official_start_submit_poll_and_cancel_contracts() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let address = listener.local_addr().expect("address");
        let server = tokio::spawn(async move {
            let responses = [
                json!({
                    "flow": "pkce",
                    "auth_url": "https://auth.example/authorize",
                    "expires_in": 600,
                    "session_id": "session-1"
                }),
                json!({ "message": "Approved", "ok": true, "status": "approved" }),
                json!({ "session_id": "session-1", "status": "approved" }),
                json!({ "ok": true }),
            ];
            let mut requests = Vec::new();
            for body in responses {
                let (mut stream, _) = listener.accept().await.expect("connection");
                let mut request = Vec::new();
                let mut chunk = [0_u8; 4096];
                loop {
                    let count = stream.read(&mut chunk).await.expect("request bytes");
                    if count == 0 {
                        break;
                    }
                    request.extend_from_slice(&chunk[..count]);
                    let text = String::from_utf8_lossy(&request);
                    if let Some(headers_end) = text.find("\r\n\r\n") {
                        let content_length = text[..headers_end]
                            .lines()
                            .find_map(|line| line.strip_prefix("content-length: "))
                            .and_then(|value| value.parse::<usize>().ok())
                            .unwrap_or_default();
                        if request.len() >= headers_end + 4 + content_length {
                            break;
                        }
                    }
                }
                let body = body.to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream
                    .write_all(response.as_bytes())
                    .await
                    .expect("response");
                requests.push(String::from_utf8(request).expect("UTF-8 request"));
            }
            requests
        });
        let services = GatewayServices {
            client: Arc::new(RwLock::new(None)),
            rest: Arc::new(RwLock::new(Some(GatewayRest {
                client: reqwest::Client::builder()
                    .redirect(reqwest::redirect::Policy::none())
                    .build()
                    .expect("client"),
                base_url: url::Url::parse(&format!("http://{address}/hermes/")).expect("URL"),
                session_token: Some("oauth-token".into()),
            }))),
            connection_store: test_connection_store(),
        };

        let start = ProviderService::start_oauth(&services, Some("work profile"), "openai-codex")
            .await
            .expect("start OAuth");
        assert_eq!(start.session_id(), "session-1");
        assert_eq!(start.browser_url(), "https://auth.example/authorize");
        let submit = ProviderService::submit_oauth(
            &services,
            Some("work profile"),
            "openai-codex",
            "session-1",
            "auth-code",
        )
        .await
        .expect("submit OAuth");
        assert!(submit.ok);
        let poll = ProviderService::poll_oauth(
            &services,
            Some("work profile"),
            "openai-codex",
            "session-1",
        )
        .await
        .expect("poll OAuth");
        assert_eq!(poll.status, "approved");
        ProviderService::cancel_oauth(&services, Some("work profile"), "session-1")
            .await
            .expect("cancel OAuth");

        let requests = server.await.expect("server");
        assert!(requests[0].starts_with(
            "POST /hermes/api/providers/oauth/openai-codex/start?profile=work+profile "
        ));
        assert!(requests[0].ends_with("{}"));
        assert!(requests[1].starts_with(
            "POST /hermes/api/providers/oauth/openai-codex/submit?profile=work+profile "
        ));
        assert!(requests[1].ends_with("{\"code\":\"auth-code\",\"session_id\":\"session-1\"}"));
        assert!(requests[2].starts_with(
            "GET /hermes/api/providers/oauth/openai-codex/poll/session-1?profile=work+profile "
        ));
        assert!(requests[3].starts_with(
            "DELETE /hermes/api/providers/oauth/sessions/session-1?profile=work+profile "
        ));
        for request in requests {
            assert!(request.contains("x-hermes-session-token: oauth-token"));
        }
    }

    #[test]
    fn provider_inputs_reject_path_injection_and_credentialed_endpoint_urls() {
        assert!(validate_path_id("nous", "provider").is_ok());
        assert!(validate_path_id("../oauth", "provider").is_err());
        assert!(validate_env_key("OPENAI_API_KEY").is_ok());
        assert!(validate_env_key("OPENAI_API_KEY?profile=other").is_err());
        assert!(
            validate_custom_endpoint(&CustomEndpointUpdate {
                base_url: "https://user:password@example.com/v1".into(),
                name: "Unsafe".into(),
                ..CustomEndpointUpdate::default()
            })
            .is_err()
        );
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn model_settings_use_the_official_info_options_auxiliary_and_set_contracts() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let address = listener.local_addr().expect("address");
        let server = tokio::spawn(async move {
            let responses = [
                json!({ "provider": "nous", "model": "Hermes-4" }),
                json!({
                    "provider": "nous",
                    "model": "Hermes-4",
                    "providers": [{
                        "name": "Nous Portal",
                        "slug": "nous",
                        "models": ["Hermes-4"],
                        "capabilities": { "Hermes-4": { "reasoning": true, "fast": true } }
                    }]
                }),
                json!({
                    "main": { "provider": "nous", "model": "Hermes-4" },
                    "tasks": []
                }),
                json!({
                    "default_preset": "default",
                    "active_preset": "",
                    "presets": {}
                }),
                json!({ "ok": true, "scope": "auxiliary", "tasks": ["vision"] }),
                json!({
                    "ok": true,
                    "default_preset": "default",
                    "active_preset": "",
                    "presets": {}
                }),
            ];
            let mut requests = Vec::new();
            for body in responses {
                let (mut stream, _) = listener.accept().await.expect("connection");
                let mut request = Vec::new();
                let mut chunk = [0_u8; 2048];
                loop {
                    let count = stream.read(&mut chunk).await.expect("request bytes");
                    if count == 0 {
                        break;
                    }
                    request.extend_from_slice(&chunk[..count]);
                    let text = String::from_utf8_lossy(&request);
                    if let Some(headers_end) = text.find("\r\n\r\n") {
                        let content_length = text[..headers_end]
                            .lines()
                            .find_map(|line| line.strip_prefix("content-length: "))
                            .and_then(|value| value.parse::<usize>().ok())
                            .unwrap_or_default();
                        if request.len() >= headers_end + 4 + content_length {
                            break;
                        }
                    }
                }
                let body = body.to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream
                    .write_all(response.as_bytes())
                    .await
                    .expect("response");
                requests.push(String::from_utf8(request).expect("UTF-8 request"));
            }
            requests
        });
        let services = GatewayServices {
            client: Arc::new(RwLock::new(None)),
            rest: Arc::new(RwLock::new(Some(GatewayRest {
                client: reqwest::Client::builder()
                    .redirect(reqwest::redirect::Policy::none())
                    .build()
                    .expect("client"),
                base_url: url::Url::parse(&format!("http://{address}/hermes/")).expect("URL"),
                session_token: Some("model-token".into()),
            }))),
            connection_store: test_connection_store(),
        };
        let loaded = ModelService::load(&services, Some("work profile"))
            .await
            .expect("load models");
        assert_eq!(loaded.info.model, "Hermes-4");
        assert!(loaded.options.providers[0].capabilities["Hermes-4"].fast);
        assert!(loaded.auxiliary.tasks.is_empty());
        let moa = loaded.moa.clone().expect("MoA");
        assert_eq!(moa.default_preset, "default");
        let response = ModelService::assign(
            &services,
            Some("work profile"),
            &ModelAssignmentRequest {
                model: "Hermes-4".into(),
                provider: "nous".into(),
                scope: "auxiliary".into(),
                task: Some("vision".into()),
                base_url: None,
            },
        )
        .await
        .expect("assign model");
        assert_eq!(response.tasks, ["vision"]);
        let saved_moa = ModelService::save_moa(&services, Some("work profile"), &moa)
            .await
            .expect("save MoA");
        assert_eq!(saved_moa.default_preset, "default");

        let requests = server.await.expect("server");
        for (request, endpoint) in requests.iter().zip([
            "/hermes/api/model/info?profile=work+profile",
            "/hermes/api/model/options?explicit_only=1&profile=work+profile",
            "/hermes/api/model/auxiliary?profile=work+profile",
            "/hermes/api/model/moa?profile=work+profile",
            "/hermes/api/model/set?profile=work+profile",
            "/hermes/api/model/moa?profile=work+profile",
        ]) {
            assert!(request.contains(endpoint));
            assert!(request.contains("x-hermes-session-token: model-token"));
        }
        assert!(requests[0].starts_with("GET "));
        assert!(requests[1].starts_with("GET "));
        assert!(requests[2].starts_with("GET "));
        assert!(requests[3].starts_with("GET "));
        assert!(requests[4].starts_with("POST "));
        assert!(requests[5].starts_with("PUT "));
        let assigned_body = requests[4]
            .split_once("\r\n\r\n")
            .map(|(_, body)| body)
            .expect("request body");
        let assigned: Value = serde_json::from_str(assigned_body).expect("assignment JSON");
        assert_eq!(
            assigned,
            json!({
                "model": "Hermes-4",
                "provider": "nous",
                "scope": "auxiliary",
                "task": "vision"
            })
        );
        let saved_body = requests[5]
            .split_once("\r\n\r\n")
            .map(|(_, body)| body)
            .expect("MoA body");
        let saved: Value = serde_json::from_str(saved_body).expect("MoA JSON");
        assert_eq!(saved, serde_json::to_value(moa).expect("serialized MoA"));
    }

    #[tokio::test]
    async fn session_submit_uses_the_official_text_payload() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let address = listener.local_addr().expect("address");
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("connection");
            let mut socket = accept_async(stream).await.expect("WebSocket");
            let message = socket.next().await.expect("request").expect("frame");
            let frame: hermes_protocol::JsonRpcFrame =
                serde_json::from_str(message.to_text().expect("text frame")).expect("JSON-RPC");
            let response = json!({
                "jsonrpc": "2.0",
                "id": frame.id,
                "result": { "accepted": true }
            });
            socket
                .send(Message::Text(response.to_string().into()))
                .await
                .expect("response");
            frame
        });
        let client = GatewayClient::connect(
            &format!("ws://{address}/api/ws"),
            hermes_agent_client::GatewayOptions::default(),
        )
        .await
        .expect("gateway");
        let services = GatewayServices {
            client: Arc::new(RwLock::new(Some(client))),
            rest: Arc::new(RwLock::new(None)),
            connection_store: test_connection_store(),
        };
        services
            .submit("runtime-1", "hello Hermes")
            .await
            .expect("submit");
        let frame = server.await.expect("server");
        assert_eq!(frame.method.as_deref(), Some("prompt.submit"));
        assert_eq!(
            frame.params,
            Some(json!({ "session_id": "runtime-1", "text": "hello Hermes" }))
        );
    }

    #[tokio::test]
    async fn session_reactions_use_the_canonical_gateway_write() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let address = listener.local_addr().expect("address");
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("connection");
            let mut socket = accept_async(stream).await.expect("WebSocket");
            let mut frames = Vec::new();
            for result in [
                json!({
                    "row_id": 42,
                    "reactions": [{ "emoji": "👍", "author": "user", "at": 1.0 }]
                }),
                json!({ "row_id": 43, "reactions": [] }),
            ] {
                let message = socket.next().await.expect("request").expect("frame");
                let frame: hermes_protocol::JsonRpcFrame =
                    serde_json::from_str(message.to_text().expect("text frame")).expect("JSON-RPC");
                socket
                    .send(Message::Text(
                        json!({ "jsonrpc": "2.0", "id": frame.id, "result": result })
                            .to_string()
                            .into(),
                    ))
                    .await
                    .expect("response");
                frames.push(frame);
            }
            frames
        });
        let client = GatewayClient::connect(
            &format!("ws://{address}/api/ws"),
            hermes_agent_client::GatewayOptions::default(),
        )
        .await
        .expect("gateway");
        let services = GatewayServices {
            client: Arc::new(RwLock::new(Some(client))),
            rest: Arc::new(RwLock::new(None)),
            connection_store: test_connection_store(),
        };

        let applied = SessionService::react(
            &services,
            "runtime-1",
            Some("42"),
            hermes_protocol::MessageRole::Assistant,
            Some("👍"),
        )
        .await
        .expect("apply reaction");
        assert_eq!(applied.row_id, "42");
        assert_eq!(applied.reactions[0].emoji, "👍");

        SessionService::react(
            &services,
            "runtime-1",
            None,
            hermes_protocol::MessageRole::User,
            None,
        )
        .await
        .expect("remove newest reaction");

        let frames = server.await.expect("server");
        assert_eq!(frames[0].method.as_deref(), Some("message.react"));
        assert_eq!(
            frames[0].params,
            Some(json!({
                "session_id": "runtime-1",
                "row_id": 42,
                "emoji": "👍",
                "author": "user"
            }))
        );
        assert_eq!(
            frames[1].params,
            Some(json!({
                "session_id": "runtime-1",
                "newest_role": "user",
                "emoji": null,
                "author": "user"
            }))
        );
    }

    #[tokio::test]
    async fn projects_use_the_official_gateway_contracts() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let address = listener.local_addr().expect("address");
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("connection");
            let mut socket = accept_async(stream).await.expect("WebSocket");
            let mut frames = Vec::new();
            for result in [
                json!({
                    "project": {
                        "id": "project-1",
                        "slug": "demo",
                        "name": "Demo",
                        "primary_path": "C:\\\\Code\\\\Demo",
                        "folders": [{ "path": "C:\\\\Code\\\\Demo", "is_primary": true }]
                    }
                }),
                json!({ "active_id": "project-1" }),
                json!({ "projects": [], "active_id": null }),
            ] {
                let message = socket.next().await.expect("request").expect("frame");
                let frame: hermes_protocol::JsonRpcFrame =
                    serde_json::from_str(message.to_text().expect("text frame")).expect("JSON-RPC");
                let response = json!({
                    "jsonrpc": "2.0",
                    "id": frame.id,
                    "result": result
                });
                socket
                    .send(Message::Text(response.to_string().into()))
                    .await
                    .expect("response");
                frames.push(frame);
            }
            frames
        });
        let client = GatewayClient::connect(
            &format!("ws://{address}/api/ws"),
            hermes_agent_client::GatewayOptions::default(),
        )
        .await
        .expect("gateway");
        let services = GatewayServices {
            client: Arc::new(RwLock::new(Some(client))),
            rest: Arc::new(RwLock::new(None)),
            connection_store: test_connection_store(),
        };
        let folders = vec![r"C:\Code\Demo".to_owned()];
        let project = ProjectService::create(&services, "Demo", &folders)
            .await
            .expect("create project");
        assert_eq!(project.id, "project-1");
        ProjectService::set_active(&services, Some("project-1"))
            .await
            .expect("activate project");
        ProjectService::remove(&services, "project-1")
            .await
            .expect("remove project");

        let frames = server.await.expect("server");
        assert_eq!(frames[0].method.as_deref(), Some("projects.create"));
        assert_eq!(
            frames[0].params,
            Some(json!({
                "name": "Demo",
                "folders": [r"C:\Code\Demo"],
                "primary_path": r"C:\Code\Demo",
                "use": false
            }))
        );
        assert_eq!(frames[1].method.as_deref(), Some("projects.set_active"));
        assert_eq!(frames[1].params, Some(json!({ "id": "project-1" })));
        assert_eq!(frames[2].method.as_deref(), Some("projects.remove"));
        assert_eq!(frames[2].params, Some(json!({ "id": "project-1" })));
    }

    #[tokio::test]
    async fn session_resume_stream_and_interrupt_share_the_official_runtime_identity() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let address = listener.local_addr().expect("address");
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("connection");
            let mut socket = accept_async(stream).await.expect("WebSocket");

            let resume_message = socket.next().await.expect("resume").expect("frame");
            let resume: hermes_protocol::JsonRpcFrame =
                serde_json::from_str(resume_message.to_text().expect("text frame"))
                    .expect("JSON-RPC");
            socket
                .send(Message::Text(
                    json!({
                        "jsonrpc": "2.0",
                        "id": resume.id,
                        "result": {
                            "stored_session_id": "stored-1",
                            "session_id": "runtime-9",
                            "messages": [{ "id": "m1", "role": "user", "text": "hello" }],
                            "running": true
                        }
                    })
                    .to_string()
                    .into(),
                ))
                .await
                .expect("resume response");
            socket
                .send(Message::Text(
                    json!({
                        "jsonrpc": "2.0",
                        "method": "event",
                        "params": {
                            "type": "message.delta",
                            "session_id": "runtime-9",
                            "payload": { "text": "world" }
                        }
                    })
                    .to_string()
                    .into(),
                ))
                .await
                .expect("stream event");

            let interrupt_message = socket.next().await.expect("interrupt").expect("frame");
            let interrupt: hermes_protocol::JsonRpcFrame =
                serde_json::from_str(interrupt_message.to_text().expect("text frame"))
                    .expect("JSON-RPC");
            socket
                .send(Message::Text(
                    json!({ "jsonrpc": "2.0", "id": interrupt.id, "result": { "ok": true } })
                        .to_string()
                        .into(),
                ))
                .await
                .expect("interrupt response");
            (resume, interrupt)
        });
        let client = GatewayClient::connect(
            &format!("ws://{address}/api/ws"),
            hermes_agent_client::GatewayOptions::default(),
        )
        .await
        .expect("gateway");
        let services = GatewayServices {
            client: Arc::new(RwLock::new(Some(client))),
            rest: Arc::new(RwLock::new(None)),
            connection_store: test_connection_store(),
        };
        let mut events = SessionService::events(&services).expect("events");
        let resumed = SessionService::resume(&services, "stored-1")
            .await
            .expect("resume");
        assert_eq!(resumed.session_id, "runtime-9");
        assert_eq!(resumed.stored_session_id.as_deref(), Some("stored-1"));
        let event = events.next().await.expect("stream event");
        assert_eq!(event.kind, "message.delta");
        assert_eq!(event.session_id.as_deref(), Some("runtime-9"));
        assert_eq!(event.payload, json!({ "text": "world" }));
        SessionService::interrupt(&services, "runtime-9")
            .await
            .expect("interrupt");

        let (resume, interrupt) = server.await.expect("server");
        assert_eq!(resume.method.as_deref(), Some("session.resume"));
        assert_eq!(resume.params, Some(json!({ "session_id": "stored-1" })));
        assert_eq!(interrupt.method.as_deref(), Some("session.interrupt"));
        assert_eq!(interrupt.params, Some(json!({ "session_id": "runtime-9" })));
    }

    #[tokio::test]
    async fn project_centre_clone_pin_and_archive_match_the_source_contracts() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let address = listener.local_addr().expect("address");
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("connection");
            let mut socket = accept_async(stream).await.expect("WebSocket");
            let snapshot = json!({
                "projects": [{ "id": "project-2", "name": "Clone" }],
                "active_id": "project-2",
                "pinned_ids": ["project-2"]
            });
            let mut frames = Vec::new();
            for result in [
                snapshot.clone(),
                json!({ "project": { "id": "project-2", "name": "Clone" } }),
                snapshot.clone(),
                snapshot,
            ] {
                let message = socket.next().await.expect("request").expect("frame");
                let frame: hermes_protocol::JsonRpcFrame =
                    serde_json::from_str(message.to_text().expect("text frame")).expect("JSON-RPC");
                socket
                    .send(Message::Text(
                        json!({ "jsonrpc": "2.0", "id": frame.id, "result": result })
                            .to_string()
                            .into(),
                    ))
                    .await
                    .expect("response");
                frames.push(frame);
            }
            frames
        });
        let client = GatewayClient::connect(
            &format!("ws://{address}/api/ws"),
            hermes_agent_client::GatewayOptions::default(),
        )
        .await
        .expect("gateway");
        let services = GatewayServices {
            client: Arc::new(RwLock::new(Some(client))),
            rest: Arc::new(RwLock::new(None)),
            connection_store: test_connection_store(),
        };
        let snapshot = ProjectService::snapshot(&services)
            .await
            .expect("Project Centre");
        assert_eq!(snapshot.pinned_ids, ["project-2"]);
        let cloned = ProjectService::clone_repository(
            &services,
            "Clone",
            "git@github.com:example/clone.git",
            r"C:\Code",
        )
        .await
        .expect("clone project");
        assert_eq!(cloned.id, "project-2");
        ProjectService::set_pinned(&services, "project-2", true)
            .await
            .expect("pin");
        ProjectService::set_archived(&services, "project-2", false)
            .await
            .expect("restore");

        let frames = server.await.expect("server");
        assert_eq!(frames[0].method.as_deref(), Some("projects.centre"));
        assert_eq!(frames[0].params, Some(json!({})));
        assert_eq!(frames[1].method.as_deref(), Some("projects.clone"));
        assert_eq!(
            frames[1].params,
            Some(json!({
                "name": "Clone",
                "repository_url": "git@github.com:example/clone.git",
                "parent_path": r"C:\Code",
                "use": true
            }))
        );
        assert_eq!(frames[2].method.as_deref(), Some("projects.pin"));
        assert_eq!(
            frames[2].params,
            Some(json!({ "id": "project-2", "pinned": true }))
        );
        assert_eq!(frames[3].method.as_deref(), Some("projects.archive"));
        assert_eq!(
            frames[3].params,
            Some(json!({ "id": "project-2", "restore": true }))
        );
    }

    #[tokio::test]
    async fn project_repair_and_file_deletion_match_the_source_contracts() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let address = listener.local_addr().expect("address");
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("connection");
            let mut socket = accept_async(stream).await.expect("WebSocket");
            let results = [
                json!({
                    "project": {
                        "id": "project-broken",
                        "name": "Demo",
                        "primary_path": r"C:\Code\New",
                        "path_state": "available",
                        "folders": [{
                            "path": r"C:\Code\New",
                            "is_primary": true,
                            "path_state": "available",
                            "repository_id": "github.com/example/demo"
                        }]
                    }
                }),
                json!({
                    "projects": [],
                    "active_id": null,
                    "pinned_ids": [],
                    "deleted_paths": [r"C:\Code\New"]
                }),
            ];
            let mut frames = Vec::new();
            for result in results {
                let message = socket.next().await.expect("request").expect("frame");
                let frame: hermes_protocol::JsonRpcFrame =
                    serde_json::from_str(message.to_text().expect("text frame")).expect("JSON-RPC");
                socket
                    .send(Message::Text(
                        json!({ "jsonrpc": "2.0", "id": frame.id, "result": result })
                            .to_string()
                            .into(),
                    ))
                    .await
                    .expect("response");
                frames.push(frame);
            }
            frames
        });
        let client = GatewayClient::connect(
            &format!("ws://{address}/api/ws"),
            hermes_agent_client::GatewayOptions::default(),
        )
        .await
        .expect("gateway");
        let services = GatewayServices {
            client: Arc::new(RwLock::new(Some(client))),
            rest: Arc::new(RwLock::new(None)),
            connection_store: test_connection_store(),
        };

        let repaired = ProjectService::recover_path(
            &services,
            "project-broken",
            r"C:\Code\Old",
            r"C:\Code\New",
            Some("github.com/example/demo"),
        )
        .await
        .expect("repair project path");
        assert_eq!(repaired.primary_path.as_deref(), Some(r"C:\Code\New"));
        let deleted = ProjectService::delete_files(&services, "project-broken", "DELETE Demo")
            .await
            .expect("delete project files");
        assert!(deleted.snapshot.projects.is_empty());
        assert_eq!(deleted.deleted_paths, [r"C:\Code\New"]);

        let frames = server.await.expect("server");
        assert_eq!(frames[0].method.as_deref(), Some("projects.recover_path"));
        assert_eq!(
            frames[0].params,
            Some(json!({
                "id": "project-broken",
                "old_path": r"C:\Code\Old",
                "new_path": r"C:\Code\New",
                "repository_id": "github.com/example/demo"
            }))
        );
        assert_eq!(frames[1].method.as_deref(), Some("projects.delete_files"));
        assert_eq!(
            frames[1].params,
            Some(json!({
                "id": "project-broken",
                "confirmation": "DELETE Demo"
            }))
        );
    }

    #[tokio::test]
    async fn rest_adapter_maps_permission_and_missing_responses() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let address = listener.local_addr().expect("address");
        let server = tokio::spawn(async move {
            for response in [
                b"HTTP/1.1 403 Forbidden\r\nContent-Length: 9\r\nConnection: close\r\n\r\nforbidden"
                    .as_slice(),
                b"HTTP/1.1 404 Not Found\r\nContent-Length: 7\r\nConnection: close\r\n\r\nmissing"
                    .as_slice(),
            ] {
                let (mut stream, _) = listener.accept().await.expect("connection");
                let mut request = [0_u8; 2048];
                let _ = stream.read(&mut request).await.expect("request");
                stream.write_all(response).await.expect("response");
            }
        });
        let rest = GatewayRest {
            client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("client"),
            base_url: url::Url::parse(&format!("http://{address}/")).expect("URL"),
            session_token: None,
        };
        let forbidden = rest
            .request(Method::GET, "/private", None)
            .await
            .expect_err("permission error");
        assert!(matches!(
            forbidden,
            ServiceError::PermissionDenied(detail) if detail == "forbidden"
        ));
        let missing = rest
            .request(Method::GET, "/missing", None)
            .await
            .expect_err("not found");
        assert!(matches!(
            missing,
            ServiceError::NotFound(detail) if detail == "missing"
        ));
        server.await.expect("server");
    }
}
