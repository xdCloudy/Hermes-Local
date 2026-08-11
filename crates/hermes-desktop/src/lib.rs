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
    AppSettings, ConnectionState, FileEntry, GitStatus, ProjectSummary, ProjectsSnapshot,
    RuntimeStatus, SessionCreateRequest, SessionCreateResponse, SessionResumeResponse,
    SessionSummary, TaskSummary, TrustSnapshot,
};
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use reqwest::Method;
use serde_json::{Value, json};
use tokio::sync::broadcast;
use uuid::Uuid;

#[derive(Clone)]
struct GatewayServices {
    client: Arc<RwLock<Option<GatewayClient>>>,
    rest: Arc<RwLock<Option<GatewayRest>>>,
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
    use futures_util::{SinkExt, StreamExt};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio_tungstenite::{accept_async, tungstenite::Message};

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
}
