//! Dioxus presentation layer. This crate has no filesystem, process, or OS authority.

use std::{collections::BTreeMap, sync::Arc};

use dioxus::prelude::*;
use futures_util::StreamExt;
use hermes_core::{AgentConfigService, AppServices, ModelService, SessionTranscript};
use hermes_protocol::{
    AgentConfigSnapshot, AppSettings, MessageRole, ModelAssignmentRequest, ModelSettingsSnapshot,
    ProjectsSnapshot, SessionCreateRequest, SessionSummary, ThemeMode,
};
use serde_json::{Map, Value, json};

const APP_CSS: &str = include_str!("../assets/app.css");
const CODICON_SPRITE: &str = include_str!("../assets/codicon-sprite.svg");

/// Platform window commands supplied by a composition root. The shared UI does
/// not import a desktop/windowing crate and a future Web host can supply its own
/// implementation.
#[derive(Clone)]
pub struct WindowActions {
    pub drag: Callback<()>,
    pub minimize: Callback<()>,
    pub toggle_maximized: Callback<()>,
    pub close: Callback<()>,
}

#[derive(Clone, Copy)]
struct ProjectUiState {
    snapshot: Signal<ProjectsSnapshot>,
    loading: Signal<bool>,
    error: Signal<Option<String>>,
    refresh: Signal<u64>,
}

#[derive(Clone, Copy)]
struct SettingsUiState {
    settings: Signal<AppSettings>,
    loading: Signal<bool>,
    error: Signal<Option<String>>,
}

#[derive(Clone, Debug, PartialEq, Routable)]
pub enum Route {
    #[layout(AppShell)]
    #[route("/")]
    Overview {},
    #[route("/chat")]
    Chat {},
    #[route("/tui")]
    Tui {},
    #[route("/dashboard")]
    Dashboard {},
    #[route("/session/:id")]
    Session { id: String },
    #[route("/projects")]
    Projects {},
    #[route("/project/:id")]
    Project { id: String },
    #[route("/files")]
    Files {},
    #[route("/git")]
    Git {},
    #[route("/worktrees")]
    Worktrees {},
    #[route("/review")]
    Review {},
    #[route("/terminal")]
    Terminal {},
    #[route("/tasks")]
    Tasks {},
    #[route("/services")]
    Services {},
    #[route("/models")]
    Models {},
    #[route("/profiles")]
    Profiles {},
    #[route("/tools")]
    Tools {},
    #[route("/memory")]
    Memory {},
    #[route("/sessions")]
    Sessions {},
    #[route("/integrations")]
    Integrations {},
    #[route("/benchmarks")]
    Benchmarks {},
    #[route("/security")]
    Security {},
    #[route("/logs")]
    Logs {},
    #[route("/model")]
    Model {},
    #[route("/runtime")]
    Runtime {},
    #[route("/trust")]
    Trust {},
    #[route("/skills")]
    Skills {},
    #[route("/mcp")]
    Mcp {},
    #[route("/delegations")]
    Delegations {},
    #[route("/cloud")]
    Cloud {},
    #[route("/usage")]
    Usage {},
    #[route("/automations")]
    Automations {},
    #[route("/notifications")]
    Notifications {},
    #[route("/quick-entry")]
    QuickEntry {},
    #[route("/settings")]
    Settings {},
    #[route("/settings/appearance")]
    Appearance {},
    #[route("/settings/general")]
    GeneralSettings {},
    #[route("/settings/provider")]
    ProviderSettings {},
    #[route("/settings/updates")]
    UpdateSettings {},
    #[route("/about")]
    About {},
    #[end_layout]
    #[route("/:..segments")]
    NotFound { segments: Vec<String> },
}

#[component]
pub fn App() -> Element {
    let services = use_context::<AppServices>();
    let connection = services.connection.clone();
    let settings_service = services.settings.clone();
    let mut app_settings = use_signal(AppSettings::default);
    let mut settings_loading = use_signal(|| true);
    let mut settings_error = use_signal(|| None::<String>);
    let boot = use_resource(move || {
        let connection = connection.clone();
        async move { connection.initialize().await }
    });
    use_context_provider(|| boot);
    let _settings = use_resource(move || {
        let settings_service = settings_service.clone();
        async move {
            settings_loading.set(true);
            match settings_service.load().await {
                Ok(settings) => {
                    app_settings.set(settings);
                    settings_error.set(None);
                }
                Err(error) => settings_error.set(Some(error.to_string())),
            }
            settings_loading.set(false);
        }
    });
    use_context_provider(|| SettingsUiState {
        settings: app_settings,
        loading: settings_loading,
        error: settings_error,
    });
    let theme_class = match app_settings().theme {
        ThemeMode::Dark => " theme-dark",
        ThemeMode::Light => " theme-light",
        ThemeMode::System => "",
    };
    let skin_class = format!(
        " skin-{}",
        app_settings().theme_name.as_deref().unwrap_or("nous")
    );
    rsx! {
        style { dangerous_inner_html: APP_CSS }
        div { class: "icon-sprite", aria_hidden: "true", dangerous_inner_html: CODICON_SPRITE }
        div { class: "window-root{theme_class}{skin_class}",
            Titlebar {}
            Router::<Route> {}
            footer { class: "connection-state",
                span { class: "status-brand", "Hermes Local" }
                match &*boot.read_unchecked() {
                    Some(Ok(_)) => rsx! { span { class: "online", "● Agent connected" } },
                    Some(Err(_)) => rsx! { span { class: "offline", "○ Agent offline" } },
                    None => rsx! { span { class: "connecting", "◌ Connecting to Agent" } },
                }
                span { class: "status-spacer" }
                span { "Local workstation" }
                span { "UTF-8" }
            }
        }
    }
}

#[component]
fn Titlebar() -> Element {
    let actions = use_context::<WindowActions>();
    let drag = actions.drag;
    let minimize = actions.minimize;
    let toggle_maximized = actions.toggle_maximized;
    let close = actions.close;
    rsx! {
        header {
            class: "titlebar",
            onmousedown: move |_| drag.call(()),
            ondoubleclick: move |_| toggle_maximized.call(()),
            div { class: "titlebar-brand", span { class: "app-symbol", "✣" } span { "Hermes Local" } }
            div { class: "titlebar-drag", aria_hidden: "true" }
            div { class: "window-controls", onmousedown: move |event| event.stop_propagation(), ondoubleclick: move |event| event.stop_propagation(),
                button { aria_label: "Minimize", title: "Minimize", onclick: move |event| { event.stop_propagation(); minimize.call(()); }, "—" }
                button { aria_label: "Maximize", title: "Maximize", onclick: move |event| { event.stop_propagation(); toggle_maximized.call(()); }, "□" }
                button { class: "window-close", aria_label: "Close", title: "Close", onclick: move |event| { event.stop_propagation(); close.call(()); }, "×" }
            }
        }
    }
}

#[component]
fn AppShell() -> Element {
    let services = use_context::<AppServices>();
    let boot =
        use_context::<Resource<hermes_core::ServiceResult<hermes_protocol::ConnectionState>>>();
    let session_service = services.sessions.clone();
    let project_service = services.projects.clone();
    let mut session_rows = use_signal(Vec::<SessionSummary>::new);
    let mut sessions_loading = use_signal(|| true);
    let mut sessions_error = use_signal(|| None::<String>);
    let mut refresh = use_signal(|| 0_u64);
    let mut query = use_signal(String::new);
    let mut menu_session = use_signal(|| None::<String>);
    let mut rename_session = use_signal(|| None::<String>);
    let mut rename_value = use_signal(String::new);
    let mut delete_session = use_signal(|| None::<String>);
    let mut mutation_epoch = use_signal(|| 0_u64);
    let mut project_snapshot = use_signal(ProjectsSnapshot::default);
    let mut projects_loading = use_signal(|| false);
    let mut projects_error = use_signal(|| None::<String>);
    let projects_refresh = use_signal(|| 0_u64);
    use_context_provider(|| ProjectUiState {
        snapshot: project_snapshot,
        loading: projects_loading,
        error: projects_error,
        refresh: projects_refresh,
    });
    let _sessions = use_resource(move || {
        let _ = refresh();
        let connected = matches!(
            &*boot.read(),
            Some(Ok(hermes_protocol::ConnectionState::Open))
        );
        let session_service = session_service.clone();
        async move {
            if !connected {
                sessions_loading.set(false);
                sessions_error.set(None);
                return;
            }
            sessions_loading.set(true);
            match session_service.list().await {
                Ok(rows) => {
                    session_rows.set(rows);
                    sessions_error.set(None);
                }
                Err(hermes_core::ServiceError::Unavailable(_)) => sessions_error.set(None),
                Err(error) => sessions_error.set(Some(error.to_string())),
            }
            sessions_loading.set(false);
        }
    });
    let _projects = use_resource(move || {
        let _ = projects_refresh();
        let connected = matches!(
            &*boot.read(),
            Some(Ok(hermes_protocol::ConnectionState::Open))
        );
        let project_service = project_service.clone();
        async move {
            if !connected {
                projects_loading.set(false);
                projects_error.set(None);
                return;
            }
            projects_loading.set(true);
            match project_service.snapshot().await {
                Ok(snapshot) => {
                    project_snapshot.set(snapshot);
                    projects_error.set(None);
                }
                Err(error) => projects_error.set(Some(error.to_string())),
            }
            projects_loading.set(false);
        }
    });

    let normalized_query = query().trim().to_lowercase();
    let visible_sessions = session_rows()
        .into_iter()
        .filter(|session| {
            !session.archived
                && (normalized_query.is_empty()
                    || session.title.to_lowercase().contains(&normalized_query)
                    || session
                        .cwd
                        .as_deref()
                        .is_some_and(|cwd| cwd.to_lowercase().contains(&normalized_query))
                    || session
                        .model
                        .as_deref()
                        .is_some_and(|model| model.to_lowercase().contains(&normalized_query)))
        })
        .collect::<Vec<_>>();
    let pinned_sessions = visible_sessions
        .iter()
        .filter(|session| session.pinned)
        .cloned()
        .collect::<Vec<_>>();
    let recent_sessions = visible_sessions
        .iter()
        .filter(|session| !session.pinned)
        .take(12)
        .cloned()
        .collect::<Vec<_>>();
    rsx! {
        div { class: "app-shell",
            aside { class: "rail", aria_label: "Hermes navigation",
                nav { class: "primary-nav", aria_label: "Primary navigation",
                    NavItem { to: Route::Overview {}, icon: "home", label: "Home" }
                    NavItem { to: Route::Chat {}, icon: "comment-discussion", label: "Chat" }
                    NavItem { to: Route::Tui {}, icon: "terminal", label: "TUI" }
                    NavItem { to: Route::Dashboard {}, icon: "dashboard", label: "Web Dashboard" }
                    NavItem { to: Route::Tasks {}, icon: "checklist", label: "Tasks" }
                    NavItem { to: Route::Services {}, icon: "server-process", label: "Services" }
                    NavItem { to: Route::Models {}, icon: "hubot", label: "Models" }
                    NavItem { to: Route::Profiles {}, icon: "settings-gear", label: "Profiles" }
                    NavItem { to: Route::Tools {}, icon: "tools", label: "Tools" }
                    NavItem { to: Route::Memory {}, icon: "database", label: "Memory" }
                    NavItem { to: Route::Skills {}, icon: "symbol-misc", label: "Skills" }
                    NavItem { to: Route::Sessions {}, icon: "history", label: "Sessions" }
                    NavItem { to: Route::Projects {}, icon: "project", label: "Projects" }
                    NavItem { to: Route::Integrations {}, icon: "plug", label: "Integrations" }
                    NavItem { to: Route::Benchmarks {}, icon: "graph-line", label: "Benchmarks" }
                    NavItem { to: Route::Security {}, icon: "shield", label: "Security" }
                    NavItem { to: Route::Logs {}, icon: "output", label: "Logs" }
                }
                div { class: "sidebar-search",
                    Codicon { name: "search" }
                    input {
                        aria_label: "Search sessions",
                        placeholder: "Search sessions",
                        value: "{query}",
                        oninput: move |event| query.set(event.value())
                    }
                }
                section { class: "sidebar-sessions", aria_label: "Sessions",
                    if !normalized_query.is_empty() {
                        div { class: "sidebar-section-label", "RESULTS" }
                    } else {
                        div { class: "sidebar-section-label", "PINNED" }
                        if pinned_sessions.is_empty() {
                            p { class: "sidebar-empty", "No pinned sessions" }
                        }
                        for session in pinned_sessions {
                            SidebarSessionRow {
                                session: session.clone(),
                                menu_open: menu_session().as_deref() == Some(session.id.as_str()),
                                renaming: rename_session().as_deref() == Some(session.id.as_str()),
                                rename_value: rename_value(),
                                on_open_menu: move |id: String| menu_session.set(Some(id)),
                                on_close_menu: move |()| menu_session.set(None),
                                on_start_rename: move |(id, title): (String, String)| { rename_session.set(Some(id)); rename_value.set(title); menu_session.set(None); },
                                on_rename_input: move |value: String| rename_value.set(value),
                                on_cancel_rename: move |()| rename_session.set(None),
                                on_commit_rename: {
                                    let service = services.sessions.clone();
                                    move |(id, runtime_id): (String, Option<String>)| {
                                        let title = rename_value().trim().to_owned();
                                        let before = session_rows();
                                        let epoch = mutation_epoch() + 1;
                                        mutation_epoch.set(epoch);
                                        for row in session_rows.write().iter_mut().filter(|row| row.id == id) { row.title.clone_from(&title); }
                                        rename_session.set(None);
                                        let service = service.clone();
                                        spawn(async move {
                                            if let Err(error) = service.rename(&id, runtime_id.as_deref(), &title).await {
                                                if mutation_epoch() == epoch { session_rows.set(before); }
                                                sessions_error.set(Some(error.to_string()));
                                            }
                                        });
                                    }
                                },
                                on_toggle_pin: {
                                    let service = services.sessions.clone();
                                    move |(id, durable_id, pinned): (String, String, bool)| {
                                        let before = session_rows();
                                        let epoch = mutation_epoch() + 1;
                                        mutation_epoch.set(epoch);
                                        for row in session_rows.write().iter_mut().filter(|row| row.id == id) { row.pinned = pinned; }
                                        menu_session.set(None);
                                        let service = service.clone();
                                        spawn(async move {
                                            if let Err(error) = service.set_pinned(&durable_id, pinned).await {
                                                if mutation_epoch() == epoch { session_rows.set(before); }
                                                sessions_error.set(Some(error.to_string()));
                                            }
                                        });
                                    }
                                },
                                on_archive: {
                                    let service = services.sessions.clone();
                                    move |id: String| {
                                        let before = session_rows();
                                        let epoch = mutation_epoch() + 1;
                                        mutation_epoch.set(epoch);
                                        session_rows.write().retain(|row| row.id != id);
                                        menu_session.set(None);
                                        let service = service.clone();
                                        spawn(async move {
                                            if let Err(error) = service.set_archived(&id, true).await {
                                                if mutation_epoch() == epoch { session_rows.set(before); }
                                                sessions_error.set(Some(error.to_string()));
                                            }
                                        });
                                    }
                                },
                                on_request_delete: move |id: String| { delete_session.set(Some(id)); menu_session.set(None); },
                            }
                        }
                        div { class: "sidebar-section-label recent", "RECENT" }
                    }
                    for session in recent_sessions {
                        SidebarSessionRow {
                            session: session.clone(),
                            menu_open: menu_session().as_deref() == Some(session.id.as_str()),
                            renaming: rename_session().as_deref() == Some(session.id.as_str()),
                            rename_value: rename_value(),
                            on_open_menu: move |id: String| menu_session.set(Some(id)),
                            on_close_menu: move |()| menu_session.set(None),
                            on_start_rename: move |(id, title): (String, String)| { rename_session.set(Some(id)); rename_value.set(title); menu_session.set(None); },
                            on_rename_input: move |value: String| rename_value.set(value),
                            on_cancel_rename: move |()| rename_session.set(None),
                            on_commit_rename: {
                                let service = services.sessions.clone();
                                move |(id, runtime_id): (String, Option<String>)| {
                                    let title = rename_value().trim().to_owned();
                                    let before = session_rows();
                                    let epoch = mutation_epoch() + 1;
                                    mutation_epoch.set(epoch);
                                    for row in session_rows.write().iter_mut().filter(|row| row.id == id) { row.title.clone_from(&title); }
                                    rename_session.set(None);
                                    let service = service.clone();
                                    spawn(async move {
                                        if let Err(error) = service.rename(&id, runtime_id.as_deref(), &title).await {
                                            if mutation_epoch() == epoch { session_rows.set(before); }
                                            sessions_error.set(Some(error.to_string()));
                                        }
                                    });
                                }
                            },
                            on_toggle_pin: {
                                let service = services.sessions.clone();
                                move |(id, durable_id, pinned): (String, String, bool)| {
                                    let before = session_rows();
                                    let epoch = mutation_epoch() + 1;
                                    mutation_epoch.set(epoch);
                                    for row in session_rows.write().iter_mut().filter(|row| row.id == id) { row.pinned = pinned; }
                                    menu_session.set(None);
                                    let service = service.clone();
                                    spawn(async move {
                                        if let Err(error) = service.set_pinned(&durable_id, pinned).await {
                                            if mutation_epoch() == epoch { session_rows.set(before); }
                                            sessions_error.set(Some(error.to_string()));
                                        }
                                    });
                                }
                            },
                            on_archive: {
                                let service = services.sessions.clone();
                                move |id: String| {
                                    let before = session_rows();
                                    let epoch = mutation_epoch() + 1;
                                    mutation_epoch.set(epoch);
                                    session_rows.write().retain(|row| row.id != id);
                                    menu_session.set(None);
                                    let service = service.clone();
                                    spawn(async move {
                                        if let Err(error) = service.set_archived(&id, true).await {
                                            if mutation_epoch() == epoch { session_rows.set(before); }
                                            sessions_error.set(Some(error.to_string()));
                                        }
                                    });
                                }
                            },
                            on_request_delete: move |id: String| { delete_session.set(Some(id)); menu_session.set(None); },
                        }
                    }
                    if sessions_loading() {
                        p { class: "sidebar-empty", "Loading sessions…" }
                    } else if visible_sessions.is_empty() {
                        p { class: "sidebar-empty", if normalized_query.is_empty() { "No recent sessions" } else { "No matching sessions" } }
                    }
                    if let Some(error) = sessions_error() {
                        div { class: "sidebar-session-error", role: "alert",
                            span { "{error}" }
                            button { title: "Retry", aria_label: "Retry loading sessions", onclick: move |_| { sessions_error.set(None); refresh += 1; }, Codicon { name: "refresh" } }
                        }
                    }
                }
                nav { class: "secondary-nav", aria_label: "Application navigation",
                    NavItem { to: Route::Settings {}, icon: "settings", label: "Settings" }
                    NavItem { to: Route::About {}, icon: "info", label: "About" }
                }
                div { class: "sidebar-footer", span { class: "avatar", "C" } span { "Local profile" } span { class: "chevron", "›" } }
                if let Some(id) = delete_session() {
                    div { class: "session-delete-confirm", role: "alertdialog", aria_label: "Delete session?",
                        strong { "Delete session?" }
                        span { "This cannot be undone." }
                        div {
                            button { onclick: move |_| delete_session.set(None), "Cancel" }
                            button {
                                class: "danger",
                                onclick: {
                                    let service = services.sessions.clone();
                                    move |_| {
                                        let id = id.clone();
                                        let before = session_rows();
                                        let epoch = mutation_epoch() + 1;
                                        mutation_epoch.set(epoch);
                                        session_rows.write().retain(|row| row.id != id);
                                        delete_session.set(None);
                                        let service = service.clone();
                                        spawn(async move {
                                            if let Err(error) = service.delete(&id).await {
                                                if mutation_epoch() == epoch { session_rows.set(before); }
                                                sessions_error.set(Some(error.to_string()));
                                            }
                                        });
                                    }
                                },
                                "Delete"
                            }
                        }
                    }
                }
            }
            main { class: "workspace", Outlet::<Route> {} }
        }
    }
}

#[component]
fn SidebarSessionRow(
    session: SessionSummary,
    menu_open: bool,
    renaming: bool,
    rename_value: String,
    on_open_menu: EventHandler<String>,
    on_close_menu: EventHandler<()>,
    on_start_rename: EventHandler<(String, String)>,
    on_rename_input: EventHandler<String>,
    on_cancel_rename: EventHandler<()>,
    on_commit_rename: EventHandler<(String, Option<String>)>,
    on_toggle_pin: EventHandler<(String, String, bool)>,
    on_archive: EventHandler<String>,
    on_request_delete: EventHandler<String>,
) -> Element {
    let id = session.id.clone();
    let durable_id = session
        .lineage_root
        .clone()
        .unwrap_or_else(|| session.id.clone());
    let title = if session.title.is_empty() {
        "Untitled session".to_owned()
    } else {
        session.title.clone()
    };
    let runtime_id = session.runtime_id.clone();
    rsx! {
        div {
            class: "session-row",
            oncontextmenu: {
                let id = id.clone();
                move |event| { event.prevent_default(); on_open_menu.call(id.clone()); }
            },
            if renaming {
                input {
                    class: "session-rename",
                    aria_label: "Rename session",
                    value: "{rename_value}",
                    autofocus: true,
                    oninput: move |event| on_rename_input.call(event.value()),
                    onkeydown: {
                        let id = id.clone();
                        move |event| match event.key() {
                            Key::Enter => on_commit_rename.call((id.clone(), runtime_id.clone())),
                            Key::Escape => on_cancel_rename.call(()),
                            _ => {}
                        }
                    }
                }
            } else {
                Link { class: "session-link", to: Route::Session { id: id.clone() }, title: "{title}",
                    span { class: if session.running { "session-dot running" } else { "session-dot" } }
                    span { "{title}" }
                }
                button {
                    class: "session-actions-button",
                    aria_label: "Session actions",
                    title: "Session actions",
                    onclick: {
                        let id = id.clone();
                        move |event| { event.stop_propagation(); if menu_open { on_close_menu.call(()) } else { on_open_menu.call(id.clone()) } }
                    },
                    Codicon { name: "kebab-vertical" }
                }
            }
            if menu_open {
                div { class: "session-menu", role: "menu",
                    button { role: "menuitem", onclick: {
                        let id = id.clone(); let title = session.title.clone();
                        move |_| on_start_rename.call((id.clone(), title.clone()))
                    }, Codicon { name: "edit" } "Rename" }
                    button { role: "menuitem", onclick: {
                        let id = id.clone(); let durable_id = durable_id.clone(); let pinned = !session.pinned;
                        move |_| on_toggle_pin.call((id.clone(), durable_id.clone(), pinned))
                    }, Codicon { name: if session.pinned { "pinned-dirty" } else { "pin" } } if session.pinned { "Unpin" } else { "Pin" } }
                    button { role: "menuitem", onclick: { let id = id.clone(); move |_| on_archive.call(id.clone()) }, Codicon { name: "archive" } "Archive" }
                    div { class: "session-menu-separator" }
                    button { class: "danger", role: "menuitem", onclick: { let id = id.clone(); move |_| on_request_delete.call(id.clone()) }, Codicon { name: "trash" } "Delete" }
                }
            }
        }
    }
}

#[component]
fn NavItem(to: Route, icon: &'static str, label: &'static str) -> Element {
    rsx! {
        Link { class: "nav-item", to, aria_label: label, title: label,
            span { class: "nav-glyph", Codicon { name: icon } }
            span { class: "nav-label", "{label}" }
        }
    }
}

#[component]
fn Codicon(name: String) -> Element {
    let reference = format!("#{name}");
    rsx! {
        svg { class: "codicon", view_box: "0 0 16 16", "aria-hidden": "true", "focusable": "false",
            r#use { href: reference }
        }
    }
}

#[component]
fn ProjectPicker() -> Element {
    let services = use_context::<AppServices>();
    let state = use_context::<ProjectUiState>();
    let mut open = use_signal(|| false);
    let snapshot = (state.snapshot)();
    let active = snapshot
        .active_id
        .as_ref()
        .and_then(|id| snapshot.projects.iter().find(|project| &project.id == id));
    let label = active.map_or_else(
        || "No project".to_owned(),
        |project| {
            if project.name.is_empty() {
                "Unnamed project".to_owned()
            } else {
                project.name.clone()
            }
        },
    );
    rsx! {
        div { class: "project-picker",
            button {
                class: "composer-project",
                aria_expanded: open(),
                aria_label: "Select project",
                onclick: move |_| open.toggle(),
                Codicon { name: "folder" }
                span { "{label}" }
                Codicon { name: "chevron-down" }
            }
            if open() {
                div { class: "project-picker-menu", role: "menu",
                    button {
                        class: if snapshot.active_id.is_none() { "selected" } else { "" },
                        role: "menuitem",
                        onclick: {
                            let service = services.projects.clone();
                            let before = snapshot.clone();
                            move |_| {
                                let mut next = before.clone();
                                next.active_id = None;
                                let mut snapshot_signal = state.snapshot;
                                let mut error_signal = state.error;
                                snapshot_signal.set(next);
                                open.set(false);
                                let service = service.clone();
                                let before = before.clone();
                                spawn(async move {
                                    if let Err(error) = service.set_active(None).await {
                                        snapshot_signal.set(before);
                                        error_signal.set(Some(error.to_string()));
                                    }
                                });
                            }
                        },
                        Codicon { name: "circle-slash" }
                        "No project"
                    }
                    for project in snapshot.projects.iter().filter(|project| !project.archived) {
                        button {
                            class: if snapshot.active_id.as_deref() == Some(project.id.as_str()) { "selected" } else { "" },
                            role: "menuitem",
                            onclick: {
                                let service = services.projects.clone();
                                let before = snapshot.clone();
                                let id = project.id.clone();
                                move |_| {
                                    let mut next = before.clone();
                                    next.active_id = Some(id.clone());
                                    let mut snapshot_signal = state.snapshot;
                                    let mut error_signal = state.error;
                                    snapshot_signal.set(next);
                                    open.set(false);
                                    let service = service.clone();
                                    let before = before.clone();
                                    let id = id.clone();
                                    spawn(async move {
                                        if let Err(error) = service.set_active(Some(&id)).await {
                                            snapshot_signal.set(before);
                                            error_signal.set(Some(error.to_string()));
                                        }
                                    });
                                }
                            },
                            Codicon { name: "project" }
                            span { if project.name.is_empty() { "Unnamed project" } else { "{project.name}" } }
                        }
                    }
                    div { class: "project-picker-separator" }
                    Link { class: "project-picker-manage", to: Route::Projects {}, onclick: move |_| open.set(false), Codicon { name: "settings" } "Manage projects" }
                }
            }
        }
    }
}

#[component]
fn Chat() -> Element {
    let services = use_context::<AppServices>();
    let projects = use_context::<ProjectUiState>();
    let create_service = services.sessions.clone();
    let navigator = use_navigator();
    let mut prompt = use_signal(String::new);
    let mut submitting = use_signal(|| false);
    let mut submit_error = use_signal(|| None::<String>);
    let send = Callback::new(move |()| {
        let service = create_service.clone();
        let text = prompt().trim().to_owned();
        if text.is_empty() || submitting() {
            return;
        }
        submitting.set(true);
        submit_error.set(None);
        let snapshot = (projects.snapshot)();
        let project_id = snapshot.active_id.clone();
        let cwd = project_id.as_ref().and_then(|active_id| {
            snapshot
                .projects
                .iter()
                .find(|project| &project.id == active_id)
                .and_then(|project| project.primary_path.clone())
        });
        spawn(async move {
            let result = async {
                let session = service
                    .create(SessionCreateRequest {
                        cwd,
                        project_id,
                        ..SessionCreateRequest::default()
                    })
                    .await?;
                service
                    .submit(session.runtime_id.as_deref().unwrap_or(&session.id), &text)
                    .await?;
                Ok::<_, hermes_core::ServiceError>(session.id)
            }
            .await;
            submitting.set(false);
            match result {
                Ok(id) => {
                    prompt.set(String::new());
                    navigator.push(Route::Session { id });
                }
                Err(error) => submit_error.set(Some(error.to_string())),
            }
        });
    });

    rsx! {
        section { class: "new-chat-surface",
            div { class: "new-chat-hero",
                h1 { "HERMES AGENT" }
                p { "Send a prompt to trigger tool calls. Supports multi-file edits, test runs, git ops, and web fetches." }
            }
            div { class: "chat-composer-dock",
                ProjectPicker {}
                div { class: "composer-card",
                    button { class: "composer-tool", title: "Attach", aria_label: "Attach", Codicon { name: "add" } }
                    textarea {
                        aria_label: "Start a conversation",
                        placeholder: "What are we building?",
                        rows: "1",
                        value: "{prompt}",
                        oninput: move |event| prompt.set(event.value()),
                        onkeydown: move |event| {
                            if event.key() == Key::Enter && !event.modifiers().contains(Modifiers::SHIFT) {
                                event.prevent_default();
                                send.call(());
                            }
                        }
                    }
                    div { class: "composer-actions",
                        span { class: "composer-model", "Agents A1" }
                        button { class: "composer-tool", title: "Voice", aria_label: "Voice", Codicon { name: "mic" } }
                        button {
                            class: "send-button",
                            aria_label: "Send message",
                            disabled: submitting() || prompt().trim().is_empty(),
                            onclick: move |_| send.call(()),
                            if submitting() { "…" } else { "↑" }
                        }
                    }
                }
                if let Some(error) = submit_error() { p { class: "inline-error composer-error", role: "alert", "{error}" } }
            }
        }
    }
}

#[component]
fn Overview() -> Element {
    let services = use_context::<AppServices>();
    let runtime_service = services.runtime.clone();
    let runtime = use_resource(move || {
        let runtime_service = runtime_service.clone();
        async move { runtime_service.status().await }
    });

    let runtime_read = runtime.read_unchecked();
    let (ready, phase, model, provider) = match &*runtime_read {
        Some(Ok(status)) => (
            matches!(status.phase.as_str(), "ready" | "running" | "online"),
            if status.phase.is_empty() {
                "Available".to_owned()
            } else {
                status.phase.clone()
            },
            status.model.as_deref().unwrap_or("Hermes model").to_owned(),
            status
                .provider
                .as_deref()
                .unwrap_or("Local inference")
                .to_owned(),
        ),
        Some(Err(_)) => (
            false,
            "Offline".to_owned(),
            "Hermes model".to_owned(),
            "Local inference".to_owned(),
        ),
        None => (
            false,
            "Checking".to_owned(),
            "Hermes model".to_owned(),
            "Local inference".to_owned(),
        ),
    };

    rsx! {
        Surface { eyebrow: "Hermes Local", title: "Local AI workstation", subtitle: "One control centre for the model, Hermes and local operations.",
            section { class: "status-card",
                div { class: "status-accent" }
                div { class: "status-copy",
                    div { class: "status-meta",
                        span { class: if ready { "health-pill good" } else { "health-pill" },
                            span { class: "health-dot" }
                            "{phase}"
                        }
                        span { "{model}" }
                    }
                    h2 { if ready { "Local workstation is ready" } else { "Start the local workstation" } }
                    p {
                        if ready {
                            "Hermes and the local model are available for private work on this computer."
                        } else {
                            "Start the managed local services to use Hermes, the model runtime and the web dashboard."
                        }
                    }
                }
                div { class: "status-actions",
                    button { class: "button outline", Codicon { name: "refresh" } "Restart" }
                    button { class: "button danger", Codicon { name: "primitive-square" } "Stop stack" }
                }
            }

            div { class: "dashboard-grid",
                section { class: "panel services-panel",
                    PanelTitle { label: "Services" }
                    ServiceRow { icon: "hubot", name: "Hermes model server", detail: "OpenAI-compatible · loopback", healthy: ready }
                    ServiceRow { icon: "server-process", name: "Hermes serve", detail: "JSON-RPC / WebSocket · local", healthy: ready }
                    ServiceRow { icon: "dashboard", name: "Web Dashboard", detail: "Unified Hermes management surface", healthy: ready }
                }
                section { class: "panel resources-panel",
                    PanelTitle { label: "Resources" }
                    ResourceRow { icon: "database", label: "System memory", value: "—", note: "Local telemetry" }
                    ResourceRow { icon: "dashboard", label: "GPU memory", value: "—", note: "Accelerator telemetry" }
                }
            }

            div { class: "action-grid",
                LauncherAction { to: Route::Chat {}, icon: "rocket", label: "Open Chat", detail: "Chat with Hermes through the local Agent." }
                LauncherAction { to: Route::Tui {}, icon: "terminal", label: "Open TUI", detail: "Run the keyboard-driven Hermes terminal UI." }
                LauncherAction { to: Route::Logs {}, icon: "output", label: "View Logs", detail: "Inspect service logs without exposing secrets." }
            }

            section { class: "panel integrity-panel",
                PanelTitle { label: "Integrity and provenance" }
                div { class: "integrity-grid",
                    IntegrityItem { label: "Model", value: model }
                    IntegrityItem { label: "Provider", value: provider }
                    IntegrityItem { label: "Hermes Agent", value: "Local gateway" }
                    IntegrityItem { label: "Local authentication", value: "Per-user" }
                }
            }
        }
    }
}

#[component]
fn PanelTitle(label: &'static str) -> Element {
    rsx! { header { class: "panel-title", "{label}" } }
}

#[component]
fn ServiceRow(
    icon: &'static str,
    name: &'static str,
    detail: &'static str,
    healthy: bool,
) -> Element {
    rsx! {
        div { class: "service-row",
            span { class: "service-icon", Codicon { name: icon } }
            div { class: "service-copy", strong { "{name}" } span { "{detail}" } }
            span { class: if healthy { "health-pill good" } else { "health-pill" },
                span { class: "health-dot" }
                if healthy { "Healthy" } else { "Offline" }
            }
        }
    }
}

#[component]
fn ResourceRow(
    icon: &'static str,
    label: &'static str,
    value: &'static str,
    note: &'static str,
) -> Element {
    rsx! {
        div { class: "resource-row",
            div { class: "resource-heading", span { class: "resource-icon", Codicon { name: icon } } strong { "{label}" } b { "{value}" } }
            div { class: "resource-track", span {} }
            small { "{note}" }
        }
    }
}

#[component]
fn LauncherAction(
    to: Route,
    icon: &'static str,
    label: &'static str,
    detail: &'static str,
) -> Element {
    rsx! {
        Link { class: "launcher-action", to,
            span { class: "action-icon", Codicon { name: icon } }
            div { strong { "{label}" } span { "{detail}" } }
            b { "›" }
        }
    }
}

#[component]
fn IntegrityItem(label: &'static str, value: String) -> Element {
    rsx! { div { span { "{label}" } strong { "{value}" } } }
}

#[component]
fn WorkCard(title: &'static str, detail: &'static str, accent: &'static str) -> Element {
    rsx! {
        article { class: "work-card {accent}",
            div { class: "card-icon", "✦" }
            h3 { "{title}" }
            p { "{detail}" }
        }
    }
}

#[component]
fn Surface(eyebrow: String, title: String, subtitle: String, children: Element) -> Element {
    rsx! {
        section { class: "surface",
            header { class: "surface-header",
                div { class: "eyebrow", "{eyebrow}" }
                h1 { "{title}" }
                p { "{subtitle}" }
            }
            {children}
        }
    }
}

macro_rules! simple_surface {
    ($name:ident, $eyebrow:literal, $title:literal, $subtitle:literal) => {
        #[component]
        fn $name() -> Element {
            rsx! {
                Surface { eyebrow: $eyebrow, title: $title, subtitle: $subtitle,
                    div { class: "empty-state",
                        div { class: "empty-orbit", "✦" }
                        h2 { "Nothing needs your attention" }
                        p { "This workspace will update as local activity arrives." }
                    }
                }
            }
        }
    };
}

simple_surface!(
    Files,
    "Workspace",
    "Files",
    "Browse and edit files inside a selected project root."
);
simple_surface!(
    Tui,
    "Hermes Local",
    "TUI",
    "Run the official keyboard-driven Hermes terminal interface."
);
simple_surface!(
    Dashboard,
    "Hermes Local",
    "Dashboard",
    "Open the full local Hermes web management surface."
);
simple_surface!(
    Services,
    "Hermes Local",
    "Services",
    "Structured health, process ownership and lifecycle controls."
);
simple_surface!(
    Models,
    "Hermes Local",
    "Models",
    "Verified weights, runtime support and context configuration."
);
simple_surface!(
    Profiles,
    "Hermes Local",
    "Profiles",
    "Editable inference profiles kept as versioned structured data."
);
simple_surface!(
    Tools,
    "Hermes Local",
    "Tools",
    "Inspect the tools available to the local Hermes Agent."
);
simple_surface!(
    Memory,
    "Hermes Local",
    "Memory",
    "Local state, session index and explicit memory controls."
);
simple_surface!(
    Sessions,
    "Hermes Local",
    "Sessions",
    "Resume and manage persistent local Hermes conversations."
);
simple_surface!(
    Integrations,
    "Hermes Local",
    "Integrations",
    "Connect explicit, trusted local and messaging services."
);
simple_surface!(
    Benchmarks,
    "Hermes Local",
    "Benchmarks",
    "Measured performance, stability and profile selection evidence."
);
simple_surface!(
    Security,
    "Hermes Local",
    "Security",
    "Loopback trust boundaries, audits and remediation evidence."
);
simple_surface!(
    Logs,
    "Hermes Local",
    "Logs",
    "Live, redacted output from each local service."
);
simple_surface!(
    Git,
    "Source control",
    "Git",
    "Review, stage, and understand local changes."
);
simple_surface!(
    Worktrees,
    "Source control",
    "Worktrees",
    "Keep parallel branches isolated and clear."
);
simple_surface!(
    Review,
    "Source control",
    "Review",
    "Inspect a change as one coherent story."
);
simple_surface!(
    Terminal,
    "Developer tools",
    "Terminal",
    "A native ConPTY session scoped to your workspace."
);
simple_surface!(
    Tasks,
    "Activity",
    "Tasks",
    "Track local work without losing context."
);
simple_surface!(
    Model,
    "Intelligence",
    "Model",
    "Manage the on-device inference runtime."
);
simple_surface!(
    Runtime,
    "Intelligence",
    "Runtime",
    "See Agent and model health at a glance."
);
simple_surface!(
    Trust,
    "Safety",
    "Trust",
    "Understand what Hermes may access and execute."
);
simple_surface!(
    Skills,
    "Extensions",
    "Skills",
    "Curate reusable local capabilities."
);
simple_surface!(
    Mcp,
    "Extensions",
    "MCP servers",
    "Connect explicit, trusted tool providers."
);
simple_surface!(
    Delegations,
    "Extensions",
    "Delegations",
    "Review bounded work assigned to agents."
);
simple_surface!(
    Cloud,
    "Connectivity",
    "Cloud",
    "Manage optional authenticated services."
);
simple_surface!(
    Usage,
    "Insights",
    "Usage",
    "Understand local and provider consumption."
);
simple_surface!(
    Automations,
    "Activity",
    "Automations",
    "Schedule and monitor recurring work."
);
simple_surface!(
    Notifications,
    "Activity",
    "Notifications",
    "Control attention without noise."
);
simple_surface!(
    QuickEntry,
    "Capture",
    "Quick entry",
    "Start a thought from anywhere."
);
simple_surface!(
    GeneralSettings,
    "Preferences",
    "General",
    "Control startup and desktop behaviour."
);
simple_surface!(
    ProviderSettings,
    "Preferences",
    "Providers",
    "Configure private model connections."
);
simple_surface!(
    UpdateSettings,
    "Preferences",
    "Updates",
    "Keep signed desktop components current."
);
simple_surface!(
    About,
    "Hermes Local",
    "About",
    "Private, capable, and built for your computer."
);

#[component]
fn Projects() -> Element {
    let services = use_context::<AppServices>();
    let state = use_context::<ProjectUiState>();
    let settings_state = use_context::<SettingsUiState>();
    let snapshot = (state.snapshot)();
    let mut query = use_signal(String::new);
    let mut filter = use_signal(|| "all".to_owned());
    let mut create_open = use_signal(|| false);
    let mut create_mode = use_signal(|| "empty".to_owned());
    let mut project_name = use_signal(String::new);
    let mut project_path = use_signal(String::new);
    let mut repository_url = use_signal(String::new);
    let mut creating = use_signal(|| false);
    let mut choosing_folder = use_signal(|| false);
    let mut remove_target = use_signal(|| None::<String>);

    let needle = query().trim().to_lowercase();
    let visible = snapshot
        .projects
        .iter()
        .filter(|project| match filter().as_str() {
            "active" => !project.archived,
            "archived" => project.archived,
            "pinned" => snapshot.pinned_ids.contains(&project.id),
            _ => true,
        })
        .filter(|project| {
            needle.is_empty()
                || project.name.to_lowercase().contains(&needle)
                || project.slug.to_lowercase().contains(&needle)
                || project
                    .description
                    .as_deref()
                    .is_some_and(|description| description.to_lowercase().contains(&needle))
                || project
                    .primary_path
                    .as_deref()
                    .is_some_and(|path| path.to_lowercase().contains(&needle))
                || project
                    .folders
                    .iter()
                    .any(|folder| folder.path.to_lowercase().contains(&needle))
        })
        .cloned()
        .collect::<Vec<_>>();
    rsx! {
        Surface { eyebrow: "Workspace", title: "Project Centre", subtitle: "Find and maintain stable projects. Removing a registration keeps files on disk.",
            div { class: "project-centre",
                div { class: "project-centre-toolbar",
                    label { class: "project-search",
                        Codicon { name: "search" }
                        input {
                            aria_label: "Search projects or paths",
                            placeholder: "Search projects or paths",
                            value: "{query}",
                            oninput: move |event| query.set(event.value())
                        }
                    }
                    button { class: "button project-create-button", onclick: move |_| create_open.set(true),
                        Codicon { name: "add" }
                        "New project"
                    }
                }
                div { class: "project-filters", aria_label: "Project filters",
                    for (id, label) in [("all", "All"), ("active", "Active"), ("archived", "Archived"), ("pinned", "Pinned")] {
                        button {
                            class: if filter() == id { "selected" } else { "" },
                            onclick: move |_| filter.set(id.to_owned()),
                            "{label}"
                        }
                    }
                }
                if (state.loading)() {
                    div { class: "project-centre-empty", "Loading projects…" }
                } else if visible.is_empty() {
                    div { class: "project-centre-empty", "No projects match this view." }
                } else {
                    div { class: "project-centre-list",
                        for project in visible {
                            article { class: "project-centre-row",
                                div { class: "project-centre-row-head",
                                    Link { class: "project-centre-copy", to: Route::Project { id: project.id.clone() },
                                        div { class: "project-title-line",
                                            strong { if project.name.is_empty() { "Unnamed project" } else { "{project.name}" } }
                                            if snapshot.active_id.as_deref() == Some(project.id.as_str()) {
                                                span { class: "project-badge", "Active" }
                                            }
                                            if project.archived {
                                                span { class: "project-badge", "Archived" }
                                            }
                                        }
                                        span { class: "project-path", title: project.primary_path.clone().unwrap_or_default(),
                                            if let Some(path) = &project.primary_path { "{path}" } else { "No folder attached" }
                                        }
                                        small { "{project.folders.len()} registered folder(s)" }
                                    }
                                    button {
                                        class: "icon-button",
                                        aria_label: if snapshot.pinned_ids.contains(&project.id) { "Unpin project" } else { "Pin project" },
                                        title: if snapshot.pinned_ids.contains(&project.id) { "Unpin project" } else { "Pin project" },
                                        onclick: {
                                            let service = services.projects.clone();
                                            let id = project.id.clone();
                                            let before = snapshot.clone();
                                            let pinned = snapshot.pinned_ids.contains(&project.id);
                                            move |_| {
                                                let mut next = before.clone();
                                                if pinned { next.pinned_ids.retain(|candidate| candidate != &id); }
                                                else if !next.pinned_ids.contains(&id) { next.pinned_ids.push(id.clone()); }
                                                let mut snapshot_signal = state.snapshot;
                                                let mut error_signal = state.error;
                                                snapshot_signal.set(next);
                                                let service = service.clone();
                                                let before = before.clone();
                                                let id = id.clone();
                                                spawn(async move {
                                                    match service.set_pinned(&id, !pinned).await {
                                                        Ok(authoritative) => snapshot_signal.set(authoritative),
                                                        Err(error) => { snapshot_signal.set(before); error_signal.set(Some(error.to_string())); }
                                                    }
                                                });
                                            }
                                        },
                                        Codicon { name: if snapshot.pinned_ids.contains(&project.id) { "pinned-dirty" } else { "pin" } }
                                    }
                                    if !project.archived && snapshot.active_id.as_deref() != Some(project.id.as_str()) {
                                        button {
                                            class: "icon-button",
                                            aria_label: "Set active project",
                                            title: "Set active project",
                                            onclick: {
                                                let service = services.projects.clone();
                                                let id = project.id.clone();
                                                let before = snapshot.clone();
                                                move |_| {
                                                    let mut next = before.clone();
                                                    next.active_id = Some(id.clone());
                                                    let mut snapshot_signal = state.snapshot;
                                                    let mut error_signal = state.error;
                                                    snapshot_signal.set(next);
                                                    let service = service.clone();
                                                    let before = before.clone();
                                                    let id = id.clone();
                                                    spawn(async move {
                                                        if let Err(error) = service.set_active(Some(&id)).await {
                                                            snapshot_signal.set(before);
                                                            error_signal.set(Some(error.to_string()));
                                                        }
                                                    });
                                                }
                                            },
                                            Codicon { name: "target" }
                                        }
                                    }
                                }
                                div { class: "project-row-actions",
                                    Link { class: "project-action", to: Route::Project { id: project.id.clone() }, "Open" }
                                    button {
                                        class: "project-action",
                                        onclick: {
                                            let service = services.projects.clone();
                                            let id = project.id.clone();
                                            let archived = !project.archived;
                                            let before = snapshot.clone();
                                            move |_| {
                                                let mut next = before.clone();
                                                if let Some(row) = next.projects.iter_mut().find(|row| row.id == id) { row.archived = archived; }
                                                if archived && next.active_id.as_deref() == Some(id.as_str()) { next.active_id = None; }
                                                let mut snapshot_signal = state.snapshot;
                                                let mut error_signal = state.error;
                                                snapshot_signal.set(next);
                                                let service = service.clone();
                                                let before = before.clone();
                                                let id = id.clone();
                                                spawn(async move {
                                                    match service.set_archived(&id, archived).await {
                                                        Ok(authoritative) => snapshot_signal.set(authoritative),
                                                        Err(error) => { snapshot_signal.set(before); error_signal.set(Some(error.to_string())); }
                                                    }
                                                });
                                            }
                                        },
                                        if project.archived { "Restore" } else { "Archive" }
                                    }
                                    button {
                                        class: "project-action danger",
                                        onclick: { let id = project.id.clone(); move |_| remove_target.set(Some(id.clone())) },
                                        "Remove registration"
                                    }
                                }
                            }
                        }
                    }
                }
                if let Some(error) = (state.error)() {
                    p { class: "inline-error", "{error}" }
                }
            }
            if create_open() {
                div { class: "dialog-backdrop", role: "presentation",
                    section { class: "hermes-dialog", role: "dialog", aria_modal: "true", aria_label: "Create project",
                        header { h2 { "Create project" } p {
                            if create_mode() == "empty" { "Create a stable project without a folder or Git repository." }
                            else if create_mode() == "attach" { "Register an existing local folder as a stable project." }
                            else { "Clone a repository into a chosen parent folder and register it." }
                        } }
                        div { class: "project-create-modes",
                            for (id, label) in [("empty", "Empty"), ("attach", "Attach folder"), ("clone", "Clone Git")] {
                                button { class: if create_mode() == id { "selected" } else { "" }, onclick: move |_| create_mode.set(id.to_owned()), "{label}" }
                            }
                        }
                        label { class: "dialog-field", span { "Project name" }
                            input { autofocus: true, placeholder: "Project name", value: "{project_name}", oninput: move |event| project_name.set(event.value()) }
                        }
                        if create_mode() == "clone" {
                            label { class: "dialog-field", span { "Repository URL" }
                                input { placeholder: "HTTPS or SSH repository URL", value: "{repository_url}", oninput: move |event| repository_url.set(event.value()) }
                            }
                        }
                        if create_mode() != "empty" {
                            label { class: "dialog-field", span { if create_mode() == "clone" { "Clone destination parent folder" } else { "Folder to attach" } }
                                div { class: "folder-picker-row",
                                    input { disabled: true, placeholder: "Choose a local folder", value: "{project_path}" }
                                    button { class: "button", disabled: choosing_folder() || creating(), onclick: {
                                        let platform = services.platform.clone();
                                        move |_| {
                                            choosing_folder.set(true);
                                            let platform = platform.clone();
                                            let starting_directory = (settings_state.settings)().default_project_dir.map(std::path::PathBuf::from);
                                            let title = if create_mode() == "clone" { "Choose clone destination parent folder" } else { "Choose project folder" };
                                            spawn(async move {
                                                match platform.pick_folder(title, starting_directory.as_deref()).await {
                                                    Ok(Some(path)) => project_path.set(path.to_string_lossy().into_owned()),
                                                    Ok(None) => {}
                                                    Err(error) => {
                                                        let mut error_signal = state.error;
                                                        error_signal.set(Some(error.to_string()));
                                                    }
                                                }
                                                choosing_folder.set(false);
                                            });
                                        }
                                    }, Codicon { name: "folder" } if choosing_folder() { "Choosing…" } else { "Choose…" } }
                                }
                            }
                        } else {
                            p { class: "dialog-hint", "This project starts without a filesystem location. You can attach a folder later without changing its project identity." }
                        }
                        footer {
                            button { class: "button", disabled: creating(), onclick: move |_| create_open.set(false), "Cancel" }
                            button {
                                class: "button primary",
                                disabled: creating() || project_name().trim().is_empty()
                                    || (create_mode() == "attach" && project_path().trim().is_empty())
                                    || (create_mode() == "clone" && (project_path().trim().is_empty() || repository_url().trim().is_empty())),
                                onclick: {
                                    let service = services.projects.clone();
                                    move |_| {
                                        let name = project_name().trim().to_owned();
                                        let path = project_path().trim().to_owned();
                                        let url = repository_url().trim().to_owned();
                                        let mode = create_mode();
                                        if name.is_empty() || creating() { return; }
                                        creating.set(true);
                                        let folders = if path.is_empty() { Vec::new() } else { vec![path] };
                                        let service = service.clone();
                                        let mut refresh = state.refresh;
                                        let mut error = state.error;
                                        spawn(async move {
                                            let result = async {
                                                let project = if mode == "clone" {
                                                    service.clone_repository(&name, &url, folders.first().map_or("", String::as_str)).await?
                                                } else {
                                                    service.create(&name, &folders).await?
                                                };
                                                if mode != "clone" { service.set_active(Some(&project.id)).await?; }
                                                Ok::<_, hermes_core::ServiceError>(())
                                            }.await;
                                            creating.set(false);
                                            match result {
                                                Ok(()) => {
                                                    project_name.set(String::new());
                                                    project_path.set(String::new());
                                                    repository_url.set(String::new());
                                                    create_mode.set("empty".to_owned());
                                                    create_open.set(false);
                                                    refresh += 1;
                                                }
                                                Err(problem) => error.set(Some(problem.to_string())),
                                            }
                                        });
                                    }
                                },
                                "Create project"
                            }
                        }
                    }
                }
            }
            if let Some(id) = remove_target() {
                div { class: "dialog-backdrop", role: "presentation",
                    section { class: "hermes-dialog compact", role: "alertdialog", aria_modal: "true", aria_label: "Remove project registration",
                        header { h2 { "Remove project?" } p { "This removes the Project Centre registration only. Files and Git repositories remain on disk." } }
                        footer {
                            button { class: "button", onclick: move |_| remove_target.set(None), "Cancel" }
                            button {
                                class: "button danger",
                                onclick: {
                                    let service = services.projects.clone();
                                    let before = snapshot.clone();
                                    move |_| {
                                        let mut next = before.clone();
                                        next.projects.retain(|project| project.id != id);
                                        if next.active_id.as_deref() == Some(id.as_str()) { next.active_id = None; }
                                        let mut snapshot_signal = state.snapshot;
                                        let mut error_signal = state.error;
                                        snapshot_signal.set(next);
                                        remove_target.set(None);
                                        let service = service.clone();
                                        let before = before.clone();
                                        let id = id.clone();
                                        spawn(async move {
                                            if let Err(error) = service.remove(&id).await {
                                                snapshot_signal.set(before);
                                                error_signal.set(Some(error.to_string()));
                                            }
                                        });
                                    }
                                },
                                "Remove registration"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn Settings() -> Element {
    rsx! { SettingsOverlay { initial: "model" } }
}

#[component]
fn Appearance() -> Element {
    rsx! { SettingsOverlay { initial: "appearance" } }
}

#[component]
fn SettingsOverlay(initial: &'static str) -> Element {
    let navigator = use_navigator();
    let mut active = use_signal(|| initial.to_owned());
    let sections = [
        ("model", "hubot", "Model"),
        ("chat", "comment-discussion", "Chat"),
        ("appearance", "symbol-color", "Appearance"),
        ("workspace", "desktop-download", "Workspace"),
        ("safety", "lock", "Safety"),
        ("memory", "database", "Memory & Context"),
        ("voice", "unmute", "Voice"),
        ("advanced", "tools", "Advanced"),
        ("notifications", "bell", "Notifications"),
        ("providers", "plug", "Providers"),
        ("gateway", "globe", "Gateway"),
        ("keybinds", "symbol-key", "Keybindings"),
        ("keys", "key", "API Keys"),
        ("plugins", "extensions", "Plugins"),
        ("sessions", "archive", "Archived Chats"),
        ("startup", "rocket", "Startup"),
        ("about", "info", "About"),
    ];
    let active_section = active();
    let active_icon = sections
        .iter()
        .find(|item| item.0 == active_section)
        .map_or("settings-gear", |item| item.1);
    let active_label = sections
        .iter()
        .find(|item| item.0 == active_section)
        .map_or("Settings", |item| item.2);
    rsx! {
        div { class: "settings-overlay", role: "dialog", aria_modal: "true", aria_label: "Settings",
            div { class: "settings-window",
                button { class: "settings-close", aria_label: "Close settings", title: "Close settings", onclick: move |_| navigator.go_back(), Codicon { name: "close" } }
                aside { class: "settings-rail", aria_label: "Settings sections",
                    for (id, icon, label) in sections {
                        button { class: if active() == id { "active" } else { "" }, onclick: move |_| active.set(id.to_owned()),
                            Codicon { name: icon }
                            span { "{label}" }
                        }
                    }
                    div { class: "settings-rail-footer",
                        button { aria_label: "Export config", title: "Export config", Codicon { name: "export" } }
                        button { aria_label: "Import config", title: "Import config", Codicon { name: "cloud-download" } }
                        button { aria_label: "Reset to defaults", title: "Reset to defaults", Codicon { name: "refresh" } }
                    }
                }
                main { class: "settings-main",
                    if active() == "appearance" {
                        AppearanceSettingsPanel {}
                    } else if active() == "model" {
                        AgentConfigPanel { section: "model", icon: "hubot", label: "Model" }
                    } else if active() == "chat" {
                        AgentConfigPanel { section: "chat", icon: "comment-discussion", label: "Chat" }
                    } else {
                        section { class: "settings-placeholder",
                            div { class: "settings-section-title", Codicon { name: active_icon } h1 { "{active_label}" } }
                            p { "This section is being connected to its typed Rust service without changing the OG Hermes layout." }
                        }
                    }
                }
            }
        }
    }
}

const PERSONALITIES: &[&str] = &[
    "helpful",
    "concise",
    "technical",
    "creative",
    "teacher",
    "kawaii",
    "catgirl",
    "pirate",
    "shakespeare",
    "surfer",
    "noir",
    "uwu",
    "philosopher",
    "hype",
];

const REASONING_EFFORTS: &[&str] = &[
    "none", "minimal", "low", "medium", "high", "xhigh", "max", "ultra",
];

const AUXILIARY_TASKS: &[(&str, &str, &str)] = &[
    ("vision", "Vision", "Image analysis"),
    ("web_extract", "Web extract", "Page summarization"),
    ("compression", "Compression", "Context compaction"),
    ("skills_hub", "Skills hub", "Skill search"),
    ("approval", "Approval", "Smart auto-approve"),
    ("mcp", "MCP", "MCP tool routing"),
    ("title_generation", "Title gen", "Session titles"),
    ("curator", "Curator", "Skill-usage review"),
];

#[component]
fn AgentConfigPanel(section: &'static str, icon: &'static str, label: &'static str) -> Element {
    let services = use_context::<AppServices>();
    let settings = use_context::<SettingsUiState>();
    let load_service = services.agent_config.clone();
    let profile = (settings.settings)().profile;
    let load_profile = profile.clone();
    let mut snapshot = use_signal(|| None::<AgentConfigSnapshot>);
    let mut loading = use_signal(|| true);
    let saving = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut refresh = use_signal(|| 0_u64);
    let _load = use_resource(move || {
        let service = load_service.clone();
        let profile = load_profile.clone();
        let _revision = refresh();
        async move {
            loading.set(true);
            match service.load(profile.as_deref()).await {
                Ok(loaded) => {
                    snapshot.set(Some(loaded));
                    error.set(None);
                }
                Err(load_error) => error.set(Some(load_error.to_string())),
            }
            loading.set(false);
        }
    });
    let load_error_text =
        error().unwrap_or_else(|| "The Agent config service is unavailable.".to_owned());

    rsx! {
        section { class: "agent-config-settings",
            div { class: "settings-section-title", Codicon { name: icon } h1 { "{label}" } }
            p { class: "settings-intro",
                if section == "model" {
                    "Configure the context window and the fallback models Hermes tries when the default model is unavailable."
                } else {
                    "Choose how new chats behave and how model output is presented."
                }
            }
            if loading() && snapshot().is_none() {
                div { class: "settings-skeleton", aria_label: "Loading settings",
                    for _ in 0..4 { div { class: "settings-skeleton-row", i {} span {} } }
                }
            } else if snapshot().is_none() {
                div { class: "settings-load-error", role: "alert",
                    Codicon { name: "error" }
                    strong { "Could not load Hermes settings" }
                    p { "{load_error_text}" }
                    button { class: "button", onclick: move |_| refresh += 1, "Retry" }
                }
            } else if let Some(loaded) = snapshot() {
                if section == "model" {
                    ModelConfigFields {
                        snapshot,
                        loaded,
                        profile: profile.clone(),
                        saving,
                        error,
                    }
                } else {
                    ChatConfigFields {
                        snapshot,
                        loaded,
                        profile: profile.clone(),
                        saving,
                        error,
                    }
                }
                if saving() { p { class: "settings-save-state", "Saving…" } }
                if let Some(save_error) = error() { p { class: "inline-error", role: "alert", "{save_error}" } }
            }
        }
    }
}

#[component]
fn ModelAssignmentFields(
    config_snapshot: Signal<Option<AgentConfigSnapshot>>,
    config: AgentConfigSnapshot,
    profile: Option<String>,
    config_saving: Signal<bool>,
    config_error: Signal<Option<String>>,
) -> Element {
    let services = use_context::<AppServices>();
    let load_service = services.models.clone();
    let load_profile = profile.clone();
    let mut model_snapshot = use_signal(|| None::<ModelSettingsSnapshot>);
    let mut loading = use_signal(|| true);
    let applying = use_signal(|| false);
    let mut model_error = use_signal(|| None::<String>);
    let mut refresh = use_signal(|| 0_u64);
    let mut selected_provider = use_signal(String::new);
    let mut selected_model = use_signal(String::new);
    let mut editing_task = use_signal(|| None::<String>);
    let mut auxiliary_provider = use_signal(String::new);
    let mut auxiliary_model = use_signal(String::new);
    let _load = use_resource(move || {
        let service = load_service.clone();
        let profile = load_profile.clone();
        let _revision = refresh();
        async move {
            loading.set(true);
            match service.load(profile.as_deref()).await {
                Ok(loaded) => {
                    selected_provider.set(loaded.info.provider.clone());
                    selected_model.set(loaded.info.model.clone());
                    model_snapshot.set(Some(loaded));
                    model_error.set(None);
                }
                Err(error) => model_error.set(Some(error.to_string())),
            }
            loading.set(false);
        }
    });
    let model_error_text =
        model_error().unwrap_or_else(|| "The model service is unavailable.".to_owned());
    let Some(models) = model_snapshot() else {
        return if loading() {
            rsx! {
                div { class: "model-settings-skeleton",
                    p { "Loading model configuration…" }
                    div { i {} i {} span {} }
                }
            }
        } else {
            rsx! {
                div { class: "settings-load-error compact", role: "alert",
                    Codicon { name: "error" }
                    strong { "Could not load model configuration" }
                    p { "{model_error_text}" }
                    button { class: "button", onclick: move |_| refresh += 1, "Retry" }
                }
            }
        };
    };
    let main_provider = selected_provider();
    let main_model = selected_model();
    let main_models = models
        .options
        .providers
        .iter()
        .find(|provider| provider.slug == main_provider)
        .map(|provider| provider.models.clone())
        .unwrap_or_default();
    let applied_provider = models
        .options
        .providers
        .iter()
        .find(|provider| provider.slug == models.info.provider);
    let capabilities =
        applied_provider.and_then(|provider| provider.capabilities.get(&models.info.model));
    let reasoning_supported = capabilities.is_none_or(|capabilities| capabilities.reasoning);
    let fast_supported = capabilities.is_some_and(|capabilities| capabilities.fast);
    let reasoning = config_value(&config.config, "agent.reasoning_effort")
        .and_then(Value::as_str)
        .unwrap_or("medium")
        .to_owned();
    let fast = config_value(&config.config, "agent.service_tier")
        .and_then(Value::as_str)
        .is_some_and(|tier| matches!(tier, "fast" | "priority" | "on"));
    let auxiliary_models = models
        .options
        .providers
        .iter()
        .find(|provider| provider.slug == auxiliary_provider())
        .map(|provider| provider.models.clone())
        .unwrap_or_default();

    rsx! {
        section { class: "model-assignment-section",
            p { class: "settings-intro", "Applies to new sessions. Use the model picker in the composer to hot-swap the active chat." }
            div { class: "model-picker-row",
                select {
                    class: "settings-select provider",
                    disabled: applying(),
                    value: "{main_provider}",
                    onchange: {
                        let providers = models.options.providers.clone();
                        move |event| {
                            let provider = event.value();
                            let model = providers
                                .iter()
                                .find(|candidate| candidate.slug == provider)
                                .and_then(|candidate| candidate.models.first())
                                .cloned()
                                .unwrap_or_default();
                            selected_provider.set(provider);
                            selected_model.set(model);
                        }
                    },
                    for provider in &models.options.providers {
                        option { value: "{provider.slug}", selected: provider.slug == main_provider, "{provider.name}" }
                    }
                }
                select {
                    class: "settings-select model",
                    disabled: applying() || main_models.is_empty(),
                    value: "{main_model}",
                    onchange: move |event| selected_model.set(event.value()),
                    for model in &main_models { option { value: "{model}", selected: *model == main_model, "{model}" } }
                }
                button {
                    class: "button primary",
                    disabled: applying() || main_provider.is_empty() || main_model.is_empty(),
                    onclick: {
                        let service = services.models.clone();
                        let profile = profile.clone();
                        let providers = models.options.providers.clone();
                        move |_| {
                            let provider = selected_provider();
                            let base_url = providers
                                .iter()
                                .find(|candidate| candidate.slug == provider)
                                .and_then(|candidate| candidate.api_url.clone());
                            assign_model(
                                service.clone(),
                                profile.clone(),
                                ModelAssignmentRequest {
                                    provider,
                                    model: selected_model(),
                                    scope: "main".into(),
                                    base_url,
                                    ..ModelAssignmentRequest::default()
                                },
                                applying,
                                model_error,
                                refresh,
                            );
                        }
                    },
                    if applying() { "Applying…" } else { "Apply" }
                }
            }
            if reasoning_supported || fast_supported {
                div { class: "model-defaults-row",
                    span { "Defaults" }
                    if reasoning_supported {
                        label { "Reasoning"
                            select {
                                class: "settings-select compact",
                                disabled: config_saving(),
                                value: "{reasoning}",
                                onchange: {
                                    let service = services.agent_config.clone();
                                    let profile = profile.clone();
                                    let current = config.config.clone();
                                    move |event| commit_agent_config(
                                        config_snapshot,
                                        config_saving,
                                        config_error,
                                        service.clone(),
                                        profile.clone(),
                                        set_config_value(&current, "agent.reasoning_effort", json!(event.value())),
                                    )
                                },
                                for effort in REASONING_EFFORTS {
                                    option { value: "{effort}", selected: reasoning == *effort,
                                        if *effort == "none" { "Off" } else { "{effort}" }
                                    }
                                }
                            }
                        }
                    }
                    if fast_supported {
                        label { class: "model-fast-control", "Fast"
                            span { class: "settings-switch",
                                input {
                                    r#type: "checkbox",
                                    checked: fast,
                                    disabled: config_saving(),
                                    onchange: {
                                        let service = services.agent_config.clone();
                                        let profile = profile.clone();
                                        let current = config.config.clone();
                                        move |event| commit_agent_config(
                                            config_snapshot,
                                            config_saving,
                                            config_error,
                                            service.clone(),
                                            profile.clone(),
                                            set_config_value(&current, "agent.service_tier", json!(if event.checked() { "fast" } else { "normal" })),
                                        )
                                    }
                                }
                                span {}
                            }
                        }
                    }
                }
            }
            if let Some(error) = model_error() { p { class: "inline-error", role: "alert", "{error}" } }
        }
        section { class: "auxiliary-model-section",
            div { class: "settings-subheading",
                div { Codicon { name: "server-process" } strong { "Auxiliary models" } }
                button {
                    class: "button",
                    disabled: applying() || models.info.provider.is_empty() || models.info.model.is_empty(),
                    onclick: {
                        let service = services.models.clone();
                        let profile = profile.clone();
                        let info = models.info.clone();
                        move |_| assign_model(
                            service.clone(),
                            profile.clone(),
                            ModelAssignmentRequest {
                                provider: info.provider.clone(),
                                model: info.model.clone(),
                                scope: "auxiliary".into(),
                                task: Some("__reset__".into()),
                                ..ModelAssignmentRequest::default()
                            },
                            applying,
                            model_error,
                            refresh,
                        )
                    },
                    "Reset all to main"
                }
            }
            p { class: "settings-intro", "Helper tasks run on the main model by default. Assign a dedicated model to any task to override." }
            div { class: "auxiliary-list",
                for (task, task_label, hint) in AUXILIARY_TASKS {
                    {
                        let assignment = models.auxiliary.tasks.iter().find(|assignment| assignment.task == *task).cloned();
                        let is_editing = editing_task().as_deref() == Some(*task);
                        rsx! {
                            div { class: "auxiliary-row", key: "{task}",
                                div { class: "settings-row-copy",
                                    strong { "{task_label}" }
                                    p {
                                        if let Some(current) = &assignment {
                                            "{current.provider} · {current.model}"
                                        } else {
                                            "auto · use main model — {hint}"
                                        }
                                    }
                                }
                                if is_editing {
                                    div { class: "auxiliary-editor",
                                        select {
                                            class: "settings-select compact",
                                            value: "{auxiliary_provider}",
                                            onchange: {
                                                let providers = models.options.providers.clone();
                                                move |event| {
                                                    let provider = event.value();
                                                    let model = providers
                                                        .iter()
                                                        .find(|candidate| candidate.slug == provider)
                                                        .and_then(|candidate| candidate.models.first())
                                                        .cloned()
                                                        .unwrap_or_default();
                                                    auxiliary_provider.set(provider);
                                                    auxiliary_model.set(model);
                                                }
                                            },
                                            for provider in &models.options.providers {
                                                option { value: "{provider.slug}", selected: provider.slug == auxiliary_provider(), "{provider.name}" }
                                            }
                                        }
                                        select {
                                            class: "settings-select compact model",
                                            value: "{auxiliary_model}",
                                            onchange: move |event| auxiliary_model.set(event.value()),
                                            for model in &auxiliary_models {
                                                option { value: "{model}", selected: *model == auxiliary_model(), "{model}" }
                                            }
                                        }
                                        button {
                                            class: "button primary",
                                            disabled: applying() || auxiliary_provider().is_empty() || auxiliary_model().is_empty(),
                                            onclick: {
                                                let service = services.models.clone();
                                                let profile = profile.clone();
                                                let task = (*task).to_owned();
                                                let providers = models.options.providers.clone();
                                                move |_| {
                                                    let provider = auxiliary_provider();
                                                    let base_url = providers
                                                        .iter()
                                                        .find(|candidate| candidate.slug == provider)
                                                        .and_then(|candidate| candidate.api_url.clone());
                                                    editing_task.set(None);
                                                    assign_model(
                                                        service.clone(),
                                                        profile.clone(),
                                                        ModelAssignmentRequest {
                                                            provider,
                                                            model: auxiliary_model(),
                                                            scope: "auxiliary".into(),
                                                            task: Some(task.clone()),
                                                            base_url,
                                                        },
                                                        applying,
                                                        model_error,
                                                        refresh,
                                                    );
                                                }
                                            },
                                            "Apply"
                                        }
                                        button { class: "button", onclick: move |_| editing_task.set(None), "Cancel" }
                                    }
                                } else {
                                    div { class: "auxiliary-actions",
                                        button {
                                            class: "button",
                                            disabled: applying(),
                                            onclick: {
                                                let service = services.models.clone();
                                                let profile = profile.clone();
                                                let info = models.info.clone();
                                                let task = (*task).to_owned();
                                                move |_| assign_model(
                                                    service.clone(),
                                                    profile.clone(),
                                                    ModelAssignmentRequest {
                                                        provider: info.provider.clone(),
                                                        model: info.model.clone(),
                                                        scope: "auxiliary".into(),
                                                        task: Some(task.clone()),
                                                        ..ModelAssignmentRequest::default()
                                                    },
                                                    applying,
                                                    model_error,
                                                    refresh,
                                                )
                                            },
                                            "Set to main"
                                        }
                                        button {
                                            class: "button",
                                            disabled: applying() || models.options.providers.is_empty(),
                                            onclick: {
                                                let assignment = assignment.clone();
                                                let main = models.info.clone();
                                                move |_| {
                                                    let provider = assignment.as_ref().map_or_else(|| main.provider.clone(), |value| value.provider.clone());
                                                    let model = assignment.as_ref().map_or_else(|| main.model.clone(), |value| value.model.clone());
                                                    auxiliary_provider.set(provider);
                                                    auxiliary_model.set(model);
                                                    editing_task.set(Some((*task).to_owned()));
                                                }
                                            },
                                            "Change"
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
}

fn assign_model(
    service: Arc<dyn ModelService>,
    profile: Option<String>,
    request: ModelAssignmentRequest,
    mut applying: Signal<bool>,
    mut error: Signal<Option<String>>,
    mut refresh: Signal<u64>,
) {
    if applying() {
        return;
    }
    applying.set(true);
    error.set(None);
    spawn(async move {
        match service.assign(profile.as_deref(), &request).await {
            Ok(_) => refresh += 1,
            Err(assign_error) => error.set(Some(assign_error.to_string())),
        }
        applying.set(false);
    });
}

#[component]
fn ModelConfigFields(
    snapshot: Signal<Option<AgentConfigSnapshot>>,
    loaded: AgentConfigSnapshot,
    profile: Option<String>,
    saving: Signal<bool>,
    error: Signal<Option<String>>,
) -> Element {
    let service = use_context::<AppServices>().agent_config.clone();
    let context_length = config_value(&loaded.config, "model_context_length")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let fallback_entries = fallback_entries(config_value(&loaded.config, "fallback_providers"));
    let mut context_draft = use_signal(|| context_length.to_string());
    let mut fallback_draft = use_signal(|| fallback_entries);
    rsx! {
        ModelAssignmentFields {
            config_snapshot: snapshot,
            config: loaded.clone(),
            profile: profile.clone(),
            config_saving: saving,
            config_error: error,
        }
        SettingsRow {
            title: "Context Window",
            description: "Leave at 0 to use the selected model's detected context window.",
            input {
                class: "settings-input number",
                r#type: "number",
                min: "0",
                disabled: saving(),
                value: "{context_draft}",
                oninput: move |event| context_draft.set(event.value()),
                onblur: {
                    let loaded = loaded.clone();
                    let profile = profile.clone();
                    let service = service.clone();
                    move |_| {
                        if let Ok(value) = context_draft().parse::<i64>() {
                            commit_agent_config(
                                snapshot,
                                saving,
                                error,
                                service.clone(),
                                profile.clone(),
                                set_config_value(&loaded.config, "model_context_length", json!(value)),
                            );
                        }
                    }
                }
            }
        }
        section { class: "settings-list-row wide",
            div { class: "settings-row-copy",
                strong { "Fallback Models" }
                p { "Backup provider:model entries to try if the default model fails." }
            }
            div { class: "fallback-list",
                if fallback_draft().is_empty() { p { class: "fallback-empty", "No fallback models configured." } }
                for (index, (provider, model)) in fallback_draft().into_iter().enumerate() {
                    div { class: "fallback-row", key: "fallback-{index}",
                        span { class: "fallback-index", "{index + 1}" }
                        input {
                            class: "settings-input",
                            disabled: saving(),
                            value: "{provider}",
                            placeholder: "Provider",
                            aria_label: "Fallback provider",
                            oninput: move |event| {
                                if let Some(entry) = fallback_draft.write().get_mut(index) {
                                    entry.0 = event.value();
                                }
                            },
                            onblur: {
                                let profile = profile.clone();
                                let service = service.clone();
                                let config = loaded.config.clone();
                                move |_| commit_agent_config(
                                    snapshot,
                                    saving,
                                    error,
                                    service.clone(),
                                    profile.clone(),
                                    set_config_value(&config, "fallback_providers", fallback_value(&fallback_draft())),
                                )
                            }
                        }
                        input {
                            class: "settings-input grow",
                            disabled: saving(),
                            value: "{model}",
                            placeholder: "Model",
                            aria_label: "Fallback model",
                            oninput: move |event| {
                                if let Some(entry) = fallback_draft.write().get_mut(index) {
                                    entry.1 = event.value();
                                }
                            },
                            onblur: {
                                let profile = profile.clone();
                                let service = service.clone();
                                let config = loaded.config.clone();
                                move |_| commit_agent_config(
                                    snapshot,
                                    saving,
                                    error,
                                    service.clone(),
                                    profile.clone(),
                                    set_config_value(&config, "fallback_providers", fallback_value(&fallback_draft())),
                                )
                            }
                        }
                        button {
                            class: "icon-button",
                            aria_label: "Remove fallback model",
                            title: "Remove",
                            disabled: saving(),
                            onclick: {
                                let profile = profile.clone();
                                let service = service.clone();
                                let config = loaded.config.clone();
                                move |_| {
                                    let mut next = fallback_draft();
                                    next.remove(index);
                                    fallback_draft.set(next.clone());
                                    commit_agent_config(
                                        snapshot,
                                        saving,
                                        error,
                                        service.clone(),
                                        profile.clone(),
                                        set_config_value(&config, "fallback_providers", fallback_value(&next)),
                                    );
                                }
                            },
                            Codicon { name: "close" }
                        }
                    }
                }
                button {
                    class: "button fallback-add",
                    disabled: saving(),
                    onclick: move |_| fallback_draft.write().push((String::new(), String::new())),
                    Codicon { name: "add" }
                    "Add fallback"
                }
            }
        }
    }
}

#[component]
fn ChatConfigFields(
    snapshot: Signal<Option<AgentConfigSnapshot>>,
    loaded: AgentConfigSnapshot,
    profile: Option<String>,
    saving: Signal<bool>,
    error: Signal<Option<String>>,
) -> Element {
    let service = use_context::<AppServices>().agent_config.clone();
    let personality = config_value(&loaded.config, "display.personality")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let timezone = config_value(&loaded.config, "timezone")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let show_reasoning = config_value(&loaded.config, "display.show_reasoning")
        .and_then(Value::as_bool)
        .unwrap_or_default();
    let image_mode = config_value(&loaded.config, "agent.image_input_mode")
        .and_then(Value::as_str)
        .unwrap_or("auto")
        .to_owned();
    let timezone_options = loaded
        .schema
        .fields
        .get("timezone")
        .map(|field| {
            field
                .options
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut timezone_draft = use_signal(|| timezone);
    rsx! {
        SettingsRow { title: "Personality", description: "Default assistant style for new sessions.",
            select {
                class: "settings-select",
                disabled: saving(),
                value: "{personality}",
                onchange: {
                    let config = loaded.config.clone();
                    let profile = profile.clone();
                    let service = service.clone();
                    move |event| commit_agent_config(snapshot, saving, error, service.clone(), profile.clone(), set_config_value(&config, "display.personality", json!(event.value())))
                },
                option { value: "", selected: personality.is_empty(), "None" }
                for option in PERSONALITIES {
                    option { value: "{option}", selected: personality == *option, "{option}" }
                }
            }
        }
        SettingsRow { title: "Timezone", description: "IANA timezone identifier. Blank uses the system timezone.",
            div { class: "timezone-control",
                input {
                    class: "settings-input",
                    list: "hermes-timezones",
                    placeholder: "System default",
                    disabled: saving(),
                    value: "{timezone_draft}",
                    oninput: move |event| timezone_draft.set(event.value()),
                    onblur: {
                        let config = loaded.config.clone();
                        let profile = profile.clone();
                        let service = service.clone();
                        move |_| commit_agent_config(snapshot, saving, error, service.clone(), profile.clone(), set_config_value(&config, "timezone", json!(timezone_draft())))
                    }
                }
                datalist { id: "hermes-timezones",
                    for timezone in timezone_options { option { value: "{timezone}" } }
                }
            }
        }
        SettingsRow { title: "Reasoning Blocks", description: "Show reasoning sections when the backend provides them.",
            label { class: "settings-switch",
                input {
                    r#type: "checkbox",
                    checked: show_reasoning,
                    disabled: saving(),
                    onchange: {
                        let config = loaded.config.clone();
                        let profile = profile.clone();
                        let service = service.clone();
                        move |event| commit_agent_config(snapshot, saving, error, service.clone(), profile.clone(), set_config_value(&config, "display.show_reasoning", json!(event.checked())))
                    }
                }
                span {}
            }
        }
        SettingsRow { title: "Image Attachments", description: "Controls how image attachments are sent to the model.",
            select {
                class: "settings-select",
                disabled: saving(),
                value: "{image_mode}",
                onchange: {
                    let config = loaded.config.clone();
                    let profile = profile.clone();
                    let service = service.clone();
                    move |event| commit_agent_config(snapshot, saving, error, service.clone(), profile.clone(), set_config_value(&config, "agent.image_input_mode", json!(event.value())))
                },
                option { value: "auto", selected: image_mode == "auto", "Auto" }
                option { value: "native", selected: image_mode == "native", "Native" }
                option { value: "text", selected: image_mode == "text", "Text" }
            }
        }
    }
}

#[component]
fn SettingsRow(title: &'static str, description: &'static str, children: Element) -> Element {
    rsx! {
        section { class: "settings-list-row",
            div { class: "settings-row-copy", strong { "{title}" } p { "{description}" } }
            div { class: "settings-row-action", {children} }
        }
    }
}

fn commit_agent_config(
    mut snapshot: Signal<Option<AgentConfigSnapshot>>,
    mut saving: Signal<bool>,
    mut error: Signal<Option<String>>,
    service: Arc<dyn AgentConfigService>,
    profile: Option<String>,
    config: BTreeMap<String, Value>,
) {
    if saving() {
        return;
    }
    let before = snapshot();
    if let Some(mut optimistic) = before.clone() {
        optimistic.config.clone_from(&config);
        snapshot.set(Some(optimistic));
    }
    saving.set(true);
    error.set(None);
    spawn(async move {
        if let Err(save_error) = service.save(profile.as_deref(), &config).await {
            snapshot.set(before);
            error.set(Some(save_error.to_string()));
        }
        saving.set(false);
    });
}

fn config_value<'a>(config: &'a BTreeMap<String, Value>, path: &str) -> Option<&'a Value> {
    let mut parts = path.split('.');
    let mut value = config.get(parts.next()?)?;
    for part in parts {
        value = value.as_object()?.get(part)?;
    }
    Some(value)
}

fn set_config_value(
    config: &BTreeMap<String, Value>,
    path: &str,
    value: Value,
) -> BTreeMap<String, Value> {
    fn insert(parts: &[&str], target: &mut Map<String, Value>, value: Value) {
        if let Some((head, tail)) = parts.split_first() {
            if tail.is_empty() {
                target.insert((*head).to_owned(), value);
            } else {
                let child = target
                    .entry((*head).to_owned())
                    .or_insert_with(|| Value::Object(Map::new()));
                if !child.is_object() {
                    *child = Value::Object(Map::new());
                }
                insert(
                    tail,
                    child.as_object_mut().expect("object created above"),
                    value,
                );
            }
        }
    }
    let mut root: Map<String, Value> = config.clone().into_iter().collect();
    insert(&path.split('.').collect::<Vec<_>>(), &mut root, value);
    root.into_iter().collect()
}

fn fallback_entries(value: Option<&Value>) -> Vec<(String, String)> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            if let Some(entry) = entry.as_object() {
                return Some((
                    entry
                        .get("provider")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                    entry
                        .get("model")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                ));
            }
            let entry = entry.as_str()?;
            let (provider, model) = entry.split_once('/').unwrap_or(("", entry));
            Some((provider.to_owned(), model.to_owned()))
        })
        .collect()
}

fn fallback_value(entries: &[(String, String)]) -> Value {
    Value::Array(
        entries
            .iter()
            .filter(|(provider, model)| !provider.is_empty() && !model.is_empty())
            .map(|(provider, model)| json!({ "provider": provider, "model": model }))
            .collect(),
    )
}

#[component]
fn AppearanceSettingsPanel() -> Element {
    let services = use_context::<AppServices>();
    let state = use_context::<SettingsUiState>();
    let save_service = services.settings.clone();
    let current = (state.settings)();
    let themes = [
        ("nous", "Nous", "Glass neutrals with Nous blue accents"),
        ("midnight", "Midnight", "Deep blue-violet with cool accents"),
        ("ember", "Ember", "Warm crimson and bronze — forge vibes"),
        ("mono", "Mono", "Clean grayscale — minimal and focused"),
        (
            "cyberpunk",
            "Cyberpunk",
            "Neon green on black — matrix terminal",
        ),
        (
            "slate",
            "Slate",
            "Cool slate blue — focused developer theme",
        ),
    ];
    rsx! {
        section { class: "appearance-settings",
            div { class: "settings-section-title", Codicon { name: "symbol-color" } h1 { "Appearance" } }
            p { class: "settings-intro", "Choose how Hermes looks and feels. Theme changes apply immediately and stay with this workstation." }
            if (state.loading)() {
                p { class: "settings-intro", "Loading appearance…" }
            } else {
                section { class: "settings-list-row wide",
                    div { class: "settings-row-copy",
                        div { class: "settings-row-heading",
                            strong { "Theme" }
                            div { class: "settings-mode-control",
                                for (mode, label, icon) in [
                                    (ThemeMode::Light, "Light", "color-mode"),
                                    (ThemeMode::Dark, "Dark", "color-mode"),
                                    (ThemeMode::System, "System", "desktop-download"),
                                ] {
                                    button { class: if current.theme == mode { "selected" } else { "" }, onclick: {
                                        let service = save_service.clone();
                                        let before = current.clone();
                                        let mut next = current.clone();
                                        let mode = mode.clone();
                                        move |_| {
                                            next.theme = mode.clone();
                                            let committed = next.clone();
                                            let mut settings_signal = state.settings;
                                            let mut error_signal = state.error;
                                            settings_signal.set(committed.clone());
                                            let service = service.clone();
                                            let before = before.clone();
                                            spawn(async move {
                                                if let Err(error) = service.save(&committed).await {
                                                    settings_signal.set(before);
                                                    error_signal.set(Some(error.to_string()));
                                                }
                                            });
                                        }
                                    }, Codicon { name: icon } "{label}" }
                                }
                            }
                        }
                        p { "Pick a built-in Hermes theme, then choose Light, Dark, or your system setting." }
                    }
                    div { class: "theme-search", Codicon { name: "search" } input { placeholder: "Search your themes or the VS Code Marketplace…", disabled: true } }
                    div { class: "theme-grid",
                        for (name, label, description) in themes {
                            button { class: if current.theme_name.as_deref().unwrap_or("nous") == name { "theme-card active" } else { "theme-card" }, onclick: {
                                let service = save_service.clone();
                                let before = current.clone();
                                let mut next = current.clone();
                                move |_| {
                                    next.theme_name = Some(name.to_owned());
                                    let committed = next.clone();
                                    let mut settings_signal = state.settings;
                                    let mut error_signal = state.error;
                                    settings_signal.set(committed.clone());
                                    let service = service.clone();
                                    let before = before.clone();
                                    spawn(async move {
                                        if let Err(error) = service.save(&committed).await {
                                            settings_signal.set(before);
                                            error_signal.set(Some(error.to_string()));
                                        }
                                    });
                                }
                            },
                                div { class: "theme-preview {name}", div { class: "theme-preview-rail" } div { class: "theme-preview-content", i {} i {} span {} } }
                                strong { "{label}" }
                                small { "{description}" }
                            }
                        }
                    }
                }
                section { class: "settings-list-row",
                    div { class: "settings-row-copy", strong { "UI scale" } p { "Scale the desktop interface. Current size: 100%." } }
                    div { class: "settings-mode-control compact", button { "90%" } button { class: "selected", "100%" } button { "110%" } button { "125%" } }
                }
                section { class: "settings-list-row",
                    div { class: "settings-row-copy", strong { "Local privacy" } p { "Native authority stays behind typed Rust services; the WebView receives no generic shell bridge." } }
                    span { class: "privacy-pill", "● Enforced" }
                }
                if let Some(error) = (state.error)() {
                    p { class: "inline-error", role: "alert", "{error}" }
                }
            }
        }
    }
}

#[component]
fn Session(id: String) -> Element {
    let services = use_context::<AppServices>();
    let load_service = services.sessions.clone();
    let submit_service = services.sessions.clone();
    let interrupt_service = services.sessions.clone();
    let events_service = services.sessions.clone();
    let session_id = id.clone();
    let mut transcript = use_signal(|| None::<SessionTranscript>);
    let mut loading = use_signal(|| true);
    let mut load_error = use_signal(|| None::<String>);
    let mut events_ready = use_signal(|| false);
    let _load = use_resource(move || {
        let load_service = load_service.clone();
        let session_id = session_id.clone();
        async move {
            loading.set(true);
            match load_service.resume(&session_id).await {
                Ok(response) => {
                    transcript.set(Some(SessionTranscript::load(session_id, response)));
                    load_error.set(None);
                    events_ready.set(true);
                }
                Err(error) => load_error.set(Some(error.to_string())),
            }
            loading.set(false);
        }
    });
    let _events = use_resource(move || {
        let ready = events_ready();
        let events_service = events_service.clone();
        async move {
            if !ready {
                return;
            }
            let Ok(mut events) = events_service.events() else {
                return;
            };
            while let Some(event) = events.next().await {
                if let Some(state) = transcript.write().as_mut() {
                    state.apply_event(&event);
                }
            }
        }
    });
    let mut draft = use_signal(String::new);
    let mut send_error = use_signal(|| None::<String>);
    let send = Callback::new(move |()| {
        let text = draft().trim().to_owned();
        let Some(before) = transcript() else {
            return;
        };
        if text.is_empty() || before.busy {
            return;
        }
        let runtime_id = before.runtime_id.clone();
        let optimistic_id = format!("user-local-{}", before.messages.len());
        if let Some(state) = transcript.write().as_mut() {
            state.push_user(optimistic_id, text.clone());
        }
        draft.set(String::new());
        send_error.set(None);
        let service = submit_service.clone();
        spawn(async move {
            if let Err(error) = service.submit(&runtime_id, &text).await {
                transcript.set(Some(before));
                draft.set(text);
                send_error.set(Some(error.to_string()));
            }
        });
    });
    let busy = transcript().as_ref().is_some_and(|state| state.busy);
    let header_interrupt = interrupt_service.clone();
    let composer_interrupt = interrupt_service;
    rsx! {
        section { class: "conversation-surface",
            header { class: "conversation-header",
                div { span { class: if busy { "session-dot running" } else { "session-dot" } } strong { "Session" } small { "{id}" } }
                if busy {
                    button {
                        class: "stop-button",
                        onclick: move |_| {
                            let service = header_interrupt.clone();
                            let runtime_id = transcript().map(|state| state.runtime_id).unwrap_or_default();
                            if runtime_id.is_empty() { return; }
                            spawn(async move {
                                if let Err(error) = service.interrupt(&runtime_id).await {
                                    send_error.set(Some(error.to_string()));
                                }
                            });
                        },
                        Codicon { name: "primitive-square" }
                        "Stop"
                    }
                }
            }
            div { class: "conversation-scroll",
                div { class: "transcript",
                    if loading() {
                        LoadingState { label: "Loading conversation" }
                    } else if let Some(error) = load_error() {
                        ErrorState { error }
                    } else if let Some(state) = transcript() {
                        if state.messages.is_empty() {
                            div { class: "conversation-empty", "Write a message below to continue this conversation." }
                        }
                        for message in state.messages {
                            if message.role == MessageRole::Tool {
                                article { class: "tool-message",
                                    div { class: "tool-message-head", Codicon { name: "tools" } strong { if let Some(name) = message.tool_name.as_deref() { "{name}" } else { "Tool" } } span { if message.streaming { "Running" } else { "Done" } } }
                                    if !message.text.is_empty() { pre { "{message.text}" } }
                                }
                            } else {
                                article { class: if message.role == MessageRole::User { "message user" } else { "message assistant" },
                                    div { class: "message-role", if message.role == MessageRole::User { "You" } else { "Hermes" } }
                                    if let Some(reasoning) = message.metadata.get("reasoning").and_then(serde_json::Value::as_str) {
                                        details { class: "reasoning", summary { "Thinking" } p { "{reasoning}" } }
                                    }
                                    if !message.text.is_empty() { p { "{message.text}" } }
                                    if message.streaming { span { class: "stream-cursor", aria_label: "Hermes is responding" } }
                                }
                            }
                        }
                        if state.needs_input { div { class: "needs-input", Codicon { name: "question" } "Hermes is waiting for input in this session." } }
                        if let Some(error) = state.error { p { class: "inline-error transcript-error", role: "alert", "{error}" } }
                    }
                }
            }
            div { class: "session-composer-dock",
                ProjectPicker {}
                div { class: "composer-card",
                    button { class: "composer-tool", title: "Attach", aria_label: "Attach", Codicon { name: "add" } }
                    textarea {
                        aria_label: "Message Hermes",
                        placeholder: if busy { "Hermes is working…" } else { "What are we building?" },
                        rows: "1",
                        value: "{draft}",
                        disabled: loading(),
                        oninput: move |event| draft.set(event.value()),
                        onkeydown: move |event| {
                            if event.key() == Key::Enter && !event.modifiers().contains(Modifiers::SHIFT) {
                                event.prevent_default();
                                send.call(());
                            }
                        }
                    }
                    div { class: "composer-actions",
                        span { class: "composer-model", "Private session" }
                        button {
                            class: if busy { "send-button stop" } else { "send-button" },
                            aria_label: if busy { "Stop response" } else { "Send message" },
                            disabled: !busy && draft().trim().is_empty(),
                            onclick: move |_| {
                                if busy {
                                    let service = composer_interrupt.clone();
                                    let runtime_id = transcript().map(|state| state.runtime_id).unwrap_or_default();
                                    spawn(async move { let _ = service.interrupt(&runtime_id).await; });
                                } else {
                                    send.call(());
                                }
                            },
                            if busy { Codicon { name: "primitive-square" } } else { "↑" }
                        }
                    }
                }
                if let Some(error) = send_error() { p { class: "inline-error composer-error", role: "alert", "{error}" } }
            }
        }
    }
}

#[component]
fn Project(id: String) -> Element {
    rsx! { Surface { eyebrow: "Workspace", title: "Project", subtitle: "Project {id}" } }
}

#[component]
fn NotFound(segments: Vec<String>) -> Element {
    let path = segments.join("/");
    rsx! {
        section { class: "surface not-found",
            div { class: "eyebrow", "Not found" }
            h1 { "That view does not exist." }
            p { "No route matches /{path}." }
            Link { class: "primary-button", to: Route::Overview {}, "Return home" }
        }
    }
}

#[component]
fn EmptyState(label: String, detail: String) -> Element {
    rsx! {
        div { class: "empty-state compact",
            div { class: "empty-orbit", "✦" }
            h2 { "{label}" }
            p { "{detail}" }
        }
    }
}

#[component]
fn LoadingState(label: String) -> Element {
    rsx! { div { class: "loading-state", role: "status", "◌ {label}" } }
}

#[component]
fn ErrorState(error: String) -> Element {
    rsx! { div { class: "error-state", role: "alert", h2 { "Could not load this view" } p { "{error}" } } }
}
