use std::{fs, path::PathBuf};

fn workspace_file(relative: &str) -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    fs::read_to_string(root.join(relative))
        .expect("workspace source")
        .replace("\r\n", "\n")
}

#[test]
fn artifacts_route_uses_bounded_history_and_typed_safe_preview_actions() {
    let ui = workspace_file("crates/hermes-ui/src/artifacts.rs");
    let core = workspace_file("crates/hermes-core/src/lib.rs");
    let desktop = workspace_file("apps/desktop/src/preview_service.rs");

    assert!(ui.contains("const MAX_SESSIONS: usize = 30"));
    assert!(ui.contains("const MAX_MESSAGES_PER_SESSION: usize = 2_000"));
    assert!(ui.contains("buffer_unordered(HISTORY_CONCURRENCY)"));
    assert!(ui.contains(".preview\n                .load("));
    assert!(ui.contains(".preview\n                .open("));
    assert!(core.contains("fn open(&self, _raw_target: &str, _base_dir: Option<&Path>)"));
    assert!(desktop.contains("PreviewNormalizationService"));
    assert!(desktop.contains("fn open("));
    assert!(!ui.contains("std::fs"));
    assert!(!ui.contains("std::process"));
}
