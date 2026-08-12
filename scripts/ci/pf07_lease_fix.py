from pathlib import Path

path = Path("apps/desktop/src/preview_watcher.rs")
text = path.read_text(encoding="utf-8")

text = text.replace(
    '#![allow(dead_code)] // PF-07 service foundation; Dioxus watcher consumers are a later stage.\n\n',
    '',
    1,
)

old = '''struct WatchLease {
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
'''
new = '''struct WatchLease {
    cleanup: Option<Box<dyn FnOnce() + Send + 'static>>,
}

impl WatchLease {
    fn new(registry: Arc<Mutex<PreviewWatcherRegistry>>, id: String) -> Self {
        Self::with_cleanup(move || {
            if let Ok(mut registry) = registry.lock() {
                registry.stop(&id);
            }
        })
    }

    fn with_cleanup<F>(cleanup: F) -> Self
    where
        F: FnOnce() + Send + 'static,
    {
        Self {
            cleanup: Some(Box::new(cleanup)),
        }
    }
}

impl Drop for WatchLease {
    fn drop(&mut self) {
        if let Some(cleanup) = self.cleanup.take() {
            cleanup();
        }
    }
}
'''
if text.count(old) != 1:
    raise SystemExit("WatchLease block changed")
text = text.replace(old, new)

old = '''        let lease = WatchLease {
            registry: self.registry.clone(),
            id,
        };'''
new = '''        let lease = WatchLease::new(self.registry.clone(), id);'''
if text.count(old) != 1:
    raise SystemExit("WatchLease construction changed")
text = text.replace(old, new)

old = '''    #[cfg(windows)]
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
new = '''    #[test]
    fn dropping_watch_lease_runs_cleanup_without_native_helper_startup() {
        use std::sync::atomic::AtomicBool;

        let cleaned = Arc::new(AtomicBool::new(false));
        let observed = cleaned.clone();
        drop(WatchLease::with_cleanup(move || {
            observed.store(true, Ordering::SeqCst);
        }));
        assert!(cleaned.load(Ordering::SeqCst));
    }
'''
if text.count(old) != 1:
    raise SystemExit("flaky WatchLease test changed")
text = text.replace(old, new)

path.write_text(text, encoding="utf-8")

contract = Path("crates/hermes-desktop/tests/files_ui_contract.rs")
contract_text = contract.read_text(encoding="utf-8")
old_contract = '''        desktop.contains("impl FileService for WatchedFileService")
            && desktop.contains("struct WatchLease")
            && desktop.contains("registry.stop(&self.id)"),'''
new_contract = '''        desktop.contains("impl FileService for WatchedFileService")
            && desktop.contains("struct WatchLease")
            && desktop.contains("WatchLease::new(self.registry.clone(), id)")
            && desktop.contains("registry.stop(&id)"),'''
if contract_text.count(old_contract) != 1:
    raise SystemExit("PF-07 Files watcher lifecycle contract changed")
contract.write_text(contract_text.replace(old_contract, new_contract), encoding="utf-8")
