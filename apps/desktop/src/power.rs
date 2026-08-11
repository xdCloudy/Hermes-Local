use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread::{self, JoinHandle},
};

#[cfg(windows)]
const ES_SYSTEM_REQUIRED: u32 = 0x0000_0001;
#[cfg(windows)]
const ES_CONTINUOUS: u32 = 0x8000_0000;

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn SetThreadExecutionState(es_flags: u32) -> u32;
}

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
        let (tx, rx) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("hermes-power-blocker".to_owned())
            .spawn(move || power_worker(rx))
            .ok();
        Self {
            tx,
            available: worker.is_some() && cfg!(windows),
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
    let mut active = false;
    while let Ok(command) = rx.recv() {
        match command {
            Command::Set { enabled, reply } => {
                let result = if enabled == active {
                    Ok(active)
                } else {
                    apply_power_state(enabled).map(|()| {
                        active = enabled;
                        active
                    })
                };
                let _ = reply.send(result);
            }
            Command::Query(reply) => {
                let _ = reply.send(active);
            }
            Command::Shutdown => break,
        }
    }

    if active {
        let _ = apply_power_state(false);
    }
}

#[cfg(windows)]
fn apply_power_state(enabled: bool) -> Result<(), String> {
    let flags = if enabled {
        ES_CONTINUOUS | ES_SYSTEM_REQUIRED
    } else {
        ES_CONTINUOUS
    };
    let previous = unsafe { SetThreadExecutionState(flags) };
    if previous == 0 {
        Err("Windows rejected the keep-awake execution state.".to_owned())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn apply_power_state(_enabled: bool) -> Result<(), String> {
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
        assert_eq!(service.set(true).expect("enable"), true);
        assert!(service.is_active());
        assert_eq!(service.set(true).expect("idempotent enable"), true);
        assert_eq!(service.set(false).expect("disable"), false);
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
