use std::{
    collections::VecDeque,
    env,
    ffi::OsString,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex, OnceLock, mpsc},
    thread,
    time::{Duration, Instant},
};

use hermes_core::{AppServices, ConnectionService, ServiceError, ServiceFuture, ServiceResult};
use hermes_protocol::{
    ConnectionConfig, ConnectionConfigInput, ConnectionMode, ConnectionOauthLoginResult,
    ConnectionOauthLogoutResult, ConnectionProbeResult, ConnectionState, ConnectionTestResult,
};
use url::Url;
use uuid::Uuid;

const READY_TIMEOUT: Duration = Duration::from_secs(90);
const READY_POLL: Duration = Duration::from_millis(100);
const STDERR_TAIL_LINES: usize = 24;
const LOCAL_ROOT_WALK_DEPTH: usize = 8;
static LOCAL_GATEWAY: OnceLock<Mutex<Option<GatewayLease>>> = OnceLock::new();

pub fn install(services: &mut AppServices) {
    let inner = services.connection.clone();
    services.connection = Arc::new(LocalGatewayConnection { inner });
}

pub async fn prepare(services: &AppServices) -> Result<(), String> {
    if environment_connection_override() {
        return Ok(());
    }
    let config = services
        .connection
        .config(None)
        .await
        .map_err(|error| format!("Could not read Gateway settings: {error}"))?;
    if config.env_override || config.mode != ConnectionMode::Local {
        return Ok(());
    }
    services
        .connection
        .initialize()
        .await
        .map(|_| ())
        .map_err(|error| format!("Could not start the local Hermes Agent: {error}"))
}

pub fn shutdown() {
    let Ok(mut slot) = gateway_slot().lock() else {
        return;
    };
    slot.take();
}

struct LocalGatewayConnection {
    inner: Arc<dyn ConnectionService>,
}

impl LocalGatewayConnection {
    fn connect_local(&self) -> ServiceFuture<'_, ConnectionState> {
        Box::pin(async move {
            let websocket = ensure_running().await.map_err(ServiceError::Platform)?;
            self.inner.connect(&websocket).await
        })
    }
}

impl ConnectionService for LocalGatewayConnection {
    fn initialize(&self) -> ServiceFuture<'_, ConnectionState> {
        Box::pin(async move {
            if self.inner.state()? == ConnectionState::Open {
                return Ok(ConnectionState::Open);
            }
            if environment_connection_override() {
                return self.inner.initialize().await;
            }
            let config = self.inner.config(None).await?;
            if config.env_override || config.mode != ConnectionMode::Local {
                self.inner.initialize().await
            } else {
                self.connect_local().await
            }
        })
    }

    fn connect(&self, websocket_url: &str) -> ServiceFuture<'_, ConnectionState> {
        self.inner.connect(websocket_url)
    }

    fn disconnect(&self) -> ServiceFuture<'_, ()> {
        self.inner.disconnect()
    }

    fn state(&self) -> ServiceResult<ConnectionState> {
        self.inner.state()
    }

    fn config(&self, profile: Option<&str>) -> ServiceFuture<'_, ConnectionConfig> {
        self.inner.config(profile)
    }

    fn save_config(&self, input: &ConnectionConfigInput) -> ServiceFuture<'_, ConnectionConfig> {
        self.inner.save_config(input)
    }

    fn apply_config(&self, input: &ConnectionConfigInput) -> ServiceFuture<'_, ConnectionConfig> {
        let input = input.clone();
        Box::pin(async move {
            if input.mode != ConnectionMode::Local {
                return self.inner.apply_config(&input).await;
            }
            let config = self.inner.save_config(&input).await?;
            self.inner.disconnect().await?;
            if input.profile.is_none() {
                self.connect_local().await?;
                return Ok(config);
            }
            let global = self.inner.config(None).await?;
            if global.env_override || global.mode != ConnectionMode::Local {
                self.inner.initialize().await?;
            } else {
                self.connect_local().await?;
            }
            Ok(config)
        })
    }

    fn test_config(
        &self,
        input: &ConnectionConfigInput,
    ) -> ServiceFuture<'_, ConnectionTestResult> {
        self.inner.test_config(input)
    }

    fn probe_config(&self, remote_url: &str) -> ServiceFuture<'_, ConnectionProbeResult> {
        self.inner.probe_config(remote_url)
    }

    fn oauth_login(&self, remote_url: &str) -> ServiceFuture<'_, ConnectionOauthLoginResult> {
        self.inner.oauth_login(remote_url)
    }

    fn oauth_logout(&self, remote_url: &str) -> ServiceFuture<'_, ConnectionOauthLogoutResult> {
        self.inner.oauth_logout(remote_url)
    }
}

fn environment_connection_override() -> bool {
    ["HERMES_DESKTOP_GATEWAY_WS_URL", "HERMES_DESKTOP_REMOTE_URL"]
        .into_iter()
        .any(|name| env::var(name).is_ok_and(|value| !value.trim().is_empty()))
}

async fn ensure_running() -> Result<String, String> {
    tokio::task::spawn_blocking(ensure_running_sync)
        .await
        .map_err(|error| format!("Local Gateway supervisor task failed: {error}"))?
}

fn gateway_slot() -> &'static Mutex<Option<GatewayLease>> {
    LOCAL_GATEWAY.get_or_init(|| Mutex::new(None))
}

fn ensure_running_sync() -> Result<String, String> {
    let mut slot = gateway_slot()
        .lock()
        .map_err(|_| "Local Gateway supervisor lock was poisoned.".to_owned())?;
    if let Some(lease) = slot.as_mut() {
        if lease.child.try_wait().is_ok_and(|status| status.is_none()) {
            return Ok(lease.websocket_url.clone());
        }
        *slot = None;
    }
    let lease = spawn_gateway()?;
    let websocket = lease.websocket_url.clone();
    *slot = Some(lease);
    Ok(websocket)
}

struct GatewayLease {
    child: Child,
    websocket_url: String,
}

impl Drop for GatewayLease {
    fn drop(&mut self) {
        terminate_child(&mut self.child);
    }
}

struct ChildGuard {
    child: Option<Child>,
}

impl ChildGuard {
    const fn new(child: Child) -> Self {
        Self { child: Some(child) }
    }

    fn child_mut(&mut self) -> &mut Child {
        self.child.as_mut().expect("child guard remains armed")
    }

    fn into_child(mut self) -> Child {
        self.child.take().expect("child guard remains armed")
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            terminate_child(child);
        }
    }
}

struct RuntimeCommand {
    program: OsString,
    prefix_args: Vec<OsString>,
    cwd: Option<PathBuf>,
    label: String,
}

fn spawn_gateway() -> Result<GatewayLease, String> {
    let runtime = resolve_runtime();
    let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let mut command = gateway_command(&runtime, &token);

    let child = command.spawn().map_err(|error| {
        format!(
            "Could not start the app-owned Hermes Gateway via {}: {error}. Repair the Hermes Agent runtime or set HERMES_DESKTOP_HERMES to a working hermes executable.",
            runtime.label
        )
    })?;
    let mut child = ChildGuard::new(child);
    let stdout = child
        .child_mut()
        .stdout
        .take()
        .ok_or_else(|| "Hermes Gateway stdout was not captured.".to_owned())?;
    let stderr = child
        .child_mut()
        .stderr
        .take()
        .ok_or_else(|| "Hermes Gateway stderr was not captured.".to_owned())?;
    let stderr_tail = Arc::new(Mutex::new(VecDeque::<String>::new()));
    let stderr_tail_writer = stderr_tail.clone();
    thread::Builder::new()
        .name("hermes-gateway-stderr".into())
        .spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                let Ok(mut tail) = stderr_tail_writer.lock() else {
                    return;
                };
                if tail.len() >= STDERR_TAIL_LINES {
                    tail.pop_front();
                }
                tail.push_back(line);
            }
        })
        .map_err(|error| format!("Could not start Gateway stderr monitor: {error}"))?;

    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    thread::Builder::new()
        .name("hermes-gateway-stdout".into())
        .spawn(move || {
            let reader = BufReader::new(stdout);
            let mut announced = false;
            for line in reader.lines().map_while(Result::ok) {
                if !announced && let Some(port) = parse_ready_port(&line) {
                    let _ = ready_tx.send(port);
                    announced = true;
                }
            }
        })
        .map_err(|error| format!("Could not start Gateway stdout monitor: {error}"))?;

    let deadline = Instant::now() + READY_TIMEOUT;
    let port = loop {
        if let Ok(port) = ready_rx.recv_timeout(READY_POLL) {
            break port;
        }
        match child.child_mut().try_wait() {
            Ok(Some(status)) => {
                return Err(with_stderr_tail(
                    format!(
                        "App-owned Hermes Gateway ({}) exited before becoming ready ({status}).",
                        runtime.label
                    ),
                    &stderr_tail,
                ));
            }
            Ok(None) => {}
            Err(error) => return Err(format!("Could not inspect Hermes Gateway process: {error}")),
        }
        if Instant::now() >= deadline {
            return Err(with_stderr_tail(
                format!(
                    "Timed out waiting for the app-owned Hermes Gateway ({}) to announce its port.",
                    runtime.label
                ),
                &stderr_tail,
            ));
        }
    };

    let mut websocket = Url::parse(&format!("ws://127.0.0.1:{port}/api/ws"))
        .map_err(|error| format!("Could not construct the local Gateway URL: {error}"))?;
    websocket.query_pairs_mut().append_pair("token", &token);
    Ok(GatewayLease {
        child: child.into_child(),
        websocket_url: websocket.to_string(),
    })
}

fn gateway_command(runtime: &RuntimeCommand, token: &str) -> Command {
    let mut command = Command::new(&runtime.program);
    command
        .args(&runtime.prefix_args)
        .args(["serve", "--host", "127.0.0.1", "--port", "0"])
        .env("HERMES_DASHBOARD_SESSION_TOKEN", token)
        .env("HERMES_DESKTOP", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(cwd) = &runtime.cwd {
        command.current_dir(cwd);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        command.creation_flags(0x0800_0000);
    }
    command
}

fn terminate_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn with_stderr_tail(message: String, stderr_tail: &Arc<Mutex<VecDeque<String>>>) -> String {
    let Ok(tail) = stderr_tail.lock() else {
        return message;
    };
    if tail.is_empty() {
        return message;
    }
    format!(
        "{message} Backend stderr: {}",
        tail.iter().cloned().collect::<Vec<_>>().join(" | ")
    )
}

fn resolve_runtime() -> RuntimeCommand {
    if let Some(path) = env::var_os("HERMES_DESKTOP_HERMES").map(PathBuf::from)
        && path.is_file()
    {
        return runtime_from_program(path, None, "HERMES_DESKTOP_HERMES override");
    }

    if let Some(root) = env::var_os("HERMES_DESKTOP_HERMES_ROOT").map(PathBuf::from)
        && let Some(runtime) = runtime_under_source_root(&root)
    {
        return runtime;
    }

    if let Some(root) = resolve_hermes_local_root()
        && let Some(runtime) = runtime_under_local_root(&root)
    {
        return runtime;
    }

    let home = env::var_os("HERMES_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("LOCALAPPDATA").map(|value| PathBuf::from(value).join("hermes")));
    if let Some(root) = home.map(|home| home.join("hermes-agent"))
        && let Some(runtime) = runtime_under_source_root(&root)
    {
        return runtime;
    }

    RuntimeCommand {
        program: "hermes".into(),
        prefix_args: Vec::new(),
        cwd: None,
        label: "`hermes` from PATH".into(),
    }
}

fn resolve_hermes_local_root() -> Option<PathBuf> {
    if let Some(root) = env::var_os("HERMES_LOCAL_ROOT").map(PathBuf::from)
        && is_hermes_local_root(&root)
    {
        return Some(root);
    }

    if let Ok(current) = env::current_dir()
        && let Some(root) = walk_for_hermes_local_root(current)
    {
        return Some(root);
    }

    let executable_dir = env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_owned));
    executable_dir.and_then(walk_for_hermes_local_root)
}

fn walk_for_hermes_local_root(mut candidate: PathBuf) -> Option<PathBuf> {
    for _ in 0..=LOCAL_ROOT_WALK_DEPTH {
        if is_hermes_local_root(&candidate) {
            return Some(candidate);
        }
        if !candidate.pop() {
            break;
        }
    }
    None
}

fn is_hermes_local_root(candidate: &Path) -> bool {
    candidate.join("VERSION.json").is_file()
        && candidate.join("scripts/Common-Hermes.psm1").is_file()
}

fn runtime_under_local_root(root: &Path) -> Option<RuntimeCommand> {
    let venv = root.join("runtimes/python/hermes");
    let source = root.join("source/hermes-agent");
    let cwd = if source.is_dir() {
        source
    } else {
        root.to_owned()
    };
    runtime_from_venv(&venv, cwd, "Hermes Local managed runtime")
}

fn runtime_under_source_root(root: &Path) -> Option<RuntimeCommand> {
    for venv in [root.join(".venv"), root.join("venv")] {
        if let Some(runtime) = runtime_from_venv(&venv, root.to_owned(), "Hermes source runtime") {
            return Some(runtime);
        }
    }
    None
}

fn runtime_from_venv(venv: &Path, cwd: PathBuf, label: &str) -> Option<RuntimeCommand> {
    #[cfg(windows)]
    let hermes = venv.join("Scripts/hermes.exe");
    #[cfg(not(windows))]
    let hermes = venv.join("bin/hermes");
    if hermes.is_file() {
        return Some(RuntimeCommand {
            label: format!("{label} at {}", hermes.display()),
            program: hermes.into_os_string(),
            prefix_args: Vec::new(),
            cwd: Some(cwd),
        });
    }

    #[cfg(windows)]
    let python = venv.join("Scripts/python.exe");
    #[cfg(not(windows))]
    let python = venv.join("bin/python");
    python.is_file().then(|| RuntimeCommand {
        label: format!("{label} Python at {}", python.display()),
        program: python.into_os_string(),
        prefix_args: vec!["-m".into(), "hermes_cli.main".into()],
        cwd: Some(cwd),
    })
}

fn runtime_from_program(path: PathBuf, cwd: Option<PathBuf>, label: &str) -> RuntimeCommand {
    let fallback_cwd = path.parent().map(Path::to_owned);
    RuntimeCommand {
        label: format!("{label} at {}", path.display()),
        program: path.into_os_string(),
        prefix_args: Vec::new(),
        cwd: cwd.or(fallback_cwd),
    }
}

fn parse_ready_port(line: &str) -> Option<u16> {
    ["HERMES_BACKEND_READY port=", "HERMES_DASHBOARD_READY port="]
        .into_iter()
        .find_map(|marker| line.trim().strip_prefix(marker)?.parse::<u16>().ok())
        .filter(|port| *port != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_current_and_legacy_ready_announcements() {
        assert_eq!(
            parse_ready_port("HERMES_BACKEND_READY port=49152"),
            Some(49_152)
        );
        assert_eq!(
            parse_ready_port("HERMES_DASHBOARD_READY port=9119"),
            Some(9_119)
        );
        assert_eq!(parse_ready_port("unrelated output"), None);
    }

    #[test]
    fn recognizes_hermes_local_root_markers() {
        let root = env::temp_dir().join(format!(
            "hermes-local-gateway-root-{}",
            Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(root.join("scripts")).expect("scripts");
        std::fs::write(root.join("VERSION.json"), "{}").expect("version");
        std::fs::write(root.join("scripts/Common-Hermes.psm1"), "# module").expect("module");
        assert!(is_hermes_local_root(&root));
        std::fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn gateway_command_is_loopback_ephemeral_and_session_scoped() {
        let root = env::temp_dir().join("hermes-local-command-contract");
        let runtime = RuntimeCommand {
            program: "hermes-test".into(),
            prefix_args: vec!["--shim".into()],
            cwd: Some(root.clone()),
            label: "test runtime".into(),
        };
        let command = gateway_command(&runtime, "private-session-token");
        let args = command
            .get_args()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            args,
            ["--shim", "serve", "--host", "127.0.0.1", "--port", "0"]
        );
        assert_eq!(command.get_current_dir(), Some(root.as_path()));
        let environment = command
            .get_envs()
            .map(|(name, value)| {
                (
                    name.to_string_lossy().into_owned(),
                    value.map(|value| value.to_string_lossy().into_owned()),
                )
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(
            environment.get("HERMES_DASHBOARD_SESSION_TOKEN"),
            Some(&Some("private-session-token".to_owned()))
        );
        assert_eq!(
            environment.get("HERMES_DESKTOP"),
            Some(&Some("1".to_owned()))
        );
    }
}
