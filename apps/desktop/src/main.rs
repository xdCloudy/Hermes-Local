use std::path::PathBuf;

use hermes_desktop::NativeApp;

fn main() {
    let data_dir = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("Hermes Local");
    let native = NativeApp::new(data_dir);
    dioxus::LaunchBuilder::desktop()
        .with_context(native.services)
        .launch(hermes_ui::App);
}
