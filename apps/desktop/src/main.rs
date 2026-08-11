#![cfg_attr(feature = "bundle", windows_subsystem = "windows")]

#[path = "base64.rs"]
mod base64_impl;
pub use base64_impl::{Engine, engine};
extern crate self as base64;

mod login_item;
mod ssh;
mod ssh_lifecycle;
mod ssh_service;
#[cfg(windows)]
mod ssh_windows;
mod startup;

use std::path::PathBuf;

use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
use dioxus::prelude::*;
use hermes_core::AppServices;
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

    let services = use_context::<AppServices>();
    let startup_services = services.clone();
    let mut startup_attempt = use_signal(|| 0_u64);
    let local_startup = use_resource(move || {
        let _ = startup_attempt();
        let services = startup_services.clone();
        async move { startup::prepare_local_agent(&services).await }
    });

    match &*local_startup.read() {
        None => rsx! {
            div {
                style: "height:100vh;display:grid;place-items:center;background:rgb(9 11 16);color:rgb(229 231 235);font:13px system-ui,sans-serif;",
                div { style: "display:grid;gap:8px;text-align:center;",
                    strong { "Starting Hermes Local" }
                    span { style: "color:rgb(148 163 184);", "Preparing the local Agent runtime…" }
                }
            }
        },
        Some(Ok(())) => rsx! { hermes_ui::App {} },
        Some(Err(error)) => rsx! {
            div {
                style: "height:100vh;display:grid;place-items:center;background:rgb(9 11 16);color:rgb(229 231 235);font:13px system-ui,sans-serif;padding:32px;box-sizing:border-box;",
                div { style: "display:grid;gap:10px;max-width:680px;",
                    strong { "Hermes Local could not start" }
                    p { style: "margin:0;color:rgb(203 213 225);line-height:1.5;", "{error}" }
                    p { style: "margin:0;color:rgb(148 163 184);font-size:12px;", "Fix the runtime problem, then retry without losing this window." }
                    div { style: "display:flex;gap:8px;margin-top:4px;",
                        button {
                            style: "border:1px solid rgb(71 85 105);border-radius:4px;background:rgb(30 41 59);color:rgb(241 245 249);padding:6px 12px;font:inherit;cursor:pointer;",
                            onclick: move |_| startup_attempt += 1,
                            "Retry"
                        }
                    }
                }
            }
        },
    }
}

fn main() {
    let data_dir = std::env::var_os("APPDATA")
        .map_or_else(std::env::temp_dir, PathBuf::from)
        .join("Hermes Local");
    let mut native = NativeApp::new(data_dir.clone());
    startup::install_local_bootstrap(&mut native.services);
    ssh_service::install_ssh_probe(&mut native.services, data_dir);
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
