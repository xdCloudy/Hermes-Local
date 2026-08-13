use std::{fs, path::PathBuf};

#[test]
fn about_surface_uses_typed_native_identity_without_rendering_raw_update_state() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let ui =
        fs::read_to_string(root.join("../hermes-ui/src/about.rs")).expect("about surface source");

    assert!(ui.contains("services.platform.version()"));
    assert!(ui.contains("services.runtime.status()"));
    assert!(ui.contains("services.updates.check()"));
    assert!(ui.contains(".get(\"status\")"));
    assert!(!ui.contains("{value}"));
    assert!(ui.contains("generated SBOM, checksums and attestation bundles"));
}
