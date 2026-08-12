from pathlib import Path


def replace_once(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one match, found {count}: {old[:120]!r}")
    path.write_text(text.replace(old, new), encoding="utf-8")


core = Path("crates/hermes-core/src/lib.rs")
replace_once(
    core,
    "pub type EventStream = Pin<Box<dyn Stream<Item = GatewayEvent> + Send>>;\npub type ServiceResult<T> = Result<T, ServiceError>;",
    "pub type EventStream = Pin<Box<dyn Stream<Item = GatewayEvent> + Send>>;\npub type FileWatchStream = Pin<Box<dyn Stream<Item = FileWatchEvent> + Send>>;\npub type ServiceResult<T> = Result<T, ServiceError>;\n\n#[derive(Clone, Debug, Eq, PartialEq)]\npub struct FileWatchEvent {\n    pub path: PathBuf,\n}",
)
replace_once(
    core,
    "    fn trash(&self, root: &Path, relative: &Path) -> ServiceFuture<'_, ()>;\n}",
    "    fn trash(&self, root: &Path, relative: &Path) -> ServiceFuture<'_, ()>;\n    fn watch_directory(&self, _root: &Path, _relative: &Path) -> ServiceResult<FileWatchStream> {\n        Err(ServiceError::Unavailable(\n            \"directory watching is unavailable on this platform\".into(),\n        ))\n    }\n}",
)

cargo = Path("apps/desktop/Cargo.toml")
replace_once(
    cargo,
    '[dependencies]\ndioxus = { workspace = true, features = ["desktop", "launch"] }',
    '[dependencies]\nasync-stream.workspace = true\ndioxus = { workspace = true, features = ["desktop", "launch"] }',
)
replace_once(
    cargo,
    'tokio = { workspace = true, features = ["io-util", "process", "rt", "time"] }',
    'tokio = { workspace = true, features = ["io-util", "process", "rt", "sync", "time"] }',
)

watcher = Path("apps/desktop/src/preview_watcher.rs")
replace_once(
    watcher,
    "    sync::mpsc,\n    thread,",
    "    sync::{Arc, Mutex, mpsc},\n    thread,",
)
replace_once(
    watcher,
    "use url::Url;\nuse uuid::Uuid;\n\nuse crate::preview_normalization::{PreviewNormalizationService, PreviewTarget};",
    "use hermes_core::{\n    AppServices, FileService, FileWatchEvent, FileWatchStream, ServiceError, ServiceFuture,\n    ServiceResult, validate_relative_path,\n};\nuse hermes_protocol::FileEntry;\nuse tokio::sync::mpsc as tokio_mpsc;\nuse url::Url;\nuse uuid::Uuid;\n\nuse crate::preview_normalization::{PreviewNormalizationService, PreviewTarget};",
)
insert = r'''

struct WatchedFileService {
    inner: Arc<dyn FileService>,
    registry: Arc<Mutex<PreviewWatcherRegistry>>,
}

struct WatchLease {
    registry: Arc<Mutex<PreviewWatcherRegistry>>,
    id: String,
}

impl Drop for WatchLease {
    fn drop(&mut self) {
        if let Ok(mut registry) = self.registry.lock() {
            registry.stop(&self.id);
        }
    }
}

impl FileService for WatchedFileService {
    fn read_dir(&self, root: &Path, relative: &Path) -> ServiceFuture<'_, Vec<FileEntry>> {
        self.inner.read_dir(root, relative)
    }

    fn read_text(&self, root: &Path, relative: &Path) -> ServiceFuture<'_, String> {
        self.inner.read_text(root, relative)
    }

    fn write_text(&self, root: &Path, relative: &Path, content: &str) -> ServiceFuture<'_, ()> {
        self.inner.write_text(root, relative, content)
    }

    fn rename(&self, root: &Path, relative: &Path, new_name: &str) -> ServiceFuture<'_, String> {
        self.inner.rename(root, relative, new_name)
    }

    fn reveal(&self, root: &Path, relative: &Path) -> ServiceFuture<'_, ()> {
        self.inner.reveal(root, relative)
    }

    fn open(&self, root: &Path, relative: &Path) -> ServiceFuture<'_, ()> {
        self.inner.open(root, relative)
    }

    fn trash(&self, root: &Path, relative: &Path) -> ServiceFuture<'_, ()> {
        self.inner.trash(root, relative)
    }

    fn watch_directory(&self, root: &Path, relative: &Path) -> ServiceResult<FileWatchStream> {
        let target = contained_watch_directory(root, relative)?;
        let (sender, mut receiver) = tokio_mpsc::unbounded_channel();
        let id = self
            .registry
            .lock()
            .map_err(|_| ServiceError::Platform("preview watcher registry lock was poisoned".into()))?
            .watch_directory(&target, move |event| {
                let _ = sender.send(FileWatchEvent { path: event.path });
            })
            .map_err(ServiceError::Platform)?;
        let lease = WatchLease {
            registry: self.registry.clone(),
            id,
        };
        Ok(Box::pin(async_stream::stream! {
            let lease = lease;
            while let Some(event) = receiver.recv().await {
                yield event;
            }
            drop(lease);
        }))
    }
}

fn contained_watch_directory(root: &Path, relative: &Path) -> ServiceResult<PathBuf> {
    validate_relative_path(relative)?;
    let canonical_root = root
        .canonicalize()
        .map_err(|error| ServiceError::Platform(format!("could not resolve watched root: {error}")))?;
    let target = canonical_root
        .join(relative)
        .canonicalize()
        .map_err(|error| ServiceError::Platform(format!("could not resolve watched directory: {error}")))?;
    if !target.starts_with(&canonical_root) {
        return Err(ServiceError::PermissionDenied(
            "watched directory escapes the selected project root".into(),
        ));
    }
    if !target.is_dir() {
        return Err(ServiceError::InvalidInput(
            "watched target must be a directory".into(),
        ));
    }
    Ok(target)
}

pub fn install(services: &mut AppServices) {
    let inner = services.files.clone();
    services.files = Arc::new(WatchedFileService {
        inner,
        registry: Arc::new(Mutex::new(PreviewWatcherRegistry::new())),
    });
}
'''
text = watcher.read_text(encoding="utf-8")
marker = "\nfn new_watch_id() -> String {"
if text.count(marker) != 1:
    raise SystemExit("preview watcher insertion marker changed")
watcher.write_text(text.replace(marker, insert + marker), encoding="utf-8")

main = Path("apps/desktop/src/main.rs")
replace_once(
    main,
    "    notification_service::install(&mut native.services);\n    startup::install_local_bootstrap(&mut native.services);",
    "    notification_service::install(&mut native.services);\n    preview_watcher::install(&mut native.services);\n    startup::install_local_bootstrap(&mut native.services);",
)

files = Path("crates/hermes-ui/src/files.rs")
replace_once(
    files,
    "use dioxus::prelude::*;\nuse hermes_core::AppServices;",
    "use dioxus::prelude::*;\nuse futures_util::StreamExt;\nuse hermes_core::{AppServices, ServiceError};",
)
watch_resource = r'''

    let watch_service = services.files.clone();
    let watch_snapshot = project_state.snapshot;
    let _watching = use_resource(move || {
        let snapshot = watch_snapshot();
        let root = active_project_root(&snapshot).map(|(_, root)| root);
        let directory = current_dir();
        let service = watch_service.clone();
        async move {
            let Some(root) = root else {
                return;
            };
            match service.watch_directory(Path::new(&root), Path::new(&directory)) {
                Ok(mut events) => {
                    while events.next().await.is_some() {
                        refresh.set(refresh() + 1);
                    }
                }
                Err(ServiceError::Unavailable(_)) => {}
                Err(next_error) => error.set(Some(next_error.to_string())),
            }
        }
    });
'''
text = files.read_text(encoding="utf-8")
marker = "\n    let snapshot = (project_state.snapshot)();"
if text.count(marker) != 1:
    raise SystemExit("Files watcher insertion marker changed")
files.write_text(text.replace(marker, watch_resource + marker), encoding="utf-8")

contract = Path("crates/hermes-desktop/tests/files_ui_contract.rs")
text = contract.read_text(encoding="utf-8")
addition = r'''

#[test]
fn files_surface_consumes_the_typed_directory_watch_stream() {
    let core = read_repo_file("crates/hermes-core/src/lib.rs");
    let desktop = read_repo_file("apps/desktop/src/preview_watcher.rs");
    let main = read_repo_file("apps/desktop/src/main.rs");
    let ui = read_repo_file("crates/hermes-ui/src/files.rs");

    assert!(
        core.contains("fn watch_directory(&self, _root: &Path, _relative: &Path) -> ServiceResult<FileWatchStream>"),
        "FileService must keep a typed directory-watch stream contract"
    );
    assert!(
        desktop.contains("impl FileService for WatchedFileService")
            && desktop.contains("struct WatchLease")
            && desktop.contains("registry.stop(&self.id)"),
        "Desktop watcher adapter must own native watcher lifecycle and disposal"
    );
    assert!(
        main.contains("preview_watcher::install(&mut native.services);"),
        "Desktop composition root must install the native watcher adapter"
    );
    assert!(
        ui.contains(".watch_directory(Path::new(&root), Path::new(&directory))")
            && ui.contains("while events.next().await.is_some()")
            && ui.contains("refresh.set(refresh() + 1);"),
        "Files must refresh from the typed directory-watch stream"
    );
    assert!(
        !ui.contains("FileSystemWatcher") && !ui.contains("powershell.exe"),
        "hermes-ui must not acquire native watcher/process authority"
    );
}
'''
contract.write_text(text.rstrip() + addition + "\n", encoding="utf-8")

text = watcher.read_text(encoding="utf-8")
test_marker = "    #[test]\n    fn debouncer_coalesces_a_burst_into_one_trailing_callback() {"
focused_tests = r'''    #[test]
    fn project_scoped_watch_directory_rejects_parent_escape() {
        let root = test_directory("contained");
        let nested = root.join("nested");
        fs::create_dir_all(&nested).expect("nested directory");
        assert_eq!(
            contained_watch_directory(&root, Path::new("nested")).expect("contained directory"),
            nested.canonicalize().expect("canonical nested")
        );
        assert!(contained_watch_directory(&root, Path::new("../")).is_err());
        cleanup(root);
    }

    #[cfg(windows)]
    #[test]
    fn dropping_watch_lease_releases_native_registry_entry() {
        let root = test_directory("lease");
        let registry = Arc::new(Mutex::new(PreviewWatcherRegistry::new()));
        let id = registry
            .lock()
            .expect("registry")
            .watch_directory(&root, |_| {})
            .expect("start watcher");
        assert_eq!(registry.lock().expect("registry").active_count(), 1);
        drop(WatchLease {
            registry: registry.clone(),
            id,
        });
        assert_eq!(registry.lock().expect("registry").active_count(), 0);
        cleanup(root);
    }

'''
if text.count(test_marker) != 1:
    raise SystemExit("watcher test insertion marker changed")
watcher.write_text(text.replace(test_marker, focused_tests + test_marker), encoding="utf-8")
