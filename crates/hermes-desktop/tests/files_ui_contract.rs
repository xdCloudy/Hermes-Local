use std::{fs, path::PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read_repo_file(relative: &str) -> String {
    let path = repo_root().join(relative);
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

#[test]
fn files_surface_stays_on_the_typed_file_service_boundary() {
    let core = read_repo_file("crates/hermes-core/src/lib.rs");
    let ui = read_repo_file("crates/hermes-ui/src/files.rs");

    for contract in [
        "fn read_dir(&self, root: &Path, relative: &Path) -> ServiceFuture<'_, Vec<FileEntry>>",
        "fn read_text(&self, root: &Path, relative: &Path) -> ServiceFuture<'_, String>",
        "fn write_text(&self, root: &Path, relative: &Path, content: &str) -> ServiceFuture<'_, ()>",
    ] {
        assert!(
            core.contains(contract),
            "FileService contract disappeared: {contract}"
        );
    }

    for call in [
        ".read_dir(Path::new(&root), Path::new(&directory))",
        ".read_text(Path::new(&root), Path::new(&path))",
        ".write_text(Path::new(&root), Path::new(&path), &content)",
    ] {
        assert!(
            ui.contains(call),
            "Files surface stopped using typed FileService call: {call}"
        );
    }

    assert!(
        !ui.contains("std::fs") && !ui.contains("tokio::fs") && !ui.contains("Command::"),
        "hermes-ui must not gain filesystem or process authority"
    );
}

#[test]
fn files_surface_keeps_editor_and_empty_state_contracts() {
    let ui = read_repo_file("crates/hermes-ui/src/files.rs");

    for required in [
        "Select a project first",
        "This folder is empty.",
        "Loading files…",
        "Loading file…",
        "Open a text file from the tree.",
        "Select a UTF-8 text file to edit.",
        "dirty.set(true);",
        "dirty.set(false);",
        "Saved.",
    ] {
        assert!(
            ui.contains(required),
            "PF-05 UI-state regression: {required}"
        );
    }
}

#[test]
fn native_files_keep_containment_helpers_on_every_mutating_boundary() {
    let desktop = read_repo_file("crates/hermes-desktop/src/lib.rs");

    assert!(
        desktop.contains("let target = contained_existing(&root, &relative)?;"),
        "file reads must remain contained to an existing project root"
    );
    assert!(
        desktop.contains("let target = contained_for_write(&root, &relative)?;"),
        "file writes must remain contained to the selected project root"
    );
    assert!(
        desktop.contains("validate_relative_path(relative)?"),
        "relative-path validation must remain part of containment"
    );
}

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
