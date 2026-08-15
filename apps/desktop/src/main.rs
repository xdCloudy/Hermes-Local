#![cfg_attr(feature = "bundle", windows_subsystem = "windows")]

#[path = "base64.rs"]
mod base64_impl;
pub use base64_impl::{Engine, engine};
extern crate self as base64;

mod clipboard_bridge;
mod clipboard_service;
mod crash_forensics;
mod cron_service;
mod deep_link;
mod deep_link_bridge;
mod diagnostics_export;
mod general_settings;
mod git_branch_service;
mod git_discard_service;
mod git_repo_scan_service;
mod git_ship_service;
mod git_worktree_service;
mod local_gateway;
mod login_item;
mod memory_service;
mod messaging_service;
mod notification_service;
mod platform_diagnostics;
mod power;
mod preview_normalization;
mod preview_service;
mod preview_watcher;
mod quick_entry;
mod quick_entry_host;
mod shell_accessibility;
mod shell_focus_guard;
mod shell_i18n;
mod shell_instance;
mod shell_interaction;
mod shell_keymap;
mod shell_layout;
mod shell_parity;
mod shell_validation;
mod shell_window_contract;
mod skills_service;
mod ssh;
mod ssh_config;
mod ssh_lifecycle;
mod ssh_service;
mod ssh_terminal;
#[cfg(windows)]
mod ssh_windows;
mod subagent_bridge;
mod update_activation;
mod webhook_service;
mod window_state;

use std::path::PathBuf;

use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
use dioxus::prelude::*;
use hermes_core::AppServices;
use hermes_desktop::NativeApp;

#[derive(Clone)]
struct DesktopDataDir(PathBuf);

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
    quick_entry_host::use_quick_entry_host();
    let startup_services = services.clone();
    let mut startup_attempt = use_signal(|| 0_u64);
    let local_startup = use_resource(move || {
        let _ = startup_attempt();
        let services = startup_services.clone();
        async move { local_gateway::prepare(&services).await }
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
        Some(Ok(())) => rsx! {
            deep_link_bridge::DeepLinkBridge {
                clipboard_bridge::ClipboardBridge {
                    subagent_bridge::SubagentBridge {
                        shell_focus_guard::FocusGuard {
                            shell_parity::ParityShellHost {
                                shell_interaction::ShellHost {
                                    hermes_ui::App {}
                                }
                            }
                        }
                    }
                }
            }
        },
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
    if let Some(result) = update_activation::run_helper_if_requested() {
        if let Err(error) = result {
            eprintln!("Hermes Local update helper failed: {error}");
        }
        return;
    }
    match update_activation::activate_pending_on_startup() {
        Ok(true) => return,
        Ok(false) => {}
        Err(error) => eprintln!("Hermes Local pending update was not activated: {error}"),
    }

    let data_dir = std::env::var_os("APPDATA")
        .map_or_else(std::env::temp_dir, PathBuf::from)
        .join("Hermes Local");
    let startup_deep_link = deep_link::extract_from_args(std::env::args_os());
    let _instance_guard = match shell_instance::InstanceGuard::acquire(&data_dir) {
        Ok(Some(guard)) => guard,
        Ok(None) => {
            if let Some(uri) = startup_deep_link.as_deref() {
                if let Err(error) = deep_link::enqueue(&data_dir, uri) {
                    eprintln!("Hermes Local could not forward the deep-link activation: {error}");
                }
            } else {
                eprintln!("Hermes Local is already running; refusing a second Desktop authority.");
            }
            return;
        }
        Err(error) => {
            eprintln!("Hermes Local single-instance guard is unavailable: {error}");
            return;
        }
    };
    if let Err(error) = crash_forensics::install(&data_dir) {
        eprintln!("Hermes Local crash diagnostics are unavailable: {error}");
    }
    if let Err(error) = deep_link::register() {
        eprintln!("Hermes Local protocol registration is unavailable: {error}");
    }
    if let Some(uri) = startup_deep_link.as_deref()
        && let Err(error) = deep_link::enqueue(&data_dir, uri)
    {
        eprintln!("Hermes Local could not queue the startup deep link: {error}");
    }
    let saved_window_state = match window_state::load(&data_dir.join("window-state.json")) {
        Ok(state) => state,
        Err(error) => {
            eprintln!("Hermes Local window state is unavailable: {error}");
            None
        }
    };
    // Display-aware position/capping is implemented in the native service, but
    // Dioxus owns the event loop. Until DI-06's live monitor integration lands,
    // consume the shared Electron state only for size/maximized restoration and
    // let the platform choose a safe centered position.
    let window_options = window_state::compute_options(saved_window_state.as_ref(), &[]);

    let mut native = NativeApp::new(data_dir.clone());
    notification_service::install(&mut native.services);
    git_branch_service::install(&mut native.services);
    git_discard_service::install(&mut native.services);
    git_repo_scan_service::install(&mut native.services);
    git_ship_service::install(&mut native.services);
    git_worktree_service::install(&mut native.services);
    general_settings::install(&mut native.services, &data_dir);
    preview_service::install(&mut native.services);
    preview_watcher::install(&mut native.services);
    diagnostics_export::install(&mut native.services, data_dir.clone());
    local_gateway::install(&mut native.services);
    ssh_service::install_ssh_probe(&mut native.services, data_dir.clone());
    ssh_terminal::install(&mut native.services);

    let window = WindowBuilder::new()
        .with_title("Hermes Local")
        .with_inner_size(LogicalSize::new(
            window_options.width as f64,
            window_options.height as f64,
        ))
        .with_min_inner_size(LogicalSize::new(
            shell_window_contract::WINDOW_MIN_WIDTH as f64,
            shell_window_contract::WINDOW_MIN_HEIGHT as f64,
        ))
        .with_maximized(window_options.is_maximized)
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
        .with_context(DesktopDataDir(data_dir))
        .launch(desktop_root);
    local_gateway::shutdown();
}
