use std::{
    path::PathBuf,
    sync::{Arc, RwLock},
    time::Duration,
};

use dioxus::{
    desktop::{
        Config, DesktopContext, HotKey, HotKeyState, LogicalPosition, LogicalSize, ShortcutHandle,
        WindowBuilder, WindowCloseBehaviour,
    },
    prelude::*,
};
use hermes_core::{AppServices, ServiceError, ServiceResult};
use hermes_protocol::{ConnectionState, SessionCreateRequest, SessionSummary};

use super::{DesktopDataDir, quick_entry};

const RECENT_SESSION_LIMIT: usize = 8;
const POPUP_FOCUS_GRACE_MS: u64 = 650;
const POPUP_FOCUS_POLL_MS: u64 = 180;

struct DioxusShortcutBackend {
    desktop: DesktopContext,
    summon_epoch: Signal<u64>,
}

impl quick_entry::ShortcutBackend for DioxusShortcutBackend {
    type Handle = ShortcutHandle;

    fn register(&mut self, accelerator: &str) -> Result<Self::Handle, ()> {
        let hotkey = accelerator.parse::<HotKey>().map_err(|_| ())?;
        let mut summon_epoch = self.summon_epoch;
        self.desktop
            .create_shortcut(hotkey, move |state| {
                if state == HotKeyState::Pressed {
                    summon_epoch.set(summon_epoch().wrapping_add(1));
                }
            })
            .map_err(|_| ())
    }

    fn unregister(&mut self, handle: Self::Handle) {
        self.desktop.remove_shortcut(handle);
    }
}

#[derive(Clone)]
struct PopupShared {
    current_session: Arc<RwLock<Option<String>>>,
}

fn registration_view(
    settings: &quick_entry::QuickEntrySettings,
    registration: &quick_entry::RegistrationState,
) -> hermes_ui::QuickEntryState {
    let error = registration.error.map(|problem| match problem {
        quick_entry::RegistrationError::Invalid => "Shortcut is invalid or reserved.".to_owned(),
        quick_entry::RegistrationError::Taken => {
            "Shortcut could not be registered; another application may already own it.".to_owned()
        }
    });
    hermes_ui::QuickEntryState {
        enabled: settings.enabled,
        shortcut: registration.shortcut.clone(),
        registered: registration.registered,
        error,
    }
}

fn popup_bounds(desktop: &DesktopContext) -> quick_entry::WindowBounds {
    let monitor = desktop
        .cursor_position()
        .ok()
        .and_then(|position| desktop.monitor_from_point(position.x, position.y))
        .or_else(|| desktop.current_monitor())
        .or_else(|| desktop.primary_monitor());
    let work_area = monitor.map(|monitor| {
        let scale = monitor.scale_factor();
        let position = monitor.position().to_logical::<f64>(scale);
        let size = monitor.size().to_logical::<f64>(scale);
        quick_entry::WorkArea {
            x: position.x.round() as i64,
            y: position.y.round() as i64,
            width: size.width.round() as i64,
            height: size.height.round() as i64,
        }
    });
    quick_entry::window_bounds(work_area)
}

fn popup_config(bounds: quick_entry::WindowBounds) -> Config {
    let window = WindowBuilder::new()
        .with_title("Hermes Quick Entry")
        .with_inner_size(LogicalSize::new(bounds.width as f64, bounds.height as f64))
        .with_position(LogicalPosition::new(bounds.x as f64, bounds.y as f64))
        .with_decorations(false)
        .with_resizable(false)
        .with_always_on_top(true)
        .with_skip_taskbar(true)
        .with_visible(true);
    Config::new()
        .with_window(window)
        .with_menu(None)
        .with_background_color((9, 11, 16, 255))
        .with_disable_context_menu(true)
        .with_navigation_handler(|_| false)
}

async fn submit_prompt(
    services: &AppServices,
    current_session: &Arc<RwLock<Option<String>>>,
    target: &str,
    text: &str,
) -> ServiceResult<()> {
    let text = text.trim();
    if text.is_empty() {
        return Err(ServiceError::InvalidInput("Quick Entry prompt is empty".into()));
    }
    let stored_id = if target == "new" {
        None
    } else if target == "current" {
        current_session.read().ok().and_then(|value| value.clone())
    } else {
        target.strip_prefix("session:").map(str::to_owned)
    };
    let runtime_id = if let Some(stored_id) = stored_id {
        services.sessions.resume(&stored_id).await?.session_id
    } else {
        let session = services
            .sessions
            .create(SessionCreateRequest::default())
            .await?;
        session.runtime_id.unwrap_or(session.id)
    };
    services.sessions.submit(&runtime_id, text).await
}

#[component]
fn QuickEntryPopup() -> Element {
    let services = use_context::<AppServices>();
    let shared = use_context::<PopupShared>();
    let window = dioxus::desktop::window();
    let connected = matches!(services.connection.state(), Ok(ConnectionState::Open));
    let current_session = shared
        .current_session
        .read()
        .ok()
        .and_then(|value| value.clone());
    let mut prompt = use_signal(String::new);
    let mut target = use_signal(|| {
        if current_session.is_some() {
            "current".to_owned()
        } else {
            "new".to_owned()
        }
    });
    let mut recent = use_signal(Vec::<SessionSummary>::new);
    let mut loading = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut composer = use_signal(|| None::<MountedData>);

    let list_service = services.sessions.clone();
    let _recent_sessions = use_resource(move || {
        let list_service = list_service.clone();
        async move {
            if !matches!(services.connection.state(), Ok(ConnectionState::Open)) {
                recent.set(Vec::new());
                return;
            }
            match list_service.list().await {
                Ok(rows) => recent.set(
                    rows.into_iter()
                        .filter(|row| !row.archived)
                        .take(RECENT_SESSION_LIMIT)
                        .collect(),
                ),
                Err(problem) => error.set(Some(problem.to_string())),
            }
        }
    });

    let focus_window = window.clone();
    let _focus_guard = use_future(move || async move {
        tokio::time::sleep(Duration::from_millis(POPUP_FOCUS_GRACE_MS)).await;
        loop {
            tokio::time::sleep(Duration::from_millis(POPUP_FOCUS_POLL_MS)).await;
            if focus_window.is_visible() == Some(true) && !focus_window.is_focused() {
                focus_window.set_visible(false);
            }
        }
    });

    use_effect(move || {
        window.set_close_behavior(WindowCloseBehaviour::WindowHides);
        if let Some(element) = composer() {
            spawn(async move {
                let _ = element.set_focus(true).await;
            });
        }
    });

    let submit_services = services.clone();
    let submit_shared = shared.current_session.clone();
    let submit_window = window.clone();
    let submit = Callback::new(move |()| {
        if loading() || prompt().trim().is_empty() {
            return;
        }
        let services = submit_services.clone();
        let current_session = submit_shared.clone();
        let selected = target();
        let text = prompt();
        let window = submit_window.clone();
        loading.set(true);
        error.set(None);
        spawn(async move {
            match submit_prompt(&services, &current_session, &selected, &text).await {
                Ok(()) => {
                    prompt.set(String::new());
                    window.set_visible(false);
                }
                Err(problem) => error.set(Some(problem.to_string())),
            }
            loading.set(false);
        });
    });

    let escape_window = window.clone();
    rsx! {
        style { dangerous_inner_html: r#"
            html,body,#main{margin:0;width:100%;height:100%;background:#090b10;color:#e5e7eb;font:13px system-ui,sans-serif;}
            *{box-sizing:border-box}.qe{height:100%;display:grid;grid-template-columns:1fr auto;grid-template-rows:auto 1fr auto;gap:8px;padding:12px;border:1px solid #334155;border-radius:8px;background:#0f131b;box-shadow:0 16px 42px rgba(0,0,0,.45)}
            .qe strong{font-size:12px;letter-spacing:.06em}.qe select,.qe textarea{border:1px solid #334155;background:#0b0f16;color:#e5e7eb;border-radius:5px;padding:7px;font:inherit}.qe select{grid-column:2}.qe textarea{grid-column:1/3;resize:none;outline:none}.qe button{justify-self:end;border:1px solid #475569;background:#1e293b;color:#f8fafc;border-radius:5px;padding:6px 12px;font:inherit}.qe .err{grid-column:1/3;color:#fca5a5;font-size:11px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
        "# }
        div {
            class: "qe",
            tabindex: "0",
            onkeydown: move |event| {
                if event.key() == Key::Escape {
                    escape_window.set_visible(false);
                }
            },
            strong { "HERMES QUICK ENTRY" }
            select {
                aria_label: "Quick Entry target",
                value: "{target}",
                disabled: loading(),
                onchange: move |event| target.set(event.value()),
                if current_session.is_some() {
                    option { value: "current", "Current chat" }
                }
                option { value: "new", "New session" }
                for session in recent() {
                    option { value: "session:{session.id}", "{session.title}" }
                }
            }
            textarea {
                aria_label: "Quick Entry prompt",
                rows: "3",
                placeholder: if connected { "Ask Hermes…" } else { "Hermes Agent is offline" },
                disabled: !connected || loading(),
                value: "{prompt}",
                onmounted: move |event| composer.set(Some(event.data())),
                oninput: move |event| prompt.set(event.value()),
                onkeydown: move |event| {
                    if event.key() == Key::Escape {
                        window.set_visible(false);
                    } else if event.key() == Key::Enter && !event.modifiers().contains(Modifiers::SHIFT) {
                        event.prevent_default();
                        submit.call(());
                    }
                }
            }
            button {
                disabled: !connected || loading() || prompt().trim().is_empty(),
                onclick: move |_| submit.call(()),
                if loading() { "Sending…" } else { "Send" }
            }
            if let Some(problem) = error() {
                div { class: "err", role: "alert", "{problem}" }
            }
        }
    }
}

#[component]
pub fn QuickEntryBridge(children: Element) -> Element {
    let desktop = dioxus::desktop::window();
    let services = use_context::<AppServices>();
    let data_dir = use_context::<DesktopDataDir>().0.clone();
    let settings_path = data_dir.join("quick-entry.json");
    let initial_settings = quick_entry::load_settings(&settings_path).unwrap_or_default();
    let mut settings = use_signal(move || initial_settings.clone());
    let mut summon_epoch = use_signal(|| 0_u64);
    let mut registration = use_signal(|| quick_entry::RegistrationState {
        error: None,
        registered: false,
        shortcut: initial_settings.shortcut.clone(),
    });
    let mut controller = use_signal(move || {
        quick_entry::QuickEntryShortcutController::new(DioxusShortcutBackend {
            desktop: desktop.clone(),
            summon_epoch,
        })
    });
    let mut initialized = use_signal(|| false);
    let mut popup = use_signal(|| None::<DesktopContext>);
    let current_session_shared = use_hook(|| Arc::new(RwLock::new(None::<String>)));
    let current_session = use_signal(|| None::<String>);

    use_effect(move || {
        if initialized() {
            return;
        }
        registration.set(controller.write().apply(&settings()));
        initialized.set(true);
    });

    let shared_session = current_session_shared.clone();
    use_effect(move || {
        if let Ok(mut slot) = shared_session.write() {
            *slot = current_session();
        }
    });

    let show_desktop = desktop.clone();
    let show_services = services.clone();
    let show_current = current_session_shared.clone();
    let _summon = use_resource(move || {
        let epoch = summon_epoch();
        let desktop = show_desktop.clone();
        let services = show_services.clone();
        let current_session = show_current.clone();
        async move {
            if epoch == 0 {
                return;
            }
            let bounds = popup_bounds(&desktop);
            if let Some(existing) = popup() {
                existing.set_outer_position(LogicalPosition::new(bounds.x as f64, bounds.y as f64));
                existing.set_visible(true);
                existing.set_focus();
                return;
            }
            let dom = VirtualDom::new(QuickEntryPopup)
                .with_root_context(services)
                .with_root_context(PopupShared { current_session });
            let created = desktop.new_window(dom, popup_config(bounds)).await;
            created.set_close_behavior(WindowCloseBehaviour::WindowHides);
            created.set_focus();
            popup.set(Some(created));
        }
    });

    let save_path = settings_path.clone();
    let save = Callback::new(move |(enabled, shortcut): (bool, String)| {
        let next = quick_entry::QuickEntrySettings { enabled, shortcut };
        match quick_entry::save_settings(&save_path, &next) {
            Ok(saved) => {
                settings.set(saved.clone());
                registration.set(controller.write().apply(&saved));
            }
            Err(problem) => {
                let mut view = registration();
                view.error = Some(quick_entry::RegistrationError::Invalid);
                registration.set(view);
                eprintln!("Hermes Quick Entry settings were not saved: {problem}");
            }
        }
    });
    let summon = Callback::new(move |()| summon_epoch.set(summon_epoch().wrapping_add(1)));

    let settings_snapshot = settings();
    let registration_snapshot = registration();
    let state = use_signal(move || registration_view(&settings_snapshot, &registration_snapshot));
    let mut state_signal = state;
    use_effect(move || {
        state_signal.set(registration_view(&settings(), &registration()));
    });
    use_context_provider(move || hermes_ui::QuickEntryActions {
        state,
        current_session,
        save,
        summon,
    });

    use_drop(move || controller.write().dispose());
    rsx! { {children} }
}

#[cfg(test)]
mod tests {
    use super::registration_view;
    use crate::quick_entry::{QuickEntrySettings, RegistrationError, RegistrationState};

    #[test]
    fn registration_view_preserves_taken_state_without_disabling_preference() {
        let view = registration_view(
            &QuickEntrySettings {
                enabled: true,
                shortcut: "Alt+K".into(),
            },
            &RegistrationState {
                error: Some(RegistrationError::Taken),
                registered: false,
                shortcut: "Alt+K".into(),
            },
        );
        assert!(view.enabled);
        assert!(!view.registered);
        assert!(view.error.unwrap().contains("already own"));
    }
}
