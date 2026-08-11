use std::path::PathBuf;

use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
use dioxus::prelude::*;
use hermes_desktop::NativeApp;

fn desktop_root() -> Element {
    let desktop = dioxus::desktop::window();
    use_context_provider(move || hermes_ui::WindowActions {
        drag: {
            let desktop = desktop.clone();
            Callback::new(move |()| desktop.drag())
        },
        minimize: {
            let window = desktop.window.clone();
            Callback::new(move |()| window.set_minimized(true))
        },
        toggle_maximized: {
            let desktop = desktop.clone();
            Callback::new(move |()| desktop.toggle_maximized())
        },
        close: Callback::new(move |()| desktop.close()),
    });
    rsx! { hermes_ui::App {} }
}

fn main() {
    let data_dir = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("Hermes Local");
    let native = NativeApp::new(data_dir);
    let window = WindowBuilder::new()
        .with_title("Hermes Local")
        .with_inner_size(LogicalSize::new(1_280.0, 800.0))
        .with_min_inner_size(LogicalSize::new(760.0, 560.0))
        .with_decorations(false);
    let config = Config::new()
        .with_window(window)
        .with_menu(None)
        .with_background_color((9, 11, 16, 255))
        .with_disable_context_menu(true)
        .with_navigation_handler(|_| false);
    dioxus::LaunchBuilder::desktop()
        .with_cfg(config)
        .with_context(native.services)
        .launch(desktop_root);
}
