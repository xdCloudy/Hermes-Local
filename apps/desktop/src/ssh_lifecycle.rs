//! Owned SSH remote-dashboard lifecycle for the native Dioxus client.
//!
//! The renderer never receives SSH process authority or the dashboard session
//! token. This module keeps those capabilities in the Desktop composition
//! layer, mirrors the OG launcher's ownership invariants, and returns only a
//! lease that the typed native connection adapter can use to open the Agent
//! WebSocket.

use std::{
    fs::{self, OpenOptions},
    io::Write as _,
    net::{Ipv4Addr, TcpListener},
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use hermes_core::{ServiceError, ServiceResult};
use hermes_protocol::SshErrorKind;
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

use crate::ssh::{self, SshConfig};

const LOCKFILE_SCHEMA_VERSION: u32 = 2;
const PROTOCOL_VERSION: u32 = 1;
const READY_TIMEOUT: Duration = Duration::from_secs(45);
const READY_POLL_INTERVAL: Duration = Duration::from_millis(750);
const SSH_TIMEOUT: Duration = Duration::from_secs(20);
const REMOTE_LOCK_DIR: &str = "~/.hermes/desktop-ssh";
const MAX_SSH_OUTPUT_BYTES: usize = 256 * 1024;

#[derive(Debug)]
pub struct SshLease {
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

impl SshLease {
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

impl Drop for SshLease {
    fn drop(&mut self) {
        let _ = self.forward.start_kill();
    }
}

#[derive(Clone, Debug)]
pub struct SshLifecycleConfig {
    pub ssh: SshConfig,
    pub profile_scope: String,
    pub remote_profile: String,
    pub data_dir: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct UnixLock {
    schema_version: u32,
    protocol_version: u32,
    ownership_id: String,
    spawn_nonce: String,
    pid: u32,
    port: u16,
    profile: String,
    hermes_path: String,
    hermes_home: String,
    log_path: String,
    token_fingerprint: String,
    started_at: String,
}

#[derive(Clone, Debug)]
struct RuntimeProbe {
    os: String,
    arch: String,
    hermes_path: String,
    hermes_version: String,
}

#[derive(Clone, Debug)]
struct SpawnedUnix {
    pid: u32,
    spawn_nonce: String,
    log_path: String,
    token_file_path: String,
}

#[derive(Debug)]
struct SshExecError {
    kind: SshErrorKind,
    message: String,
}

impl SshExecError {
    fn into_service(self) -> ServiceError {
        match self.kind {
            SshErrorKind::AuthFailed | SshErrorKind::HostKeyChanged => {
                ServiceError::PermissionDenied(self.message)
            }
            _ => ServiceError::Transport(self.message),
        }
    }
}

pub async fn connect(config: &SshLifecycleConfig) -> ServiceResult<SshLease> {
    validate_profile(&config.profile_scope)?;
    validate_profile(&config.remote_profile)?;
    let installation_path = config.data_dir.join("desktop-installation.json");
    let installation_id = load_or_create_installation_id(&installation_path)?;
    let ownership_id = ownership_id(&installation_id, &config.profile_scope)?;
    let runtime = runtime_probe(&config.ssh).await?;

    match runtime.os.as_str() {
        "Linux" | "Darwin" => connect_unix(config, &runtime, &ownership_id).await,
        "Windows" => Err(ServiceError::Unavailable(
            "Windows-host SSH ownership lifecycle is the next migration slice; connection testing is available, but apply is not enabled yet.".into(),
        )),
        _ => Err(ServiceError::Unavailable(format!(
            "unsupported SSH remote platform: {}",
            runtime.os
        ))),
    }
}

async fn runtime_probe(config: &SshConfig) -> ServiceResult<RuntimeProbe> {
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
    let hermes_path = result
        .remote_hermes_path
        .filter(|path| !path.is_empty())
        .ok_or_else(|| ServiceError::Transport("SSH probe did not resolve Hermes".into()))?;
    Ok(RuntimeProbe {
        os: os.to_owned(),
        arch: arch.to_owned(),
        hermes_path,
        hermes_version: result.remote_hermes_version.unwrap_or_default(),
    })
}

async fn connect_unix(
    config: &SshLifecycleConfig,
    runtime: &RuntimeProbe,
    ownership_id: &str,
) -> ServiceResult<SshLease> {
    let hermes_home = run_ssh(
        &config.ssh,
        "printf '%s' \"${HERMES_HOME:-$HOME/.hermes}\"",
        None,
    )
    .await?
    .trim()
    .to_owned();
    validate_unix_remote_path(&hermes_home)?;
    let reuse_token = load_reuse_token(ownership_id).unwrap_or_default();

    if let Some(lock) = read_unix_lock(&config.ssh, ownership_id).await? {
        let alive = remote_pid_alive(&config.ssh, lock.pid).await?;
        let owned = alive
            && pid_is_our_dashboard(&config.ssh, lock.pid, &lock.spawn_nonce, &lock.hermes_path)
                .await?;
        let reusable = alive
            && owned
            && lock.port > 0
            && lock.profile == config.remote_profile
            && !reuse_token.is_empty()
            && lock.token_fingerprint == fingerprint_token(&reuse_token)
            && lock.hermes_path == runtime.hermes_path
            && lock.hermes_home == hermes_home;

        if reusable {
            let mut forward = open_forward(&config.ssh, lock.port).await?;
            let local_port = forward.local_port;
            let base_url = format!("http://127.0.0.1:{local_port}");
            match probe_reuse_proof(&base_url, &reuse_token, &lock.spawn_nonce).await {
                Ok(true) => {
                    let token =
                        adopt_served_token(&config.ssh, &base_url, &reuse_token, lock.pid).await?;
                    store_reuse_token(ownership_id, &token)?;
                    return Ok(SshLease {
                        base_url,
                        token,
                        forward: forward.child,
                        remote_port: lock.port,
                        local_port,
                        remote_pid: lock.pid,
                        reused: true,
                        remote_platform: format!("{}/{}", runtime.os, runtime.arch),
                        remote_hermes_path: runtime.hermes_path.clone(),
                        remote_hermes_version: runtime.hermes_version.clone(),
                    });
                }
                Ok(false) => {
                    let _ = forward.child.start_kill();
                    cleanup_unix_stale(&config.ssh, ownership_id, &lock, true).await?;
                }
                Err(error) => {
                    let _ = forward.child.start_kill();
                    return Err(error);
                }
            }
        } else {
            cleanup_unix_stale(&config.ssh, ownership_id, &lock, alive).await?;
        }
    }

    let spawn_token = mint_token();
    let spawned = spawn_unix_dashboard(
        &config.ssh,
        &runtime.hermes_path,
        &config.remote_profile,
        ownership_id,
        &spawn_token,
    )
    .await?;
    let mut owned = UnixLock {
        schema_version: LOCKFILE_SCHEMA_VERSION,
        protocol_version: PROTOCOL_VERSION,
        ownership_id: ownership_id.to_owned(),
        spawn_nonce: spawned.spawn_nonce.clone(),
        pid: spawned.pid,
        port: 0,
        profile: config.remote_profile.clone(),
        hermes_path: runtime.hermes_path.clone(),
        hermes_home,
        log_path: spawned.log_path.clone(),
        token_fingerprint: fingerprint_token(&spawn_token),
        started_at: unix_timestamp_string(),
    };

    let result = async {
        write_unix_lock(&config.ssh, ownership_id, &owned).await?;
        let remote_port = wait_unix_ready(&config.ssh, &spawned.log_path, spawned.pid).await?;
        let forward = open_forward(&config.ssh, remote_port).await?;
        let local_port = forward.local_port;
        let base_url = format!("http://127.0.0.1:{local_port}");
        wait_for_dashboard(&base_url, &spawn_token).await?;
        let token = adopt_served_token(&config.ssh, &base_url, &spawn_token, spawned.pid).await?;
        owned.port = remote_port;
        owned.token_fingerprint = fingerprint_token(&token);
        write_unix_lock(&config.ssh, ownership_id, &owned).await?;
        store_reuse_token(ownership_id, &token)?;
        Ok(SshLease {
            base_url,
            token,
            forward: forward.child,
            remote_port,
            local_port,
            remote_pid: spawned.pid,
            reused: false,
            remote_platform: format!("{}/{}", runtime.os, runtime.arch),
            remote_hermes_path: runtime.hermes_path.clone(),
            remote_hermes_version: runtime.hermes_version.clone(),
        })
    }
    .await;

    if result.is_err() {
        let _ = run_ssh(
            &config.ssh,
            &format!("rm -f {}", expand_unix_path(&spawned.token_file_path)?),
            None,
        )
        .await;
        let _ = cleanup_unix_stale(&config.ssh, ownership_id, &owned, true).await;
    }
    result
}

async fn read_unix_lock(config: &SshConfig, ownership_id: &str) -> ServiceResult<Option<UnixLock>> {
    validate_ownership_id(ownership_id)?;
    let path = lockfile_path(ownership_id)?;
    let expanded = expand_unix_path(&path)?;
    let raw = run_ssh(
        config,
        &format!("if [ ! -e {expanded} ]; then exit 0; fi; cat {expanded}"),
        None,
    )
    .await?;
    if raw.trim().is_empty() {
        return Ok(None);
    }
    let lock: UnixLock = match serde_json::from_str(raw.trim()) {
        Ok(lock) => lock,
        Err(_) => return Ok(None),
    };
    if valid_unix_lock(&lock, ownership_id) {
        Ok(Some(lock))
    } else {
        Ok(None)
    }
}

async fn write_unix_lock(
    config: &SshConfig,
    ownership_id: &str,
    lock: &UnixLock,
) -> ServiceResult<()> {
    validate_ownership_id(ownership_id)?;
    if !valid_unix_lock(lock, ownership_id) {
        return Err(ServiceError::InvalidInput(
            "refusing to write an invalid SSH ownership lock".into(),
        ));
    }
    let directory = ownership_directory(ownership_id)?;
    let path = lockfile_path(ownership_id)?;
    let temporary = format!("{directory}/.{}.lock.tmp", random_hex(8));
    let json = serde_json::to_string(lock).map_err(platform)?;
    let command = format!(
        "umask 077 && mkdir -p {} && printf '%s' {} > {} && mv -f {} {}",
        expand_unix_path(&directory)?,
        shell_quote(&json),
        expand_unix_path(&temporary)?,
        expand_unix_path(&temporary)?,
        expand_unix_path(&path)?
    );
    run_ssh(config, &command, None).await.map(|_| ())
}

fn valid_unix_lock(lock: &UnixLock, ownership_id: &str) -> bool {
    lock.schema_version == LOCKFILE_SCHEMA_VERSION
        && lock.protocol_version == PROTOCOL_VERSION
        && lock.ownership_id == ownership_id
        && is_lower_hex(&lock.spawn_nonce, 16)
        && lock.pid > 0
        && lock.pid <= 4_194_304
        && is_lower_hex(&lock.token_fingerprint, 32)
        && lock.log_path == spawn_log_path(ownership_id, &lock.spawn_nonce).unwrap_or_default()
        && lock.profile.len() <= 1_024
        && lock.hermes_path.len() <= 1_024
        && lock.hermes_home.len() <= 1_024
        && lock.started_at.len() <= 1_024
}

async fn remote_pid_alive(config: &SshConfig, pid: u32) -> ServiceResult<bool> {
    let output = run_ssh(
        config,
        &format!("kill -0 {pid} 2>/dev/null && echo ALIVE || echo DEAD"),
        None,
    )
    .await?;
    Ok(output.trim() == "ALIVE")
}

async fn pid_is_our_dashboard(
    config: &SshConfig,
    pid: u32,
    spawn_nonce: &str,
    hermes_path: &str,
) -> ServiceResult<bool> {
    validate_spawn_nonce(spawn_nonce)?;
    validate_unix_remote_path(hermes_path)?;
    let script = format!(
        "import os,shlex,subprocess\n\
         pid={pid}\n\
         expected=os.path.expanduser({expected})\n\
         nonce={nonce}\n\
         try:\n\
          raw=open(f'/proc/{{pid}}/cmdline','rb').read()\n\
          args=[x.decode('utf-8','surrogateescape') for x in raw.split(b'\\0') if x]\n\
         except OSError:\n\
          line=subprocess.check_output(['ps','-o','command=','-p',str(pid)],text=True).strip()\n\
          args=shlex.split(line)\n\
         ok=False\n\
         try:\n\
          serve=args.index('serve')\n\
          owner=args.index('--ssh-owner-nonce',serve+1)\n\
          direct=args[0]==expected\n\
          python_entry=len(args)>1 and args[1]==expected and os.path.basename(args[0]).startswith('python')\n\
          ok=(direct or python_entry) and '--isolated' in args[serve+1:] and args[owner+1]==nonce\n\
         except (ValueError,IndexError):pass\n\
         print('OWNED' if ok else 'FOREIGN')",
        expected = shell_quote(hermes_path),
        nonce = shell_quote(spawn_nonce),
    );
    let output = run_ssh(
        config,
        &format!("python3 -c {}", shell_quote(&script)),
        None,
    )
    .await?;
    Ok(output.trim() == "OWNED")
}

async fn cleanup_unix_stale(
    config: &SshConfig,
    ownership_id: &str,
    lock: &UnixLock,
    pid_alive: bool,
) -> ServiceResult<()> {
    if pid_alive
        && pid_is_our_dashboard(config, lock.pid, &lock.spawn_nonce, &lock.hermes_path).await?
    {
        run_ssh(
            config,
            &format!(
                "kill {} && i=0; while kill -0 {} 2>/dev/null; do i=$((i+1)); [ \"$i\" -ge 50 ] && exit 1; sleep 0.1; done",
                lock.pid, lock.pid
            ),
            None,
        )
        .await?;
    }
    let expected_log = spawn_log_path(ownership_id, &lock.spawn_nonce)?;
    if lock.log_path == expected_log {
        let _ = run_ssh(
            config,
            &format!("rm -f {}", expand_unix_path(&lock.log_path)?),
            None,
        )
        .await;
    }
    remove_unix_lock(config, ownership_id).await
}

async fn remove_unix_lock(config: &SshConfig, ownership_id: &str) -> ServiceResult<()> {
    let path = lockfile_path(ownership_id)?;
    run_ssh(config, &format!("rm -f {}", expand_unix_path(&path)?), None)
        .await
        .map(|_| ())
}

async fn spawn_unix_dashboard(
    config: &SshConfig,
    hermes_path: &str,
    profile: &str,
    ownership_id: &str,
    token: &str,
) -> ServiceResult<SpawnedUnix> {
    validate_unix_remote_path(hermes_path)?;
    validate_profile(profile)?;
    validate_ownership_id(ownership_id)?;
    let spawn_nonce = random_hex(8);
    let directory = ownership_directory(ownership_id)?;
    let token_file_path = format!("{directory}/{spawn_nonce}.token");
    let log_path = spawn_log_path(ownership_id, &spawn_nonce)?;
    upload_unix_token(config, &token_file_path, token).await?;

    let hermes = expand_unix_path(hermes_path)?;
    let profile_arg = if profile.is_empty() {
        String::new()
    } else {
        format!("--profile {} ", shell_quote(profile))
    };
    let dashboard = format!(
        "env HERMES_DESKTOP=1 {hermes} {profile_arg}serve --isolated --host 127.0.0.1 --port 0 --ssh-session-token-file {} --ssh-owner-nonce {}",
        expand_unix_path(&token_file_path)?,
        spawn_nonce
    );
    let detached = format!(
        "{dashboard} </dev/null >> {} 2>&1 & echo $!",
        expand_unix_path(&log_path)?
    );
    let command = format!(
        "mkdir -p \"$(dirname {})\" && \"$(command -v setsid || echo nohup)\" sh -c {}",
        expand_unix_path(&log_path)?,
        shell_quote(&detached)
    );
    let output = match run_ssh(config, &command, None).await {
        Ok(output) => output,
        Err(error) => {
            let _ = run_ssh(
                config,
                &format!("rm -f {}", expand_unix_path(&token_file_path)?),
                None,
            )
            .await;
            return Err(error);
        }
    };
    let pid = output
        .lines()
        .rev()
        .find_map(|line| line.trim().parse::<u32>().ok())
        .filter(|pid| *pid > 0)
        .ok_or_else(|| {
            ServiceError::Transport("remote Hermes dashboard did not return a pid".into())
        })?;
    Ok(SpawnedUnix {
        pid,
        spawn_nonce,
        log_path,
        token_file_path,
    })
}

async fn upload_unix_token(
    config: &SshConfig,
    token_file_path: &str,
    token: &str,
) -> ServiceResult<()> {
    let script = format!(
        "import os,sys,stat,time\n\
         p=os.path.expanduser({path})\n\
         d=os.path.dirname(p)\n\
         n=os.path.basename(p)\n\
         os.makedirs(d,mode=0o700,exist_ok=True)\n\
         df=os.O_RDONLY|getattr(os,'O_DIRECTORY',0)|getattr(os,'O_NOFOLLOW',0)\n\
         dd=os.open(d,df)\n\
         try:\n\
          s=os.fstat(dd)\n\
          if not stat.S_ISDIR(s.st_mode):raise SystemExit('unsafe token directory')\n\
          if hasattr(os,'getuid') and s.st_uid!=os.getuid():raise SystemExit('token directory owner mismatch')\n\
          if (s.st_mode&0o777)!=0o700:os.fchmod(dd,0o700)\n\
          fl=os.O_WRONLY|os.O_CREAT|os.O_EXCL|getattr(os,'O_NOFOLLOW',0)\n\
          now=time.time()\n\
          for stale in os.listdir(dd):\n\
           if stale.endswith('.token') and len(stale)==22:\n\
            try:\n\
             ss=os.stat(stale,dir_fd=dd,follow_symlinks=False)\n\
             if stat.S_ISREG(ss.st_mode) and now-ss.st_mtime>3600:os.unlink(stale,dir_fd=dd)\n\
            except OSError:pass\n\
          fd=os.open(n,fl,0o600,dir_fd=dd)\n\
          try:os.write(fd,sys.stdin.buffer.read())\n\
          except BaseException:\n\
           try:os.unlink(n,dir_fd=dd)\n\
           except OSError:pass\n\
           raise\n\
          finally:os.close(fd)\n\
         finally:os.close(dd)",
        path = shell_quote(token_file_path),
    );
    run_ssh(
        config,
        &format!("python3 -c {}", shell_quote(&script)),
        Some(token.as_bytes()),
    )
    .await
    .map(|_| ())
}

async fn wait_unix_ready(config: &SshConfig, log_path: &str, pid: u32) -> ServiceResult<u16> {
    let deadline = tokio::time::Instant::now() + READY_TIMEOUT;
    let expanded_log = expand_unix_path(log_path)?;
    while tokio::time::Instant::now() < deadline {
        if !remote_pid_alive(config, pid).await? {
            return Err(ServiceError::Transport(
                "remote Hermes dashboard exited before announcing its port".into(),
            ));
        }
        let output = run_ssh(
            config,
            &format!("cat {expanded_log} 2>/dev/null || true"),
            None,
        )
        .await
        .unwrap_or_default();
        if let Some(port) = parse_ready_port(&output) {
            return Ok(port);
        }
        sleep(READY_POLL_INTERVAL).await;
    }
    Err(ServiceError::Transport(
        "timed out waiting for the remote Hermes dashboard to announce its port".into(),
    ))
}

fn parse_ready_port(log: &str) -> Option<u16> {
    log.lines().find_map(|line| {
        let line = line.trim();
        let value = line
            .strip_prefix("HERMES_BACKEND_READY port=")
            .or_else(|| line.strip_prefix("HERMES_DASHBOARD_READY port="))?;
        value.trim().parse::<u16>().ok().filter(|port| *port > 0)
    })
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
        .map(|addr| addr.port())
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

async fn adopt_served_token(
    config: &SshConfig,
    base_url: &str,
    expected_token: &str,
    pid: u32,
) -> ServiceResult<String> {
    let served = match loopback_client()?.get(format!("{base_url}/")).send().await {
        Ok(response) if response.status().is_success() => response
            .text()
            .await
            .ok()
            .and_then(|html| extract_served_token(&html)),
        _ => None,
    }
    .unwrap_or_else(|| expected_token.to_owned());
    if !remote_pid_alive(config, pid).await? {
        if served != expected_token {
            return Err(ServiceError::PermissionDenied(
                "the owned SSH dashboard exited while a different process served the forwarded port".into(),
            ));
        }
        return Err(ServiceError::Transport(
            "the owned SSH dashboard exited while its token was being resolved".into(),
        ));
    }
    Ok(served)
}

fn extract_served_token(html: &str) -> Option<String> {
    let marker = "window.__HERMES_SESSION_TOKEN__";
    let tail = html.get(html.find(marker)? + marker.len()..)?;
    let value = tail.get(tail.find('=')? + 1..)?.trim_start();
    if !value.starts_with('"') {
        return None;
    }
    let mut escaped = false;
    for (index, character) in value.char_indices().skip(1) {
        if character == '"' && !escaped {
            return serde_json::from_str(value.get(..=index)?).ok();
        }
        escaped = character == '\\' && !escaped;
        if character != '\\' {
            escaped = false;
        }
    }
    None
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
    run_ssh_inner(config, remote_command, stdin)
        .await
        .map_err(SshExecError::into_service)
}

async fn run_ssh_inner(
    config: &SshConfig,
    remote_command: &str,
    stdin: Option<&[u8]>,
) -> Result<String, SshExecError> {
    let executable = resolve_ssh_executable().map_err(|error| SshExecError {
        kind: SshErrorKind::Unreachable,
        message: error.to_string(),
    })?;
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
        .map_err(|error| SshExecError {
            kind: SshErrorKind::Unreachable,
            message: format!("Could not start OpenSSH: {error}"),
        })?;
    if let Some(bytes) = stdin {
        let mut pipe = child.stdin.take().ok_or_else(|| SshExecError {
            kind: SshErrorKind::Unknown,
            message: "OpenSSH stdin was unavailable".into(),
        })?;
        pipe.write_all(bytes).await.map_err(|error| SshExecError {
            kind: SshErrorKind::Unknown,
            message: format!("Could not write SSH stdin: {error}"),
        })?;
        drop(pipe);
    }
    let output = tokio::time::timeout(SSH_TIMEOUT, child.wait_with_output())
        .await
        .map_err(|_| SshExecError {
            kind: SshErrorKind::Timeout,
            message: format!("SSH operation to {} timed out", ssh_target(config)),
        })?
        .map_err(|error| SshExecError {
            kind: SshErrorKind::Unreachable,
            message: format!("OpenSSH failed: {error}"),
        })?;
    if output.stdout.len() > MAX_SSH_OUTPUT_BYTES || output.stderr.len() > MAX_SSH_OUTPUT_BYTES {
        return Err(SshExecError {
            kind: SshErrorKind::Unknown,
            message: "SSH operation returned an oversized response".into(),
        });
    }
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    let stderr = sanitize_remote_text(&String::from_utf8_lossy(&output.stderr));
    let kind = classify_ssh_error(&stderr);
    Err(SshExecError {
        kind,
        message: ssh_error_message(kind, config, &stderr),
    })
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
    if cfg!(windows) {
        let system_root = std::env::var_os("SystemRoot")
            .map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
        let native = system_root.join("System32/OpenSSH/ssh.exe");
        if native.is_file() {
            return Ok(native);
        }
    } else {
        for candidate in ["/usr/bin/ssh", "/usr/local/bin/ssh"] {
            let path = PathBuf::from(candidate);
            if path.is_file() {
                return Ok(path);
            }
        }
    }
    Err(ServiceError::Unavailable(
        "OpenSSH client was not found".into(),
    ))
}

fn classify_ssh_error(stderr: &str) -> SshErrorKind {
    let lower = stderr.to_ascii_lowercase();
    if lower.contains("remote host identification has changed")
        || lower.contains("host key verification failed")
        || lower.contains("offending ed25519")
        || lower.contains("offending ecdsa")
        || lower.contains("offending rsa")
    {
        SshErrorKind::HostKeyChanged
    } else if lower.contains("permission denied")
        || lower.contains("too many authentication failures")
        || lower.contains("publickey")
        || lower.contains("keyboard-interactive")
    {
        SshErrorKind::AuthFailed
    } else if lower.contains("could not resolve hostname")
        || lower.contains("connection refused")
        || lower.contains("connection timed out")
        || lower.contains("no route to host")
        || lower.contains("network is unreachable")
        || lower.contains("operation timed out")
    {
        SshErrorKind::Unreachable
    } else {
        SshErrorKind::Unknown
    }
}

fn ssh_error_message(kind: SshErrorKind, config: &SshConfig, stderr: &str) -> String {
    let target = ssh_target(config);
    match kind {
        SshErrorKind::HostKeyChanged => format!(
            "The host key for {target} changed. SSH refused the connection. Verify the host before updating known_hosts. {stderr}"
        ),
        SshErrorKind::AuthFailed => format!(
            "SSH authentication to {target} failed. Hermes Local uses BatchMode; load interactive credentials into ssh-agent first. {stderr}"
        ),
        SshErrorKind::Unreachable => format!("Could not reach {target} over SSH. {stderr}"),
        SshErrorKind::Timeout => format!("SSH operation to {target} timed out"),
        _ => format!("SSH error connecting to {target}: {stderr}"),
    }
}

fn is_bind_collision(error: &ServiceError) -> bool {
    let text = error.to_string().to_ascii_lowercase();
    text.contains("address already in use")
        || text.contains("cannot listen to port")
        || (text.contains("bind") && text.contains("failed"))
}

fn ownership_directory(ownership_id: &str) -> ServiceResult<String> {
    validate_ownership_id(ownership_id)?;
    Ok(format!("{REMOTE_LOCK_DIR}/{ownership_id}"))
}

fn lockfile_path(ownership_id: &str) -> ServiceResult<String> {
    Ok(format!(
        "{}/backend.lock.json",
        ownership_directory(ownership_id)?
    ))
}

fn spawn_log_path(ownership_id: &str, spawn_nonce: &str) -> ServiceResult<String> {
    validate_spawn_nonce(spawn_nonce)?;
    Ok(format!(
        "{}/{}.log",
        ownership_directory(ownership_id)?,
        spawn_nonce
    ))
}

fn expand_unix_path(path: &str) -> ServiceResult<String> {
    validate_unix_remote_path(path)?;
    if path == "~" {
        return Ok("\"$HOME\"".into());
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return Ok(format!("\"$HOME\"{}", shell_quote(&format!("/{rest}"))));
    }
    Ok(shell_quote(path))
}

fn validate_unix_remote_path(path: &str) -> ServiceResult<()> {
    if path.is_empty()
        || path
            .chars()
            .any(|character| matches!(character, '\0' | '\n' | '\r'))
        || !(path == "~" || path.starts_with("~/") || path.starts_with('/'))
    {
        return Err(ServiceError::InvalidInput(
            "remote SSH path must be absolute or begin with ~/".into(),
        ));
    }
    Ok(())
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
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

fn fingerprint_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    format!("{digest:x}")[..32].to_owned()
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
    Ok(format!("{digest:x}")[..32].to_owned())
}

fn load_or_create_installation_id(path: &Path) -> ServiceResult<String> {
    if let Some(id) = read_installation_id(path)? {
        return Ok(id);
    }
    let parent = path
        .parent()
        .ok_or_else(|| ServiceError::Platform("installation id path has no parent".into()))?;
    fs::create_dir_all(parent).map_err(platform)?;
    let id = Uuid::new_v4().hyphenated().to_string().to_ascii_lowercase();
    let temporary = path.with_extension(format!("json.{}.tmp", random_hex(8)));
    let payload = serde_json::json!({ "installationId": id });
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)
        .map_err(platform)?;
    serde_json::to_writer(&mut file, &payload).map_err(platform)?;
    file.flush().map_err(platform)?;
    drop(file);
    match fs::rename(&temporary, path) {
        Ok(()) => Ok(id),
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            if let Some(winner) = read_installation_id(path)? {
                Ok(winner)
            } else {
                Err(platform(error))
            }
        }
    }
}

fn read_installation_id(path: &Path) -> ServiceResult<Option<String>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(platform(error)),
    };
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Ok(None);
    }
    let value: Value =
        serde_json::from_slice(&fs::read(path).map_err(platform)?).map_err(platform)?;
    let Some(id) = value.get("installationId").and_then(Value::as_str) else {
        return Ok(None);
    };
    let parsed = match Uuid::parse_str(id) {
        Ok(parsed) if parsed.get_version_num() == 4 => parsed,
        _ => return Ok(None),
    };
    Ok(Some(parsed.hyphenated().to_string().to_ascii_lowercase()))
}

#[cfg(windows)]
fn load_reuse_token(ownership_id: &str) -> ServiceResult<Option<String>> {
    validate_ownership_id(ownership_id)?;
    let entry = keyring::Entry::new("Hermes Local SSH", ownership_id).map_err(platform)?;
    match entry.get_password() {
        Ok(token) => Ok(Some(token)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(platform(error)),
    }
}

#[cfg(not(windows))]
fn load_reuse_token(_ownership_id: &str) -> ServiceResult<Option<String>> {
    Ok(None)
}

#[cfg(windows)]
fn store_reuse_token(ownership_id: &str, token: &str) -> ServiceResult<()> {
    validate_ownership_id(ownership_id)?;
    if token.is_empty() || token.len() > 4_096 {
        return Err(ServiceError::InvalidInput("invalid SSH reuse token".into()));
    }
    keyring::Entry::new("Hermes Local SSH", ownership_id)
        .map_err(platform)?
        .set_password(token)
        .map_err(platform)
}

#[cfg(not(windows))]
fn store_reuse_token(_ownership_id: &str, _token: &str) -> ServiceResult<()> {
    Ok(())
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

fn transport(error: impl std::fmt::Display) -> ServiceError {
    ServiceError::Transport(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ownership_identity_is_stable_and_scoped() {
        let installation = "123e4567-e89b-42d3-a456-426614174000";
        let global = ownership_id(installation, "").expect("global ownership id");
        let project = ownership_id(installation, "project").expect("project ownership id");
        assert!(is_lower_hex(&global, 32));
        assert!(is_lower_hex(&project, 32));
        assert_ne!(global, project);
        assert_eq!(global, ownership_id(installation, "").expect("stable"));
    }

    #[test]
    fn remote_paths_and_ownership_values_fail_closed() {
        assert!(validate_unix_remote_path("~/.hermes/desktop-ssh").is_ok());
        assert!(validate_unix_remote_path("/opt/hermes").is_ok());
        assert!(validate_unix_remote_path("relative/hermes").is_err());
        assert!(validate_unix_remote_path("/tmp/x\nrm -rf /").is_err());
        assert!(validate_ownership_id("abcd").is_err());
        assert!(validate_spawn_nonce("0011223344556677").is_ok());
    }

    #[test]
    fn ready_port_accepts_only_known_readiness_markers() {
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

    #[test]
    fn served_token_parser_requires_json_string_literal() {
        let html = r#"<script>window.__HERMES_SESSION_TOKEN__ = \"abc\\\"def\";</script>"#;
        assert_eq!(extract_served_token(html).as_deref(), Some("abc\"def"));
        assert_eq!(
            extract_served_token("window.__HERMES_SESSION_TOKEN__ = token"),
            None
        );
    }

    #[test]
    fn lock_validation_binds_log_path_and_protocol() {
        let ownership = "00112233445566778899aabbccddeeff";
        let nonce = "0011223344556677";
        let lock = UnixLock {
            schema_version: LOCKFILE_SCHEMA_VERSION,
            protocol_version: PROTOCOL_VERSION,
            ownership_id: ownership.into(),
            spawn_nonce: nonce.into(),
            pid: 42,
            port: 9119,
            profile: String::new(),
            hermes_path: "/opt/hermes".into(),
            hermes_home: "/home/test/.hermes".into(),
            log_path: spawn_log_path(ownership, nonce).expect("log path"),
            token_fingerprint: fingerprint_token("secret"),
            started_at: "1".into(),
        };
        assert!(valid_unix_lock(&lock, ownership));
        let mut wrong = lock.clone();
        wrong.log_path = "~/.hermes/desktop-ssh/other.log".into();
        assert!(!valid_unix_lock(&wrong, ownership));
    }
}
