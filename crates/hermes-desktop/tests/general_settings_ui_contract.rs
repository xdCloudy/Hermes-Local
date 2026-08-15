use std::{fs, path::PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn general_settings_use_a_typed_native_boundary_and_persisted_preferences() {
    let root = root();
    let core = fs::read_to_string(root.join("crates/hermes-core/src/lib.rs")).expect("core");
    let ui = fs::read_to_string(root.join("crates/hermes-ui/src/general.rs")).expect("UI");
    let native = fs::read_to_string(root.join("apps/desktop/src/general_settings.rs"))
        .expect("native settings");
    let main = fs::read_to_string(root.join("apps/desktop/src/main.rs")).expect("main");

    assert!(core.contains("pub trait DesktopSettingsService"));
    assert!(core.contains("pub desktop_settings: Arc<dyn DesktopSettingsService>"));
    assert!(ui.contains("set_keep_awake"));
    assert!(ui.contains("set_launch_at_login"));
    assert!(ui.contains("services.settings.save"));
    assert!(ui.contains("set_keep_awake(!enabled)"));
    assert!(native.contains("tokio::task::spawn_blocking"));
    assert!(native.contains("GetSystemPowerStatus"));
    assert!(native.contains("MAX_SETTINGS_BYTES"));
    assert!(main.contains("general_settings::install"));
}
