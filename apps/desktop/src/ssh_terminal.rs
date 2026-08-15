//! SSH-aware terminal ownership for the native Desktop client.
//!
//! The shared Dioxus terminal UI continues to speak the platform-neutral
//! `TerminalService` contract. This adapter keeps local terminals on the
//! existing Desktop PTY implementation and, when the active connection mode is
//! SSH, runs the system OpenSSH client inside a native PTY so input, resize,
//! output, Ctrl-C, ANSI rendering and disposal follow the same UI path.

use std::{
    collections::HashMap,
    env,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use hermes_core::{
    AppServices, ConnectionService, ServiceError, ServiceFuture, ServiceResult, TerminalService,
    validate_identifier,
};
use hermes_protocol::{ConnectionConfig, ConnectionMode};
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use uuid::Uuid;

use crate::ssh::SshConfig;

const REMOTE_TERMINAL_PREFIX: &str = "ssh-";
const MAX_BUFFERED_OUTPUT: usize = 2 * 1024 * 1024;

/// Wrap the native terminal boundary after the SSH connection boundary is
/// installed. Local sessions keep delegating to the proven Desktop PTY engine;
/// SSH sessions are owned here because OpenSSH configuration is Desktop-only.
pub fn install(services: &mut AppServices) {
    let inner = services.terminal.clone();
    let connection = services.connection.clone();
    services.terminal = Arc::new(SshAwareTerminal {
        inner,
        connection,
        remote: RemotePtyPool::default(),
    });
}

struct SshAwareTerminal {
    inner: Arc<dyn TerminalService>,
    connection: Arc<dyn ConnectionService>,
    remote: RemotePtyPool,
}

impl TerminalService for SshAwareTerminal {
    fn start(&self, cwd: &Path, cols: u16, rows: u16) -> ServiceFuture<'_, String> {
        let cwd = cwd.to_owned();
        Box::pin(async move {
            let connection = self.connection.config(None).await?;
            if connection.mode != ConnectionMode::Ssh {
                return self.inner.start(&cwd, cols, rows).await;
            }
            let config = ssh_config_from_connection(&connection)?;
            self.remote.start(&config, cols, rows)
        })
    }

    fn write(&self, id: &str, data: &[u8]) -> ServiceFuture<'_, ()> {
        let id = id.to_owned();
        let data = data.to_vec();
        Box::pin(async move {
            if is_remote_terminal(&id) {
                return self.remote.write(&id, &data);
            }
            self.inner.write(&id, &data).await
        })
    }

    fn read(&self, id: &str) -> ServiceFuture<'_, Vec<u8>> {
        let id = id.to_owned();
        Box::pin(async move {
            if is_remote_terminal(&id) {
                return self.remote.read(&id);
            }
            self.inner.read(&id).await
        })
    }

    fn resize(&self, id: &str, cols: u16, rows: u16) -> ServiceFuture<'_, ()> {
        let id = id.to_owned();
        Box::pin(async move {
            if is_remote_terminal(&id) {
                return self.remote.resize(&id, cols, rows);
            }
            self.inner.resize(&id, cols, rows).await
        })
    }

    fn dispose(&self, id: &str) -> ServiceFuture<'_, ()> {
        let id = id.to_owned();
        Box::pin(async move {
            if is_remote_terminal(&id) {
                return self.remote.dispose(&id);
            }
            self.inner.dispose(&id).await
        })
    }

    fn dispose_now(&self, id: &str) -> ServiceResult<()> {
        if is_remote_terminal(id) {
            self.remote.dispose(id)
        } else {
            self.inner.dispose_now(id)
        }
    }
}

fn ssh_config_from_connection(connection: &ConnectionConfig) -> ServiceResult<SshConfig> {
    SshConfig::new(
        &connection.ssh_host,
        non_empty(&connection.ssh_user),
        connection.ssh_port,
        non_empty(&connection.ssh_key_path),
        non_empty(&connection.ssh_remote_hermes_path),
    )
    .map_err(ServiceError::InvalidInput)
}

fn non_empty(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

fn is_remote_terminal(id: &str) -> bool {
    id.starts_with(REMOTE_TERMINAL_PREFIX)
}

struct RemoteTerminalProcess {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
    output: Arc<Mutex<Vec<u8>>>,
    control_tail: Vec<u8>,
}

#[derive(Default)]
struct RemotePtyPool {
    processes: Mutex<HashMap<String, RemoteTerminalProcess>>,
}

impl RemotePtyPool {
    fn start(&self, config: &SshConfig, cols: u16, rows: u16) -> ServiceResult<String> {
        if cols == 0 || rows == 0 {
            return Err(ServiceError::InvalidInput(
                "invalid terminal dimensions".into(),
            ));
        }
        let executable = resolve_ssh_executable()?;
        let args = interactive_ssh_args(config);
        let pair = native_pty_system()
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(platform)?;
        let mut command = CommandBuilder::new(executable);
        for arg in &args {
            command.arg(arg);
        }
        let child = pair.slave.spawn_command(command).map_err(platform)?;
        let writer = pair.master.take_writer().map_err(platform)?;
        let mut reader = pair.master.try_clone_reader().map_err(platform)?;
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
                    if output.len() > MAX_BUFFERED_OUTPUT {
                        let excess = output.len() - MAX_BUFFERED_OUTPUT;
                        output.drain(..excess);
                    }
                }
            }
        });
        let id = format!("{REMOTE_TERMINAL_PREFIX}{}", Uuid::new_v4());
        self.processes
            .lock()
            .map_err(|_| ServiceError::Platform("SSH terminal lock was poisoned".into()))?
            .insert(
                id.clone(),
                RemoteTerminalProcess {
                    master: pair.master,
                    writer,
                    child,
                    output,
                    control_tail: Vec::new(),
                },
            );
        Ok(id)
    }

    fn write(&self, id: &str, data: &[u8]) -> ServiceResult<()> {
        validate_remote_id(id)?;
        let mut processes = self
            .processes
            .lock()
            .map_err(|_| ServiceError::Platform("SSH terminal lock was poisoned".into()))?;
        let process = processes
            .get_mut(id)
            .ok_or_else(|| ServiceError::NotFound("SSH terminal".into()))?;
        process.writer.write_all(data).map_err(platform)?;
        process.writer.flush().map_err(platform)
    }

    fn read(&self, id: &str) -> ServiceResult<Vec<u8>> {
        validate_remote_id(id)?;
        let mut processes = self
            .processes
            .lock()
            .map_err(|_| ServiceError::Platform("SSH terminal lock was poisoned".into()))?;
        let process = processes
            .get_mut(id)
            .ok_or_else(|| ServiceError::NotFound("SSH terminal".into()))?;
        let bytes = {
            let mut output = process.output.lock().map_err(|_| {
                ServiceError::Platform("SSH terminal output lock was poisoned".into())
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
    }

    fn resize(&self, id: &str, cols: u16, rows: u16) -> ServiceResult<()> {
        validate_remote_id(id)?;
        if cols == 0 || rows == 0 {
            return Err(ServiceError::InvalidInput(
                "invalid terminal dimensions".into(),
            ));
        }
        let processes = self
            .processes
            .lock()
            .map_err(|_| ServiceError::Platform("SSH terminal lock was poisoned".into()))?;
        let process = processes
            .get(id)
            .ok_or_else(|| ServiceError::NotFound("SSH terminal".into()))?;
        process
            .master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(platform)
    }

    fn dispose(&self, id: &str) -> ServiceResult<()> {
        validate_remote_id(id)?;
        let mut process = self
            .processes
            .lock()
            .map_err(|_| ServiceError::Platform("SSH terminal lock was poisoned".into()))?
            .remove(id)
            .ok_or_else(|| ServiceError::NotFound("SSH terminal".into()))?;
        process.child.kill().map_err(platform)
    }
}

fn validate_remote_id(id: &str) -> ServiceResult<()> {
    if !is_remote_terminal(id) {
        return Err(ServiceError::InvalidInput(
            "invalid SSH terminal identifier".into(),
        ));
    }
    validate_identifier(id, "SSH terminal")
}

fn interactive_ssh_args(config: &SshConfig) -> Vec<String> {
    let mut args = vec![
        "-tt".into(),
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
    args.extend(["--".into(), ssh_target(config)]);
    args
}

fn ssh_target(config: &SshConfig) -> String {
    config.user.as_ref().map_or_else(
        || config.host.clone(),
        |user| format!("{user}@{}", config.host),
    )
}

fn resolve_ssh_executable() -> ServiceResult<PathBuf> {
    if let Some(explicit) = env::var_os("HERMES_LOCAL_SSH") {
        let path = PathBuf::from(explicit);
        if path.is_absolute() && path.is_file() {
            return Ok(path);
        }
        return Err(ServiceError::Unavailable(
            "HERMES_LOCAL_SSH must point to an absolute OpenSSH executable.".into(),
        ));
    }

    #[cfg(windows)]
    {
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
    }

    #[cfg(not(windows))]
    {
        if let Some(path) = ["/usr/bin/ssh", "/usr/local/bin/ssh"]
            .into_iter()
            .map(PathBuf::from)
            .find(|path| path.is_file())
        {
            return Ok(path);
        }
    }

    Err(ServiceError::Unavailable(
        "OpenSSH client was not found.".into(),
    ))
}

fn platform(error: impl std::fmt::Display) -> ServiceError {
    ServiceError::Platform(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        thread,
        time::{Duration, Instant},
    };

    fn config() -> SshConfig {
        SshConfig::new(
            "example.test",
            Some("cloudy"),
            Some(2222),
            Some(r"C:\keys\id_ed25519"),
            None,
        )
        .expect("valid config")
    }

    #[test]
    fn interactive_ssh_argv_is_tty_and_option_safe() {
        let args = interactive_ssh_args(&config());
        assert_eq!(args.first().map(String::as_str), Some("-tt"));
        assert!(args.windows(2).any(|pair| pair == ["-p", "2222"]));
        assert!(
            args.windows(2)
                .any(|pair| pair[0] == "-i" && pair[1] == r"C:\keys\id_ed25519")
        );
        let separator = args.iter().position(|arg| arg == "--").expect("separator");
        assert_eq!(
            args.get(separator + 1).map(String::as_str),
            Some("cloudy@example.test")
        );
        assert_eq!(separator + 2, args.len());
    }

    #[test]
    fn connection_config_maps_to_validated_ssh_config() {
        let connection = ConnectionConfig {
            mode: ConnectionMode::Ssh,
            ssh_host: "example.test".into(),
            ssh_user: "cloudy".into(),
            ssh_port: Some(2200),
            ssh_key_path: r"C:\keys\id_ed25519".into(),
            ..ConnectionConfig::default()
        };
        let mapped = ssh_config_from_connection(&connection).expect("mapped config");
        assert_eq!(mapped.host, "example.test");
        assert_eq!(mapped.user.as_deref(), Some("cloudy"));
        assert_eq!(mapped.port, 2200);
    }

    #[test]
    fn remote_ids_are_explicitly_namespaced() {
        assert!(is_remote_terminal("ssh-123"));
        assert!(!is_remote_terminal("123"));
        assert!(validate_remote_id("ssh-123").is_ok());
        assert!(validate_remote_id("local-123").is_err());
    }

    #[test]
    #[ignore = "requires the explicitly provisioned SSH interoperability fixture"]
    fn live_remote_pty_round_trip() {
        let host = env::var("HERMES_SSH_TEST_HOST").unwrap_or_else(|_| "127.0.0.1".into());
        let user = env::var("HERMES_SSH_TEST_USER").expect("HERMES_SSH_TEST_USER");
        let port = env::var("HERMES_SSH_TEST_PORT")
            .expect("HERMES_SSH_TEST_PORT")
            .parse::<u16>()
            .expect("HERMES_SSH_TEST_PORT must be a u16");
        let key = env::var("HERMES_SSH_TEST_KEY").expect("HERMES_SSH_TEST_KEY");
        let config = SshConfig::new(&host, Some(&user), Some(port), Some(&key), None)
            .expect("live SSH terminal config");
        let pool = RemotePtyPool::default();
        let id = pool.start(&config, 80, 24).expect("start live SSH PTY");
        pool.resize(&id, 100, 30).expect("resize live SSH PTY");
        pool.write(&id, b"printf 'HERMES_REMOTE_PTY_OK\\n'\n")
            .expect("write live SSH PTY");

        let deadline = Instant::now() + Duration::from_secs(10);
        let mut output = Vec::new();
        while Instant::now() < deadline {
            output.extend(pool.read(&id).expect("read live SSH PTY"));
            if output
                .windows(b"HERMES_REMOTE_PTY_OK".len())
                .any(|window| window == b"HERMES_REMOTE_PTY_OK")
            {
                pool.dispose(&id).expect("dispose live SSH PTY");
                return;
            }
            thread::sleep(Duration::from_millis(50));
        }

        let _ = pool.dispose(&id);
        panic!(
            "remote PTY marker not observed; output={}",
            String::from_utf8_lossy(&output)
        );
    }
}
