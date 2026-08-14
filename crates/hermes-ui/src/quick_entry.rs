use dioxus::prelude::*;

use super::Surface;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct QuickEntryState {
    pub enabled: bool,
    pub shortcut: String,
    pub registered: bool,
    pub error: Option<String>,
}

#[derive(Clone)]
pub struct QuickEntryActions {
    pub state: Signal<QuickEntryState>,
    pub current_session: Signal<Option<String>>,
    pub save: Callback<(bool, String)>,
    pub summon: Callback<()>,
}

#[component]
pub(super) fn QuickEntry() -> Element {
    let actions = try_use_context::<QuickEntryActions>();
    let initial = actions
        .as_ref()
        .map(|actions| (actions.state)())
        .unwrap_or_default();
    let mut enabled = use_signal(|| initial.enabled);
    let mut shortcut = use_signal(|| initial.shortcut.clone());
    let live = actions.as_ref().map(|actions| (actions.state)());

    rsx! {
        Surface {
            eyebrow: "Capture",
            title: "Quick entry",
            subtitle: "Summon a small always-on-top composer from anywhere without creating another Agent authority.",
            if let Some(actions) = actions {
                section { class: "panel", style: "display:grid;gap:.75rem;",
                    label { class: "field-stack",
                        span { "Global shortcut" }
                        input {
                            class: "settings-input",
                            value: "{shortcut}",
                            disabled: !enabled(),
                            oninput: move |event| shortcut.set(event.value())
                        }
                    }
                    label { style: "display:flex;gap:.5rem;align-items:center;",
                        input {
                            r#type: "checkbox",
                            checked: enabled(),
                            onchange: move |event| enabled.set(event.checked())
                        }
                        span { "Enable global Quick Entry shortcut" }
                    }
                    div { style: "display:flex;gap:.5rem;flex-wrap:wrap;",
                        button {
                            class: "primary-button",
                            onclick: move |_| actions.save.call((enabled(), shortcut())),
                            "Save shortcut"
                        }
                        button {
                            class: "button",
                            onclick: move |_| actions.summon.call(()),
                            "Open Quick Entry"
                        }
                    }
                    if let Some(live) = live {
                        p { class: "muted",
                            if !live.enabled {
                                "Shortcut disabled"
                            } else if live.registered {
                                "Registered: {live.shortcut}"
                            } else {
                                "Not registered: {live.shortcut}"
                            }
                        }
                        if let Some(problem) = live.error.as_deref() {
                            p { class: "inline-error", role: "alert", "{problem}" }
                        }
                    }
                }
            } else {
                div { class: "empty-state",
                    h2 { "Desktop-only capability" }
                    p { "Global Quick Entry registration is supplied by the native Desktop host." }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::QuickEntryState;

    #[test]
    fn quick_entry_state_keeps_registration_and_setting_distinct() {
        let state = QuickEntryState {
            enabled: true,
            shortcut: "CommandOrControl+Shift+Space".into(),
            registered: false,
            error: Some("Shortcut is already in use".into()),
        };
        assert!(state.enabled);
        assert!(!state.registered);
        assert!(state.error.is_some());
    }
}
