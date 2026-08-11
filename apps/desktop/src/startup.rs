use std::{
    env, fs,
    net::IpAddr,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use hermes_core::AppServices;
use hermes_protocol::ConnectionMode;
use serde_json::Value;
use tokio::process::Command;
use url::Url;

const START_TIMEOUT: Duration = Duration::from_secs(17 * 60);
const TOKEN_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_DIAGNOSTIC_BYTES: usize = 2_048;

/// Prepare the native local Hermes runtime before the shared Dioxus UI mounts.
///
/// Remote, Cloud, SSH and explicit development overrides remain owned by the
/// existing connection resolver. Local mode reuses the canonical PowerShell
/// supervisor instead of duplicating its model/runtime lifecycle in Rust.
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

    let root = resolve_project_root()?;
    let powershell = resolve_powershell()?;
    start_local_stack(&powershell, &root).await?;

    let token = read_local_token(&powershell, &root).await?;
    let (host, port) = read_local_endpoint(&root)?;
    let websocket = local_websocket_url(&host, port, &token)?;

    services
        .connection
        .connect(websocket.as_str())
        .await
        .map_err(|error| format!("Hermes Agent started but the Desktop connection failed: {error}"))?;
    Ok(())
}

fn environment_connection_override() -> bool {
    [
        "HERMES_DESKTOP_GATEWAY_WS_URL",
        "HERMES_DESKTOP_REMOTE_URL",
    ]
    .into_iter()
    .any(|name| env::var(name).is_ok_and(|value| !value.trim().is_empty()))
}

fn resolve_project_root() -> Result<PathBuf, String> {
    if let Some(explicit) = env::var_os("HERMES_LOCAL_ROOT") {
        let explicit = PathBuf::from(explicit);
        return canonical_root(&explicit).ok_or_else(|| {
            format!(
                "HERMES_LOCAL_ROOT does not point to a Hermes Local installation: {}",
                explicit.display()
            )
        });
    }

    let mut seeds = Vec::with_capacity(2);
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
        .find_map(|seed| walk_for_root(seed, 8))
        .ok_or_else(|| {
            "Could not locate the Hermes Local installation root. Set HERMES_LOCAL_ROOT to the directory containing VERSION.json and scripts\\Common-Hermes.psm1."
                .to_owned()
        })
}

fn walk_for_root(seed: &Path, max_parents: usize) -> Option<PathBuf> {
    let mut candidate = seed.to_owned();
    for _ in 0..=max_parents {
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
        && candidate.join("scripts").join("Common-Hermes.psm1").is_file();
    valid
        .then(|| candidate.canonicalize().unwrap_or_else(|_| candidate.to_owned()))
}

fn resolve_powershell() -> Result<PathBuf, String> {
    if let Some(explicit) = env::var_os("HERMES_LOCAL_PWSH") {
        let explicit = PathBuf::from(explicit);
        if explicit.is_file() {
            return Ok(explicit);
        }
        return Err(format!(
            "HERMES_LOCAL_PWSH does not point to pwsh.exe: {}",
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

    if let Ok(output) = std::process::Command::new("where.exe")
        .arg("pwsh.exe")
        .output()
        && output.status.success()
        && let Some(candidate) = String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .map(PathBuf::from)
        && candidate.is_file()
    {
        return Ok(candidate);
    }

    Err("PowerShell 7 (pwsh.exe) is required to start the Hermes Local runtime.".to_owned())
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
        .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .arg("-NonInteractive")
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    let output = tokio::time::timeout(START_TIMEOUT, command.output())
        .await
        .map_err(|_| "Timed out waiting for the Hermes Local supervisor to become ready.".to_owned())?
        .map_err(|error| format!("Could not start Hermes Local: {error}"))?;
    if output.status.success() {
        return Ok(());
    }

    let diagnostic = bounded_diagnostic(&output.stderr);
    Err(if diagnostic.is_empty() {
        format!("Hermes Local startup failed with {}.", output.status)
    } else {
        format!("Hermes Local startup failed with {}: {diagnostic}", output.status)
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
        .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
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
                host = value.to_owned();
            }
            if let Some(value) = network.get("hermesPort").and_then(Value::as_u64) {
                port = value;
            }
        }
    }

    let host = normalize_loopback_host(&host)?;
    if !(1_024..=65_535).contains(&port) {
        return Err(format!("Invalid Hermes Agent port in workstation configuration: {port}"));
    }
    Ok((host, u16::try_from(port).expect("validated port fits u16")))
}

fn read_json(path: &Path) -> Result<Value, String> {
    let bytes = fs::read(path)
        .map_err(|error| format!("Could not read {}: {error}", path.display()))?;
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
    let authority = if host.contains(':') {
        format!("[{host}]")
    } else {
        host.to_owned()
    };
    let mut url = Url::parse(&format!("ws://{authority}:{port}/api/ws"))
        .map_err(|error| format!("Could not construct the local Hermes Agent URL: {error}"))?;
    url.query_pairs_mut().append_pair("token", token);
    Ok(url)
}

fn bounded_diagnostic(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    text.chars()
        .filter(|character| !character.is_control() || matches!(character, '\n' | '\r' | '\t'))
        .take(MAX_DIAGNOSTIC_BYTES)
        .collect::<String>()
        .trim()
        .to_owned()
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
        env::temp_dir().join(format!("hermes-local-startup-{name}-{}-{nonce}", std::process::id()))
    }

    #[test]
    fn root_resolution_requires_the_canonical_markers() {
        let root = test_directory("root");
        let nested = root.join("target/debug/deps");
        fs::create_dir_all(root.join("scripts")).expect("scripts");
        fs::create_dir_all(&nested).expect("nested");
        fs::write(root.join("VERSION.json"), "{}").expect("version");
        fs::write(root.join("scripts/Common-Hermes.psm1"), "# module").expect("module");

        assert_eq!(walk_for_root(&nested, 8), root.canonicalize().ok());
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

        assert_eq!(read_local_endpoint(&root).expect("endpoint"), ("127.0.0.1".into(), 9_123));
        assert!(normalize_loopback_host("192.168.1.10").is_err());
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn local_token_and_url_validation_never_accepts_path_or_query_injection() {
        let token = "a".repeat(64);
        assert_eq!(validate_local_token(&token).expect("token"), token);
        assert!(validate_local_token(&format!("{}?admin=1", "a".repeat(40))).is_err());

        let url = local_websocket_url("127.0.0.1", 9_119, &"a b&c".repeat(10))
            .expect("URL");
        assert_eq!(url.path(), "/api/ws");
        assert_eq!(
            url.query_pairs().find(|(key, _)| key == "token").map(|(_, value)| value.into_owned()),
            Some("a b&c".repeat(10))
        );
    }
}
