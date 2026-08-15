use std::{fs, path::PathBuf};

#[test]
fn memory_surface_uses_typed_profile_bound_service_and_confirmed_reset() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let ui =
        fs::read_to_string(root.join("../hermes-ui/src/memory.rs")).expect("Memory surface source");
    let core =
        fs::read_to_string(root.join("../hermes-core/src/lib.rs")).expect("core service source");
    let desktop = fs::read_to_string(root.join("src/lib.rs")).expect("Desktop service source");

    assert!(core.contains("pub trait MemoryService"));
    assert!(core.contains("pub memory: Arc<dyn MemoryService>"));
    assert!(ui.contains("services.memory.status().await"));
    assert!(ui.contains("services.memory.curator_status().await"));
    assert!(ui.contains("services.memory.set_curator_paused"));
    assert!(ui.contains("services.memory.run_curator().await"));
    assert!(ui.contains("services.memory.reset(target).await"));
    assert!(ui.contains("Confirm reset"));
    for route in [
        "\"/api/memory\"",
        "\"/api/memory/reset\"",
        "\"/api/curator\"",
        "\"/api/curator/paused\"",
        "\"/api/curator/run\"",
    ] {
        assert!(desktop.contains(route), "missing typed route {route}");
    }
    assert!(desktop.contains("MAX_MEMORY_RESPONSE_BYTES"));
    assert!(desktop.contains("MEMORY_REQUEST_TIMEOUT"));
}
