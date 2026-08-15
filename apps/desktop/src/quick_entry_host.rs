use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use dioxus::{
    desktop::{
        Config, DesktopContext, HotKeyState, LogicalPosition, LogicalSize, ShortcutHandle,
        WindowBuilder, WindowCloseBehaviour, tao::event::{Event, WindowEvent},
        use_wry_event_handler,
    },
    prelude::*,
};
use hermes_core::{AppServices, ServiceError};
use hermes_protocol::{SessionCreateRequest, SessionSummary};

use crate::{
    DesktopDataDir,
    quick_entry::{
        QuickEntrySettings, QuickEntryShortcutController, RegistrationError, ShortcutBackend,
        WorkArea, load_settings, window_bounds,
    },
};

const TARGET_CURRENT: &str = "__current__";
const TARGET_NEW: &str = "__new__";
const RECENT_SESSION_LIMIT: usize = 5;
const QUICK_ENTRY_SETTINGS_FILE: &str = "quick-entry.json";

const QUICK_ENTRY_CSS: &str = r#"
html, body, #main { margin: 0; width: 100%; height: 100%; overflow: hidden; background: transparent; }
body { font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; color: #f4f4f5; }
.quick-entry-root { box-sizing: border-box; width: 100%; height: 100%; padding: 12px; display: flex; align-items: center; justify-content: center; background: transparent; }
.quick-entry-card { box-sizing: border-box; width: 100%; display: grid; gap: 9px; padding: 12px 14px; border: 1px solid rgba(148,163,184,.32); border-radius: 12px; background: rgb(17 20 27); box-shadow: 0 18px 48px rgba(0,0,0,.45); }
.quick-entry-row { display: flex; align-items: center; gap: 10px; min-width: 0; }
.quick-entry-chevron { color: #9ca3af; font-size: 17px; line-height: 1; user-select: none; }
.quick-entry-input { box-sizing: border-box; flex: 1; min-width: 0; border: 0; outline: 0; resize: none; padding: 3px 0; background: transparent; color: inherit; font: inherit; font-size: 15px; line-height: 22px; }
.quick-entry-input:disabled { opacity: .55; }
.quick-entry-footer { display: flex; align-items: center; gap: 8px; min-width: 0; }
.quick-entry-label { color: #9ca3af; font-size: 11px; white-space: nowrap; }
.quick-entry-select { min-width: 0; max-width: 330px; height: 26px; border: 1px solid rgba(148,163,184,.28); border-radius: 6px; padding: 0 24px 0 8px; background: rgb(30 34 43); color: #e5e7eb; font: inherit; font-size: 11px; }
.quick-entry-spacer { flex: 1; }
.quick-entry-error { min-width: 0; max-width: 260px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: #fca5a5; font-size: 11px; }
.quick-entry-hint { color: #71717a; font-size: 10px; white-space: nowrap; }
"#;

#[derive(Clone)]
struct QuickEntryRuntime {
    services: AppServices,
    current_session: Rc<RefCell<Option<String>>>,
}

struct DesktopShortcutBackend {
    desktop: DesktopContext,
    on_trigger: Callback<()>,
}

impl ShortcutBackend for DesktopShortcutBackend {
    type Handle = ShortcutHandle;

    fn register(&mut self, accelerator: &str) -> Result<Self::Handle, ()> {
        let hotkey = accelerator.parse().map_err(|_| ())?;
        let on_trigger = self.on_trigger.clone();
        self.desktop
            .create_shortcut(hotkey, move |state| {
                if state == HotKeyState::Pressed {
                    on_trigger.call(());
                }
            })
            .map_err(|_| ())
    }

    fn unregister(&mut self, handle: Self::Handle) {
        self.desktop.remove_shortcut(handle);
    }
}

/// Install DI-07's one native global shortcut and own the reusable secondary
/// Dioxus Quick Entry window. This hook belongs in the Desktop composition root:
/// shared UI remains free of OS/window authority and the quick window receives
/// the same typed AppServices as the primary UI instead of opening another
/// gateway connection.
pub fn use_quick_entry_host() {
    let desktop = dioxus::desktop::window();
    let services = use_context::<AppServices>();
    let data_dir = use_context::<DesktopDataDir>().0;
    let quick_window = use_hook(|| Rc::new(RefCell::new(None::<DesktopContext>)));
    let creating_window = use_hook(|| Rc::new(Cell::new(false)));
    let current_session = use_hook(|| Rc::new(RefCell::new(None::<String>)));

    let on_trigger = use_callback({
        let desktop = desktop.clone();
        let services = services.clone();
        let quick_window = quick_window.clone();
        let creating_window = creating_window.clone();
        let current_session = current_session.clone();
        move |()| {
            *current_session.borrow_mut() = desktop
                .webview
                .url()
                .ok()
                .as_deref()
                .and_then(session_id_from_url);

            if let Some(window) = quick_window.borrow().as_ref().cloned() {
                show_quick_entry(&window);
                return;
            }
            if creating_window.replace(true) {
                return;
            }

            let desktop = desktop.clone();
            let services = services.clone();
            let quick_window = quick_window.clone();
            let creating_window = creating_window.clone();
            let current_session = current_session.clone();
            spawn(async move {
                let runtime = QuickEntryRuntime {
                    services,
                    current_session,
                };
                let dom = VirtualDom::new(quick_entry_window).with_root_context(runtime);
                let window = desktop.new_window(dom, quick_entry_config(&desktop)).await;
                window.set_close_behavior(WindowCloseBehaviour::WindowHides);
                show_quick_entry(&window);
                *quick_window.borrow_mut() = Some(window);
                creating_window.set(false);
            });
        }
    });

    let settings = use_hook(move || match load_settings(&data_dir.join(QUICK_ENTRY_SETTINGS_FILE)) {
        Ok(settings) => settings,
        Err(error) => {
            eprintln!("Hermes Local Quick Entry settings are unavailable: {error}");
            QuickEntrySettings::default()
        }
    });
    let controller = use_hook({
        let desktop = desktop.clone();
        let on_trigger = on_trigger.clone();
        move || {
            Rc::new(RefCell::new(QuickEntryShortcutController::new(
                DesktopShortcutBackend {
                    desktop,
                    on_trigger,
                },
            )))
        }
    });

    use_effect({
        let controller = controller.clone();
        let settings = settings.clone();
        move || {
            let state = controller.borrow_mut().apply(&settings);
            if let Some(error) = state.error {
                let reason = match error {
                    RegistrationError::Invalid => "invalid shortcut",
                    RegistrationError::Taken => "shortcut already owned by another application",
                };
                eprintln!(
                    "Hermes Local Quick Entry could not register {}: {reason}",
                    state.shortcut
                );
            }
        }
    });
    use_drop(move || controller.borrow_mut().dispose());
}

fn quick_entry_config(primary: &DesktopContext) -> Config {
    let bounds = window_bounds(primary.current_monitor().map(|monitor| {
        let scale = monitor.scale_factor();
        let position = monitor.position().to_logical::<f64>(scale);
        let size = monitor.size().to_logical::<f64>(scale);
        WorkArea {
            x: position.x.round() as i64,
            y: position.y.round() as i64,
            width: size.width.round() as i64,
            height: size.height.round() as i64,
        }
    }));
    let window = WindowBuilder::new()
        .with_title("Hermes Quick Entry")
        .with_inner_size(LogicalSize::new(bounds.width as f64, bounds.height as f64))
        .with_min_inner_size(LogicalSize::new(bounds.width as f64, bounds.height as f64))
        .with_max_inner_size(LogicalSize::new(bounds.width as f64, bounds.height as f64))
        .with_position(LogicalPosition::new(bounds.x as f64, bounds.y as f64))
        .with_resizable(false)
        .with_decorations(false)
        .with_always_on_top(true)
        .with_visible(false)
        .with_focused(false);
    Config::new()
        .with_window(window)
        .with_menu(None)
        .with_background_color((17, 20, 27, 255))
        .with_disable_context_menu(true)
        .with_navigation_handler(|_| false)
}

fn show_quick_entry(window: &DesktopContext) {
    window.set_visible(true);
    window.set_focus();
    let _ = window.webview.evaluate_script(
        "requestAnimationFrame(() => document.querySelector('[data-quick-entry-input]')?.focus())",
    );
}

#[component]
fn quick_entry_window() -> Element {
    let runtime = use_context::<QuickEntryRuntime>();
    let window = dioxus::desktop::window();
    let mut draft = use_signal(String::new);
    let mut target = use_signal(|| TARGET_CURRENT.to_owned());
    let mut sessions = use_signal(Vec::<SessionSummary>::new);
    let mut connected = use_signal(|| false);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut submitting = use_signal(|| false);
    let mut refresh = use_signal(|| 0_u64);

    use_effect({
        let window = window.clone();
        move || window.set_close_behavior(WindowCloseBehaviour::WindowHides)
    });

    let list_service = runtime.services.sessions.clone();
    let _session_list = use_resource(move || {
        let _ = refresh();
        let service = list_service.clone();
        async move {
            loading.set(true);
            match service.list().await {
                Ok(rows) => {
                    sessions.set(
                        rows.into_iter()
                            .filter(|session| !session.archived)
                            .take(RECENT_SESSION_LIMIT)
                            .collect(),
                    );
                    connected.set(true);
                    error.set(None);
                }
                Err(load_error) => {
                    connected.set(false);
                    error.set(Some(load_error.to_string()));
                }
            }
            loading.set(false);
        }
    });

    use_wry_event_handler({
        let window = window.clone();
        move |event, _| {
            if let Event::WindowEvent {
                event: WindowEvent::Focused(focused),
                ..
            } = event
            {
                if *focused {
                    draft.set(String::new());
                    target.set(TARGET_CURRENT.to_owned());
                    error.set(None);
                    refresh += 1;
                    let _ = window.webview.evaluate_script(
                        "requestAnimationFrame(() => document.querySelector('[data-quick-entry-input]')?.focus())",
                    );
                } else if window.is_visible().unwrap_or(false) {
                    draft.set(String::new());
                    window.set_visible(false);
                }
            }
        }
    });

    let submit_service = runtime.services.sessions.clone();
    let current_session = runtime.current_session.clone();
    let submit_window = window.clone();
    let send = Callback::new(move |()| {
        let text = draft().trim().to_owned();
        if text.is_empty() || submitting() || !connected() {
            return;
        }
        let selected = target();
        let current = current_session.borrow().clone();
        let service = submit_service.clone();
        let window = submit_window.clone();
        submitting.set(true);
        error.set(None);
        spawn(async move {
            let result: Result<(), ServiceError> = async {
                let runtime_id = match resolve_submission_target(&selected, current.as_deref()) {
                    SubmissionTarget::New => create_runtime_session(service.as_ref()).await?,
                    SubmissionTarget::Stored(stored_id) => match service.resume(&stored_id).await {
                        Ok(resumed) => resumed.session_id,
                        Err(_) if selected != TARGET_CURRENT => {
                            if let Some(active_id) = current.as_deref() {
                                service.resume(active_id).await?.session_id
                            } else {
                                create_runtime_session(service.as_ref()).await?
                            }
                        }
                        Err(error) => return Err(error),
                    },
                };
                service.submit(&runtime_id, &text).await
            }
            .await;

            submitting.set(false);
            match result {
                Ok(()) => {
                    draft.set(String::new());
                    target.set(TARGET_CURRENT.to_owned());
                    window.set_visible(false);
                }
                Err(submit_error) => error.set(Some(submit_error.to_string())),
            }
        });
    });

    let dismiss_window = window.clone();
    rsx! {
        style { "{QUICK_ENTRY_CSS}" }
        div { class: "quick-entry-root",
            div { class: "quick-entry-card",
                div { class: "quick-entry-row",
                    span { class: "quick-entry-chevron", aria_hidden: "true", "›" }
                    input {
                        class: "quick-entry-input",
                        "data-quick-entry-input": "true",
                        aria_label: "Quick Entry",
                        autocomplete: "off",
                        spellcheck: "false",
                        disabled: !connected() || submitting(),
                        placeholder: if connected() { "Ask Hermes…" } else { "Not connected — open Hermes to reconnect" },
                        value: "{draft}",
                        oninput: move |event| draft.set(event.value()),
                        onkeydown: move |event| {
                            if event.key() == Key::Enter && !event.modifiers().contains(Modifiers::SHIFT) {
                                event.prevent_default();
                                send.call(());
                            } else if event.key() == Key::Escape {
                                event.prevent_default();
                                draft.set(String::new());
                                dismiss_window.set_visible(false);
                            }
                        }
                    }
                }
                div { class: "quick-entry-footer",
                    span { class: "quick-entry-label", "Send to" }
                    select {
                        class: "quick-entry-select",
                        aria_label: "Target session",
                        disabled: !connected() || submitting(),
                        value: "{target}",
                        onchange: move |event| target.set(event.value()),
                        option { value: TARGET_CURRENT, "Current chat" }
                        option { value: TARGET_NEW, "New session" }
                        for session in sessions() {
                            option { key: "{session.id}", value: "{session.id}", "{session.title}" }
                        }
                    }
                    span { class: "quick-entry-spacer" }
                    if let Some(message) = error() {
                        span { class: "quick-entry-error", title: "{message}", "{message}" }
                    } else if loading() {
                        span { class: "quick-entry-hint", "Refreshing sessions…" }
                    } else {
                        span { class: "quick-entry-hint", "Enter to send · Esc to dismiss" }
                    }
                }
            }
        }
    }
}

async fn create_runtime_session(service: &dyn hermes_core::SessionService) -> Result<String, ServiceError> {
    let session = service.create(SessionCreateRequest::default()).await?;
    Ok(session.runtime_id.unwrap_or(session.id))
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum SubmissionTarget {
    New,
    Stored(String),
}

fn resolve_submission_target(target: &str, current_session: Option<&str>) -> SubmissionTarget {
    if target == TARGET_NEW {
        return SubmissionTarget::New;
    }
    if target == TARGET_CURRENT {
        return current_session
            .filter(|session_id| !session_id.is_empty())
            .map_or(SubmissionTarget::New, |session_id| {
                SubmissionTarget::Stored(session_id.to_owned())
            });
    }
    SubmissionTarget::Stored(target.to_owned())
}

fn session_id_from_url(url: &str) -> Option<String> {
    let (_, tail) = url.split_once("/session/")?;
    let session_id = tail
        .split(['/', '?', '#'])
        .next()
        .map(str::trim)
        .filter(|session_id| !session_id.is_empty())?;
    Some(session_id.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_target_uses_active_session_or_creates_a_new_one() {
        assert_eq!(
            resolve_submission_target(TARGET_CURRENT, Some("session-42")),
            SubmissionTarget::Stored("session-42".into())
        );
        assert_eq!(
            resolve_submission_target(TARGET_CURRENT, None),
            SubmissionTarget::New
        );
    }

    #[test]
    fn explicit_targets_keep_new_and_recent_session_semantics() {
        assert_eq!(resolve_submission_target(TARGET_NEW, Some("active")), SubmissionTarget::New);
        assert_eq!(
            resolve_submission_target("stored-7", Some("active")),
            SubmissionTarget::Stored("stored-7".into())
        );
    }

    #[test]
    fn active_session_is_derived_from_primary_dioxus_route() {
        assert_eq!(
            session_id_from_url("http://dioxus.local/session/abc-123?tab=chat"),
            Some("abc-123".into())
        );
        assert_eq!(
            session_id_from_url("http://dioxus.local/#/session/hash-session"),
            Some("hash-session".into())
        );
        assert_eq!(session_id_from_url("http://dioxus.local/chat"), None);
        assert_eq!(session_id_from_url("http://dioxus.local/session/"), None);
    }
}