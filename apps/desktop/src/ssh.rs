//! Native OpenSSH transport used by the Dioxus desktop connection boundary.
//!
//! This intentionally uses the system OpenSSH client rather than an SSH crate so
//! Hermes Local keeps the OG launcher's ~/.ssh/config, ProxyJump, ssh-agent and
//! hardware-key behavior. All local process arguments are passed as an argv
//! vector; only fixed, narrowly quoted commands cross the SSH remote-shell
//! boundary.

use std::{
    env,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use hermes_protocol::{ConnectionTestResult, SshErrorKind};
use serde_json::Value;
use tokio::process::Command;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const EXEC_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_OUTPUT_BYTES: usize = 256 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SshConfig {
    pub host: String,
    pub user: Option<String>,
    pub port: u16,
    pub key_path: Option<PathBuf>,
    pub remote_hermes_path: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RemoteRuntime {
    os: String,
    arch: String,
    hermes_path: String,
    hermes_version: String,
}

#[derive(Debug)]
struct SshFailure {
    kind: SshErrorKind,
    message: String,
}

impl SshConfig {
    pub fn new(
        host: &str,
        user: Option<&str>,
        port: Option<u16>,
        key_path: Option<&str>,
        remote_hermes_path: Option<&str>,
    ) -> Result<Self, String> {
        let host = host.trim();
        validate_target_component(host, "host")?;
        let user = user.map(str::trim).filter(|value| !value.is_empty());
        if let Some(user) = user {
            validate_target_component(user, "user")?;
        }
        let key_path = key_path
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from);
        if let Some(path) = key_path.as_deref() {
            validate_key_path(path)?;
        }
        let remote_hermes_path = remote_hermes_path
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        if let Some(path) = remote_hermes_path.as_deref() {
            validate_remote_path(path)?;
        }
        Ok(Self {
            host: host.to_owned(),
            user: user.map(str::to_owned),
            port: port.unwrap_or(22),
            key_path,
            remote_hermes_path,
        })
    }

    fn target(&self) -> String {
        self.user
            .as_ref()
            .map_or_else(|| self.host.clone(), |user| format!("{user}@{}", self.host))
    }
}

/// Probe an SSH target without mutating the saved connection or starting a
/// remote Hermes process. The result shape matches the OG Gateway settings UI.
pub async fn test_connection(config: &SshConfig) -> ConnectionTestResult {
    match probe_runtime(config).await {
        Ok(runtime) => ConnectionTestResult {
            ok: Some(true),
            reachable: Some(true),
            ssh_error: None,
            error: None,
            host: Some(config.target()),
            remote_platform: Some(format!("{}/{}", runtime.os, runtime.arch)),
            remote_hermes_path: Some(runtime.hermes_path),
            remote_hermes_version: Some(runtime.hermes_version),
            ..ConnectionTestResult::default()
        },
        Err(error) => ConnectionTestResult {
            ok: Some(false),
            reachable: Some(false),
            ssh_error: Some(error.kind),
            error: Some(error.message),
            host: Some(config.target()),
            ..ConnectionTestResult::default()
        },
    }
}

async fn probe_runtime(config: &SshConfig) -> Result<RemoteRuntime, SshFailure> {
    // `exit 0` is understood by POSIX login shells and Windows PowerShell/cmd.
    // Doing this first keeps auth/host-key/network failures distinct from the
    // expected `uname` failure on a Windows SSH host.
    exec(config, "exit 0", CONNECT_TIMEOUT).await?;

    match exec(config, "uname -s; uname -m", EXEC_TIMEOUT).await {
        Ok(output) => {
            let mut lines = output
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty());
            let os = lines.next().unwrap_or_default();
            let arch = lines.next().unwrap_or_default();
            if matches!(os, "Linux" | "Darwin") {
                return probe_unix_runtime(config, os, arch).await;
            }
        }
        Err(error) if is_transport_kind(error.kind) => return Err(error),
        Err(_) => {}
    }

    probe_windows_runtime(config).await
}

async fn probe_unix_runtime(
    config: &SshConfig,
    os: &str,
    arch: &str,
) -> Result<RemoteRuntime, SshFailure> {
    let hermes_path = if let Some(explicit) = config.remote_hermes_path.as_deref() {
        let expanded = expand_unix_path(explicit).map_err(invalid_remote)?;
        let command = format!("[ -x {expanded} ] && printf '%s' {expanded} || exit 44");
        exec(config, &command, EXEC_TIMEOUT)
            .await
            .map_err(|error| map_missing_hermes(error, explicit))?
            .trim()
            .to_owned()
    } else {
        let command = concat!(
            "p=$(bash -lc 'command -v hermes' 2>/dev/null || true); ",
            "if [ -n \"$p\" ] && [ -x \"$p\" ]; then printf '%s' \"$p\"; exit 0; fi; ",
            "for p in \"$HOME/.local/bin/hermes\" /usr/local/bin/hermes ",
            "\"$HOME/.hermes/hermes-agent/venv/bin/hermes\"; do ",
            "if [ -x \"$p\" ]; then printf '%s' \"$p\"; exit 0; fi; done; exit 44"
        );
        exec(config, command, EXEC_TIMEOUT)
            .await
            .map_err(|error| map_missing_hermes(error, "auto-detected Hermes"))?
            .trim()
            .to_owned()
    };

    if hermes_path.is_empty() {
        return Err(SshFailure {
            kind: SshErrorKind::HermesNotFound,
            message: "Hermes is not installed on the remote host.".into(),
        });
    }
    validate_remote_path(&hermes_path).map_err(invalid_remote)?;
    let expanded = expand_unix_path(&hermes_path).map_err(invalid_remote)?;
    let version = exec(
        config,
        &format!("{expanded} --version 2>&1 | head -n 1"),
        EXEC_TIMEOUT,
    )
    .await
    .unwrap_or_default()
    .trim()
    .to_owned();

    let support = exec(
        config,
        &format!(
            "help=$({expanded} serve --help 2>&1); printf '%s' \"$help\" | grep -q ssh-session-token-file && printf '%s' \"$help\" | grep -q ssh-owner-nonce && echo YES || echo NO"
        ),
        EXEC_TIMEOUT,
    )
    .await?;
    if support.trim() != "YES" {
        return Err(SshFailure {
            kind: SshErrorKind::UpdateRequired,
            message: "The remote Hermes install does not support Desktop SSH ownership tokens. Update Hermes on the remote host before connecting.".into(),
        });
    }

    Ok(RemoteRuntime {
        os: os.to_owned(),
        arch: arch.to_owned(),
        hermes_path,
        hermes_version: version,
    })
}

fn map_missing_hermes(error: SshFailure, path: &str) -> SshFailure {
    if error.kind == SshErrorKind::Unknown {
        SshFailure {
            kind: SshErrorKind::HermesNotFound,
            message: format!("Hermes is not installed or executable on the remote host ({path})."),
        }
    } else {
        error
    }
}

async fn probe_windows_runtime(config: &SshConfig) -> Result<RemoteRuntime, SshFailure> {
    let explicit = config.remote_hermes_path.as_deref().unwrap_or_default();
    let script = format!(
        concat!(
            "$ErrorActionPreference='Stop';",
            "$explicit={explicit};",
            "$hermesHome=$env:HERMES_HOME;",
            "if(-not $hermesHome){{$hermesHome=Join-Path $env:LOCALAPPDATA 'hermes'}};",
            "$candidates=@();",
            "if($explicit){{$candidates+=$explicit}};",
            "$cmd=Get-Command hermes.exe -ErrorAction SilentlyContinue;",
            "if($cmd){{$candidates+=$cmd.Source}};",
            "$candidates+=(Join-Path $hermesHome 'hermes-agent\\venv\\Scripts\\hermes.exe');",
            "$candidates+=(Join-Path $HOME 'hermes-agent\\.venv\\Scripts\\hermes.exe');",
            "$hermes=$candidates|Where-Object{{Test-Path -LiteralPath $_ -PathType Leaf}}|Select-Object -First 1;",
            "if(-not $hermes){{throw 'Hermes is not installed on the remote Windows host.'}};",
            "if($explicit -and $hermes -ne $explicit){{throw 'The configured Hermes path is not executable.'}};",
            "$help=& $hermes serve --help 2>&1 | Out-String;",
            "if($help -notmatch 'ssh-session-token-file' -or $help -notmatch 'ssh-owner-nonce'){{throw 'UPDATE_REQUIRED'}};",
            "$version=(& $hermes --version 2>&1 | Select-Object -First 1 | Out-String).Trim();",
            "[ordered]@{{os='Windows';arch=$env:PROCESSOR_ARCHITECTURE;hermesPath=$hermes;version=$version}}|ConvertTo-Json -Compress"
        ),
        explicit = ps_literal(explicit),
    );
    let command = encoded_powershell(&script);
    let output = exec(config, &command, EXEC_TIMEOUT)
        .await
        .map_err(map_windows_probe_error)?;
    let value: Value = serde_json::from_str(output.trim()).map_err(|_| SshFailure {
        kind: SshErrorKind::UnsupportedPlatform,
        message: "The remote Windows probe returned an invalid response.".into(),
    })?;
    let hermes_path = value
        .get("hermesPath")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    if hermes_path.is_empty() {
        return Err(SshFailure {
            kind: SshErrorKind::HermesNotFound,
            message: "Hermes is not installed on the remote Windows host.".into(),
        });
    }
    Ok(RemoteRuntime {
        os: "Windows".into(),
        arch: value
            .get("arch")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        hermes_path,
        hermes_version: value
            .get("version")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
    })
}

fn map_windows_probe_error(mut error: SshFailure) -> SshFailure {
    if error.kind != SshErrorKind::Unknown {
        return error;
    }
    if error.message.contains("UPDATE_REQUIRED") {
        error.kind = SshErrorKind::UpdateRequired;
        error.message = "The remote Hermes install does not support Desktop SSH ownership tokens. Update Hermes on the remote host before connecting.".into();
    } else if error.message.to_ascii_lowercase().contains("hermes is not installed")
        || error
            .message
            .to_ascii_lowercase()
            .contains("configured hermes path")
    {
        error.kind = SshErrorKind::HermesNotFound;
        error.message = sanitize_remote_text(&error.message);
    } else {
        error.kind = SshErrorKind::UnsupportedPlatform;
        error.message = format!(
            "The remote operating system or Hermes installation is not supported by Desktop SSH. {}",
            sanitize_remote_text(&error.message)
        );
    }
    error
}

async fn exec(
    config: &SshConfig,
    remote_command: &str,
    timeout: Duration,
) -> Result<String, SshFailure> {
    let ssh = resolve_ssh_executable().map_err(|message| SshFailure {
        kind: SshErrorKind::Unreachable,
        message,
    })?;
    let args = ssh_args(config, remote_command);
    let mut command = Command::new(ssh);
    command
        .args(&args)
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = tokio::time::timeout(timeout, command.output())
        .await
        .map_err(|_| SshFailure {
            kind: SshErrorKind::Timeout,
            message: format!("SSH operation to {} timed out.", config.target()),
        })?
        .map_err(|error| SshFailure {
            kind: SshErrorKind::Unreachable,
            message: format!("Could not start OpenSSH: {error}"),
        })?;
    if output.stdout.len() > MAX_OUTPUT_BYTES || output.stderr.len() > MAX_OUTPUT_BYTES {
        return Err(SshFailure {
            kind: SshErrorKind::Unknown,
            message: "SSH probe returned an oversized response.".into(),
        });
    }
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    let stderr = sanitize_remote_text(&String::from_utf8_lossy(&output.stderr));
    let kind = classify_error(&stderr);
    Err(SshFailure {
        kind,
        message: error_message(kind, config, &stderr),
    })
}

fn ssh_args(config: &SshConfig, remote_command: &str) -> Vec<String> {
    let mut args = vec![
        "-o".into(),
        "BatchMode=yes".into(),
        "-o".into(),
        "StrictHostKeyChecking=accept-new".into(),
        "-o".into(),
        "ExitOnForwardFailure=yes".into(),
        "-o".into(),
        "ConnectTimeout=15".into(),
    ];
    if config.port != 22 {
        args.extend(["-p".into(), config.port.to_string()]);
    }
    if let Some(path) = &config.key_path {
        args.extend(["-i".into(), path.to_string_lossy().into_owned()]);
    }
    args.extend(["--".into(), config.target(), remote_command.to_owned()]);
    args
}

fn resolve_ssh_executable() -> Result<PathBuf, String> {
    if let Some(explicit) = env::var_os("HERMES_LOCAL_SSH") {
        let path = PathBuf::from(explicit);
        if path.is_absolute() && path.is_file() {
            return Ok(path);
        }
        return Err("HERMES_LOCAL_SSH must point to an absolute OpenSSH executable.".into());
    }
    if cfg!(windows) {
        let system_root =
            env::var_os("SystemRoot").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
        let native = system_root.join("System32/OpenSSH/ssh.exe");
        if native.is_file() {
            return Ok(native);
        }
        let where_exe = system_root.join("System32/where.exe");
        if let Ok(output) = std::process::Command::new(where_exe)
            .arg("ssh.exe")
            .output()
            && output.status.success()
            && let Some(path) = String::from_utf8_lossy(&output.stdout)
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(PathBuf::from)
                .find(|path| path.is_absolute() && path.is_file())
        {
            return Ok(path);
        }
        return Err("Windows OpenSSH client was not found.".into());
    }
    ["/usr/bin/ssh", "/usr/local/bin/ssh"]
        .into_iter()
        .map(PathBuf::from)
        .find(|path| path.is_file())
        .ok_or_else(|| "OpenSSH client was not found.".into())
}

fn validate_target_component(value: &str, field: &str) -> Result<(), String> {
    if value.is_empty() {
        return Err(format!("SSH {field} is required."));
    }
    if value.starts_with('-') || value.chars().any(char::is_control) {
        return Err(format!("Unsafe SSH {field}."));
    }
    if value.len() > 255 {
        return Err(format!("SSH {field} is too long."));
    }
    Ok(())
}

fn validate_key_path(path: &Path) -> Result<(), String> {
    let value = path.to_string_lossy();
    if value.is_empty() || value.starts_with('-') || value.chars().any(char::is_control) {
        return Err("Unsafe SSH key path.".into());
    }
    Ok(())
}

fn validate_remote_path(path: &str) -> Result<(), String> {
    if path.is_empty()
        || path
            .chars()
            .any(|character| matches!(character, '\0' | '\n' | '\r'))
        || !(path == "~"
            || path.starts_with("~/")
            || path.starts_with('/')
            || is_windows_absolute(path))
    {
        return Err("Remote Hermes path must be absolute or start with ~/.".into());
    }
    Ok(())
}

fn is_windows_absolute(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/')
}

fn expand_unix_path(path: &str) -> Result<String, String> {
    validate_remote_path(path)?;
    if path == "~" {
        return Ok("\"$HOME\"".into());
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return Ok(format!("\"$HOME\"/{}", shell_quote(rest)));
    }
    if path.starts_with('/') {
        return Ok(shell_quote(path));
    }
    Err("Remote path is not a Unix path.".into())
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn ps_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn encoded_powershell(script: &str) -> String {
    let bytes = script
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    format!(
        "powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand {}",
        STANDARD.encode(bytes)
    )
}

fn classify_error(stderr: &str) -> SshErrorKind {
    let lower = stderr.to_ascii_lowercase();
    if lower.contains("remote host identification has changed")
        || lower.contains("host key verification failed")
        || lower.contains("offending ed25519")
        || lower.contains("offending ecdsa")
        || lower.contains("offending rsa")
    {
        return SshErrorKind::HostKeyChanged;
    }
    if lower.contains("permission denied")
        || lower.contains("too many authentication failures")
        || lower.contains("publickey")
        || lower.contains("keyboard-interactive")
    {
        return SshErrorKind::AuthFailed;
    }
    if lower.contains("could not resolve hostname")
        || lower.contains("connection refused")
        || lower.contains("connection timed out")
        || lower.contains("no route to host")
        || lower.contains("network is unreachable")
        || lower.contains("operation timed out")
    {
        return SshErrorKind::Unreachable;
    }
    SshErrorKind::Unknown
}

fn error_message(kind: SshErrorKind, config: &SshConfig, stderr: &str) -> String {
    let target = config.target();
    match kind {
        SshErrorKind::HostKeyChanged => format!(
            "The host key for {target} changed. SSH refused the connection. Verify the host before updating known_hosts. {stderr}"
        ),
        SshErrorKind::AuthFailed => format!(
            "SSH authentication to {target} failed. Hermes Local uses BatchMode; load passphrase-protected or interactive credentials into ssh-agent first. {stderr}"
        ),
        SshErrorKind::Unreachable => {
            format!("Could not reach {target} over SSH. Check the host, port and network. {stderr}")
        }
        SshErrorKind::Timeout => format!("SSH operation to {target} timed out."),
        _ => format!("SSH error connecting to {target}: {stderr}"),
    }
}

fn is_transport_kind(kind: SshErrorKind) -> bool {
    matches!(
        kind,
        SshErrorKind::AuthFailed
            | SshErrorKind::HostKeyChanged
            | SshErrorKind::Timeout
            | SshErrorKind::Unreachable
    )
}

fn invalid_remote(message: String) -> SshFailure {
    SshFailure {
        kind: SshErrorKind::HermesNotFound,
        message,
    }
}

fn sanitize_remote_text(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_control() || matches!(character, '\n' | '\r' | '\t'))
        .take(2_048)
        .collect::<String>()
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> SshConfig {
        SshConfig::new(
            "example.test",
            Some("cloudy"),
            Some(2222),
            Some(r"C:\keys\id_ed25519"),
            None,
        )
        .expect("config")
    }

    #[test]
    fn rejects_option_and_control_character_injection() {
        assert!(SshConfig::new("-oProxyCommand=evil", None, None, None, None).is_err());
        assert!(SshConfig::new("host\nProxyCommand evil", None, None, None, None).is_err());
        assert!(SshConfig::new("host", Some("-F"), None, None, None).is_err());
        assert!(SshConfig::new("host", None, None, Some("-bad"), None).is_err());
        assert!(
            SshConfig::new("host", None, None, None, Some("relative/hermes")).is_err()
        );
    }

    #[test]
    fn ssh_argv_keeps_user_values_out_of_options() {
        let args = ssh_args(&config(), "exit 0");
        assert_eq!(args.last().map(String::as_str), Some("exit 0"));
        assert!(args.windows(2).any(|pair| pair == ["-p", "2222"]));
        assert!(
            args.windows(2)
                .any(|pair| pair[0] == "-i" && pair[1] == r"C:\keys\id_ed25519")
        );
        let separator = args.iter().position(|arg| arg == "--").expect("separator");
        assert_eq!(args[separator + 1], "cloudy@example.test");
    }

    #[test]
    fn classifies_actionable_transport_failures() {
        assert_eq!(
            classify_error("WARNING: REMOTE HOST IDENTIFICATION HAS CHANGED!"),
            SshErrorKind::HostKeyChanged
        );
        assert_eq!(
            classify_error("Permission denied (publickey)."),
            SshErrorKind::AuthFailed
        );
        assert_eq!(
            classify_error("ssh: connect to host x port 22: Connection refused"),
            SshErrorKind::Unreachable
        );
    }

    #[test]
    fn powershell_payload_is_encoded_not_shell_interpolated() {
        let command = encoded_powershell("Write-Output 'ok'");
        assert!(command.starts_with("powershell.exe -NoProfile -NonInteractive"));
        assert!(!command.contains("Write-Output"));
    }
}
