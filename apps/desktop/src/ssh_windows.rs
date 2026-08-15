#![cfg(windows)]

//! Windows-host SSH remote lifecycle for the native Dioxus Desktop client.
//!
//! This mirrors the OG Electron launcher's Windows ownership contract while
//! keeping all SSH/process/token authority in the native composition layer.
//! The Dioxus UI receives only the typed `ConnectionService` surface.

use std::{
    net::{Ipv4Addr, TcpListener},
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use hermes_core::{ServiceError, ServiceResult};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::{Child, Command},
    time::sleep,
};
use url::Url;
use uuid::Uuid;

use crate::{
    Engine as _,
    engine::general_purpose::STANDARD,
    ssh::{self, SshConfig},
};

const LOCKFILE_SCHEMA_VERSION: u32 = 2;
const PROTOCOL_VERSION: u32 = 1;
const READY_TIMEOUT: Duration = Duration::from_secs(45);
const READY_POLL_INTERVAL: Duration = Duration::from_millis(750);
const SSH_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_SSH_OUTPUT_BYTES: usize = 256 * 1024;
const MAX_TOKEN_BYTES: usize = 4_096;

#[derive(Debug)]
pub struct WindowsSshLease {
    base_url: String,
    token: String,
    forward: Child,
    pub remote_port: u16,
    pub local_port: u16,
    pub remote_pid: u32,
    pub reused: bool,
    pub remote_platform: String,
    pub remote_hermes_path: String,
    pub remote_hermes_version: String,
}

impl WindowsSshLease {
    pub fn websocket_url(&self) -> ServiceResult<String> {
        let mut url = Url::parse(&self.base_url).map_err(invalid)?;
        url.set_scheme("ws")
            .map_err(|()| ServiceError::Platform("could not build SSH WebSocket URL".into()))?;
        url.set_path("/api/ws");
        url.query_pairs_mut()
            .clear()
            .append_pair("token", &self.token);
        Ok(url.to_string())
    }
}

impl Drop for WindowsSshLease {
    fn drop(&mut self) {
        let _ = self.forward.start_kill();
    }
}

#[derive(Clone, Debug)]
struct WindowsRuntime {
    arch: String,
    hermes_home: String,
    hermes_path: String,
    python: String,
    hermes_version: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct WindowsLock {
    schema_version: u32,
    protocol_version: u32,
    ownership_id: String,
    spawn_nonce: String,
    pid: u32,
    creation_time_ns: String,
    port: u16,
    profile: String,
    hermes_path: String,
    hermes_home: String,
    token_fingerprint: String,
    started_at: String,
}

pub async fn connect(
    config: &SshConfig,
    profile_scope: &str,
    remote_profile: &str,
    data_dir: &Path,
) -> ServiceResult<WindowsSshLease> {
    validate_profile(profile_scope)?;
    validate_profile(remote_profile)?;
    let installation_id =
        load_or_create_installation_id(&data_dir.join("desktop-installation.json"))?;
    let ownership_id = ownership_id(&installation_id, profile_scope)?;
    let mut runtime = probe_windows_runtime(config).await?;

    let inspection = helper(
        config,
        &runtime,
        "inspect",
        &[runtime.hermes_path.clone()],
        None,
    )
    .await?;
    if inspection.get("supported").and_then(Value::as_bool) != Some(true) {
        return Err(ServiceError::Unavailable(
            "Update Hermes on the remote Windows host before connecting with Desktop SSH.".into(),
        ));
    }
    runtime.hermes_path = inspection
        .get("path")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ServiceError::Transport("Windows SSH inspection returned no Hermes path".into())
        })?
        .to_owned();
    runtime.hermes_version = inspection
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or(&runtime.hermes_version)
        .to_owned();

    let reuse_token = load_reuse_token(&ownership_id)?.unwrap_or_default();
    let lock_value = helper(
        config,
        &runtime,
        "read-lock",
        std::slice::from_ref(&ownership_id),
        None,
    )
    .await?;
    let parsed_lock = serde_json::from_value::<WindowsLock>(lock_value.clone()).ok();

    if let Some(lock) = parsed_lock.filter(|lock| valid_lock(lock, &ownership_id)) {
        let state = process_state(config, &runtime, &lock).await?;
        if state.indeterminate {
            return Err(ServiceError::Transport(
                "Could not determine the state of the existing remote Windows backend.".into(),
            ));
        }
        let reusable = state.alive
            && state.owned
            && lock.port > 0
            && lock.profile == remote_profile
            && !reuse_token.is_empty()
            && lock.token_fingerprint == fingerprint_token(&reuse_token)
            && lock.hermes_path == runtime.hermes_path
            && lock.hermes_home == runtime.hermes_home;

        if reusable {
            let mut forward = open_forward(config, lock.port).await?;
            let base_url = format!("http://127.0.0.1:{}", forward.local_port);
            match probe_reuse_proof(&base_url, &reuse_token, &lock.spawn_nonce).await {
                Ok(true) => {
                    return Ok(WindowsSshLease {
                        base_url,
                        token: reuse_token,
                        remote_port: lock.port,
                        local_port: forward.local_port,
                        remote_pid: lock.pid,
                        reused: true,
                        remote_platform: format!("Windows/{}", runtime.arch),
                        remote_hermes_path: runtime.hermes_path,
                        remote_hermes_version: runtime.hermes_version,
                        forward: forward.child,
                    });
                }
                Ok(false) => {
                    let _ = forward.child.start_kill();
                    cleanup_owned(config, &runtime, &ownership_id, Some(&lock)).await?;
                }
                Err(error) => {
                    let _ = forward.child.start_kill();
                    return Err(error);
                }
            }
        } else {
            cleanup_owned(config, &runtime, &ownership_id, Some(&lock)).await?;
        }
    } else if !lock_value.is_null() {
        helper(
            config,
            &runtime,
            "remove-lock",
            std::slice::from_ref(&ownership_id),
            None,
        )
        .await?;
    }

    spawn_owned(config, &runtime, &ownership_id, remote_profile).await
}

async fn spawn_owned(
    config: &SshConfig,
    runtime: &WindowsRuntime,
    ownership_id: &str,
    remote_profile: &str,
) -> ServiceResult<WindowsSshLease> {
    let token = mint_token();
    let spawn_nonce = random_hex(8);
    helper(
        config,
        runtime,
        "upload-token",
        &[ownership_id.to_owned(), spawn_nonce.clone()],
        Some(token.as_bytes()),
    )
    .await?;

    let spawn_payload = serde_json::to_vec(&serde_json::json!({
        "ownershipId": ownership_id,
        "spawnNonce": spawn_nonce,
        "profile": remote_profile,
        "hermesPath": runtime.hermes_path,
    }))
    .map_err(platform)?;
    let spawned = match helper(config, runtime, "spawn", &[], Some(&spawn_payload)).await {
        Ok(value) => value,
        Err(error) => {
            let _ = helper(
                config,
                runtime,
                "remove-token",
                &[ownership_id.to_owned(), spawn_nonce.clone()],
                None,
            )
            .await;
            return Err(error);
        }
    };

    let pid = spawned
        .get("pid")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| *value > 0)
        .ok_or_else(|| {
            ServiceError::Transport("Windows SSH runtime returned no valid pid".into())
        })?;
    let creation_time_ns = spawned
        .get("creationTimeNs")
        .and_then(Value::as_str)
        .filter(|value| valid_creation_time(value))
        .ok_or_else(|| {
            ServiceError::Transport("Windows SSH runtime returned no valid creation time".into())
        })?
        .to_owned();

    let mut lock = WindowsLock {
        schema_version: LOCKFILE_SCHEMA_VERSION,
        protocol_version: PROTOCOL_VERSION,
        ownership_id: ownership_id.to_owned(),
        spawn_nonce,
        pid,
        creation_time_ns,
        port: 0,
        profile: remote_profile.to_owned(),
        hermes_path: runtime.hermes_path.clone(),
        hermes_home: runtime.hermes_home.clone(),
        token_fingerprint: fingerprint_token(&token),
        started_at: unix_timestamp_string(),
    };

    if let Err(error) = write_lock(config, runtime, ownership_id, &lock).await {
        let _ = cleanup_owned(config, runtime, ownership_id, Some(&lock)).await;
        return Err(error);
    }
    let remote_port = match wait_ready(config, runtime, ownership_id, &lock).await {
        Ok(port) => port,
        Err(error) => {
            let _ = cleanup_owned(config, runtime, ownership_id, Some(&lock)).await;
            return Err(error);
        }
    };
    let mut forward = match open_forward(config, remote_port).await {
        Ok(forward) => forward,
        Err(error) => {
            let _ = cleanup_owned(config, runtime, ownership_id, Some(&lock)).await;
            return Err(error);
        }
    };
    let base_url = format!("http://127.0.0.1:{}", forward.local_port);
    if let Err(error) = wait_for_dashboard(&base_url, &token).await {
        let _ = forward.child.start_kill();
        let _ = cleanup_owned(config, runtime, ownership_id, Some(&lock)).await;
        return Err(error);
    }

    lock.port = remote_port;
    if let Err(error) = write_lock(config, runtime, ownership_id, &lock).await {
        let _ = forward.child.start_kill();
        let _ = cleanup_owned(config, runtime, ownership_id, Some(&lock)).await;
        return Err(error);
    }
    if let Err(error) = store_reuse_token(ownership_id, &token) {
        let _ = forward.child.start_kill();
        let _ = cleanup_owned(config, runtime, ownership_id, Some(&lock)).await;
        return Err(error);
    }

    Ok(WindowsSshLease {
        base_url,
        token,
        forward: forward.child,
        remote_port,
        local_port: forward.local_port,
        remote_pid: pid,
        reused: false,
        remote_platform: format!("Windows/{}", runtime.arch),
        remote_hermes_path: runtime.hermes_path.clone(),
        remote_hermes_version: runtime.hermes_version.clone(),
    })
}

async fn probe_windows_runtime(config: &SshConfig) -> ServiceResult<WindowsRuntime> {
    let result = ssh::test_connection(config).await;
    if result.reachable != Some(true) || result.ok != Some(true) {
        return Err(ServiceError::Transport(
            result
                .error
                .unwrap_or_else(|| "SSH remote probe failed".into()),
        ));
    }
    let platform = result.remote_platform.ok_or_else(|| {
        ServiceError::Transport("SSH probe did not report a remote platform".into())
    })?;
    let (os, arch) = platform.split_once('/').unwrap_or((&platform, ""));
    if os != "Windows" {
        return Err(ServiceError::InvalidInput(format!(
            "Windows SSH lifecycle was asked to manage {os}"
        )));
    }
    let hermes_path = result
        .remote_hermes_path
        .filter(|path| !path.is_empty())
        .ok_or_else(|| ServiceError::Transport("SSH probe did not resolve Hermes".into()))?;
    let script = format!(
        concat!(
            "$ErrorActionPreference='Stop';",
            "$hermes={hermes};",
            "$hermesHome=$env:HERMES_HOME;",
            "if(-not $hermesHome){{$hermesHome=Join-Path $env:LOCALAPPDATA 'hermes'}};",
            "if(-not (Test-Path -LiteralPath $hermes -PathType Leaf)){{throw 'Hermes is not installed on the remote Windows host.'}};",
            "$python=Join-Path (Split-Path $hermes) 'python.exe';",
            "if(-not (Test-Path -LiteralPath $python -PathType Leaf)){{throw 'The remote Hermes Python runtime was not found.'}};",
            "[ordered]@{{arch=$env:PROCESSOR_ARCHITECTURE;hermesHome=$hermesHome;hermesPath=$hermes;python=$python}}|ConvertTo-Json -Compress"
        ),
        hermes = ps_literal(&hermes_path),
    );
    let output = run_ssh(config, &encoded_powershell(&script), None).await?;
    let value: Value = serde_json::from_str(output.trim()).map_err(platform_error)?;
    let hermes_home = value
        .get("hermesHome")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let resolved_path = value
        .get("hermesPath")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let python = value
        .get("python")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    if hermes_home.is_empty() || resolved_path.is_empty() || python.is_empty() {
        return Err(ServiceError::Transport(
            "Windows SSH runtime probe returned incomplete paths".into(),
        ));
    }
    Ok(WindowsRuntime {
        arch: value
            .get("arch")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .unwrap_or(arch)
            .to_owned(),
        hermes_home,
        hermes_path: resolved_path,
        python,
        hermes_version: result.remote_hermes_version.unwrap_or_default(),
    })
}

async fn helper(
    config: &SshConfig,
    runtime: &WindowsRuntime,
    operation: &str,
    args: &[String],
    stdin: Option<&[u8]>,
) -> ServiceResult<Value> {
    const OPERATIONS: &[&str] = &[
        "inspect",
        "process-state",
        "read-lock",
        "write-lock",
        "remove-lock",
        "upload-token",
        "remove-token",
        "remove-log",
        "read-log",
        "spawn",
        "terminate",
    ];
    if !OPERATIONS.contains(&operation) {
        return Err(ServiceError::InvalidInput(
            "unsupported Windows SSH helper operation".into(),
        ));
    }
    let mut helper_argv = vec![
        runtime.python.clone(),
        "-m".into(),
        "hermes_cli.windows_ssh_runtime".into(),
        operation.to_owned(),
    ];
    helper_argv.extend(args.iter().cloned());
    let invocation = helper_argv
        .iter()
        .map(|value| ps_literal(value))
        .collect::<Vec<_>>()
        .join(" ");
    let script = format!(
        "$ErrorActionPreference='Stop';& {invocation};if($LASTEXITCODE -ne 0){{exit $LASTEXITCODE}}"
    );
    let output = run_ssh(config, &encoded_powershell(&script), stdin).await?;
    let line = output
        .trim_start_matches('\u{feff}')
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .unwrap_or("null");
    let value: Value = serde_json::from_str(line).map_err(|error| {
        ServiceError::Transport(format!("Windows SSH helper returned invalid JSON: {error}"))
    })?;
    if let Some(error) = value
        .get("error")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        return Err(ServiceError::Transport(sanitize_remote_text(error)));
    }
    Ok(value)
}

#[derive(Clone, Copy, Debug)]
struct ProcessState {
    alive: bool,
    owned: bool,
    indeterminate: bool,
}

async fn process_state(
    config: &SshConfig,
    runtime: &WindowsRuntime,
    lock: &WindowsLock,
) -> ServiceResult<ProcessState> {
    let value = helper(
        config,
        runtime,
        "process-state",
        &[
            lock.pid.to_string(),
            lock.creation_time_ns.clone(),
            lock.hermes_path.clone(),
            lock.spawn_nonce.clone(),
        ],
        None,
    )
    .await?;
    Ok(ProcessState {
        alive: value.get("alive").and_then(Value::as_bool).unwrap_or(false),
        owned: value.get("owned").and_then(Value::as_bool).unwrap_or(false),
        indeterminate: value
            .get("indeterminate")
            .and_then(Value::as_bool)
            .unwrap_or(true),
    })
}

async fn cleanup_owned(
    config: &SshConfig,
    runtime: &WindowsRuntime,
    ownership_id: &str,
    lock: Option<&WindowsLock>,
) -> ServiceResult<()> {
    if let Some(lock) = lock {
        let state = process_state(config, runtime, lock).await?;
        if state.indeterminate {
            return Err(ServiceError::Transport(
                "Refusing to clean up an indeterminate remote Windows backend.".into(),
            ));
        }
        if state.alive && state.owned {
            helper(
                config,
                runtime,
                "terminate",
                &[
                    lock.pid.to_string(),
                    lock.creation_time_ns.clone(),
                    lock.hermes_path.clone(),
                    lock.spawn_nonce.clone(),
                ],
                None,
            )
            .await?;
        }
        if !lock.spawn_nonce.is_empty() {
            let _ = helper(
                config,
                runtime,
                "remove-token",
                &[ownership_id.to_owned(), lock.spawn_nonce.clone()],
                None,
            )
            .await;
            let _ = helper(
                config,
                runtime,
                "remove-log",
                &[ownership_id.to_owned(), lock.spawn_nonce.clone()],
                None,
            )
            .await;
        }
    }
    let _ = helper(
        config,
        runtime,
        "remove-lock",
        &[ownership_id.to_owned()],
        None,
    )
    .await;
    Ok(())
}

async fn write_lock(
    config: &SshConfig,
    runtime: &WindowsRuntime,
    ownership_id: &str,
    lock: &WindowsLock,
) -> ServiceResult<()> {
    if !valid_lock(lock, ownership_id) {
        return Err(ServiceError::InvalidInput(
            "refusing to write an invalid Windows SSH ownership lock".into(),
        ));
    }
    let encoded = serde_json::to_vec(lock).map_err(platform)?;
    helper(
        config,
        runtime,
        "write-lock",
        &[ownership_id.to_owned()],
        Some(&encoded),
    )
    .await
    .map(|_| ())
}

async fn wait_ready(
    config: &SshConfig,
    runtime: &WindowsRuntime,
    ownership_id: &str,
    lock: &WindowsLock,
) -> ServiceResult<u16> {
    let deadline = tokio::time::Instant::now() + READY_TIMEOUT;
    while tokio::time::Instant::now() < deadline {
        match process_state(config, runtime, lock).await {
            Ok(state) if !state.indeterminate && (!state.alive || !state.owned) => {
                let detail = helper(
                    config,
                    runtime,
                    "read-log",
                    &[ownership_id.to_owned(), lock.spawn_nonce.clone()],
                    None,
                )
                .await
                .ok()
                .and_then(|value| {
                    value
                        .get("content")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })
                .unwrap_or_default();
                return Err(ServiceError::Transport(format!(
                    "Remote Windows backend exited before announcing its port. {}",
                    tail_sanitized(&detail, 2_000)
                )));
            }
            Ok(_) | Err(_) => {}
        }
        let content = helper(
            config,
            runtime,
            "read-log",
            &[ownership_id.to_owned(), lock.spawn_nonce.clone()],
            None,
        )
        .await
        .ok()
        .and_then(|value| {
            value
                .get("content")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_default();
        if let Some(port) = parse_ready_port(&content) {
            return Ok(port);
        }
        sleep(READY_POLL_INTERVAL).await;
    }
    Err(ServiceError::Transport(format!(
        "Timed out waiting for the remote Windows backend ({}ms).",
        READY_TIMEOUT.as_millis()
    )))
}

fn valid_lock(lock: &WindowsLock, ownership_id: &str) -> bool {
    lock.schema_version == LOCKFILE_SCHEMA_VERSION
        && lock.protocol_version == PROTOCOL_VERSION
        && lock.ownership_id == ownership_id
        && is_lower_hex(&lock.spawn_nonce, 16)
        && lock.pid > 0
        && valid_creation_time(&lock.creation_time_ns)
        && is_lower_hex(&lock.token_fingerprint, 32)
        && lock.profile.len() <= 1_024
        && !lock.hermes_path.is_empty()
        && lock.hermes_path.len() <= 1_024
        && !lock.hermes_home.is_empty()
        && lock.hermes_home.len() <= 1_024
        && lock.started_at.len() <= 1_024
}

fn valid_creation_time(value: &str) -> bool {
    (10..=20).contains(&value.len()) && value.bytes().all(|byte| byte.is_ascii_digit())
}

struct ForwardProcess {
    child: Child,
    local_port: u16,
}

async fn open_forward(config: &SshConfig, remote_port: u16) -> ServiceResult<ForwardProcess> {
    let mut last_error = None;
    for _ in 0..3 {
        let local_port = pick_local_port()?;
        match spawn_forward(config, local_port, remote_port).await {
            Ok(child) => return Ok(ForwardProcess { child, local_port }),
            Err(error) if is_bind_collision(&error) => last_error = Some(error),
            Err(error) => return Err(error),
        }
    }
    Err(last_error.unwrap_or_else(|| ServiceError::Transport("could not open SSH forward".into())))
}

async fn spawn_forward(
    config: &SshConfig,
    local_port: u16,
    remote_port: u16,
) -> ServiceResult<Child> {
    let executable = resolve_ssh_executable()?;
    let mut args = common_ssh_args(config);
    args.extend([
        "-N".into(),
        "-L".into(),
        format!("127.0.0.1:{local_port}:127.0.0.1:{remote_port}"),
        "--".into(),
        ssh_target(config),
    ]);
    let mut child = Command::new(executable)
        .args(args)
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(platform)?;
    sleep(Duration::from_millis(250)).await;
    if child.try_wait().map_err(platform)?.is_none() {
        return Ok(child);
    }
    let mut stderr = String::new();
    if let Some(mut pipe) = child.stderr.take() {
        let _ = pipe.read_to_string(&mut stderr).await;
    }
    Err(ServiceError::Transport(format!(
        "SSH port forward failed: {}",
        sanitize_remote_text(&stderr)
    )))
}

fn pick_local_port() -> ServiceResult<u16> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(platform)?;
    listener
        .local_addr()
        .map(|address| address.port())
        .map_err(platform)
}

async fn wait_for_dashboard(base_url: &str, token: &str) -> ServiceResult<()> {
    let client = loopback_client()?;
    let deadline = tokio::time::Instant::now() + READY_TIMEOUT;
    while tokio::time::Instant::now() < deadline {
        match client
            .get(format!("{base_url}/api/status"))
            .header("X-Hermes-Session-Token", token)
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => return Ok(()),
            Ok(response)
                if matches!(
                    response.status(),
                    StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
                ) =>
            {
                return Err(ServiceError::PermissionDenied(
                    "remote Hermes dashboard rejected its spawn session token".into(),
                ));
            }
            _ => sleep(Duration::from_millis(250)).await,
        }
    }
    Err(ServiceError::Transport(
        "timed out waiting for the tunneled remote Hermes dashboard".into(),
    ))
}

async fn probe_reuse_proof(base_url: &str, token: &str, spawn_nonce: &str) -> ServiceResult<bool> {
    validate_spawn_nonce(spawn_nonce)?;
    let response = loopback_client()?
        .get(format!("{base_url}/api/ssh/ownership"))
        .header("X-Hermes-Session-Token", token)
        .send()
        .await
        .map_err(transport)?;
    if matches!(
        response.status(),
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN | StatusCode::NOT_FOUND
    ) {
        return Ok(false);
    }
    if !response.status().is_success() {
        return Err(ServiceError::Transport(format!(
            "SSH reuse proof returned HTTP {}",
            response.status()
        )));
    }
    let proof: Value = response.json().await.map_err(transport)?;
    Ok(proof.get("ok").and_then(Value::as_bool) == Some(true)
        && proof.get("sshOwnerNonce").and_then(Value::as_str) == Some(spawn_nonce)
        && proof.get("protocolVersion").and_then(Value::as_u64)
            == Some(u64::from(PROTOCOL_VERSION)))
}

fn loopback_client() -> ServiceResult<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(platform)
}

async fn run_ssh(
    config: &SshConfig,
    remote_command: &str,
    stdin: Option<&[u8]>,
) -> ServiceResult<String> {
    let executable = resolve_ssh_executable()?;
    let mut args = common_ssh_args(config);
    args.extend(["--".into(), ssh_target(config), remote_command.to_owned()]);
    let mut child = Command::new(executable)
        .args(args)
        .kill_on_drop(true)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(platform)?;
    if let Some(bytes) = stdin {
        let mut pipe = child
            .stdin
            .take()
            .ok_or_else(|| ServiceError::Platform("OpenSSH stdin was unavailable".into()))?;
        pipe.write_all(bytes).await.map_err(platform)?;
        drop(pipe);
    }
    let output = tokio::time::timeout(SSH_TIMEOUT, child.wait_with_output())
        .await
        .map_err(|_| {
            ServiceError::Timeout(format!("SSH operation to {} timed out", ssh_target(config)))
        })?
        .map_err(platform)?;
    if output.stdout.len() > MAX_SSH_OUTPUT_BYTES || output.stderr.len() > MAX_SSH_OUTPUT_BYTES {
        return Err(ServiceError::Transport(
            "SSH operation returned an oversized response".into(),
        ));
    }
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    let stderr = sanitize_remote_text(&String::from_utf8_lossy(&output.stderr));
    Err(ServiceError::Transport(format!(
        "SSH operation to {} failed: {stderr}",
        ssh_target(config)
    )))
}

fn common_ssh_args(config: &SshConfig) -> Vec<String> {
    let mut args = vec![
        "-o".into(),
        "BatchMode=yes".into(),
        "-o".into(),
        "StrictHostKeyChecking=accept-new".into(),
        "-o".into(),
        "ExitOnForwardFailure=yes".into(),
        "-o".into(),
        "ConnectTimeout=15".into(),
        "-o".into(),
        "ServerAliveInterval=15".into(),
        "-o".into(),
        "ServerAliveCountMax=2".into(),
    ];
    if config.port != 22 {
        args.extend(["-p".into(), config.port.to_string()]);
    }
    if let Some(path) = &config.key_path {
        args.extend(["-i".into(), path.to_string_lossy().into_owned()]);
    }
    args
}

fn ssh_target(config: &SshConfig) -> String {
    config.user.as_ref().map_or_else(
        || config.host.clone(),
        |user| format!("{user}@{}", config.host),
    )
}

fn resolve_ssh_executable() -> ServiceResult<PathBuf> {
    if let Some(explicit) = std::env::var_os("HERMES_LOCAL_SSH") {
        let path = PathBuf::from(explicit);
        if path.is_absolute() && path.is_file() {
            return Ok(path);
        }
        return Err(ServiceError::InvalidInput(
            "HERMES_LOCAL_SSH must point to an absolute OpenSSH executable".into(),
        ));
    }
    let system_root =
        std::env::var_os("SystemRoot").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
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
    Err(ServiceError::Unavailable(
        "Windows OpenSSH client was not found".into(),
    ))
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

fn parse_ready_port(log: &str) -> Option<u16> {
    log.lines()
        .filter_map(|line| {
            let line = line.trim();
            let value = line
                .strip_prefix("HERMES_BACKEND_READY port=")
                .or_else(|| line.strip_prefix("HERMES_DASHBOARD_READY port="))?;
            value.trim().parse::<u16>().ok().filter(|port| *port > 0)
        })
        .next_back()
}

fn is_bind_collision(error: &ServiceError) -> bool {
    let text = error.to_string().to_ascii_lowercase();
    text.contains("address already in use")
        || text.contains("cannot listen to port")
        || (text.contains("bind") && text.contains("failed"))
}

fn load_reuse_token(ownership_id: &str) -> ServiceResult<Option<String>> {
    validate_ownership_id(ownership_id)?;
    let entry = keyring::Entry::new("Hermes Local SSH", ownership_id).map_err(platform)?;
    match entry.get_password() {
        Ok(token) => Ok(Some(token)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(platform(error)),
    }
}

fn store_reuse_token(ownership_id: &str, token: &str) -> ServiceResult<()> {
    validate_ownership_id(ownership_id)?;
    if token.is_empty() || token.len() > MAX_TOKEN_BYTES {
        return Err(ServiceError::InvalidInput("invalid SSH reuse token".into()));
    }
    keyring::Entry::new("Hermes Local SSH", ownership_id)
        .map_err(platform)?
        .set_password(token)
        .map_err(platform)
}

fn load_or_create_installation_id(path: &Path) -> ServiceResult<String> {
    if let Some(id) = read_installation_id(path)? {
        return Ok(id);
    }
    let parent = path
        .parent()
        .ok_or_else(|| ServiceError::Platform("installation id path has no parent".into()))?;
    std::fs::create_dir_all(parent).map_err(platform)?;
    let id = Uuid::new_v4().hyphenated().to_string().to_ascii_lowercase();
    let temporary = path.with_extension(format!("json.{}.tmp", random_hex(8)));
    std::fs::write(
        &temporary,
        serde_json::to_vec(&serde_json::json!({ "installationId": id })).map_err(platform)?,
    )
    .map_err(platform)?;
    match std::fs::rename(&temporary, path) {
        Ok(()) => Ok(id),
        Err(error) => {
            let _ = std::fs::remove_file(&temporary);
            if let Some(winner) = read_installation_id(path)? {
                Ok(winner)
            } else {
                Err(platform(error))
            }
        }
    }
}

fn read_installation_id(path: &Path) -> ServiceResult<Option<String>> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(platform(error)),
    };
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Ok(None);
    }
    let value: Value =
        serde_json::from_slice(&std::fs::read(path).map_err(platform)?).map_err(platform_error)?;
    let Some(id) = value.get("installationId").and_then(Value::as_str) else {
        return Ok(None);
    };
    let parsed = match Uuid::parse_str(id) {
        Ok(parsed) if parsed.get_version_num() == 4 => parsed,
        _ => return Ok(None),
    };
    Ok(Some(parsed.hyphenated().to_string().to_ascii_lowercase()))
}

fn ownership_id(installation_id: &str, scope: &str) -> ServiceResult<String> {
    let parsed = Uuid::parse_str(installation_id).map_err(invalid)?;
    if parsed.get_version_num() != 4 {
        return Err(ServiceError::InvalidInput(
            "desktop installation id is not a UUIDv4".into(),
        ));
    }
    let digest =
        Sha256::digest(format!("{}\0{scope}", installation_id.to_ascii_lowercase()).as_bytes());
    Ok(hex_prefix(&digest, 16))
}

fn fingerprint_token(token: &str) -> String {
    hex_prefix(&Sha256::digest(token.as_bytes()), 16)
}

fn hex_prefix(bytes: &[u8], count: usize) -> String {
    bytes[..count]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn mint_token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

fn random_hex(bytes: usize) -> String {
    let mut value = String::new();
    while value.len() < bytes * 2 {
        value.push_str(&Uuid::new_v4().simple().to_string());
    }
    value.truncate(bytes * 2);
    value
}

fn validate_profile(value: &str) -> ServiceResult<()> {
    if value.len() > 256 || value.chars().any(char::is_control) {
        return Err(ServiceError::InvalidInput(
            "invalid SSH profile name".into(),
        ));
    }
    Ok(())
}

fn validate_ownership_id(value: &str) -> ServiceResult<()> {
    if !is_lower_hex(value, 32) {
        return Err(ServiceError::InvalidInput(
            "invalid SSH ownership id".into(),
        ));
    }
    Ok(())
}

fn validate_spawn_nonce(value: &str) -> ServiceResult<()> {
    if !is_lower_hex(value, 16) {
        return Err(ServiceError::InvalidInput("invalid SSH spawn nonce".into()));
    }
    Ok(())
}

fn is_lower_hex(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
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

fn tail_sanitized(value: &str, chars: usize) -> String {
    let sanitized = sanitize_remote_text(value);
    let mut tail = sanitized.chars().rev().take(chars).collect::<Vec<_>>();
    tail.reverse();
    tail.into_iter().collect()
}

fn unix_timestamp_string() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or_else(|_| "0".into(), |duration| duration.as_secs().to_string())
}

fn invalid(error: impl std::fmt::Display) -> ServiceError {
    ServiceError::InvalidInput(error.to_string())
}

fn platform(error: impl std::fmt::Display) -> ServiceError {
    ServiceError::Platform(error.to_string())
}

fn platform_error(error: impl std::fmt::Display) -> ServiceError {
    ServiceError::Platform(error.to_string())
}

fn transport(error: impl std::fmt::Display) -> ServiceError {
    ServiceError::Transport(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ownership_identity_is_stable_and_profile_scoped() {
        let installation = "123e4567-e89b-42d3-a456-426614174000";
        let global = ownership_id(installation, "").expect("global");
        let project = ownership_id(installation, "project").expect("project");
        assert!(is_lower_hex(&global, 32));
        assert!(is_lower_hex(&project, 32));
        assert_ne!(global, project);
        assert_eq!(global, ownership_id(installation, "").expect("stable"));
    }

    #[test]
    fn windows_lock_validation_binds_process_identity() {
        let ownership = "0123456789abcdef0123456789abcdef";
        let lock = WindowsLock {
            schema_version: LOCKFILE_SCHEMA_VERSION,
            protocol_version: PROTOCOL_VERSION,
            ownership_id: ownership.into(),
            spawn_nonce: "0123456789abcdef".into(),
            pid: 10,
            creation_time_ns: "1784219690452757504".into(),
            port: 1234,
            profile: "default".into(),
            hermes_path: r"C:\h\hermes.exe".into(),
            hermes_home: r"C:\h".into(),
            token_fingerprint: fingerprint_token("stored-token"),
            started_at: "1".into(),
        };
        assert!(valid_lock(&lock, ownership));
        let mut wrong = lock.clone();
        wrong.creation_time_ns = "0".into();
        assert!(!valid_lock(&wrong, ownership));
        let mut spawn_in_progress = lock;
        spawn_in_progress.port = 0;
        assert!(valid_lock(&spawn_in_progress, ownership));
    }

    #[test]
    fn encoded_powershell_does_not_expose_raw_script() {
        let command = encoded_powershell("Write-Output 'secret'");
        assert!(command.starts_with("powershell.exe -NoProfile -NonInteractive"));
        assert!(!command.contains("Write-Output"));
        assert!(!command.contains("secret"));
    }

    #[test]
    fn readiness_parser_uses_only_owned_dashboard_markers() {
        assert_eq!(
            parse_ready_port("HERMES_DASHBOARD_READY port=9119"),
            Some(9119)
        );
        assert_eq!(
            parse_ready_port("HERMES_BACKEND_READY port=1234"),
            Some(1234)
        );
        assert_eq!(parse_ready_port("READY port=9119"), None);
        assert_eq!(parse_ready_port("HERMES_DASHBOARD_READY port=0"), None);
    }
}
