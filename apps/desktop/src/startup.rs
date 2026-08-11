use std::{
    env, fs,
    net::IpAddr,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Duration,
};

use hermes_core::{AppServices, ConnectionService, ServiceError, ServiceFuture, ServiceResult};
use hermes_protocol::{
    ConnectionConfig, ConnectionConfigInput, ConnectionMode, ConnectionOauthLoginResult,
    ConnectionOauthLogoutResult, ConnectionProbeResult, ConnectionState, ConnectionTestResult,
};
use serde_json::Value;
use tokio::process::Command;
use url::Url;

const START_TIMEOUT: Duration = Duration::from_mins(17);
const TOKEN_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_DIAGNOSTIC_BYTES: usize = 2_048;
const ROOT_SEARCH_DEPTH: usize = 8;

/// Decorate the native connection service with the one missing migration seam:
/// starting the canonical local Hermes runtime before dialing its Agent socket.
/// Remote, Cloud, SSH and OAuth behavior stays owned by the existing native
/// connection implementation.
pub fn install_local_bootstrap(services: &mut AppServices) {
    let inner = services.connection.clone();
    services.connection = Arc::new(LocalBootstrapConnection { inner });
}

/// Prepare local mode before the shared Dioxus UI mounts. The installed
/// connection decorator makes the same behavior available later for a live
/// Remote/Cloud/SSH -> Local re-home, so initial boot and settings do not drift.
pub async fn prepare_local_agent(services: &AppServices) -> Result<(), String> {
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

struct LocalBootstrapConnection {
    inner: Arc<dyn ConnectionService>,
}

impl LocalBootstrapConnection {
    fn connect_local(&self) -> ServiceFuture<'_, ConnectionState> {
        Box::pin(async move {
            let root = resolve_project_root().map_err(bootstrap_error)?;
            let powershell = resolve_powershell().map_err(bootstrap_error)?;
            start_local_stack(&powershell, &root)
                .await
                .map_err(bootstrap_error)?;

            let token = read_local_token(&powershell, &root)
                .await
                .map_err(bootstrap_error)?;
            let (host, port) = read_local_endpoint(&root).map_err(bootstrap_error)?;
            let websocket = local_websocket_url(&host, port, &token).map_err(bootstrap_error)?;
            self.inner.connect(websocket.as_str()).await
        })
    }
}

impl ConnectionService for LocalBootstrapConnection {
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

            // Per-profile Local means "use the default gateway", not "force the
            // machine-local gateway". Re-resolve the global/default connection
            // exactly as the OG client does, while still filling the missing
            // local bootstrap rung when that default is Local.
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

fn bootstrap_error(error: String) -> ServiceError {
    ServiceError::Platform(error)
}

fn environment_connection_override() -> bool {
    ["HERMES_DESKTOP_GATEWAY_WS_URL", "HERMES_DESKTOP_REMOTE_URL"]
        .into_iter()
        .any(|name| env::var(name).is_ok_and(|value| !value.trim().is_empty()))
}

fn resolve_project_root() -> Result<PathBuf, String> {
    if let Some(explicit) = env::var_os("HERMES_LOCAL_ROOT") {
        let explicit = PathBuf::from(explicit);
        return validate_explicit_root(&explicit, "HERMES_LOCAL_ROOT");
    }

    if let Some(argument) = env::args_os().find_map(|argument| {
        let value = argument.to_string_lossy();
        value
            .strip_prefix("--hermes-local-root=")
            .map(PathBuf::from)
    }) {
        return validate_explicit_root(&argument, "--hermes-local-root");
    }

    let mut seeds = Vec::with_capacity(3);
    if let Some(portable) = env::var_os("PORTABLE_EXECUTABLE_DIR") {
        let portable = PathBuf::from(portable);
        if portable.is_absolute() {
            seeds.push(portable);
        }
    }
    if let Ok(executable) = env::current_exe()
        && let Some(parent) = executable.parent()
    {
        seeds.push(parent.to_owned());
    }
    if let Ok(cwd) = env::current_dir() {
        seeds.push(cwd);
    }

    seeds
        .iter()
        .find_map(|seed| walk_for_root(seed, ROOT_SEARCH_DEPTH))
        .ok_or_else(|| {
            "Could not locate the Hermes Local installation root. Set HERMES_LOCAL_ROOT or launch the portable app from the Hermes Local installation."
                .to_owned()
        })
}

fn validate_explicit_root(path: &Path, source: &str) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err(format!("{source} must be an absolute path"));
    }
    canonical_root(path).ok_or_else(|| {
        format!(
            "{source} does not point to a Hermes Local installation: {}",
            path.display()
        )
    })
}

fn walk_for_root(seed: &Path, max_depth: usize) -> Option<PathBuf> {
    let mut candidate = seed.to_owned();
    for _ in 0..max_depth {
        if let Some(root) = canonical_root(&candidate) {
            return Some(root);
        }
        if !candidate.pop() {
            break;
        }
    }
    None
}

fn canonical_root(candidate: &Path) -> Option<PathBuf> {
    let valid = candidate.join("VERSION.json").is_file()
        && candidate
            .join("scripts")
            .join("Common-Hermes.psm1")
            .is_file();
    valid.then(|| {
        candidate
            .canonicalize()
            .unwrap_or_else(|_| candidate.to_owned())
    })
}

fn resolve_powershell() -> Result<PathBuf, String> {
    if let Some(explicit) = env::var_os("HERMES_LOCAL_PWSH") {
        let explicit = PathBuf::from(explicit);
        if explicit.is_absolute() && explicit.is_file() {
            return Ok(explicit);
        }
        return Err(format!(
            "HERMES_LOCAL_PWSH does not point to an absolute PowerShell executable: {}",
            explicit.display()
        ));
    }

    if let Some(program_files) = env::var_os("ProgramFiles") {
        let candidate = PathBuf::from(program_files)
            .join("PowerShell")
            .join("7")
            .join("pwsh.exe");
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    let system_root = env::var_os("SystemRoot")
        .map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
    let where_exe = system_root.join("System32").join("where.exe");
    if let Ok(output) = std::process::Command::new(where_exe)
        .arg("pwsh.exe")
        .output()
        && output.status.success()
        && let Some(candidate) = String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(PathBuf::from)
            .find(|candidate| candidate.is_absolute() && candidate.is_file())
    {
        return Ok(candidate);
    }

    let windows_powershell = system_root
        .join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe");
    if windows_powershell.is_file() {
        return Ok(windows_powershell);
    }

    Err("PowerShell is required to start the Hermes Local runtime.".to_owned())
}

async fn start_local_stack(powershell: &Path, root: &Path) -> Result<(), String> {
    let script = root.join("Start-Hermes-Local.ps1");
    if !script.is_file() {
        return Err(format!(
            "Hermes Local start script is missing: {}",
            script.display()
        ));
    }

    let mut command = Command::new(powershell);
    command
        .current_dir(root)
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(&script)
        .arg("-NonInteractive")
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    let output = tokio::time::timeout(START_TIMEOUT, command.output())
        .await
        .map_err(|_| {
            "Timed out waiting for the Hermes Local supervisor to become ready.".to_owned()
        })?
        .map_err(|error| format!("Could not start Hermes Local: {error}"))?;
    if output.status.success() {
        return Ok(());
    }

    let diagnostic = bounded_diagnostic(&output.stderr);
    Err(if diagnostic.is_empty() {
        format!("Hermes Local startup failed with {}.", output.status)
    } else {
        format!(
            "Hermes Local startup failed with {}: {diagnostic}",
            output.status
        )
    })
}

async fn read_local_token(powershell: &Path, root: &Path) -> Result<String, String> {
    let script = root
        .join("scripts")
        .join("launch")
        .join("Get-Hermes-Local-Token.ps1");
    if !script.is_file() {
        return Err(format!(
            "Hermes Local token helper is missing: {}",
            script.display()
        ));
    }

    let mut command = Command::new(powershell);
    command
        .current_dir(root)
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(&script)
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let output = tokio::time::timeout(TOKEN_TIMEOUT, command.output())
        .await
        .map_err(|_| "Timed out reading the protected Hermes Local API token.".to_owned())?
        .map_err(|error| format!("Could not read the protected Hermes Local API token: {error}"))?;
    if !output.status.success() {
        let diagnostic = bounded_diagnostic(&output.stderr);
        return Err(if diagnostic.is_empty() {
            "Hermes Local token helper failed.".to_owned()
        } else {
            format!("Hermes Local token helper failed: {diagnostic}")
        });
    }
    if output.stdout.len() > 4_096 {
        return Err("Hermes Local token helper returned an oversized response.".to_owned());
    }

    validate_local_token(String::from_utf8_lossy(&output.stdout).trim())
}

fn validate_local_token(token: &str) -> Result<String, String> {
    if !(40..=128).contains(&token.len())
        || !token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err("Hermes Local returned an invalid protected API token.".to_owned());
    }
    Ok(token.to_owned())
}

fn read_local_endpoint(root: &Path) -> Result<(String, u16), String> {
    let defaults = read_json(&root.join("config/defaults/workstation.json"))?;
    let network = defaults
        .get("network")
        .and_then(Value::as_object)
        .ok_or_else(|| "Default workstation configuration has no network section.".to_owned())?;
    let mut host = network
        .get("host")
        .and_then(Value::as_str)
        .unwrap_or("127.0.0.1")
        .to_owned();
    let mut port = network
        .get("hermesPort")
        .and_then(Value::as_u64)
        .unwrap_or(9_119);

    let user_path = root.join("config/launcher/user-settings.json");
    if user_path.is_file() {
        let user = read_json(&user_path)?;
        if let Some(network) = user.get("network").and_then(Value::as_object) {
            if let Some(value) = network.get("host").and_then(Value::as_str) {
                value.clone_into(&mut host);
            }
            if let Some(value) = network.get("hermesPort").and_then(Value::as_u64) {
                port = value;
            }
        }
    }

    let host = normalize_loopback_host(&host)?;
    if !(1_024..=65_535).contains(&port) {
        return Err(format!(
            "Invalid Hermes Agent port in workstation configuration: {port}"
        ));
    }
    Ok((host, u16::try_from(port).expect("validated port fits u16")))
}

fn read_json(path: &Path) -> Result<Value, String> {
    let bytes =
        fs::read(path).map_err(|error| format!("Could not read {}: {error}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("Invalid JSON in {}: {error}", path.display()))
}

fn normalize_loopback_host(raw: &str) -> Result<String, String> {
    let raw = raw.trim();
    if raw.eq_ignore_ascii_case("localhost") {
        return Ok("127.0.0.1".to_owned());
    }
    let address: IpAddr = raw
        .parse()
        .map_err(|_| format!("Invalid Hermes Agent host in workstation configuration: {raw}"))?;
    if !address.is_loopback() {
        return Err(format!(
            "Hermes Local services must use a loopback host, not '{raw}'."
        ));
    }
    Ok(address.to_string())
}

fn local_websocket_url(host: &str, port: u16, token: &str) -> Result<Url, String> {
    let token = validate_local_token(token)?;
    let authority = if host.contains(':') {
        format!("[{host}]")
    } else {
        host.to_owned()
    };
    let mut url = Url::parse(&format!("ws://{authority}:{port}/api/ws"))
        .map_err(|error| format!("Could not construct the local Hermes Agent URL: {error}"))?;
    url.query_pairs_mut().append_pair("token", &token);
    Ok(url)
}

fn bounded_diagnostic(bytes: &[u8]) -> String {
    let mut text = String::from_utf8_lossy(bytes).into_owned();
    for private_path in [
        env::var("USERPROFILE").ok(),
        env::var("HERMES_LOCAL_ROOT").ok(),
    ]
    .into_iter()
    .flatten()
    .filter(|value| !value.is_empty())
    {
        text = text.replace(&private_path, "[PRIVATE-PATH]");
        text = text.replace(&private_path.replace('\\', "/"), "[PRIVATE-PATH]");
    }
    redact_long_values(&text)
        .chars()
        .filter(|character| !character.is_control() || matches!(character, '\n' | '\r' | '\t'))
        .take(MAX_DIAGNOSTIC_BYTES)
        .collect::<String>()
        .trim()
        .to_owned()
}

fn redact_long_values(input: &str) -> String {
    fn flush(output: &mut String, run: &mut String) {
        if run.len() >= 40 {
            output.push_str("[REDACTED-LONG-VALUE]");
        } else {
            output.push_str(run);
        }
        run.clear();
    }

    let mut output = String::with_capacity(input.len());
    let mut run = String::new();
    for character in input.chars() {
        if character.is_ascii_alphanumeric() || matches!(character, '_' | '-') {
            run.push(character);
        } else {
            flush(&mut output, &mut run);
            output.push(character);
        }
    }
    flush(&mut output, &mut run);
    output
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn test_directory(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        env::temp_dir().join(format!(
            "hermes-local-startup-{name}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn root_resolution_requires_the_canonical_markers() {
        let root = test_directory("root");
        let nested = root.join("target/debug/deps");
        fs::create_dir_all(root.join("scripts")).expect("scripts");
        fs::create_dir_all(&nested).expect("nested");
        fs::write(root.join("VERSION.json"), "{}").expect("version");
        fs::write(root.join("scripts/Common-Hermes.psm1"), "# module").expect("module");

        assert_eq!(
            walk_for_root(&nested, ROOT_SEARCH_DEPTH),
            root.canonicalize().ok()
        );
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn explicit_roots_must_be_absolute_and_valid() {
        assert!(validate_explicit_root(Path::new("relative"), "test").is_err());
        let root = test_directory("explicit");
        fs::create_dir_all(&root).expect("root");
        assert!(validate_explicit_root(&root, "test").is_err());
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn endpoint_merge_honours_user_override_but_keeps_loopback_only() {
        let root = test_directory("endpoint");
        fs::create_dir_all(root.join("config/defaults")).expect("defaults");
        fs::create_dir_all(root.join("config/launcher")).expect("launcher");
        fs::write(
            root.join("config/defaults/workstation.json"),
            r#"{"network":{"host":"127.0.0.1","hermesPort":9119}}"#,
        )
        .expect("defaults JSON");
        fs::write(
            root.join("config/launcher/user-settings.json"),
            r#"{"schemaVersion":1,"network":{"host":"localhost","hermesPort":9123}}"#,
        )
        .expect("user JSON");

        assert_eq!(
            read_local_endpoint(&root).expect("endpoint"),
            ("127.0.0.1".into(), 9_123)
        );
        assert!(normalize_loopback_host("192.168.1.10").is_err());
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn local_token_and_url_validation_never_accepts_path_or_query_injection() {
        let token = "a".repeat(64);
        let url = local_websocket_url("127.0.0.1", 9_119, &token).expect("URL");
        assert_eq!(url.path(), "/api/ws");
        assert_eq!(
            url.query_pairs()
                .find(|(key, _)| key == "token")
                .map(|(_, value)| value.into_owned()),
            Some(token)
        );
        assert!(
            local_websocket_url("127.0.0.1", 9_119, &format!("{}?admin=1", "a".repeat(40)))
                .is_err()
        );
    }

    #[test]
    fn diagnostics_redact_tokens_and_keep_errors_bounded() {
        let token = "A".repeat(64);
        let diagnostic =
            bounded_diagnostic(format!("authorization failed for token {token}").as_bytes());
        assert!(!diagnostic.contains(&token));
        assert!(diagnostic.contains("[REDACTED-LONG-VALUE]"));
        assert!(diagnostic.len() <= MAX_DIAGNOSTIC_BYTES);
    }

    #[test]
    fn repository_network_contract_is_loopback() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("repository root");
        let (host, port) = read_local_endpoint(&root).expect("repository endpoint");
        assert_eq!(host, "127.0.0.1");
        assert!((1_024..=65_535).contains(&port));
    }
}
