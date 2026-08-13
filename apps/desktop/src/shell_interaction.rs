use std::time::Duration;

use dioxus::prelude::*;
use hermes_core::AppServices;
use hermes_protocol::{ConnectionState, RuntimeStatus, TaskSummary};

const SHELL_CSS: &str = r#"
.shell-host{height:100vh;display:flex;min-width:0;overflow:hidden;background:rgb(9 11 16)}
.shell-host-content{position:relative;min-width:0;flex:1;overflow:hidden}
.shell-host.sidebar-hidden .rail{display:none!important}
.shell-host .connection-state{visibility:hidden!important}
.shell-host.status-hidden .shell-status{display:none!important}
.shell-right-rail{width:292px;min-width:292px;border-left:1px solid rgb(51 65 85);background:rgb(12 15 22);color:rgb(226 232 240);display:flex;flex-direction:column;font:12px system-ui,sans-serif}
.shell-right-rail header{height:34px;display:flex;align-items:center;justify-content:space-between;padding:0 10px;border-bottom:1px solid rgb(51 65 85)}
.shell-right-rail header strong{font-size:11px;letter-spacing:.08em;text-transform:uppercase}
.shell-right-rail button,.shell-overlay button,.shell-find button{font:inherit}
.shell-right-close{border:0;background:transparent;color:rgb(148 163 184);cursor:pointer;font-size:18px}
.shell-tool-tabs{display:flex;gap:4px;padding:8px;border-bottom:1px solid rgb(30 41 59)}
.shell-tool-tabs button{flex:1;border:1px solid transparent;border-radius:4px;background:transparent;color:rgb(148 163 184);padding:6px;cursor:pointer}
.shell-tool-tabs button.active{border-color:rgb(71 85 105);background:rgb(30 41 59);color:rgb(241 245 249)}
.shell-tool-body{display:grid;gap:10px;padding:12px;align-content:start}
.shell-tool-body p{margin:0;color:rgb(148 163 184);line-height:1.45}
.shell-tool-body button{justify-self:start;border:1px solid rgb(71 85 105);border-radius:4px;background:rgb(30 41 59);color:rgb(241 245 249);padding:6px 10px;cursor:pointer}
.shell-overlay-backdrop{position:fixed;inset:0;z-index:10000;background:rgb(2 6 23 / .58);display:grid;place-items:start center;padding-top:12vh}
.shell-overlay{width:min(680px,calc(100vw - 32px));max-height:72vh;overflow:hidden;border:1px solid rgb(71 85 105);border-radius:8px;background:rgb(15 23 42);box-shadow:0 24px 80px rgb(0 0 0 / .45);color:rgb(241 245 249);font:13px system-ui,sans-serif}
.shell-overlay header{display:flex;align-items:center;gap:8px;padding:10px;border-bottom:1px solid rgb(51 65 85)}
.shell-overlay header input{min-width:0;flex:1;border:0;outline:0;background:transparent;color:inherit;font:inherit;font-size:14px}
.shell-overlay-list{overflow:auto;max-height:58vh;padding:6px}
.shell-command{width:100%;display:flex;align-items:center;gap:12px;border:0;border-radius:5px;background:transparent;color:inherit;text-align:left;padding:8px 10px;cursor:pointer}
.shell-command.selected,.shell-command:hover{background:rgb(30 41 59)}
.shell-command span{min-width:0;flex:1}.shell-command small{color:rgb(148 163 184)}
.shell-centre-section{padding:8px 10px 2px;color:rgb(148 163 184);font-size:10px;font-weight:700;letter-spacing:.08em;text-transform:uppercase}
.shell-empty{padding:22px;color:rgb(148 163 184);text-align:center}
.shell-find{position:fixed;z-index:10001;right:20px;top:44px;display:flex;align-items:center;gap:6px;padding:6px;border:1px solid rgb(71 85 105);border-radius:6px;background:rgb(15 23 42);box-shadow:0 10px 32px rgb(0 0 0 / .32);font:12px system-ui,sans-serif}
.shell-find input{width:240px;border:1px solid rgb(71 85 105);border-radius:4px;background:rgb(2 6 23);color:rgb(241 245 249);padding:6px 8px;outline:0}
.shell-find button{border:0;background:transparent;color:rgb(203 213 225);cursor:pointer;padding:4px 6px}
.shell-status{position:fixed;z-index:9990;left:0;right:0;bottom:0;height:22px;box-sizing:border-box;display:flex;align-items:center;gap:12px;padding:0 8px;border-top:1px solid rgb(30 41 59);background:rgb(12 15 22);color:rgb(148 163 184);font:11px system-ui,sans-serif;white-space:nowrap;overflow:hidden}
.shell-status .status-brand{color:rgb(226 232 240);font-weight:600}.shell-status .status-spacer{min-width:0;flex:1}.shell-status .online{color:rgb(74 222 128)}.shell-status .connecting{color:rgb(250 204 21)}.shell-status .offline{color:rgb(248 113 113)}.shell-status .runtime-phase{color:rgb(203 213 225)}.shell-status .model{max-width:300px;overflow:hidden;text-overflow:ellipsis}.shell-status .tasks{color:rgb(147 197 253)}
.shell-skip{position:fixed;z-index:10050;left:10px;top:-60px;border:1px solid rgb(96 165 250);border-radius:4px;background:rgb(15 23 42);color:white;padding:8px 10px;font:12px system-ui,sans-serif}
.shell-skip:focus{top:8px}
.shell-host :focus-visible{outline:2px solid rgb(96 165 250)!important;outline-offset:2px}
@media (prefers-reduced-motion:reduce){.shell-host *, .shell-host *::before, .shell-host *::after{scroll-behavior:auto!important;animation-duration:.01ms!important;animation-iteration-count:1!important;transition-duration:.01ms!important}}
"#;

const ZOOM_STORAGE_KEY: &str = "hermes:desktop:zoomLevel";
const ZOOM_FACTOR_BASE: f64 = 1.2;
const MIN_ZOOM_LEVEL: f64 = -9.0;
const MAX_ZOOM_LEVEL: f64 = 9.0;
const ZOOM_STEP: f64 = 0.1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Overlay {
    Palette,
    Centre,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ToolPane {
    Files,
    Terminal,
    Review,
}

impl ToolPane {
    const fn label(self) -> &'static str {
        match self {
            Self::Files => "Files",
            Self::Terminal => "Terminal",
            Self::Review => "Review",
        }
    }

    const fn path(self) -> &'static str {
        match self {
            Self::Files => "/files",
            Self::Terminal => "/terminal",
            Self::Review => "/review",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CommandAction {
    Navigate(&'static str),
    ToggleSidebar,
    ToggleRightRail,
    ToggleStatus,
    OpenFind,
    ZoomIn,
    ZoomOut,
    ZoomReset,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Command {
    id: &'static str,
    label: &'static str,
    category: &'static str,
    shortcut: &'static str,
    action: CommandAction,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct ShellStatusSnapshot {
    gateway: ConnectionState,
    runtime: Option<RuntimeStatus>,
    tasks: Vec<TaskSummary>,
}

const COMMANDS: &[Command] = &[
    Command {
        id: "nav.home",
        label: "Go to Home",
        category: "Navigation",
        shortcut: "",
        action: CommandAction::Navigate("/"),
    },
    Command {
        id: "nav.chat",
        label: "Go to Chat",
        category: "Navigation",
        shortcut: "",
        action: CommandAction::Navigate("/chat"),
    },
    Command {
        id: "nav.projects",
        label: "Go to Projects",
        category: "Navigation",
        shortcut: "",
        action: CommandAction::Navigate("/projects"),
    },
    Command {
        id: "nav.skills",
        label: "Go to Skills",
        category: "Navigation",
        shortcut: "",
        action: CommandAction::Navigate("/skills"),
    },
    Command {
        id: "nav.settings",
        label: "Open Settings",
        category: "Navigation",
        shortcut: "Ctrl/Cmd+,",
        action: CommandAction::Navigate("/settings"),
    },
    Command {
        id: "view.showFiles",
        label: "Show Files",
        category: "View",
        shortcut: "",
        action: CommandAction::Navigate("/files"),
    },
    Command {
        id: "view.showTerminal",
        label: "Show Terminal",
        category: "View",
        shortcut: "Ctrl+`",
        action: CommandAction::Navigate("/terminal"),
    },
    Command {
        id: "view.toggleReview",
        label: "Show Review",
        category: "View",
        shortcut: "Ctrl/Cmd+G",
        action: CommandAction::Navigate("/review"),
    },
    Command {
        id: "view.toggleSidebar",
        label: "Toggle Sidebar",
        category: "View",
        shortcut: "Ctrl/Cmd+B",
        action: CommandAction::ToggleSidebar,
    },
    Command {
        id: "view.toggleRightSidebar",
        label: "Toggle Right Sidebar",
        category: "View",
        shortcut: "Ctrl/Cmd+J",
        action: CommandAction::ToggleRightRail,
    },
    Command {
        id: "view.toggleStatusbar",
        label: "Toggle Status Bar",
        category: "View",
        shortcut: "Ctrl/Cmd+Shift+S",
        action: CommandAction::ToggleStatus,
    },
    Command {
        id: "view.findInPage",
        label: "Find in Page",
        category: "View",
        shortcut: "Ctrl/Cmd+F",
        action: CommandAction::OpenFind,
    },
    Command {
        id: "view.zoomIn",
        label: "Zoom In",
        category: "View",
        shortcut: "Ctrl/Cmd++",
        action: CommandAction::ZoomIn,
    },
    Command {
        id: "view.zoomOut",
        label: "Zoom Out",
        category: "View",
        shortcut: "Ctrl/Cmd+-",
        action: CommandAction::ZoomOut,
    },
    Command {
        id: "view.zoomReset",
        label: "Reset Zoom to 90%",
        category: "View",
        shortcut: "Ctrl/Cmd+0",
        action: CommandAction::ZoomReset,
    },
];

fn normalized_key(key: &Key) -> String {
    match key {
        Key::Character(value) => value.to_lowercase(),
        Key::Escape => "escape".into(),
        Key::Enter => "enter".into(),
        Key::Tab => "tab".into(),
        Key::ArrowDown => "down".into(),
        Key::ArrowUp => "up".into(),
        _ => String::new(),
    }
}

fn is_primary(modifiers: Modifiers) -> bool {
    modifiers.contains(Modifiers::CONTROL) || modifiers.contains(Modifiers::META)
}

fn command_matches(command: &Command, query: &str) -> bool {
    let needle = query.trim().to_lowercase();
    needle.is_empty()
        || command.label.to_lowercase().contains(&needle)
        || command.id.to_lowercase().contains(&needle)
        || command.category.to_lowercase().contains(&needle)
}

fn matching_commands(query: &str) -> Vec<&'static Command> {
    COMMANDS
        .iter()
        .filter(|command| command_matches(command, query))
        .collect()
}

fn default_zoom_level() -> f64 {
    0.9_f64.ln() / ZOOM_FACTOR_BASE.ln()
}

fn clamp_zoom_level(level: f64) -> f64 {
    if level.is_finite() {
        level.clamp(MIN_ZOOM_LEVEL, MAX_ZOOM_LEVEL)
    } else {
        default_zoom_level()
    }
}

fn zoom_factor(level: f64) -> f64 {
    ZOOM_FACTOR_BASE.powf(clamp_zoom_level(level))
}

#[cfg(test)]
fn zoom_percent(level: f64) -> i64 {
    (zoom_factor(level) * 100.0).round() as i64
}

fn gateway_status(state: ConnectionState) -> (&'static str, &'static str) {
    match state {
        ConnectionState::Open => ("online", "Agent connected"),
        ConnectionState::Connecting => ("connecting", "Connecting to Agent"),
        ConnectionState::Error => ("offline", "Agent error"),
        ConnectionState::Idle | ConnectionState::Closed => ("offline", "Agent offline"),
    }
}

fn task_is_active(task: &TaskSummary) -> bool {
    let state = task.state.trim().to_ascii_lowercase();
    !matches!(
        state.as_str(),
        "" | "complete"
            | "completed"
            | "cancelled"
            | "canceled"
            | "failed"
            | "error"
            | "done"
            | "success"
            | "succeeded"
    )
}

fn active_task_count(tasks: &[TaskSummary]) -> usize {
    tasks.iter().filter(|task| task_is_active(task)).count()
}

fn persist_zoom_script(level: f64) -> String {
    let level = clamp_zoom_level(level);
    format!("localStorage.setItem('{ZOOM_STORAGE_KEY}', String({level}));")
}

fn read_zoom_script() -> String {
    let default = default_zoom_level();
    format!(
        "const key='{ZOOM_STORAGE_KEY}',min={MIN_ZOOM_LEVEL},max={MAX_ZOOM_LEVEL},fallback={default}; let level=Number.parseFloat(localStorage.getItem(key)); if(!Number.isFinite(level)) level=fallback; return Math.min(max,Math.max(min,level));"
    )
}

fn run_js(script: String) {
    spawn(async move {
        let _ = document::eval(&script).await;
    });
}

fn navigation_script(path: &str) -> String {
    let path = serde_json::to_string(path).expect("static route serializes");
    format!(
        "(() => {{ const path={path}; const link=[...document.querySelectorAll('a[href]')].find(a => a.getAttribute('href') === path); if (link) {{ link.click(); return; }} history.pushState(null,'',path); window.dispatchEvent(new PopStateEvent('popstate')); }})()"
    )
}

fn find_script(query: &str, backwards: bool) -> String {
    let query = serde_json::to_string(query).unwrap_or_else(|_| "\"\"".into());
    format!(
        "(() => {{ const query={query}; if(query) window.find(query,false,{backwards},true,false,false,false); }})()"
    )
}

fn focus_workspace_script() -> String {
    "(() => { const target=document.querySelector('main.workspace'); if(target){ target.setAttribute('tabindex','-1'); target.focus(); } })()".into()
}

#[component]
pub fn ShellHost(children: Element) -> Element {
    let desktop = dioxus::desktop::window();
    let services = use_context::<AppServices>();
    let mut overlay = use_signal(|| None::<Overlay>);
    let mut palette_query = use_signal(String::new);
    let mut selected = use_signal(|| 0_usize);
    let mut sidebar_visible = use_signal(|| true);
    let mut right_rail_visible = use_signal(|| false);
    let mut status_visible = use_signal(|| true);
    let mut active_tool = use_signal(|| ToolPane::Files);
    let mut find_open = use_signal(|| false);
    let mut find_query = use_signal(String::new);
    let mut zoom_level = use_signal(default_zoom_level);
    let mut shell_status = use_signal(ShellStatusSnapshot::default);

    {
        let desktop = desktop.clone();
        use_effect(move || {
            let desktop = desktop.clone();
            spawn(async move {
                let script = read_zoom_script();
                let level = document::eval(&script)
                    .join::<f64>()
                    .await
                    .map_or_else(|_| default_zoom_level(), clamp_zoom_level);
                zoom_level.set(level);
                desktop.set_zoom_level(zoom_factor(level));
                run_js(persist_zoom_script(level));
            });
        });
    }

    {
        let services = services.clone();
        let _status_poll = use_future(move || {
            let services = services.clone();
            async move {
                loop {
                    let gateway = services
                        .connection
                        .state()
                        .unwrap_or(ConnectionState::Error);
                    let runtime = services.runtime.status().await.ok();
                    let tasks = services.runtime.actions().await.unwrap_or_default();
                    shell_status.set(ShellStatusSnapshot {
                        gateway,
                        runtime,
                        tasks,
                    });
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            }
        });
    }

    let zoom_desktop = desktop.clone();
    let apply_zoom = Callback::new(move |level: f64| {
        let level = clamp_zoom_level(level);
        zoom_level.set(level);
        zoom_desktop.set_zoom_level(zoom_factor(level));
        run_js(persist_zoom_script(level));
    });

    let execute = Callback::new(move |action: CommandAction| {
        match action {
            CommandAction::Navigate(path) => run_js(navigation_script(path)),
            CommandAction::ToggleSidebar => sidebar_visible.toggle(),
            CommandAction::ToggleRightRail => right_rail_visible.toggle(),
            CommandAction::ToggleStatus => status_visible.toggle(),
            CommandAction::OpenFind => find_open.set(true),
            CommandAction::ZoomIn => apply_zoom.call(zoom_level() + ZOOM_STEP),
            CommandAction::ZoomOut => apply_zoom.call(zoom_level() - ZOOM_STEP),
            CommandAction::ZoomReset => apply_zoom.call(default_zoom_level()),
        }
        overlay.set(None);
        palette_query.set(String::new());
        selected.set(0);
    });

    let class = format!(
        "shell-host{}{}",
        if sidebar_visible() {
            ""
        } else {
            " sidebar-hidden"
        },
        if status_visible() {
            ""
        } else {
            " status-hidden"
        },
    );
    let status = shell_status();
    let (gateway_class, gateway_label) = gateway_status(status.gateway);
    let active_tasks = active_task_count(&status.tasks);
    let active_task_label = match active_tasks {
        1 => "1 task active".to_string(),
        count => format!("{count} tasks active"),
    };

    rsx! {
        style { dangerous_inner_html: SHELL_CSS }
        div {
            class,
            onkeydown: move |event: KeyboardEvent| {
                let key = normalized_key(&event.key());
                let modifiers = event.modifiers();
                let primary = is_primary(modifiers);
                let shift = modifiers.contains(Modifiers::SHIFT);
                let ctrl = modifiers.contains(Modifiers::CONTROL);

                if key == "escape" {
                    if overlay().is_some() || find_open() {
                        event.prevent_default();
                        overlay.set(None);
                        find_open.set(false);
                    }
                    return;
                }
                if overlay().is_some() {
                    return;
                }
                if find_open() && primary && key == "g" {
                    event.prevent_default();
                    run_js(find_script(&find_query(), shift));
                    return;
                }
                if primary && matches!(key.as_str(), "k" | "p") {
                    event.prevent_default();
                    overlay.set(Some(Overlay::Palette));
                    palette_query.set(String::new());
                    selected.set(0);
                } else if primary && key == "." {
                    event.prevent_default();
                    overlay.set(Some(Overlay::Centre));
                } else if primary && key == "," {
                    event.prevent_default();
                    run_js(navigation_script("/settings"));
                } else if primary && key == "b" && !shift {
                    event.prevent_default();
                    sidebar_visible.toggle();
                } else if primary && key == "j" {
                    event.prevent_default();
                    right_rail_visible.toggle();
                } else if primary && shift && key == "s" {
                    event.prevent_default();
                    status_visible.toggle();
                } else if primary && key == "f" {
                    event.prevent_default();
                    find_open.set(true);
                } else if primary && key == "g" {
                    event.prevent_default();
                    run_js(navigation_script("/review"));
                } else if ctrl && key == "`" {
                    event.prevent_default();
                    run_js(navigation_script("/terminal"));
                } else if primary && key == "0" {
                    event.prevent_default();
                    execute.call(CommandAction::ZoomReset);
                } else if primary && matches!(key.as_str(), "+" | "=") {
                    event.prevent_default();
                    execute.call(CommandAction::ZoomIn);
                } else if primary && key == "-" {
                    event.prevent_default();
                    execute.call(CommandAction::ZoomOut);
                }
            },
            button {
                class: "shell-skip",
                onclick: move |_| run_js(focus_workspace_script()),
                "Skip to workspace"
            }
            div { class: "shell-host-content", {children} }
            if right_rail_visible() {
                aside { class: "shell-right-rail", aria_label: "Persistent tools",
                    header {
                        strong { "Tools" }
                        button { class: "shell-right-close", aria_label: "Hide right sidebar", title: "Hide right sidebar", onclick: move |_| right_rail_visible.set(false), "×" }
                    }
                    div { class: "shell-tool-tabs", role: "tablist", aria_label: "Tool panes",
                        for tool in [ToolPane::Files, ToolPane::Terminal, ToolPane::Review] {
                            button {
                                class: if active_tool() == tool { "active" } else { "" },
                                role: "tab",
                                aria_selected: active_tool() == tool,
                                onclick: move |_| active_tool.set(tool),
                                "{tool.label()}"
                            }
                        }
                    }
                    div { class: "shell-tool-body",
                        strong { "{active_tool().label()}" }
                        p { "Persistent shell tool selection is retained while the rail is hidden. Open the full workspace when you need the complete tool surface." }
                        button { onclick: move |_| execute.call(CommandAction::Navigate(active_tool().path())), "Open {active_tool().label()}" }
                    }
                }
            }
            footer { class: "shell-status", aria_live: "polite",
                span { class: "status-brand", "Hermes Local" }
                span { class: "{gateway_class}", "● {gateway_label}" }
                if let Some(runtime) = &status.runtime {
                    if !runtime.phase.trim().is_empty() {
                        span { class: "runtime-phase", title: "Runtime phase", "{runtime.phase}" }
                    }
                    if let Some(model) = runtime.model.as_deref().filter(|value| !value.is_empty()) {
                        span { class: "model", title: "Active model", "{model}" }
                    }
                    if let Some(provider) = runtime.provider.as_deref().filter(|value| !value.is_empty()) {
                        span { title: "Model provider", "{provider}" }
                    }
                }
                span { class: "status-spacer" }
                if active_tasks > 0 {
                    span { class: "tasks", title: "Active runtime tasks", "{active_task_label}" }
                }
                span { "UTF-8" }
            }
        }
        if find_open() {
            div { class: "shell-find", role: "search", aria_label: "Find in page",
                input {
                    autofocus: true,
                    aria_label: "Find text",
                    placeholder: "Find",
                    value: "{find_query}",
                    oninput: move |event| {
                        find_query.set(event.value());
                        if !find_query().is_empty() { run_js(find_script(&find_query(), false)); }
                    },
                    onkeydown: move |event| {
                        event.stop_propagation();
                        match event.key() {
                            Key::Enter => { event.prevent_default(); run_js(find_script(&find_query(), event.modifiers().contains(Modifiers::SHIFT))); }
                            Key::Escape => { event.prevent_default(); find_open.set(false); }
                            _ => {}
                        }
                    }
                }
                button { aria_label: "Previous match", title: "Previous match", onclick: move |_| run_js(find_script(&find_query(), true)), "↑" }
                button { aria_label: "Next match", title: "Next match", onclick: move |_| run_js(find_script(&find_query(), false)), "↓" }
                button { aria_label: "Close find", title: "Close find", onclick: move |_| find_open.set(false), "×" }
            }
        }
        if let Some(kind) = overlay() {
            div { class: "shell-overlay-backdrop", role: "presentation", onclick: move |_| overlay.set(None),
                section {
                    class: "shell-overlay",
                    role: "dialog",
                    aria_modal: "true",
                    aria_label: if kind == Overlay::Palette { "Command palette" } else { "Command Centre" },
                    onclick: move |event| event.stop_propagation(),
                    if kind == Overlay::Palette {
                        header {
                            span { aria_hidden: "true", "⌘" }
                            input {
                                autofocus: true,
                                aria_label: "Search commands",
                                placeholder: "Type a command",
                                value: "{palette_query}",
                                oninput: move |event| { palette_query.set(event.value()); selected.set(0); },
                                onkeydown: move |event| {
                                    event.stop_propagation();
                                    let matches = matching_commands(&palette_query());
                                    match event.key() {
                                        Key::ArrowDown if !matches.is_empty() => { event.prevent_default(); selected.set((selected() + 1) % matches.len()); }
                                        Key::ArrowUp if !matches.is_empty() => { event.prevent_default(); selected.set((selected() + matches.len() - 1) % matches.len()); }
                                        Key::Enter => {
                                            event.prevent_default();
                                            if let Some(command) = matches.get(selected().min(matches.len().saturating_sub(1))) { execute.call(command.action); }
                                        }
                                        Key::Escape => { event.prevent_default(); overlay.set(None); }
                                        _ => {}
                                    }
                                }
                            }
                            small { "Ctrl/Cmd+K" }
                        }
                        div { class: "shell-overlay-list", role: "listbox", aria_label: "Commands",
                            {
                                let matches = matching_commands(&palette_query());
                                if matches.is_empty() {
                                    rsx! { div { class: "shell-empty", "No matching commands" } }
                                } else {
                                    rsx! {
                                        for (index, command) in matches.into_iter().enumerate() {
                                            button {
                                                class: if selected() == index { "shell-command selected" } else { "shell-command" },
                                                role: "option",
                                                aria_selected: selected() == index,
                                                onclick: move |_| execute.call(command.action),
                                                span { "{command.label}" }
                                                small { "{command.shortcut}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        header { strong { "Command Centre" } span { style: "flex:1" } small { "Ctrl/Cmd+." } }
                        div { class: "shell-overlay-list",
                            for category in ["Navigation", "View"] {
                                div { class: "shell-centre-section", "{category}" }
                                for command in COMMANDS.iter().filter(|command| command.category == category) {
                                    button { class: "shell-command", onclick: move |_| execute.call(command.action),
                                        span { "{command.label}" }
                                        small { "{command.shortcut}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn command_registry_has_stable_unique_ids() {
        let ids = COMMANDS
            .iter()
            .map(|command| command.id)
            .collect::<BTreeSet<_>>();
        assert_eq!(ids.len(), COMMANDS.len());
        assert!(ids.contains("nav.settings"));
        assert!(ids.contains("view.findInPage"));
        assert!(ids.contains("view.showTerminal"));
    }

    #[test]
    fn command_search_matches_id_label_and_category() {
        assert!(command_matches(&COMMANDS[0], "home"));
        assert!(command_matches(&COMMANDS[0], "nav.home"));
        assert_eq!(matching_commands("Navigation").len(), 5);
        assert_eq!(matching_commands("no-such-command").len(), 0);
    }

    #[test]
    fn zoom_contract_matches_the_og_scale() {
        assert_eq!(zoom_percent(default_zoom_level()), 90);
        assert_eq!(zoom_percent(f64::NAN), 90);
        assert!((zoom_factor(default_zoom_level()) - 0.9).abs() < f64::EPSILON * 8.0);
        assert_eq!(clamp_zoom_level(-100.0), MIN_ZOOM_LEVEL);
        assert_eq!(clamp_zoom_level(100.0), MAX_ZOOM_LEVEL);
        assert!(zoom_percent(MAX_ZOOM_LEVEL) > zoom_percent(MIN_ZOOM_LEVEL));
    }

    #[test]
    fn gateway_status_maps_every_connection_state() {
        assert_eq!(
            gateway_status(ConnectionState::Open),
            ("online", "Agent connected")
        );
        assert_eq!(
            gateway_status(ConnectionState::Connecting),
            ("connecting", "Connecting to Agent")
        );
        assert_eq!(
            gateway_status(ConnectionState::Error),
            ("offline", "Agent error")
        );
        assert_eq!(
            gateway_status(ConnectionState::Closed),
            ("offline", "Agent offline")
        );
        assert_eq!(
            gateway_status(ConnectionState::Idle),
            ("offline", "Agent offline")
        );
    }

    #[test]
    fn active_task_count_excludes_terminal_states() {
        let tasks = [
            TaskSummary {
                state: "running".into(),
                ..TaskSummary::default()
            },
            TaskSummary {
                state: "queued".into(),
                ..TaskSummary::default()
            },
            TaskSummary {
                state: "completed".into(),
                ..TaskSummary::default()
            },
            TaskSummary {
                state: "failed".into(),
                ..TaskSummary::default()
            },
        ];
        assert_eq!(active_task_count(&tasks), 2);
    }

    #[test]
    fn tool_panes_route_to_existing_typed_destinations() {
        assert_eq!(ToolPane::Files.path(), "/files");
        assert_eq!(ToolPane::Terminal.path(), "/terminal");
        assert_eq!(ToolPane::Review.path(), "/review");
    }
}
