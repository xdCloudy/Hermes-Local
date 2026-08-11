use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread::{self, JoinHandle},
};

#[cfg(windows)]
use std::{
    io::{BufRead, BufReader},
    path::PathBuf,
    process::{Child, Command as ProcessCommand, Stdio},
};

#[cfg(windows)]
const POWER_BLOCKER_SCRIPT: &str = r#"Add-Type -TypeDefinition 'using System.Runtime.InteropServices; public static class HermesPower { [DllImport("kernel32.dll")] public static extern uint SetThreadExecutionState(uint esFlags); }'; $result=[HermesPower]::SetThreadExecutionState([uint32]0x80000001); if($result -eq 0){exit 42}; [Console]::Out.WriteLine('READY'); [Console]::Out.Flush(); [Threading.Thread]::Sleep([Threading.Timeout]::Infinite)"#;

enum Command {
    Set {
        enabled: bool,
        reply: Sender<Result<bool, String>>,
    },
    Query(Sender<bool>),
    Shutdown,
}

pub struct KeepAwakeService {
    tx: Sender<Command>,
    worker: Option<JoinHandle<()>>,
    pub available: bool,
}

impl KeepAwakeService {
    pub fn new() -> Self {
        let available = platform_available();
        let (tx, rx) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("hermes-power-blocker".to_owned())
            .spawn(move || power_worker(rx))
            .ok();
        Self {
            tx,
            available: available && worker.is_some(),
            worker,
        }
    }

    pub fn set(&self, enabled: bool) -> Result<bool, String> {
        if !self.available {
            return Err("Keep-awake is only available on Windows.".to_owned());
        }
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::Set {
                enabled,
                reply: reply_tx,
            })
            .map_err(|_| "Keep-awake worker is unavailable.".to_owned())?;
        reply_rx
            .recv()
            .map_err(|_| "Keep-awake worker stopped before replying.".to_owned())?
    }

    pub fn is_active(&self) -> bool {
        if !self.available {
            return false;
        }
        let (reply_tx, reply_rx) = mpsc::channel();
        if self.tx.send(Command::Query(reply_tx)).is_err() {
            return false;
        }
        reply_rx.recv().unwrap_or(false)
    }
}

impl Default for KeepAwakeService {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for KeepAwakeService {
    fn drop(&mut self) {
        let _ = self.tx.send(Command::Shutdown);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn power_worker(rx: Receiver<Command>) {
    #[cfg(windows)]
    let mut blocker: Option<PowerBlocker> = None;
    let mut active = false;

    while let Ok(command) = rx.recv() {
        match command {
            Command::Set { enabled, reply } => {
                let result = if enabled == active {
                    Ok(active)
                } else if enabled {
                    match start_blocker() {
                        Ok(started) => {
                            #[cfg(windows)]
                            {
                                blocker = Some(started);
                            }
                            active = true;
                            Ok(true)
                        }
                        Err(error) => Err(error),
                    }
                } else {
                    #[cfg(windows)]
                    let stop_result = blocker.take().map_or(Ok(()), PowerBlocker::stop);
                    #[cfg(not(windows))]
                    let stop_result = Ok(());
                    match stop_result {
                        Ok(()) => {
                            active = false;
                            Ok(false)
                        }
                        Err(error) => Err(error),
                    }
                };
                let _ = reply.send(result);
            }
            Command::Query(reply) => {
                let _ = reply.send(active);
            }
            Command::Shutdown => break,
        }
    }

    #[cfg(windows)]
    if let Some(blocker) = blocker.take() {
        let _ = blocker.stop();
    }
}

#[cfg(windows)]
struct PowerBlocker {
    child: Child,
}

#[cfg(windows)]
impl PowerBlocker {
    fn stop(mut self) -> Result<(), String> {
        match self.child.try_wait() {
            Ok(Some(_)) => return Ok(()),
            Ok(None) => {}
            Err(error) => return Err(format!("Could not inspect keep-awake helper: {error}")),
        }
        self.child
            .kill()
            .map_err(|error| format!("Could not stop keep-awake helper: {error}"))?;
        self.child
            .wait()
            .map_err(|error| format!("Could not reap keep-awake helper: {error}"))?;
        Ok(())
    }
}

#[cfg(windows)]
fn platform_available() -> bool {
    powershell_executable().is_ok()
}

#[cfg(not(windows))]
fn platform_available() -> bool {
    false
}

#[cfg(windows)]
fn powershell_executable() -> Result<PathBuf, String> {
    let root = std::env::var_os("SystemRoot")
        .map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
    let executable = root
        .join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe");
    if executable.is_absolute() && executable.is_file() {
        Ok(executable)
    } else {
        Err(format!(
            "Windows PowerShell is unavailable: {}",
            executable.display()
        ))
    }
}

#[cfg(windows)]
fn start_blocker() -> Result<PowerBlocker, String> {
    let mut child = ProcessCommand::new(powershell_executable()?)
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            POWER_BLOCKER_SCRIPT,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("Could not start keep-awake helper: {error}"))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Keep-awake helper stdout was unavailable.".to_owned())?;
    let mut reader = BufReader::new(stdout);
    let mut ready = String::new();
    reader
        .read_line(&mut ready)
        .map_err(|error| format!("Could not read keep-awake helper readiness: {error}"))?;
    if ready.trim() != "READY" {
        let _ = child.kill();
        let _ = child.wait();
        return Err("Windows rejected the keep-awake execution state.".to_owned());
    }
    Ok(PowerBlocker { child })
}

#[cfg(not(windows))]
fn start_blocker() -> Result<(), String> {
    Err("Keep-awake is only available on Windows.".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn windows_keep_awake_is_idempotent_and_reversible() {
        let service = KeepAwakeService::new();
        assert!(service.available);
        assert!(!service.is_active());
        assert!(service.set(true).expect("enable"));
        assert!(service.is_active());
        assert!(service.set(true).expect("idempotent enable"));
        assert!(!service.set(false).expect("disable"));
        assert!(!service.is_active());
    }

    #[cfg(not(windows))]
    #[test]
    fn non_windows_reports_unavailable() {
        let service = KeepAwakeService::new();
        assert!(!service.available);
        assert!(service.set(true).is_err());
        assert!(!service.is_active());
    }
}
