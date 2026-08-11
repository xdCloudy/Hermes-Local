//! Windows desktop authority for Hermes Local.

use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex, RwLock},
};

use hermes_agent_client::GatewayClient;
use hermes_core::{
    AppServices, ConnectionService, EventStream, FileService, GitService, PlatformService,
    ProjectService, RuntimeService, ServiceError, ServiceFuture, ServiceResult, SessionService,
    SettingsService, TerminalService, TrustService, UpdateService, validate_identifier,
    validate_relative_path,
};
use hermes_protocol::{
    AppSettings, ChatMessage, ConnectionState, FileEntry, GitStatus, ProjectSummary,
    ProjectsSnapshot, RuntimeStatus, SessionCreateRequest, SessionCreateResponse, SessionSummary,
    TaskSummary, TrustSnapshot,
};
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use serde_json::{Value, json};
use tokio::sync::broadcast;
use uuid::Uuid;

#[derive(Clone)]
struct GatewayServices {
    client: Arc<RwLock<Option<GatewayClient>>>,
}

impl GatewayServices {
    fn client(&self) -> ServiceResult<GatewayClient> {
        self.client
            .read()
            .map_err(|_| ServiceError::Platform("gateway lock was poisoned".into()))?
            .clone()
            .ok_or_else(|| ServiceError::Unavailable("Hermes Agent is not connected".into()))
    }
}

pub struct NativeApp {
    pub services: AppServices,
    gateway: Arc<RwLock<Option<GatewayClient>>>,
}

impl NativeApp {
    pub fn new(data_dir: PathBuf) -> Self {
        let gateway = Arc::new(RwLock::new(None));
        let remote = Arc::new(GatewayServices {
            client: gateway.clone(),
        });
        let settings = Arc::new(JsonSettings::new(data_dir.join("settings.json")));
        let platform = Arc::new(DesktopPlatform);
        Self {
            services: AppServices {
                connection: remote.clone(),
                sessions: remote.clone(),
                projects: remote.clone(),
                settings,
                runtime: remote.clone(),
                trust: remote,
                files: Arc::new(DesktopFiles),
                git: Arc::new(DesktopGit),
                terminal: Arc::new(DesktopTerminals::default()),
                updates: Arc::new(DesktopUpdates { data_dir }),
                platform,
            },
            gateway,
        }
    }

    pub fn set_gateway(&self, client: GatewayClient) -> ServiceResult<()> {
        *self
            .gateway
            .write()
            .map_err(|_| ServiceError::Platform("gateway lock was poisoned".into()))? =
            Some(client);
        Ok(())
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
                        return Err(ServiceError::Unavailable(
                            "no gateway is configured; local Agent bootstrap is pending".into(),
                        ));
                    }
                },
            };
            self.connect(&websocket_url).await
        })
    }

    fn connect(&self, websocket_url: &str) -> ServiceFuture<'_, ConnectionState> {
        let websocket_url = websocket_url.to_owned();
        Box::pin(async move {
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
}

impl SessionService for GatewayServices {
    fn list(&self) -> ServiceFuture<'_, Vec<SessionSummary>> {
        Box::pin(async move {
            let value: Value = self
                .client()?
                .request("session.active_list", json!({}))
                .await
                .map_err(transport)?;
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

    fn resume(&self, session_id: &str) -> ServiceFuture<'_, Vec<ChatMessage>> {
        let session_id = session_id.to_owned();
        Box::pin(async move {
            validate_identifier(&session_id, "session")?;
            let value: Value = self
                .client()?
                .request("session.resume", json!({ "session_id": session_id }))
                .await
                .map_err(transport)?;
            decode_list(value, "messages")
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
                .request(
                    "prompt.submit",
                    json!({ "session_id": session_id, "prompt": text }),
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
                .request("projects.list", json!({}))
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
                    json!({ "name": name, "folders": folders }),
                )
                .await
                .map_err(transport)?;
            serde_json::from_value(value).map_err(protocol)
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
                .request("projects.set_active", json!({ "project_id": id }))
                .await
                .map_err(transport)?;
            Ok(())
        })
    }

    fn remove(&self, id: &str) -> ServiceFuture<'_, ()> {
        let id = id.to_owned();
        Box::pin(async move {
            validate_identifier(&id, "project")?;
            let _: Value = self
                .client()?
                .request("projects.delete", json!({ "project_id": id }))
                .await
                .map_err(transport)?;
            Ok(())
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
}

#[derive(Default)]
struct DesktopTerminals {
    processes: Mutex<HashMap<String, TerminalProcess>>,
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
            let processes = self
                .processes
                .lock()
                .map_err(|_| ServiceError::Platform("terminal lock was poisoned".into()))?;
            let process = processes
                .get(&id)
                .ok_or_else(|| ServiceError::NotFound("terminal".into()))?;
            let mut output = process
                .output
                .lock()
                .map_err(|_| ServiceError::Platform("terminal output lock was poisoned".into()))?;
            Ok(std::mem::take(&mut *output))
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
        Box::pin(async move {
            validate_identifier(&id, "terminal")?;
            let mut process = self
                .processes
                .lock()
                .map_err(|_| ServiceError::Platform("terminal lock was poisoned".into()))?
                .remove(&id)
                .ok_or_else(|| ServiceError::NotFound("terminal".into()))?;
            process
                .child
                .kill()
                .map_err(|error| ServiceError::Platform(error.to_string()))
        })
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
    fn open_external(&self, url: &str) -> ServiceFuture<'_, ()> {
        let url = url.to_owned();
        Box::pin(async move {
            let parsed = url::Url::parse(&url)
                .map_err(|error| ServiceError::InvalidInput(error.to_string()))?;
            if !matches!(parsed.scheme(), "https" | "mailto") {
                return Err(ServiceError::PermissionDenied(
                    "external URL scheme is blocked".into(),
                ));
            }
            open::that_detached(parsed.as_str()).map_err(platform)
        })
    }

    fn notify(&self, _title: &str, _body: &str) -> ServiceFuture<'_, ()> {
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
    url.query_pairs_mut().append_pair("token", token);
    Ok(url.into())
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
    let branch = header
        .split(['.', ' '])
        .next()
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let ahead = parse_counter(header, "ahead ");
    let behind = parse_counter(header, "behind ");
    let changed = lines
        .filter_map(|line| line.get(3..).map(str::to_owned))
        .collect();
    Ok(GitStatus {
        branch,
        ahead,
        behind,
        changed,
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
    use super::*;

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
    fn blocks_symlink_escape_for_existing_paths() {
        assert!(validate_relative_path(Path::new("../outside")).is_err());
    }

    #[test]
    fn builds_encoded_websocket_url_without_losing_base_path() {
        let url = websocket_url("https://gateway.example/hermes", "a b&c").expect("URL");
        assert_eq!(url, "wss://gateway.example/hermes/api/ws?token=a+b%26c");
    }
}
