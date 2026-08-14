use std::{fs, path::PathBuf};

fn workspace_file(relative: &str) -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    fs::read_to_string(root.join(relative))
        .expect("workspace source")
        .replace("\r\n", "\n")
}

#[test]
fn mcp_surface_uses_typed_profile_scoped_native_operations() {
    let core = workspace_file("crates/hermes-core/src/lib.rs");
    let desktop = workspace_file("crates/hermes-desktop/src/lib.rs");
    let ui = workspace_file("crates/hermes-ui/src/mcp.rs");

    assert!(core.contains("pub trait McpService: Send + Sync"));
    assert!(desktop.contains("profiled_path(\"/api/mcp/servers\""));
    assert!(desktop.contains("MAX_MCP_RESPONSE_BYTES"));
    assert!(desktop.contains("upsert_mcp_server_map"));
    assert!(desktop.contains("value.is_empty()"));
    assert!(ui.contains("Stored environment secrets are never read back"));
    assert!(ui.contains("settings_signal().profile != profile"));
    assert!(ui.contains("I reviewed this provider and trust its source"));
    assert!(ui.contains("service.test(profile.as_deref(), &name).await"));
    assert!(ui.contains("service.install_catalog(profile.as_deref(), &name, &values).await"));
    assert!(ui.contains("but live sessions were not reloaded"));
    assert!(!ui.contains("reqwest"));
    assert!(!ui.contains("std::process"));
    assert!(!ui.contains("/api/mcp"));
}
