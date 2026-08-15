use std::{fs, path::PathBuf};

#[test]
fn runtime_surfaces_use_typed_service_and_live_task_reconciliation() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let ui = fs::read_to_string(root.join("../hermes-ui/src/runtime.rs"))
        .expect("runtime surface source");
    let core =
        fs::read_to_string(root.join("../hermes-core/src/lib.rs")).expect("core service source");

    assert!(core.contains("pub trait RuntimeService"));
    assert!(core.contains("pub runtime: Arc<dyn RuntimeService>"));
    assert!(ui.contains("services.runtime.status()"));
    assert!(ui.contains("services.runtime.actions()"));
    assert!(ui.contains("services.runtime.start_action"));
    assert!(ui.contains("services.runtime.cancel_action"));
    assert!(ui.contains("Duration::from_millis(1500)"));
    assert!(ui.contains("Task Centre"));
}
