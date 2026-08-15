use std::{fs, path::PathBuf};

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read_source(path: PathBuf) -> String {
    fs::read_to_string(path)
        .expect("source file")
        .replace("\r\n", "\n")
}

#[test]
fn persisted_task_results_cross_a_bounded_native_export_boundary() {
    let root = repository_root();
    let protocol = read_source(root.join("crates/hermes-protocol/src/lib.rs"));
    let desktop = read_source(root.join("crates/hermes-desktop/src/lib.rs"));
    let runtime = read_source(root.join("crates/hermes-ui/src/runtime.rs"));

    for field in [
        "pub stage: Option<String>",
        "pub output: String",
        "pub completed_at: Option<String>",
        "pub failure: Option<RuntimeTaskFailure>",
        "pub result: Option<RuntimeTaskResult>",
    ] {
        assert!(
            protocol.contains(field),
            "missing persisted task field {field}"
        );
    }

    assert!(desktop.contains("MAX_RUNTIME_TASKS"));
    assert!(desktop.contains("MAX_RUNTIME_TASK_OUTPUT_BYTES"));
    assert!(desktop.contains("bound_runtime_task"));
    assert!(runtime.contains("pick_folder(\"Export task report\", None)"));
    assert!(runtime.contains(".write_text(&folder, &file_name, &content)"));
    assert!(runtime.contains("serde_json::to_string_pretty(&task)"));
    assert!(!runtime.contains("open_external(&result.path"));
}
