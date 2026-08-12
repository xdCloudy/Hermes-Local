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
fn safe_preview_stays_behind_a_typed_native_service() {
    let core = read_repo_file("crates/hermes-core/src/lib.rs");
    let desktop = read_repo_file("apps/desktop/src/preview_service.rs");
    let main = read_repo_file("apps/desktop/src/main.rs");
    let ui = read_repo_file("crates/hermes-ui/src/files.rs");

    assert!(core.contains("pub trait PreviewService: Send + Sync"));
    assert!(core.contains("pub preview: Arc<dyn PreviewService>"));
    assert!(desktop.contains("impl PreviewService for DesktopPreviewService"));
    assert!(desktop.contains("read_bounded_text(&path)"));
    assert!(main.contains("preview_service::install(&mut native.services);"));
    assert!(ui.contains("service.load(&target, base.as_deref().map(Path::new)).await"));
    assert!(
        !ui.contains("std::fs") && !ui.contains("File::open") && !ui.contains("canonicalize()")
    );
}

#[test]
fn remote_preview_is_sandboxed_and_local_html_is_never_injected() {
    let ui = read_repo_file("crates/hermes-ui/src/files.rs");
    assert!(ui.contains("\"sandbox\": \"allow-scripts\""));
    assert!(!ui.contains("allow-top-navigation"));
    assert!(!ui.contains("allow-popups"));
    assert!(!ui.contains("allow-forms"));
    assert!(!ui.contains("allow-same-origin"));
    assert!(!ui.contains("dangerous_inner_html"));
    assert!(ui.contains("PreviewDocumentKind::Html | PreviewDocumentKind::Text"));
}

#[test]
fn preview_contract_keeps_size_and_sensitive_path_guards() {
    let normalization = read_repo_file("apps/desktop/src/preview_normalization.rs");
    let desktop = read_repo_file("apps/desktop/src/preview_service.rs");
    assert!(normalization.contains("TEXT_PREVIEW_MAX_BYTES: u64 = 512 * 1024"));
    assert!(normalization.contains("reject_sensitive_file_path(&real_path)?"));
    assert!(normalization.contains("Preview URLs with embedded credentials are not allowed."));
    assert!(desktop.contains("TEXT_PREVIEW_MAX_BYTES + 1"));
    assert!(desktop.contains("grew beyond the 512 KiB inline limit"));
}
