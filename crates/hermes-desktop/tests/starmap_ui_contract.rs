use std::{fs, path::PathBuf};

fn workspace_file(relative: &str) -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    fs::read_to_string(root.join(relative))
        .expect("workspace source")
        .replace("\r\n", "\n")
}

#[test]
fn starmap_uses_profile_scoped_bounded_native_graph_boundary() {
    let core = workspace_file("crates/hermes-core/src/lib.rs");
    let desktop = workspace_file("crates/hermes-desktop/src/lib.rs");
    let ui = workspace_file("crates/hermes-ui/src/starmap.rs");

    assert!(core.contains("pub trait LearningService: Send + Sync"));
    assert!(desktop.contains("profiled_path(\"/api/learning/graph\""));
    assert!(desktop.contains("MAX_LEARNING_RESPONSE_BYTES"));
    assert!(desktop.contains("MAX_LEARNING_NODES"));
    assert!(ui.contains("const MAX_RENDERED_NODES: usize = 300"));
    assert!(ui.contains("service.graph(profile.as_deref()).await"));
    assert!(ui.contains("Search Starmap nodes"));
    assert!(!ui.contains("reqwest"));
    assert!(!ui.contains("std::fs"));
}
