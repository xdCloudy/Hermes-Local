use std::{fs, path::PathBuf};

fn read(relative: &str) -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    fs::read_to_string(root.join(relative)).expect("contract source")
}

#[test]
fn skills_hub_surface_uses_typed_native_authority_and_live_action_reconciliation() {
    let ui = read("../hermes-ui/src/skills.rs");
    let core = read("../hermes-core/src/lib.rs");
    let desktop = read("src/lib.rs");

    assert!(core.contains("pub trait SkillsService"));
    assert!(core.contains("pub skills: Arc<dyn SkillsService>"));
    assert!(desktop.contains("impl SkillsService for GatewayServices"));
    assert!(desktop.contains("request_bounded"));
    assert!(desktop.contains("MAX_SKILLS_RESPONSE_BYTES"));
    assert!(ui.contains("services.skills.clone()"));
    assert!(ui.contains("hub_search"));
    assert!(ui.contains("hub_preview"));
    assert!(ui.contains("hub_scan"));
    assert!(ui.contains("action_status"));
    assert!(ui.contains("Duration::from_millis(1200)"));
    assert!(ui.contains("action_epoch"));
    assert!(ui.contains("installed_overrides"));
}
