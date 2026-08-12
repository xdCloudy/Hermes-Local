#![allow(dead_code)] // PF-07 service foundation; Dioxus watcher consumers are a later stage.

use std::{
    collections::HashMap,
    path::{Component, Path, PathBuf},
    sync::mpsc,
    thread,
    time::Duration,
};

#[cfg(windows)]
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
};

use url::Url;
use uuid::Uuid;

use crate::preview_normalization::{PreviewNormalizationService, PreviewTarget};

const WATCH_DEBOUNCE: Duration = Duration::from_millis(120);
const HELPER_READY_TIMEOUT: Duration = Duration::from_secs(5);

#[cfg(windows)]
const WATCH_SCRIPT: &str = r#"$ErrorActionPreference='Stop'; $dir=$env:HERMES_LOCAL_WATCH_DIR; $target=$env:HERMES_LOCAL_WATCH_TARGET; $watcher=[System.IO.FileSystemWatcher]::new($dir,'*'); $watcher.IncludeSubdirectories=$false; $watcher.NotifyFilter=[IO.NotifyFilters]'FileName, DirectoryName, LastWrite, Size'; $subs=@(); foreach($eventName in @('Changed','Created','Deleted','Renamed','Error')){ $subs += Register-ObjectEvent -InputObject $watcher -EventName $eventName }; $watcher.EnableRaisingEvents=$true; [Console]::Out.WriteLine('ready'); [Console]::Out.Flush(); try { while($true){ $event=Wait-Event; try { $relevant=$true; if($target -and $event.SourceEventArgs -and $event.SourceEventArgs.PSObject.Properties['FullPath']){ $name=[IO.Path]::GetFileName([string]$event.SourceEventArgs.FullPath); $relevant=[string]::Equals($name,$target,[StringComparison]::Ordinal) }; if($relevant){ [Console]::Out.WriteLine('changed'); [Console]::Out.Flush() } } finally { Remove-Event -EventIdentifier $event.EventIdentifier -ErrorAction SilentlyContinue } } } finally { foreach($sub in $subs){ Unregister-Event -SubscriptionId $sub.Id -ErrorAction SilentlyContinue }; $watcher.Dispose() }"#;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreviewWatchEvent {
    pub id: String,
    pub path: PathBuf,
    pub url: String,
}

pub struct PreviewWatcherRegistry {
    watches: HashMap<String, WatchHandle>,
}

impl Default for PreviewWatcherRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl PreviewWatcherRegistry {
    pub fn new() -> Self {
        Self {
            watches: HashMap::new(),
        }
    }

    pub fn watch_file<F>(
        &mut self,
        raw_target: &str,
        base_dir: Option<&Path>,
        callback: F,
    ) -> Result<String, String>
    where
        F: Fn(PreviewWatchEvent) + Send + Sync + 'static,
    {
        let target = PreviewNormalizationService
            .normalize(raw_target, base_dir)?
            .ok_or_else(|| "Preview watcher target does not exist.".to_owned())?;
        let (path, url) = match target {
            PreviewTarget::File { path, url, .. } => (path, url),
            PreviewTarget::Url { .. } => {
                return Err("Preview watcher only supports local files.".to_owned());
            }
        };
        let directory = path
            .parent()
            .ok_or_else(|| "Preview watcher target has no parent directory.".to_owned())?
            .to_path_buf();
        let target_name = path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| "Preview watcher target name is not valid UTF-8.".to_owned())?
            .to_owned();
        let id = new_watch_id();
        let event = PreviewWatchEvent {
            id: id.clone(),
            path: path.clone(),
            url,
        };
        let handle = start_watch(&directory, Some(&target_name), Some(path), move || {
            callback(event.clone())
        })?;
        self.watches.insert(id.clone(), handle);
        Ok(id)
    }

    pub fn watch_directory<F>(&mut self, raw_dir: &Path, callback: F) -> Result<String, String>
    where
        F: Fn(PreviewWatchEvent) + Send + Sync + 'static,
    {
        let path = resolve_watch_directory(raw_dir)?;
        let url = Url::from_file_path(&path)
            .map_err(|_| "Could not convert watched directory to a file URL.".to_owned())?
            .to_string();
        let id = new_watch_id();
        let event = PreviewWatchEvent {
            id: id.clone(),
            path: path.clone(),
            url,
        };
        let handle = start_watch(&path, None, None, move || callback(event.clone()))?;
        self.watches.insert(id.clone(), handle);
        Ok(id)
    }

    pub fn stop(&mut self, id: &str) -> bool {
        self.watches.remove(id).is_some()
    }

    pub fn close_all(&mut self) {
        self.watches.clear();
    }

    pub fn active_count(&self) -> usize {
        self.watches.len()
    }
}

fn new_watch_id() -> String {
    Uuid::new_v4().simple().to_string()
}

fn resolve_watch_directory(raw_dir: &Path) -> Result<PathBuf, String> {
    if raw_dir.as_os_str().is_empty() {
        return Err("Watch directory is required.".to_owned());
    }
    let raw = raw_dir.to_string_lossy();
    reject_device_path(&raw)?;
    let absolute = if raw_dir.is_absolute() {
        lexical_normalize(raw_dir)
    } else {
        lexical_normalize(
            &std::env::current_dir()
                .map_err(|error| format!("Could not resolve watch directory: {error}"))?
                .join(raw_dir),
        )
    };
    if !absolute.is_dir() {
        return Err(format!("Not a directory: {}", absolute.display()));
    }
    let canonical = normalize_canonical_path(
        absolute
            .canonicalize()
            .map_err(|error| format!("Could not resolve watch directory: {error}"))?,
    );
    reject_device_path(&canonical.to_string_lossy())?;
    Ok(canonical)
}

fn lexical_normalize(path: &Path) -> PathBuf {
    let mut output = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !matches!(output.components().next_back(), Some(Component::RootDir)) {
                    output.pop();
                }
            }
            _ => output.push(component.as_os_str()),
        }
    }
    output
}

fn reject_device_path(value: &str) -> Result<(), String> {
    if value.contains('\0') {
        return Err("Watch directory contains a NUL character.".to_owned());
    }
    let normalized = value.trim().replace('\\', "/").to_ascii_lowercase();
    if normalized.starts_with("//?/")
        || normalized.starts_with("//./")
        || normalized.starts_with("globalroot/device/")
        || normalized.contains("/globalroot/device/")
    {
        return Err("Windows device paths are not allowed for watchers.".to_owned());
    }
    Ok(())
}

fn normalize_canonical_path(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        let value = path.to_string_lossy();
        if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{rest}"));
        }
        if let Some(rest) = value.strip_prefix(r"\\?\") {
            return PathBuf::from(rest);
        }
    }
    path
}

#[cfg(windows)]
struct WatchHandle {
    child: Child,
    reader: Option<thread::JoinHandle<()>>,
    debouncer: Option<thread::JoinHandle<()>>,
}

#[cfg(not(windows))]
struct WatchHandle;

#[cfg(windows)]
impl Drop for WatchHandle {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
        if let Some(debouncer) = self.debouncer.take() {
            let _ = debouncer.join();
        }
    }
}

fn start_watch<F>(
    directory: &Path,
    target_name: Option<&str>,
    existence_target: Option<PathBuf>,
    callback: F,
) -> Result<WatchHandle, String>
where
    F: Fn() + Send + Sync + 'static,
{
    #[cfg(windows)]
    {
        start_windows_watch(directory, target_name, existence_target, callback)
    }

    #[cfg(not(windows))]
    {
        let _ = (directory, target_name, existence_target, callback);
        Err("Native preview watchers are only implemented for Windows Desktop.".to_owned())
    }
}

#[cfg(windows)]
fn start_windows_watch<F>(
    directory: &Path,
    target_name: Option<&str>,
    existence_target: Option<PathBuf>,
    callback: F,
) -> Result<WatchHandle, String>
where
    F: Fn() + Send + Sync + 'static,
{
    let executable = powershell_executable()?;
    let mut child = Command::new(executable)
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-STA",
            "-WindowStyle",
            "Hidden",
            "-Command",
            WATCH_SCRIPT,
        ])
        .env("HERMES_LOCAL_WATCH_DIR", directory)
        .env("HERMES_LOCAL_WATCH_TARGET", target_name.unwrap_or_default())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("Could not start native preview watcher: {error}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Native preview watcher stdout was unavailable.".to_owned())?;

    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let (signal_tx, signal_rx) = mpsc::channel();
    let reader = thread::Builder::new()
        .name("hermes-preview-watch-reader".to_owned())
        .spawn(move || {
            let mut ready_sent = false;
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                match line.trim() {
                    "ready" if !ready_sent => {
                        ready_sent = true;
                        let _ = ready_tx.send(());
                    }
                    "changed" if ready_sent && signal_tx.send(()).is_err() => {
                        break;
                    }
                    _ => {}
                }
            }
        })
        .map_err(|error| format!("Could not start preview watcher reader: {error}"))?;

    if ready_rx.recv_timeout(HELPER_READY_TIMEOUT).is_err() {
        let _ = child.kill();
        let _ = child.wait();
        let _ = reader.join();
        return Err("Native preview watcher did not become ready.".to_owned());
    }

    let debouncer = spawn_debouncer(signal_rx, existence_target, callback)?;
    Ok(WatchHandle {
        child,
        reader: Some(reader),
        debouncer: Some(debouncer),
    })
}

fn spawn_debouncer<F>(
    receiver: mpsc::Receiver<()>,
    existence_target: Option<PathBuf>,
    callback: F,
) -> Result<thread::JoinHandle<()>, String>
where
    F: Fn() + Send + Sync + 'static,
{
    thread::Builder::new()
        .name("hermes-preview-watch-debounce".to_owned())
        .spawn(move || {
            while receiver.recv().is_ok() {
                loop {
                    match receiver.recv_timeout(WATCH_DEBOUNCE) {
                        Ok(()) => continue,
                        Err(mpsc::RecvTimeoutError::Timeout) => break,
                        Err(mpsc::RecvTimeoutError::Disconnected) => return,
                    }
                }
                if existence_target.as_ref().is_some_and(|path| !path.exists()) {
                    continue;
                }
                callback();
            }
        })
        .map_err(|error| format!("Could not start preview watcher debouncer: {error}"))
}

#[cfg(windows)]
fn powershell_executable() -> Result<PathBuf, String> {
    let root =
        std::env::var_os("SystemRoot").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
    let executable = root
        .join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe");
    if executable.is_absolute() && executable.is_file() {
        Ok(executable)
    } else {
        Err(format!(
            "Windows PowerShell preview watcher is unavailable: {}",
            executable.display()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn directory_resolution_is_absolute_canonical_and_rejects_device_paths() {
        let root = test_directory("resolve");
        let resolved = resolve_watch_directory(&root).expect("directory");
        assert!(resolved.is_absolute());
        assert!(resolved.is_dir());
        assert!(reject_device_path(r"\\?\C:\secret").is_err());
        assert!(reject_device_path(r"\\.\PhysicalDrive0").is_err());
        cleanup(root);
    }

    #[test]
    fn debouncer_coalesces_a_burst_into_one_trailing_callback() {
        let (tx, rx) = mpsc::channel();
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        let worker = spawn_debouncer(rx, None, move || {
            observed.fetch_add(1, Ordering::SeqCst);
        })
        .expect("debouncer");
        tx.send(()).expect("first");
        thread::sleep(Duration::from_millis(30));
        tx.send(()).expect("second");
        thread::sleep(Duration::from_millis(30));
        tx.send(()).expect("third");
        thread::sleep(Duration::from_millis(170));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        drop(tx);
        worker.join().expect("join");
    }

    #[test]
    fn file_debouncer_suppresses_callback_after_target_disappears() {
        let root = test_directory("missing");
        let file = root.join("preview.txt");
        fs::write(&file, "hello").expect("fixture");
        let (tx, rx) = mpsc::channel();
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        let worker = spawn_debouncer(rx, Some(file.clone()), move || {
            observed.fetch_add(1, Ordering::SeqCst);
        })
        .expect("debouncer");
        tx.send(()).expect("signal");
        fs::remove_file(&file).expect("remove");
        thread::sleep(Duration::from_millis(170));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        drop(tx);
        worker.join().expect("join");
        cleanup(root);
    }

    #[cfg(windows)]
    #[test]
    fn watcher_helper_uses_trusted_absolute_powershell() {
        let executable = powershell_executable().expect("Windows PowerShell");
        assert!(executable.is_absolute());
        assert!(executable.is_file());
    }

    fn test_directory(label: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "hermes-preview-watch-{label}-{}-{suffix}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("test directory");
        path
    }

    fn cleanup(path: PathBuf) {
        let _ = fs::remove_dir_all(path);
    }
}
