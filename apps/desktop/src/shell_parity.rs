use dioxus::prelude::*;

use crate::{
    shell_i18n::{
        LOCALE_STORAGE_KEY, Locale, Message, locale_apply_script, locale_read_script, translate,
    },
    shell_keymap::{FocusContext, KeyChord, ShortcutAction, resolve_shortcut},
    shell_layout::{LAYOUT_STORAGE_KEY, LayoutModel, PaneKind, SplitAxis},
};

const PARITY_CSS: &str = r#"
.shell-parity-root{height:100vh;min-width:0;position:relative;overflow:hidden}
.shell-layout-trigger{position:fixed;right:8px;bottom:26px;z-index:9995;border:1px solid rgb(51 65 85);border-radius:4px;background:rgb(15 23 42);color:rgb(203 213 225);padding:3px 7px;font:10px system-ui,sans-serif;cursor:pointer}
.shell-layout-panel{position:fixed;z-index:10020;right:10px;top:42px;width:min(380px,calc(100vw - 20px));max-height:calc(100vh - 76px);overflow:auto;box-sizing:border-box;border:1px solid rgb(71 85 105);border-radius:8px;background:rgb(15 23 42);box-shadow:0 18px 60px rgb(0 0 0 / .45);color:rgb(226 232 240);font:12px system-ui,sans-serif}
.shell-layout-panel header{position:sticky;top:0;z-index:1;display:flex;align-items:center;gap:8px;padding:9px 10px;border-bottom:1px solid rgb(51 65 85);background:rgb(15 23 42)}
.shell-layout-panel header strong{flex:1}.shell-layout-panel button,.shell-layout-panel select{font:inherit}
.shell-layout-close{border:0;background:transparent;color:rgb(148 163 184);font-size:18px;cursor:pointer}
.shell-layout-section{display:grid;gap:8px;padding:10px;border-bottom:1px solid rgb(30 41 59)}
.shell-layout-section h3{margin:0;color:rgb(148 163 184);font-size:10px;letter-spacing:.08em;text-transform:uppercase}
.shell-layout-row{display:flex;flex-wrap:wrap;gap:6px;align-items:center}.shell-layout-row button{border:1px solid rgb(71 85 105);border-radius:4px;background:rgb(30 41 59);color:rgb(241 245 249);padding:5px 8px;cursor:pointer}
.shell-layout-row button:disabled{opacity:.45;cursor:default}.shell-layout-meta{color:rgb(148 163 184);line-height:1.45}
.shell-layout-groups{display:grid;gap:6px}.shell-layout-group{display:flex;align-items:center;gap:7px;border:1px solid rgb(51 65 85);border-radius:5px;padding:7px;background:rgb(2 6 23 / .35)}
.shell-layout-group.active{border-color:rgb(96 165 250)}.shell-layout-group button{margin-left:auto}
.shell-floating-list{display:grid;gap:6px}.shell-floating-item{display:flex;align-items:center;gap:6px;padding:6px;border:1px dashed rgb(71 85 105);border-radius:5px}
.shell-locale-select{min-width:145px;border:1px solid rgb(71 85 105);border-radius:4px;background:rgb(2 6 23);color:rgb(241 245 249);padding:5px 7px}
.shell-layout-panel :focus-visible,.shell-layout-trigger:focus-visible{outline:2px solid rgb(96 165 250);outline-offset:2px}
html[dir="rtl"] .shell-layout-panel{right:auto;left:10px}html[dir="rtl"] .shell-layout-trigger{right:auto;left:8px}
@media (prefers-reduced-motion:reduce){.shell-layout-panel,.shell-layout-trigger{scroll-behavior:auto!important;transition:none!important;animation:none!important}}
"#;

fn run_js(script: String) {
    spawn(async move {
        let _ = document::eval(&script).await;
    });
}

fn route_script(path: &str) -> String {
    let path = serde_json::to_string(path).expect("static route serializes");
    format!(
        "(() => {{ const path={path}; const link=[...document.querySelectorAll('a[href]')].find(a => a.getAttribute('href') === path); if(link){{link.click();return;}} history.pushState(null,'',path); window.dispatchEvent(new PopStateEvent('popstate')); }})()"
    )
}

fn layout_read_script() -> String {
    format!("return localStorage.getItem('{LAYOUT_STORAGE_KEY}') || '';")
}

fn layout_persist_script(layout: &LayoutModel) -> String {
    let json = layout.to_json().unwrap_or_default();
    let value = serde_json::to_string(&json).expect("layout json string serializes");
    format!("localStorage.setItem('{LAYOUT_STORAGE_KEY}', {value});")
}

fn locale_observer_script(locale: Locale) -> String {
    let apply = locale_apply_script(locale);
    format!(
        r#"(() => {{
      {apply};
      if(window.__hermesLocaleObserver) window.__hermesLocaleObserver.disconnect();
      let queued=false;
      const observer=new MutationObserver(() => {{
        if(queued) return;
        queued=true;
        queueMicrotask(() => {{ queued=false; {apply}; }});
      }});
      observer.observe(document.body,{{subtree:true,childList:true}});
      window.__hermesLocaleObserver=observer;
    }})()"#
    )
}

fn accessibility_audit_script() -> String {
    r#"(() => {
      const violations=[];
      for(const button of document.querySelectorAll('button')) {
        const name=(button.getAttribute('aria-label')||button.getAttribute('title')||button.textContent||'').trim();
        if(!name) violations.push('button-without-name');
      }
      for(const input of document.querySelectorAll('input,select,textarea')) {
        const name=(input.getAttribute('aria-label')||input.getAttribute('title')||'').trim();
        if(!name && !input.closest('label')) violations.push('control-without-name');
      }
      for(const tab of document.querySelectorAll('[role="tab"]')) {
        if(!tab.hasAttribute('aria-selected')) violations.push('tab-without-selected-state');
      }
      document.documentElement.dataset.shellA11yViolations=String(violations.length);
      return violations.length;
    })()"#
        .into()
}

fn mutate_layout(layout: &mut Signal<LayoutModel>, mutation: impl FnOnce(&mut LayoutModel)) {
    {
        let mut model = layout.write();
        mutation(&mut model);
        model.normalize();
    }
    run_js(layout_persist_script(&layout()));
}

#[rustfmt::skip]
#[component]
pub fn ParityShellHost(children: Element) -> Element {
    let mut panel_open = use_signal(|| false);
    let mut layout = use_signal(LayoutModel::default);
    let mut locale = use_signal(|| Locale::En);

    use_effect(move || {
        spawn(async move {
            let raw = document::eval(&layout_read_script()).join::<String>().await.unwrap_or_default();
            if !raw.trim().is_empty()
                && let Ok(restored) = LayoutModel::from_json(&raw)
                && restored.validate().is_ok()
            {
                layout.set(restored);
            }
        });
    });

    use_effect(move || {
        spawn(async move {
            let raw = document::eval(&locale_read_script())
                .join::<String>()
                .await
                .unwrap_or_else(|_| "en".into());
            let restored = Locale::from_code(&raw);
            locale.set(restored);
            run_js(locale_observer_script(restored));
        });
    });

    use_effect(move || {
        let _ = locale();
        run_js(accessibility_audit_script());
    });

    let current_layout = layout();
    let focused = current_layout.focused_group().cloned();
    let focused_title = focused
        .as_ref()
        .and_then(|group| group.active_tab())
        .map_or("None", |tab| tab.title.as_str())
        .to_owned();
    let groups = current_layout.group_ids();
    let floating = current_layout.floating.clone();
    let pane_count = current_layout.pane_ids().len();

    rsx! {
        style { dangerous_inner_html: PARITY_CSS }
        div {
            class: "shell-parity-root",
            onkeydown: move |event: KeyboardEvent| {
                let key = match event.key() {
                    Key::Character(value) => value.to_lowercase(),
                    Key::Tab => "tab".into(),
                    Key::Escape => "escape".into(),
                    _ => String::new(),
                };
                let modifiers = event.modifiers();
                let primary = modifiers.contains(Modifiers::CONTROL) || modifiers.contains(Modifiers::META);
                let control = modifiers.contains(Modifiers::CONTROL);
                let shift = modifiers.contains(Modifiers::SHIFT);
                let alt = modifiers.contains(Modifiers::ALT);

                if primary && alt && key == "l" {
                    event.prevent_default();
                    panel_open.toggle();
                    return;
                }
                if !panel_open() {
                    return;
                }

                let chord = KeyChord::new(key, primary, control, shift, alt);
                let Some(action) = resolve_shortcut(FocusContext::Pane, &chord) else { return; };
                match action {
                    ShortcutAction::SelectPaneTab(index) => {
                        event.prevent_default();
                        let group_id = layout().focused_group.clone();
                        mutate_layout(&mut layout, |model| { let _ = model.activate_tab(&group_id, index); });
                    }
                    ShortcutAction::NextPaneTab => {
                        event.prevent_default();
                        mutate_layout(&mut layout, |model| { let _ = model.cycle_tab(false); });
                    }
                    ShortcutAction::PreviousPaneTab => {
                        event.prevent_default();
                        mutate_layout(&mut layout, |model| { let _ = model.cycle_tab(true); });
                    }
                    ShortcutAction::ClosePaneTab => {
                        event.prevent_default();
                        mutate_layout(&mut layout, |model| { let _ = model.close_active_tab(); });
                    }
                    ShortcutAction::NewPaneTab => {
                        event.prevent_default();
                        mutate_layout(&mut layout, |model| { model.add_tab(PaneKind::Preview); });
                    }
                    ShortcutAction::SplitHorizontal => {
                        event.prevent_default();
                        mutate_layout(&mut layout, |model| { let _ = model.split_focused(SplitAxis::Horizontal, PaneKind::Files); });
                    }
                    ShortcutAction::SplitVertical => {
                        event.prevent_default();
                        mutate_layout(&mut layout, |model| { let _ = model.split_focused(SplitAxis::Vertical, PaneKind::Terminal); });
                    }
                    ShortcutAction::FloatPane => {
                        event.prevent_default();
                        mutate_layout(&mut layout, |model| { let _ = model.float_active(); });
                    }
                    ShortcutAction::CloseDialog => {
                        event.prevent_default();
                        panel_open.set(false);
                    }
                    _ => {}
                }
            },
            {children}
            button {
                class: "shell-layout-trigger",
                aria_label: "Workspace layout and language",
                title: "Workspace layout and language (Ctrl/Cmd+Alt+L)",
                onclick: move |_| panel_open.toggle(),
                "Layout"
            }
        }
        if panel_open() {
            aside {
                class: "shell-layout-panel",
                role: "dialog",
                aria_label: "Workspace layout",
                header {
                    strong { "Workspace layout" }
                    button {
                        class: "shell-layout-close",
                        aria_label: "Close workspace layout",
                        title: "Close",
                        onclick: move |_| panel_open.set(false),
                        "×"
                    }
                }
                section { class: "shell-layout-section",
                    h3 { "{translate(locale(), Message::Language)}" }
                    div { class: "shell-layout-row",
                        select {
                            class: "shell-locale-select",
                            aria_label: "Language",
                            value: "{locale().code()}",
                            onchange: move |event| {
                                let next = Locale::from_code(&event.value());
                                locale.set(next);
                                run_js(locale_observer_script(next));
                            },
                            for option_locale in Locale::ALL {
                                option { value: "{option_locale.code()}", "{option_locale.label()}" }
                            }
                        }
                        span { class: "shell-layout-meta", "{locale().code()} · {locale().direction()}" }
                    }
                }
                section { class: "shell-layout-section",
                    h3 { "Pane tree" }
                    div { class: "shell-layout-meta", "{pane_count} panes · {groups.len()} groups · active: {focused_title}" }
                    div { class: "shell-layout-row",
                        button { onclick: move |_| mutate_layout(&mut layout, |model| { let _ = model.split_focused(SplitAxis::Horizontal, PaneKind::Files); }), "Split horizontal" }
                        button { onclick: move |_| mutate_layout(&mut layout, |model| { let _ = model.split_focused(SplitAxis::Vertical, PaneKind::Terminal); }), "Split vertical" }
                        button { onclick: move |_| mutate_layout(&mut layout, |model| { model.add_tab(PaneKind::Review); }), "Add tab" }
                    }
                    div { class: "shell-layout-row",
                        button { onclick: move |_| mutate_layout(&mut layout, |model| { let _ = model.cycle_tab(true); }), "Previous tab" }
                        button { onclick: move |_| mutate_layout(&mut layout, |model| { let _ = model.cycle_tab(false); }), "Next tab" }
                        button { onclick: move |_| mutate_layout(&mut layout, |model| { let _ = model.float_active(); }), "Float active" }
                        button { onclick: move |_| mutate_layout(&mut layout, |model| { let _ = model.close_active_tab(); }), "Close active" }
                    }
                    div { class: "shell-layout-groups", role: "tablist", aria_label: "Pane groups",
                        for group_id in groups {
                            {
                                let is_active = current_layout.focused_group == group_id;
                                let group_id_for_click = group_id.clone();
                                rsx! {
                                    div { class: if is_active { "shell-layout-group active" } else { "shell-layout-group" },
                                        span { "{group_id}" }
                                        button {
                                            role: "tab",
                                            aria_selected: is_active,
                                            onclick: move |_| mutate_layout(&mut layout, |model| { let _ = model.focus_group(&group_id_for_click); }),
                                            if is_active { "Focused" } else { "Focus" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "shell-layout-row",
                        button { onclick: move |_| { layout.set(LayoutModel::default()); run_js(layout_persist_script(&layout())); }, "Reset layout" }
                    }
                }
                section { class: "shell-layout-section",
                    h3 { "Persistent tools" }
                    div { class: "shell-layout-row",
                        for kind in [PaneKind::Files, PaneKind::Terminal, PaneKind::Review, PaneKind::Preview] {
                            button {
                                onclick: move |_| {
                                    mutate_layout(&mut layout, |model| { model.ensure_tool_tab(kind); });
                                    run_js(route_script(kind.route()));
                                },
                                "Open {kind.title()}"
                            }
                        }
                    }
                }
                if !floating.is_empty() {
                    section { class: "shell-layout-section",
                        h3 { "Floating panes" }
                        div { class: "shell-floating-list",
                            for pane in floating {
                                {
                                    let floating_id = pane.id.clone();
                                    let floating_id_for_open = pane.id.clone();
                                    let route = pane.tab.kind.route();
                                    rsx! {
                                        div { class: "shell-floating-item",
                                            strong { "{pane.tab.title}" }
                                            span { class: "shell-layout-meta", "z{pane.z_index}" }
                                            button { onclick: move |_| run_js(route_script(route)), "Open" }
                                            button { onclick: move |_| mutate_layout(&mut layout, |model| { let _ = model.dock_floating(&floating_id); }), "Dock" }
                                            button { onclick: move |_| mutate_layout(&mut layout, |model| { let _ = model.bring_floating_to_front(&floating_id_for_open); }), "Front" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                section { class: "shell-layout-section",
                    h3 { "Keyboard" }
                    p { class: "shell-layout-meta", "Ctrl/Cmd+Alt+L toggles this panel. While it is open: Ctrl/Cmd+1…9 selects tabs, Ctrl+Tab cycles tabs, Ctrl/Cmd+W closes the active tab, Ctrl/Cmd+T adds a tab, and Ctrl/Cmd+Alt+H/V creates splits." }
                    p { class: "shell-layout-meta", "Locale: {LOCALE_STORAGE_KEY} · Layout: {LAYOUT_STORAGE_KEY}" }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persistence_scripts_use_stable_storage_contracts() {
        let layout = LayoutModel::default();
        assert!(layout_read_script().contains(LAYOUT_STORAGE_KEY));
        assert!(layout_persist_script(&layout).contains(LAYOUT_STORAGE_KEY));
        assert!(locale_read_script().contains(LOCALE_STORAGE_KEY));
    }

    #[test]
    fn accessibility_audit_covers_names_controls_and_tab_state() {
        let script = accessibility_audit_script();
        assert!(script.contains("button-without-name"));
        assert!(script.contains("control-without-name"));
        assert!(script.contains("tab-without-selected-state"));
        assert!(script.contains("shellA11yViolations"));
    }

    #[test]
    fn locale_observer_reapplies_translation_after_route_rendering() {
        let script = locale_observer_script(Locale::Ja);
        assert!(script.contains("MutationObserver"));
        assert!(script.contains("ja"));
        assert!(script.contains("__hermesLocaleObserver"));
    }

    #[test]
    fn route_script_prefers_existing_typed_links_before_history_fallback() {
        let script = route_script("/terminal");
        assert!(script.contains("querySelectorAll('a[href]')"));
        assert!(script.contains("/terminal"));
        assert!(script.contains("PopStateEvent"));
    }
}
