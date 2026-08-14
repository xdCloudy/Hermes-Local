use std::{fs, path::PathBuf};

#[test]
fn logs_surface_uses_only_the_typed_sanitized_diagnostics_boundary() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let ui = fs::read_to_string(root.join("../../crates/hermes-ui/src/logs.rs"))
        .expect("Logs surface source");
    let core = fs::read_to_string(root.join("../../crates/hermes-core/src/lib.rs"))
        .expect("core service source");
    let desktop = fs::read_to_string(root.join("src/diagnostics_export.rs"))
        .expect("Desktop diagnostics source");

    assert!(core.contains("pub trait DiagnosticsService"));
    assert!(core.contains("pub diagnostics: Arc<dyn DiagnosticsService>"));
    assert!(ui.contains("services.diagnostics.snapshot().await"));
    assert!(ui.contains("services.diagnostics.export().await"));
    assert!(ui.contains("services.diagnostics.clear_crash().await"));
    assert!(ui.contains("services.diagnostics.open_environment_settings().await"));
    assert!(ui.contains("current.environment.path_entry_count"));
    assert!(!ui.contains("std::fs"));
    assert!(desktop.contains("read_safe_log_tail"));
    assert!(desktop.contains("DiagnosticsExportService"));
    assert!(desktop.contains("clear_crash_record"));
    assert!(desktop.contains("SystemPropertiesAdvanced.exe"));
}
