//! Dioxus presentation layer. This crate has no filesystem, process, or OS authority.

use std::{collections::BTreeMap, sync::Arc};

use dioxus::prelude::*;
use futures_util::StreamExt;
use hermes_core::{AgentConfigService, AppServices, ModelService, SessionTranscript};
use hermes_protocol::{
    AgentConfigSnapshot, AppSettings, ConnectionConfig, ConnectionConfigInput, ConnectionMode,
    ConnectionProbeResult, CustomEndpoint, CustomEndpointUpdate, EnvVarInfo, MessageRole,
    MoaConfig, ModelAssignmentRequest, ModelProvider, ModelSettingsSnapshot,
    NativeNotificationKind, OAuthProvider, OAuthStart, ProbeAuthMode, ProjectFolder,
    ProjectsSnapshot, RemoteAuthMode, SessionCreateRequest, SessionSummary, ThemeMode,
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

fn project_has_broken_path(project: &hermes_protocol::ProjectSummary) -> bool {
    matches!(
        project.path_state.as_deref(),
        Some("missing" | "inaccessible")
    ) || project.folders.iter().any(|folder| {
        matches!(
            folder.path_state.as_deref(),
            Some("missing" | "inaccessible")
        )
    })
}

fn project_repair_folder(project: &hermes_protocol::ProjectSummary) -> Option<ProjectFolder> {
    project
        .folders
        .iter()
        .find(|folder| {
            matches!(
                folder.path_state.as_deref(),
                Some("missing" | "inaccessible")
            )
        })
        .or_else(|| project.folders.iter().find(|folder| folder.is_primary))
        .or_else(|| project.folders.first())
        .cloned()
}

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
    let mut busy_project = use_signal(|| None::<String>);
    let mut delete_target = use_signal(|| None::<String>);
    let mut delete_confirmation = use_signal(String::new);

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
                                            if project_has_broken_path(&project) {
                                                span { class: "project-badge warning", "Path needs repair" }
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
                                        disabled: busy_project().as_deref() == Some(project.id.as_str()),
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
                                    if project_has_broken_path(&project) {
                                        button {
                                            class: "project-action",
                                            disabled: busy_project().is_some(),
                                            onclick: {
                                                let platform = services.platform.clone();
                                                let project_service = services.projects.clone();
                                                let project = project.clone();
                                                let starting_directory = (settings_state.settings)().default_project_dir.map(std::path::PathBuf::from);
                                                move |_| {
                                                    let Some(folder) = project_repair_folder(&project) else { return; };
                                                    let platform = platform.clone();
                                                    let project_service = project_service.clone();
                                                    let project = project.clone();
                                                    let starting_directory = starting_directory.clone();
                                                    spawn(async move {
                                                        let replacement = match platform.pick_folder("Choose replacement project folder", starting_directory.as_deref()).await {
                                                            Ok(Some(path)) => path.to_string_lossy().into_owned(),
                                                            Ok(None) => return,
                                                            Err(problem) => {
                                                                let mut error = state.error;
                                                                error.set(Some(problem.to_string()));
                                                                return;
                                                            }
                                                        };
                                                        busy_project.set(Some(project.id.clone()));
                                                        match project_service.recover_path(
                                                            &project.id,
                                                            &folder.path,
                                                            &replacement,
                                                            folder.repository_id.as_deref(),
                                                        ).await {
                                                            Ok(repaired) => {
                                                                let mut next = (state.snapshot)();
                                                                if let Some(row) = next.projects.iter_mut().find(|row| row.id == repaired.id) {
                                                                    *row = repaired;
                                                                }
                                                                let mut snapshot_signal = state.snapshot;
                                                                snapshot_signal.set(next);
                                                            }
                                                            Err(problem) => {
                                                                let mut error = state.error;
                                                                error.set(Some(problem.to_string()));
                                                            }
                                                        }
                                                        busy_project.set(None);
                                                    });
                                                }
                                            },
                                            "Repair path"
                                        }
                                    }
                                    button {
                                        class: "project-action danger",
                                        disabled: busy_project().is_some(),
                                        onclick: { let id = project.id.clone(); move |_| remove_target.set(Some(id.clone())) },
                                        "Remove registration"
                                    }
                                    button {
                                        class: "project-action danger",
                                        disabled: busy_project().is_some() || project.folders.is_empty(),
                                        onclick: {
                                            let id = project.id.clone();
                                            move |_| {
                                                delete_confirmation.set(String::new());
                                                delete_target.set(Some(id.clone()));
                                            }
                                        },
                                        "Delete files…"
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
            if let Some(id) = delete_target()
                && let Some(project) = snapshot.projects.iter().find(|project| project.id == id).cloned()
            {
                div { class: "dialog-backdrop", role: "presentation",
                    section { class: "hermes-dialog compact delete-files-dialog", role: "alertdialog", aria_modal: "true", aria_label: "Delete project files",
                        header {
                            h2 { "Delete project files?" }
                            p { "This permanently deletes every folder registered to this project, then removes its registration. This cannot be undone." }
                        }
                        div { class: "delete-confirm-copy",
                            p { "Type " strong { "DELETE {project.name}" } " to confirm." }
                            input {
                                autofocus: true,
                                aria_label: "Project file deletion confirmation",
                                placeholder: "DELETE {project.name}",
                                value: "{delete_confirmation}",
                                oninput: move |event| delete_confirmation.set(event.value()),
                            }
                        }
                        footer {
                            button {
                                class: "button",
                                disabled: busy_project().is_some(),
                                onclick: move |_| {
                                    delete_target.set(None);
                                    delete_confirmation.set(String::new());
                                },
                                "Cancel"
                            }
                            button {
                                class: "button danger",
                                disabled: busy_project().is_some() || delete_confirmation() != format!("DELETE {}", project.name),
                                onclick: {
                                    let service = services.projects.clone();
                                    let id = project.id.clone();
                                    let confirmation = format!("DELETE {}", project.name);
                                    move |_| {
                                        if delete_confirmation() != confirmation || busy_project().is_some() {
                                            return;
                                        }
                                        busy_project.set(Some(id.clone()));
                                        let service = service.clone();
                                        let id = id.clone();
                                        let confirmation = confirmation.clone();
                                        spawn(async move {
                                            match service.delete_files(&id, &confirmation).await {
                                                Ok(result) => {
                                                    let mut snapshot_signal = state.snapshot;
                                                    snapshot_signal.set(result.snapshot);
                                                    delete_target.set(None);
                                                    delete_confirmation.set(String::new());
                                                }
                                                Err(problem) => {
                                                    let mut error = state.error;
                                                    error.set(Some(problem.to_string()));
                                                }
                                            }
                                            busy_project.set(None);
                                        });
                                    }
                                },
                                if busy_project().is_some() { "Deleting…" } else { "Delete files permanently" }
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
    let mut provider_view = use_signal(|| "accounts".to_owned());
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
        ("trust", "shield-check", "Trust Centre"),
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
                        button { class: if active() == id { "active" } else { "" }, onclick: move |_| {
                                active.set(id.to_owned());
                                if id == "providers" { provider_view.set("accounts".to_owned()); }
                            },
                            Codicon { name: icon }
                            span { "{label}" }
                        }
                        if id == "providers" && active() == "providers" {
                            div { class: "settings-rail-subnav", aria_label: "Provider settings",
                                for (view, view_icon, view_label) in [
                                    ("accounts", "account", "Accounts"),
                                    ("keys", "key", "API Keys"),
                                    ("custom-endpoints", "globe", "Custom Endpoints"),
                                ] {
                                    button {
                                        class: if provider_view() == view { "active" } else { "" },
                                        onclick: move |_| provider_view.set(view.to_owned()),
                                        Codicon { name: view_icon }
                                        span { "{view_label}" }
                                    }
                                }
                            }
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
                    } else if active() == "workspace" {
                        AgentConfigPanel { section: "workspace", icon: "desktop-download", label: "Workspace" }
                    } else if active() == "safety" {
                        AgentConfigPanel { section: "safety", icon: "lock", label: "Safety" }
                    } else if active() == "memory" {
                        AgentConfigPanel { section: "memory", icon: "database", label: "Memory & Context" }
                    } else if active() == "voice" {
                        AgentConfigPanel { section: "voice", icon: "unmute", label: "Voice" }
                    } else if active() == "advanced" {
                        AgentConfigPanel { section: "advanced", icon: "tools", label: "Advanced" }
                    } else if active() == "notifications" {
                        NotificationsSettingsPanel {}
                    } else if active() == "providers" && provider_view() == "accounts" {
                        ProviderAccountsPanel { on_want_api_key: move |()| provider_view.set("keys".to_owned()) }
                    } else if active() == "providers" && provider_view() == "keys" {
                        ProviderKeysPanel { on_custom_endpoint: move |()| provider_view.set("custom-endpoints".to_owned()) }
                    } else if active() == "providers" && provider_view() == "custom-endpoints" {
                        CustomEndpointsPanel {}
                    } else if active() == "gateway" {
                        GatewaySettingsPanel {}
                    } else if active() == "trust" {
                        TrustCentre {}
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

fn gateway_input(config: &ConnectionConfig, remote_token: &str) -> ConnectionConfigInput {
    ConnectionConfigInput {
        mode: config.mode,
        profile: config.profile.clone(),
        remote_auth_mode: Some(config.remote_auth_mode),
        remote_token: (!remote_token.trim().is_empty()).then(|| remote_token.trim().to_owned()),
        remote_url: Some(config.remote_url.clone()),
        cloud_org: Some(config.cloud_org.clone()),
        ssh_host: Some(config.ssh_host.clone()),
        ssh_user: Some(config.ssh_user.clone()),
        ssh_port: Some(config.ssh_port),
        ssh_key_path: Some(config.ssh_key_path.clone()),
        ssh_remote_hermes_path: Some(config.ssh_remote_hermes_path.clone()),
        ssh_remote_profile: Some(config.ssh_remote_profile.clone()),
    }
}

fn gateway_mode_copy(
    mode: ConnectionMode,
    scoped: bool,
) -> (&'static str, &'static str, &'static str) {
    match mode {
        ConnectionMode::Local if scoped => (
            "desktop-download",
            "Use default gateway",
            "Remove this profile's override and use the default connection.",
        ),
        ConnectionMode::Local => (
            "desktop-download",
            "Local gateway",
            "Start a private Hermes backend on localhost. This is the default and works offline.",
        ),
        ConnectionMode::Cloud => (
            "cloud",
            "Hermes Cloud",
            "Sign in once to Hermes Cloud and pick from the agents on your account — no URL to paste.",
        ),
        ConnectionMode::Remote => (
            "globe",
            "Remote gateway",
            "Connect this desktop shell to a remote Hermes backend.",
        ),
        ConnectionMode::Ssh => (
            "terminal",
            "Connect via SSH",
            "Launch Hermes over SSH and tunnel it to this app. Requires working key-based SSH access.",
        ),
    }
}

fn start_gateway_probe(
    service: Arc<dyn hermes_core::ConnectionService>,
    url: &str,
    mut draft: Signal<Option<ConnectionConfig>>,
    mut probe: Signal<Option<ConnectionProbeResult>>,
    mut probing: Signal<bool>,
) {
    let normalized = if url.contains("://") {
        url.trim().to_owned()
    } else {
        format!("https://{}", url.trim())
    };
    if normalized.trim_end_matches("https://").is_empty() {
        probe.set(None);
        probing.set(false);
        return;
    }
    probing.set(true);
    spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let still_current = draft().is_some_and(|config| {
            let current = if config.remote_url.contains("://") {
                config.remote_url.trim().to_owned()
            } else {
                format!("https://{}", config.remote_url.trim())
            };
            current == normalized
        });
        if !still_current {
            return;
        }
        match service.probe_config(&normalized).await {
            Ok(result) => {
                if let Some(mut config) = draft() {
                    config.remote_auth_mode = match result.auth_mode {
                        ProbeAuthMode::Oauth => RemoteAuthMode::Oauth,
                        ProbeAuthMode::Token | ProbeAuthMode::Unknown => RemoteAuthMode::Token,
                    };
                    draft.set(Some(config));
                }
                probe.set(Some(result));
            }
            Err(error) => probe.set(Some(ConnectionProbeResult {
                base_url: normalized,
                reachable: false,
                error: Some(error.to_string()),
                ..ConnectionProbeResult::default()
            })),
        }
        probing.set(false);
    });
}

#[component]
fn GatewaySettingsPanel() -> Element {
    let services = use_context::<AppServices>();
    let settings = use_context::<SettingsUiState>();
    let mut scope = use_signal(|| None::<String>);
    let mut draft = use_signal(|| None::<ConnectionConfig>);
    let mut remote_token = use_signal(String::new);
    let mut loading = use_signal(|| true);
    let mut saving = use_signal(|| false);
    let mut testing = use_signal(|| false);
    let mut signing_in = use_signal(|| false);
    let mut probe = use_signal(|| None::<ConnectionProbeResult>);
    let probing = use_signal(|| false);
    let mut message = use_signal(|| None::<String>);
    let mut error = use_signal(|| None::<String>);
    let mut refresh = use_signal(|| 0_u64);
    let mut ssh_mode = use_signal(|| false);
    let mut ssh_hosts = use_signal(|| vec![]);
    let load_service = services.connection.clone();
    let load_service_for_load = load_service.clone();
    let _load = use_resource(move || {
        let service = load_service_for_load.clone();
        let selected_scope = scope();
        let _revision = refresh();
        async move {
            loading.set(true);
            remote_token.set(String::new());
            probe.set(None);
            message.set(None);
            error.set(None);
            match service.config(selected_scope.as_deref()).await {
                Ok(config) => draft.set(Some(config)),
                Err(problem) => error.set(Some(problem.to_string())),
            }
            loading.set(false);
        }
    });

    let ssh_hosts_resource = use_resource(move || {
        let service = load_service.clone();
        let mode = ssh_mode();
        let mut hosts_signal = ssh_hosts;
        async move {
            if mode {
                let hosts_vec = service.list_ssh_hosts().await.unwrap_or_default();
                hosts_signal.set(hosts_vec.clone());
                hosts_vec
            } else {
                vec![]
            }
        }
    });

    let draft_mode = draft().map(|c| c.mode);
    use_effect(move || {
        if let Some(mode) = draft_mode {
            ssh_mode.set(mode == ConnectionMode::Ssh);
        }
    });

    let current_profile = (settings.settings)()
        .profile
        .filter(|profile| profile != "default");

    rsx! {
        section { class: "gateway-settings",
            div { class: "settings-section-title",
                Codicon { name: "globe" }
                h1 { "Gateway Connection" }
                if draft().is_some_and(|config| config.env_override) {
                    span { class: "settings-pill", "env override" }
                }
            }
            p { class: "settings-intro", "Local by default. Use remote when this app should drive a Hermes backend elsewhere. Per-profile overrides below." }
            if loading() {
                div { class: "settings-loading", span { class: "spinner" } "Loading gateway settings…" }
            } else if let Some(config) = draft() {
                if current_profile.is_some() {
                    section { class: "gateway-scope",
                        strong { "Applies to" }
                        div { class: "gateway-scope-chips",
                            button { class: if scope().is_none() { "active" } else { "" }, onclick: move |_| scope.set(None), "All profiles" }
                            if let Some(profile) = current_profile.clone() {
                                button { class: if scope().as_deref() == Some(profile.as_str()) { "active" } else { "" }, onclick: move |_| scope.set(Some(profile.clone())), "{profile}" }
                            }
                        }
                        if let Some(profile) = scope() {
                            p { "Connection used only when “{profile}” is the active profile. Choose Use default gateway to remove its override." }
                        } else {
                            p { "Default connection for every profile that has no override of its own." }
                        }
                    }
                }
                if config.env_override {
                    div { class: "gateway-warning", Codicon { name: "warning" } div { strong { "Environment variables are controlling this desktop session." } p { "Unset HERMES_DESKTOP_REMOTE_URL and HERMES_DESKTOP_REMOTE_TOKEN to use the saved setting below." } } }
                }
                section { class: "gateway-mode-section",
                    strong { "Connection mode" }
                    div { class: "gateway-mode-grid",
                        for mode in [ConnectionMode::Local, ConnectionMode::Cloud, ConnectionMode::Remote, ConnectionMode::Ssh] {
                            {
                                let (icon, title, description) = gateway_mode_copy(mode, scope().is_some());
                                rsx! {
                                    button {
                                        class: if config.mode == mode { "gateway-mode-card active" } else { "gateway-mode-card" },
                                        disabled: config.env_override,
                                        onclick: move |_| {
                                            if let Some(mut next) = draft() {
                                                next.mode = mode;
                                                next.profile = scope();
                                                draft.set(Some(next));
                                                message.set(None);
                                                error.set(None);
                                            }
                                        },
                                        div { Codicon { name: icon } strong { "{title}" } if config.mode == mode { Codicon { name: "check" } } }
                                        p { "{description}" }
                                    }
                                }
                            }
                        }
                    }
                }
                if config.mode == ConnectionMode::Cloud && !config.env_override {
                    section { class: "gateway-fields",
                        SettingsRow { title: "Hermes Cloud", description: "Sign in to Hermes Cloud to discover the agents on your account.",
                            button { class: "button primary", disabled: true, Codicon { name: "sign-in" } "Sign in to Hermes Cloud" }
                        }
                        p { class: "gateway-pending", "Hermes Cloud sign-in and agent discovery are the next connection-service slice." }
                    }
                }
                if config.mode == ConnectionMode::Remote && !config.env_override {
                    section { class: "gateway-fields",
                        section { class: "settings-list-row",
                            div { class: "settings-row-copy", strong { "Remote URL" } p { "Base URL for the remote dashboard backend. Path prefixes are supported, for example /hermes." } }
                            div { class: "settings-row-action",
                                input { class: "settings-input gateway-control", value: "{config.remote_url}", placeholder: "https://gateway.example.com/hermes", oninput: {
                                    let service = services.connection.clone();
                                    move |event| {
                                        let value = event.value();
                                        if let Some(mut next) = draft() {
                                            next.remote_url.clone_from(&value);
                                            next.profile = scope();
                                            draft.set(Some(next));
                                        }
                                        probe.set(None);
                                        start_gateway_probe(service.clone(), &value, draft, probe, probing);
                                    }
                                } }
                            }
                        }
                        if probing() {
                            p { class: "gateway-probe", span { class: "spinner" } "Checking how this gateway authenticates…" }
                        } else if let Some(result) = probe() && !result.reachable {
                            p { class: "gateway-probe error", Codicon { name: "warning" } "Could not reach this gateway yet. Check the URL — the auth method will appear once it responds." }
                        }
                        if config.remote_auth_mode == RemoteAuthMode::Oauth {
                            if config.remote_oauth_connected {
                                section { class: "settings-list-row",
                                    div { class: "settings-row-copy", strong { "Authentication" } p { "This gateway uses OAuth. You are signed in; the session refreshes automatically." } }
                                    div { class: "settings-row-action gateway-auth-actions",
                                        span { class: "settings-pill", Codicon { name: "check" } "Signed in" }
                                        button { class: "button ghost", disabled: signing_in(), onclick: {
                                            let service = services.connection.clone();
                                            let remote_url = config.remote_url.clone();
                                            let selected_scope = scope();
                                            move |_| {
                                                signing_in.set(true); message.set(None); error.set(None);
                                                let service = service.clone(); let remote_url = remote_url.clone(); let selected_scope = selected_scope.clone();
                                                spawn(async move {
                                                    match service.oauth_logout(&remote_url).await {
                                                        Ok(_) => match service.config(selected_scope.as_deref()).await {
                                                            Ok(saved) => { draft.set(Some(saved)); message.set(Some("Signed out from the remote gateway.".into())); }
                                                            Err(problem) => error.set(Some(problem.to_string())),
                                                        },
                                                        Err(problem) => error.set(Some(problem.to_string())),
                                                    }
                                                    signing_in.set(false);
                                                });
                                            }
                                        }, if signing_in() { span { class: "spinner" } } "Sign out" }
                                    }
                                }
                            } else {
                                section { class: "settings-list-row",
                                    div { class: "settings-row-copy", strong { "Authentication" } p { "This gateway uses OAuth. Sign in to authorize this desktop app." } }
                                    div { class: "settings-row-action", button { class: "button primary", disabled: signing_in() || config.remote_url.trim().is_empty(), onclick: {
                                        let service = services.connection.clone();
                                        let remote_url = config.remote_url.clone();
                                        let selected_scope = scope();
                                        let input = gateway_input(&config, "");
                                        move |_| {
                                            signing_in.set(true); message.set(None); error.set(None);
                                            let service = service.clone(); let remote_url = remote_url.clone(); let selected_scope = selected_scope.clone(); let input = input.clone();
                                            spawn(async move {
                                                match service.save_config(&input).await {
                                                    Ok(_) => match service.oauth_login(&remote_url).await {
                                                        Ok(result) if result.connected => match service.config(selected_scope.as_deref()).await {
                                                            Ok(saved) => { draft.set(Some(saved)); message.set(Some("Signed in to the remote gateway.".into())); }
                                                            Err(problem) => error.set(Some(problem.to_string())),
                                                        },
                                                        Ok(_) => error.set(Some("Gateway sign-in did not complete.".into())),
                                                        Err(problem) => error.set(Some(problem.to_string())),
                                                    },
                                                    Err(problem) => error.set(Some(problem.to_string())),
                                                }
                                                signing_in.set(false);
                                            });
                                        }
                                    }, if signing_in() { span { class: "spinner" } } else { Codicon { name: "sign-in" } } "Sign in" } }
                                }
                            }
                        } else {
                            section { class: "settings-list-row",
                                div { class: "settings-row-copy", strong { "Session token" } p { "The dashboard session token used for REST and WebSocket access. Leave blank to keep the saved token." } }
                                div { class: "settings-row-action gateway-token-control",
                                    if config.remote_token_set {
                                        if let Some(preview) = config.remote_token_preview.as_deref() {
                                            small { "Existing token {preview}" }
                                        } else {
                                            small { "Existing token saved" }
                                        }
                                    }
                                    input { class: "settings-input gateway-control mono", r#type: "password", autocomplete: "off", value: "{remote_token}", placeholder: "Paste session token", oninput: move |event| remote_token.set(event.value()) }
                                }
                            }
                        }
                    }
                }
                if config.mode == ConnectionMode::Ssh && !config.env_override {
                    section { class: "gateway-fields",
                        GatewayTextField { title: "Host", description: "user@host, or a Host alias from ~/.ssh/config.", value: config.ssh_host.clone(), placeholder: "", monospace: false, on_change: move |value| { if let Some(mut next) = draft() { next.ssh_host = value; draft.set(Some(next)); } }                         },
                        if !ssh_hosts().is_empty() {
                            section { class: "settings-list-row",
                                div { class: "settings-row-copy", strong { "Host alias" } p { "Select from saved SSH Host aliases" } },
                                div { class: "settings-row-action",
                                    select {
                                        class: "settings-input gateway-control",
                                        onchange: move |event| {
                                            let val = event.value();
                                            if let Some(mut next) = draft() {
                                                next.ssh_host = val;
                                                draft.set(Some(next));
                                            }
                                        },
                                        for host in ssh_hosts() {
                                            option { "{host}" }
                                        }
                                    }
                                }
                            }
                        }
                        GatewayTextField { title: "User", description: "Blank = ~/.ssh/config or your current user.", value: config.ssh_user.clone(), placeholder: "from ~/.ssh/config", monospace: false, on_change: move |value| { if let Some(mut next) = draft() { next.ssh_user = value; draft.set(Some(next)); } } }
                        GatewayTextField { title: "Port", description: "Blank = 22 or the ~/.ssh/config port.", value: config.ssh_port.map(|port| port.to_string()).unwrap_or_default(), placeholder: "22", monospace: false, on_change: move |value: String| { if let Some(mut next) = draft() { next.ssh_port = value.parse().ok(); draft.set(Some(next)); } } }
                        GatewayTextField { title: "Identity file", description: "Private key path. Blank = ssh-agent or ~/.ssh/config.", value: config.ssh_key_path.clone(), placeholder: "", monospace: true, on_change: move |value| { if let Some(mut next) = draft() { next.ssh_key_path = value; draft.set(Some(next)); } } }
                        GatewayTextField { title: "Hermes path (optional)", description: "Full path to the remote hermes binary. Blank = auto-detect.", value: config.ssh_remote_hermes_path.clone(), placeholder: "auto-detect", monospace: true, on_change: move |value| { if let Some(mut next) = draft() { next.ssh_remote_hermes_path = value; draft.set(Some(next)); } } }
                        if scope().is_some() {
                            GatewayTextField { title: "Remote profile (optional)", description: "Profile name on the remote host. Blank = use the Desktop profile name.", value: config.ssh_remote_profile.clone(), placeholder: "", monospace: true, on_change: move |value| { if let Some(mut next) = draft() { next.ssh_remote_profile = value; draft.set(Some(next)); } } }
                        }
                    }
                }
                if let Some(notice) = message() { p { class: "gateway-message", Codicon { name: "check" } "{notice}" } }
                if let Some(problem) = error() { p { class: "inline-error", role: "alert", "{problem}" } }
                if config.mode != ConnectionMode::Cloud {
                    footer { class: "gateway-actions",
                        if matches!(config.mode, ConnectionMode::Remote | ConnectionMode::Ssh) {
                            button { class: "button ghost gateway-test", disabled: testing(), onclick: {
                                let service = services.connection.clone();
                                let input = gateway_input(&config, &remote_token());
                                move |_| {
                                    testing.set(true); message.set(None); error.set(None);
                                    let service = service.clone(); let input = input.clone();
                                    spawn(async move {
                                        match service.test_config(&input).await {
                                            Ok(result) if result.reachable == Some(false) => error.set(result.error.or(Some("Connection test failed.".into()))),
                                            Ok(result) => message.set(Some(result.base_url.map_or_else(|| "Connection reachable".into(), |url| format!("Connected to {url}")))),
                                            Err(problem) => error.set(Some(problem.to_string())),
                                        }
                                        testing.set(false);
                                    });
                                }
                            }, if testing() { span { class: "spinner" } } if config.mode == ConnectionMode::Ssh { "Test SSH" } else { "Test remote" } }
                        }
                        button { class: "button ghost", disabled: saving() || config.env_override, onclick: {
                            let service = services.connection.clone(); let input = gateway_input(&config, &remote_token());
                            move |_| { saving.set(true); message.set(None); error.set(None); let service = service.clone(); let input = input.clone(); spawn(async move { match service.save_config(&input).await { Ok(saved) => { draft.set(Some(saved)); remote_token.set(String::new()); message.set(Some("Saved for the next restart.".into())); }, Err(problem) => error.set(Some(problem.to_string())) } saving.set(false); }); }
                        }, "Save for next restart" }
                        button { class: "button primary", disabled: saving() || config.env_override, onclick: {
                            let service = services.connection.clone(); let input = gateway_input(&config, &remote_token());
                            move |_| { saving.set(true); message.set(None); error.set(None); let service = service.clone(); let input = input.clone(); spawn(async move { match service.apply_config(&input).await { Ok(saved) => { draft.set(Some(saved)); remote_token.set(String::new()); message.set(Some("Gateway connection restarted.".into())); }, Err(problem) => { refresh += 1; error.set(Some(problem.to_string())); } } saving.set(false); }); }
                        }, if saving() { span { class: "spinner" } } "Save and reconnect" }
                    }
                }
                section { class: "settings-list-row gateway-diagnostics",
                    div { class: "settings-row-copy", strong { "Diagnostics" } p { "Reveal desktop.log in your file manager — useful when the gateway fails to start." } }
                    div { class: "settings-row-action", button { class: "button ghost", disabled: true, Codicon { name: "output" } "Open logs" } }
                }
            } else if let Some(problem) = error() {
                p { class: "inline-error", role: "alert", "{problem}" }
            }
        }
    }
}

#[component]
fn TrustCentre() -> Element {
    let services = use_context::<AppServices>();
    let mut loading = use_signal(|| true);
    let mut saving = use_signal(|| false);
    let mut message = use_signal(|| None::<String>);
    let mut error = use_signal(|| None::<String>);
    let mut current_policy = use_signal(|| String::new());
    let mut trust_items = use_signal(|| vec![]);
    let mut refresh = use_signal(|| 0_u64);

    let trust_services = services.trust.clone();
    let trust_snapshot = use_resource(move || {
        let service = trust_services.clone();
        let _rev = refresh();
        async move {
            loading.set(true);
            match service.snapshot().await {
                Ok(snapshot) => {
                    current_policy.set(snapshot.policy.clone());
                    trust_items.set({
                        let mut items = vec![];
                        for item in &snapshot.skills {
                            items.push((
                                item.id.clone(),
                                item.label.clone(),
                                item.state.clone(),
                                item.source.clone(),
                            ));
                        }
                        for item in &snapshot.mcp_servers {
                            items.push((
                                item.id.clone(),
                                item.label.clone(),
                                item.state.clone(),
                                item.source.clone(),
                            ));
                        }
                        for item in &snapshot.delegations {
                            items.push((
                                item.id.clone(),
                                item.label.clone(),
                                item.state.clone(),
                                item.source.clone(),
                            ));
                        }
                        items
                    });
                    loading.set(false);
                    Some(snapshot)
                }
                Err(e) => {
                    loading.set(false);
                    error.set(Some(e.to_string()));
                    None
                }
            }
        }
    });

    let save_policy = |policy: &str| {
        let service = services.trust.clone();
        let policy = policy.to_owned();
        let mut message = message;
        let mut error = error;
        let mut refresh = refresh;
        spawn(async move {
            match service.set_policy(&policy).await {
                Ok(_) => {
                    message.set(Some("Trust policy updated successfully.".into()));
                    error.set(None);
                    refresh += 1;
                }
                Err(e) => {
                    error.set(Some(e.to_string()));
                    message.set(None);
                }
            }
        });
    };

    rsx! {
        section { class: "trust-centre",
            div { class: "settings-section-title",
                Codicon { name: "shield-check" }
                h1 { "Trust Centre" }
            }
            p { class: "settings-intro", "Define which skills, MCP servers and delegations are allowed to run in this profile." }
            if loading() {
                div { class: "settings-loading", span { class: "spinner" } "Loading trust settings…" }
            } else if let Some(problem) = error() {
                p { class: "inline-error", role: "alert", "{problem}" }
            } else {
                section { class: "gateway-fields",
                    div { class: "settings-list-row",
                        div { class: "settings-row-copy", strong { "Trust policy" } p { "Allowed trust policy value. Set to 'default' or a custom policy name." } }
                        div { class: "settings-row-action",
                            input {
                                class: "settings-input gateway-control",
                                value: "{current_policy()}",
                                placeholder: "default",
                                oninput: move |event| current_policy.set(event.value())
                            }
                        }
                    }
                }
                if !trust_items().is_empty() {
                    section { class: "settings-list-row",
                        div { class: "settings-row-copy", strong { "Trust items" } p { "Allowed components in this profile." } }
                        div { class: "settings-row-action",
                            for item in trust_items() {
                                div { class: "trust-item",
                                    div { class: "trust-id", "{item.0}" }
                                    div { class: "trust-label", "{item.1}" }
                                    div { class: "trust-state", "{item.2}" }
                                    if let Some(src) = item.3 {
                                        div { class: "trust-source", "source: {src}" }
                                    }
                                }
                            }
                        }
                    }
                }
                if let Some(notice) = message() {
                    p { class: "gateway-message", Codicon { name: "check" } "{notice}" }
                }
                if let Some(problem) = error() {
                    p { class: "inline-error", role: "alert", "{problem}" }
                }
                footer { class: "gateway-actions",
                    button {
                        class: "button primary",
                        disabled: saving() || current_policy().is_empty(),
                        onclick: move |_| {
                            saving.set(true);
                            message.set(None);
                            error.set(None);
                            let policy = current_policy().clone();
                            let service = services.trust.clone();
                            spawn(async move {
                                match service.set_policy(&policy).await {
                                    Ok(_) => {
                                        message.set(Some("Trust policy updated successfully.".into()));
                                        error.set(None);
                                        refresh += 1;
                                    }
                                    Err(e) => {
                                        error.set(Some(e.to_string()));
                                        message.set(None);
                                    }
                                }
                                saving.set(false);
                            });
                        },
                        if saving() {
                            span { class: "spinner" }
                        } else {
                            "Save trust policy"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn GatewayTextField(
    title: &'static str,
    description: &'static str,
    value: String,
    placeholder: &'static str,
    monospace: bool,
    on_change: EventHandler<String>,
) -> Element {
    rsx! {
        section { class: "settings-list-row",
            div { class: "settings-row-copy", strong { "{title}" } p { "{description}" } }
            div { class: "settings-row-action",
                input { class: if monospace { "settings-input gateway-control mono" } else { "settings-input gateway-control" }, value: "{value}", placeholder: "{placeholder}", oninput: move |event| on_change.call(event.value()) }
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
                } else if section == "workspace" {
                    "Choose the default tool workspace, repository discovery policy, and execution boundaries for new sessions."
                } else if section == "safety" {
                    "Control approvals, private-network access, secret redaction, and rollback checkpoints."
                } else if section == "memory" {
                    "Choose what Hermes remembers and how long conversations are compressed near the context limit."
                } else if section == "voice" {
                    "Configure speech synthesis, transcription, and the active provider-specific voice controls."
                } else if section == "advanced" {
                    "Tune toolsets, execution backends, output limits, agent turns, delegation, and update behavior."
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
                    if section == "chat" {
                        ChatConfigFields {
                            snapshot,
                            loaded,
                            profile: profile.clone(),
                            saving,
                            error,
                        }
                    } else if section == "workspace" {
                        WorkspaceConfigFields {
                            snapshot,
                            loaded,
                            profile: profile.clone(),
                            saving,
                            error,
                        }
                    } else if section == "safety" {
                        SafetyConfigFields {
                            snapshot,
                            loaded,
                            profile: profile.clone(),
                            saving,
                            error,
                        }
                    } else if section == "memory" {
                        MemoryConfigFields {
                            snapshot,
                            loaded,
                            profile: profile.clone(),
                            saving,
                            error,
                        }
                    } else if section == "voice" {
                        VoiceConfigFields {
                            snapshot,
                            loaded,
                            profile: profile.clone(),
                            saving,
                            error,
                        }
                    } else {
                        AdvancedConfigFields {
                            snapshot,
                            loaded,
                            profile: profile.clone(),
                            saving,
                            error,
                        }
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
        if let Some(moa) = models.moa.clone() {
            MoaSettings { initial: moa, providers: models.options.providers.clone(), profile: profile.clone() }
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
fn MoaSettings(
    initial: MoaConfig,
    providers: Vec<ModelProvider>,
    profile: Option<String>,
) -> Element {
    let service = use_context::<AppServices>().models.clone();
    let initial_preset = initial.default_preset.clone();
    let config = use_signal(|| initial);
    let mut selected = use_signal(|| initial_preset);
    let mut new_name = use_signal(String::new);
    let saving = use_signal(|| false);
    let error = use_signal(|| None::<String>);
    let available_providers = providers
        .into_iter()
        .filter(|provider| !provider.slug.eq_ignore_ascii_case("moa"))
        .collect::<Vec<_>>();
    let current_name = selected();
    let current_config = config();
    let Some(current) = current_config.presets.get(&current_name).cloned() else {
        return rsx! {};
    };
    let preset_names = current_config.presets.keys().cloned().collect::<Vec<_>>();
    rsx! {
        section { class: "moa-settings",
            div { class: "settings-subheading",
                div { Codicon { name: "server-process" } strong { "Mixture of Agents" } }
            }
            p { class: "settings-intro", "Configure named presets that appear as models under the Mixture of Agents provider. The aggregator is the acting model." }
            div { class: "moa-toolbar",
                select {
                    class: "settings-select compact",
                    value: "{current_name}",
                    disabled: saving(),
                    onchange: move |event| selected.set(event.value()),
                    for name in &preset_names { option { value: "{name}", selected: *name == current_name, "{name}" } }
                }
                label { class: "moa-enabled", "Enabled"
                    span { class: "settings-switch",
                        input {
                            r#type: "checkbox",
                            checked: current.enabled,
                            disabled: saving(),
                            onchange: {
                                let service = service.clone();
                                let profile = profile.clone();
                                let name = current_name.clone();
                                move |event| {
                                    let mut next = config();
                                    if let Some(preset) = next.presets.get_mut(&name) {
                                        preset.enabled = event.checked();
                                    }
                                    persist_moa(service.clone(), profile.clone(), next, config, saving, error);
                                }
                            }
                        }
                        span {}
                    }
                }
                button {
                    class: "button",
                    disabled: saving(),
                    onclick: {
                        let service = service.clone();
                        let profile = profile.clone();
                        let name = current_name.clone();
                        move |_| {
                            let mut next = config();
                            next.default_preset.clone_from(&name);
                            persist_moa(service.clone(), profile.clone(), next, config, saving, error);
                        }
                    },
                    "Set default"
                }
                button {
                    class: "button ghost",
                    disabled: saving() || preset_names.len() <= 1,
                    onclick: {
                        let service = service.clone();
                        let profile = profile.clone();
                        let name = current_name.clone();
                        move |_| {
                            let mut next = config();
                            if next.presets.len() <= 1 {
                                return;
                            }
                            next.presets.remove(&name);
                            let fallback = next.presets.keys().next().cloned().unwrap_or_default();
                            if next.default_preset == name {
                                next.default_preset.clone_from(&fallback);
                            }
                            if next.active_preset == name {
                                next.active_preset.clear();
                            }
                            selected.set(fallback);
                            persist_moa(service.clone(), profile.clone(), next, config, saving, error);
                        }
                    },
                    "Delete"
                }
                input {
                    class: "settings-input moa-name",
                    placeholder: "new preset",
                    disabled: saving(),
                    value: "{new_name}",
                    oninput: move |event| new_name.set(event.value()),
                }
                button {
                    class: "button",
                    disabled: saving() || new_name().trim().is_empty() || current_config.presets.contains_key(new_name().trim()),
                    onclick: {
                        let service = service.clone();
                        let profile = profile.clone();
                        let template = current.clone();
                        move |_| {
                            let name = new_name().trim().to_owned();
                            if name.is_empty() {
                                return;
                            }
                            let mut next = config();
                            if next.presets.contains_key(&name) {
                                return;
                            }
                            next.presets.insert(name.clone(), template.clone());
                            selected.set(name);
                            new_name.set(String::new());
                            persist_moa(service.clone(), profile.clone(), next, config, saving, error);
                        }
                    },
                    "Add preset"
                }
            }
            p { class: "moa-default", "Default: " span { "{current_config.default_preset}" } }
            div { class: "moa-slot-list",
                for (index, slot) in current.reference_models.iter().cloned().enumerate() {
                    {
                        let slot_models = models_for_provider(&available_providers, &slot.provider, &slot.model);
                        let enabled = slot.enabled.unwrap_or(true);
                        rsx! {
                            div { class: if enabled { "moa-slot" } else { "moa-slot disabled" }, key: "{current_name}-ref-{index}",
                                div { class: "moa-slot-heading",
                                    div { strong { "Reference {index + 1}" } small { "{slot.provider} · {slot.model}" } }
                                    label { class: "settings-switch",
                                        input {
                                            r#type: "checkbox",
                                            checked: enabled,
                                            disabled: saving(),
                                            aria_label: "Toggle reference model",
                                            onchange: {
                                                let service = service.clone();
                                                let profile = profile.clone();
                                                let name = current_name.clone();
                                                move |event| {
                                                    let mut next = config();
                                                    if let Some(reference) = next.presets.get_mut(&name).and_then(|preset| preset.reference_models.get_mut(index)) {
                                                        reference.enabled = Some(event.checked());
                                                    }
                                                    persist_moa(service.clone(), profile.clone(), next, config, saving, error);
                                                }
                                            }
                                        }
                                        span {}
                                    }
                                }
                                div { class: "moa-slot-controls",
                                    select {
                                        class: "settings-select compact",
                                        disabled: saving(),
                                        value: "{slot.provider}",
                                        onchange: {
                                            let service = service.clone();
                                            let profile = profile.clone();
                                            let name = current_name.clone();
                                            move |event| {
                                                let mut next = config();
                                                if let Some(reference) = next.presets.get_mut(&name).and_then(|preset| preset.reference_models.get_mut(index)) {
                                                    reference.provider = event.value();
                                                    reference.model.clear();
                                                }
                                                persist_moa(service.clone(), profile.clone(), next, config, saving, error);
                                            }
                                        },
                                        for provider in &available_providers {
                                            option { value: "{provider.slug}", selected: provider.slug == slot.provider, "{provider.name}" }
                                        }
                                    }
                                    select {
                                        class: "settings-select moa-model",
                                        disabled: saving(),
                                        value: "{slot.model}",
                                        onchange: {
                                            let service = service.clone();
                                            let profile = profile.clone();
                                            let name = current_name.clone();
                                            move |event| {
                                                let mut next = config();
                                                if let Some(reference) = next.presets.get_mut(&name).and_then(|preset| preset.reference_models.get_mut(index)) {
                                                    reference.model = event.value();
                                                }
                                                persist_moa(service.clone(), profile.clone(), next, config, saving, error);
                                            }
                                        },
                                        for model in &slot_models { option { value: "{model}", selected: *model == slot.model, "{model}" } }
                                    }
                                    button {
                                        class: "button ghost",
                                        disabled: saving() || current.reference_models.len() <= 1,
                                        onclick: {
                                            let service = service.clone();
                                            let profile = profile.clone();
                                            let name = current_name.clone();
                                            move |_| {
                                                let mut next = config();
                                                if let Some(preset) = next.presets.get_mut(&name)
                                                    && preset.reference_models.len() > 1
                                                {
                                                    preset.reference_models.remove(index);
                                                }
                                                persist_moa(service.clone(), profile.clone(), next, config, saving, error);
                                            }
                                        },
                                        "Remove"
                                    }
                                }
                            }
                        }
                    }
                }
                button {
                    class: "button moa-add-reference",
                    disabled: saving(),
                    onclick: {
                        let service = service.clone();
                        let profile = profile.clone();
                        let name = current_name.clone();
                        move |_| {
                            let mut next = config();
                            if let Some(preset) = next.presets.get_mut(&name) {
                                let mut slot = preset.aggregator.clone();
                                slot.enabled = Some(true);
                                preset.reference_models.push(slot);
                            }
                            persist_moa(service.clone(), profile.clone(), next, config, saving, error);
                        }
                    },
                    Codicon { name: "add" }
                    "Add reference model"
                }
                {
                    let aggregator_models = models_for_provider(&available_providers, &current.aggregator.provider, &current.aggregator.model);
                    rsx! {
                        div { class: "moa-slot aggregator",
                            div { class: "moa-slot-heading",
                                div { strong { "Aggregator" } small { "{current.aggregator.provider} · {current.aggregator.model}" } }
                            }
                            div { class: "moa-slot-controls",
                                select {
                                    class: "settings-select compact",
                                    disabled: saving(),
                                    value: "{current.aggregator.provider}",
                                    onchange: {
                                        let service = service.clone();
                                        let profile = profile.clone();
                                        let name = current_name.clone();
                                        move |event| {
                                            let mut next = config();
                                            if let Some(preset) = next.presets.get_mut(&name) {
                                                preset.aggregator.provider = event.value();
                                                preset.aggregator.model.clear();
                                            }
                                            persist_moa(service.clone(), profile.clone(), next, config, saving, error);
                                        }
                                    },
                                    for provider in &available_providers {
                                        option { value: "{provider.slug}", selected: provider.slug == current.aggregator.provider, "{provider.name}" }
                                    }
                                }
                                select {
                                    class: "settings-select moa-model",
                                    disabled: saving(),
                                    value: "{current.aggregator.model}",
                                    onchange: {
                                        let service = service.clone();
                                        let profile = profile.clone();
                                        let name = current_name.clone();
                                        move |event| {
                                            let mut next = config();
                                            if let Some(preset) = next.presets.get_mut(&name) {
                                                preset.aggregator.model = event.value();
                                            }
                                            persist_moa(service.clone(), profile.clone(), next, config, saving, error);
                                        }
                                    },
                                    for model in &aggregator_models { option { value: "{model}", selected: *model == current.aggregator.model, "{model}" } }
                                }
                            }
                        }
                    }
                }
            }
            if saving() { p { class: "settings-save-state", "Saving MoA preset…" } }
            if let Some(save_error) = error() { p { class: "inline-error", role: "alert", "{save_error}" } }
        }
    }
}

fn models_for_provider(providers: &[ModelProvider], provider: &str, active: &str) -> Vec<String> {
    let mut models = providers
        .iter()
        .find(|candidate| candidate.slug == provider)
        .map(|candidate| candidate.models.clone())
        .unwrap_or_default();
    if !active.is_empty() && !models.iter().any(|model| model == active) {
        models.insert(0, active.to_owned());
    }
    models
}

fn moa_complete(config: &MoaConfig) -> bool {
    config.presets.values().all(|preset| {
        !preset.aggregator.provider.trim().is_empty()
            && !preset.aggregator.model.trim().is_empty()
            && !preset.reference_models.is_empty()
            && preset
                .reference_models
                .iter()
                .all(|slot| !slot.provider.trim().is_empty() && !slot.model.trim().is_empty())
    })
}

fn persist_moa(
    service: Arc<dyn ModelService>,
    profile: Option<String>,
    next: MoaConfig,
    mut state: Signal<MoaConfig>,
    mut saving: Signal<bool>,
    mut error: Signal<Option<String>>,
) {
    let before = state();
    state.set(next.clone());
    error.set(None);
    if !moa_complete(&next) || saving() {
        return;
    }
    saving.set(true);
    spawn(async move {
        match service.save_moa(profile.as_deref(), &next).await {
            Ok(saved) => state.set(saved),
            Err(save_error) => {
                state.set(before);
                error.set(Some(save_error.to_string()));
            }
        }
        saving.set(false);
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

const WORKSPACE_FIELDS: &[(&str, &str, &str)] = &[
    (
        "terminal.cwd",
        "Working Directory",
        "Default project folder for tool and terminal work.",
    ),
    (
        "desktop.repo_scan_enabled",
        "Automatic Repository Discovery",
        "Scan local folders for Git repositories to show in Projects.",
    ),
    (
        "desktop.repo_scan_roots",
        "Repository Discovery Roots",
        "Folders to scan. Leave empty to scan your home directory.",
    ),
    (
        "desktop.repo_scan_exclude_paths",
        "Excluded Repository Paths",
        "Folders and their descendants to skip during repository discovery.",
    ),
    (
        "code_execution.mode",
        "Code Execution Mode",
        "How strictly code execution is scoped to the current project.",
    ),
    (
        "terminal.persistent_shell",
        "Persistent Shell",
        "Keep shell state between commands when the backend supports it.",
    ),
    (
        "terminal.env_passthrough",
        "Environment Passthrough",
        "Environment variables to pass into tool execution.",
    ),
    (
        "file_read_max_chars",
        "File Read Limit",
        "Maximum characters Hermes can read from one file request.",
    ),
];

const SAFETY_FIELDS: &[(&str, &str, &str)] = &[
    (
        "approvals.mode",
        "Approval Mode",
        "How Hermes handles commands that need explicit approval.",
    ),
    (
        "approvals.timeout",
        "Approval Timeout",
        "How long approval prompts wait before timing out.",
    ),
    ("approvals.mcp_reload_confirm", "Confirm MCP Reloads", ""),
    ("command_allowlist", "Command Allowlist", ""),
    (
        "security.redact_secrets",
        "Redact Secrets",
        "Hide detected secrets from model-visible content when possible.",
    ),
    ("security.allow_private_urls", "Allow Private URLs", ""),
    ("browser.allow_private_urls", "Browser Private URLs", ""),
    (
        "browser.auto_local_for_private_urls",
        "Local Browser For Private URLs",
        "",
    ),
    (
        "checkpoints.enabled",
        "File Checkpoints",
        "Create rollback snapshots before file edits.",
    ),
];

const MEMORY_FIELDS: &[(&str, &str, &str)] = &[
    (
        "memory.memory_enabled",
        "Persistent Memory",
        "Save durable memories that can help future sessions.",
    ),
    (
        "memory.user_profile_enabled",
        "User Profile",
        "Maintain a compact profile of user preferences.",
    ),
    ("memory.memory_char_limit", "Memory Budget", ""),
    ("memory.user_char_limit", "Profile Budget", ""),
    ("memory.provider", "Memory Provider", ""),
    (
        "context.engine",
        "Context Engine",
        "Strategy for managing long conversations near the context limit.",
    ),
    (
        "compression.enabled",
        "Auto-Compression",
        "Summarize older context when conversations get large.",
    ),
    ("compression.threshold", "Compression Threshold", ""),
    ("compression.target_ratio", "Compression Target", ""),
    (
        "compression.protect_last_n",
        "Protected Recent Messages",
        "",
    ),
];

const VOICE_FIELDS: &[(&str, &str, &str)] = &[
    ("tts.provider", "Text-To-Speech Provider", ""),
    (
        "stt.enabled",
        "Speech To Text",
        "Enable local or provider-backed speech transcription.",
    ),
    (
        "stt.echo_transcripts",
        "Echo Transcripts",
        "Post the raw voice transcript back to the chat.",
    ),
    ("stt.provider", "Speech-To-Text Provider", ""),
    (
        "voice.auto_tts",
        "Read Responses Aloud",
        "Automatically speak assistant responses.",
    ),
    ("tts.edge.voice", "Edge Voice", ""),
    ("tts.openai.model", "OpenAI TTS Model", ""),
    ("tts.openai.voice", "OpenAI Voice", ""),
    ("tts.elevenlabs.voice_id", "ElevenLabs Voice", ""),
    ("tts.elevenlabs.model_id", "ElevenLabs Model", ""),
    ("tts.xai.voice_id", "xAI (Grok) Voice", ""),
    ("tts.xai.language", "xAI Language", ""),
    ("tts.xai.speed", "xAI Playback Speed", ""),
    ("tts.xai.auto_speech_tags", "xAI Auto Speech Tags", ""),
    (
        "tts.xai.optimize_streaming_latency",
        "xAI Streaming Latency Optimization",
        "",
    ),
    ("tts.xai.sample_rate", "xAI Sample Rate", ""),
    ("tts.xai.bit_rate", "xAI Bit Rate", ""),
    ("tts.minimax.model", "MiniMax TTS Model", ""),
    ("tts.minimax.voice_id", "MiniMax Voice", ""),
    ("tts.mistral.model", "Mistral TTS Model", ""),
    ("tts.mistral.voice_id", "Mistral Voice", ""),
    ("tts.gemini.model", "Gemini TTS Model", ""),
    ("tts.gemini.voice", "Gemini Voice", ""),
    ("tts.neutts.model", "NeuTTS Model", ""),
    ("tts.neutts.device", "NeuTTS Device", ""),
    ("tts.kittentts.model", "KittenTTS Model", ""),
    ("tts.kittentts.voice", "KittenTTS Voice", ""),
    ("tts.piper.voice", "Piper Voice", ""),
    ("tts.deepinfra.model", "DeepInfra TTS Model", ""),
    ("tts.deepinfra.voice", "DeepInfra Voice", ""),
    ("stt.local.model", "Local Transcription Model", ""),
    ("stt.local.language", "Transcription Language", ""),
    ("stt.openai.model", "OpenAI STT Model", ""),
    ("stt.groq.model", "Groq STT Model", ""),
    ("stt.mistral.model", "Mistral STT Model", ""),
    ("stt.elevenlabs.model_id", "ElevenLabs STT Model", ""),
    ("stt.elevenlabs.language_code", "ElevenLabs Language", ""),
    ("stt.elevenlabs.tag_audio_events", "Tag Audio Events", ""),
    ("stt.elevenlabs.diarize", "Speaker Diarization", ""),
    ("voice.record_key", "Voice Shortcut", ""),
    ("voice.max_recording_seconds", "Max Recording Length", ""),
];

const ADVANCED_FIELDS: &[(&str, &str, &str)] = &[
    ("toolsets", "Enabled Toolsets", ""),
    ("terminal.backend", "Execution Backend", ""),
    ("terminal.timeout", "Command Timeout", ""),
    (
        "terminal.docker_image",
        "Docker Image",
        "Container image used when the execution backend is Docker.",
    ),
    (
        "terminal.singularity_image",
        "Singularity Image",
        "Image used when the execution backend is Singularity.",
    ),
    (
        "terminal.modal_image",
        "Modal Image",
        "Image used when the execution backend is Modal.",
    ),
    (
        "terminal.daytona_image",
        "Daytona Image",
        "Image used when the execution backend is Daytona.",
    ),
    ("tool_output.max_bytes", "Terminal Output Limit", ""),
    ("tool_output.max_lines", "File Page Limit", ""),
    ("tool_output.max_line_length", "Line Length Limit", ""),
    ("checkpoints.max_snapshots", "Checkpoint Limit", ""),
    (
        "agent.max_turns",
        "Max Agent Steps",
        "Upper bound for tool-calling turns before Hermes stops a run.",
    ),
    ("agent.api_max_retries", "API Retries", ""),
    ("agent.service_tier", "Service Tier", ""),
    ("agent.tool_use_enforcement", "Tool-Use Enforcement", ""),
    ("delegation.model", "Subagent Model", ""),
    ("delegation.provider", "Subagent Provider", ""),
    ("delegation.max_iterations", "Subagent Turn Limit", ""),
    (
        "delegation.max_concurrent_children",
        "Parallel Subagents",
        "",
    ),
    ("delegation.child_timeout_seconds", "Subagent Timeout", ""),
    (
        "delegation.reasoning_effort",
        "Subagent Reasoning Effort",
        "",
    ),
    (
        "updates.non_interactive_local_changes",
        "In-App Update Local Changes",
        "When Hermes updates itself from the app, keep local source edits or throw them away. Terminal updates always ask.",
    ),
];

#[component]
fn WorkspaceConfigFields(
    snapshot: Signal<Option<AgentConfigSnapshot>>,
    loaded: AgentConfigSnapshot,
    profile: Option<String>,
    saving: Signal<bool>,
    error: Signal<Option<String>>,
) -> Element {
    rsx! {
        for (path, title, description) in WORKSPACE_FIELDS {
            CuratedConfigField {
                key: "{path}",
                snapshot,
                loaded: loaded.clone(),
                profile: profile.clone(),
                saving,
                error,
                path,
                title,
                description,
            }
        }
    }
}

#[component]
fn SafetyConfigFields(
    snapshot: Signal<Option<AgentConfigSnapshot>>,
    loaded: AgentConfigSnapshot,
    profile: Option<String>,
    saving: Signal<bool>,
    error: Signal<Option<String>>,
) -> Element {
    rsx! {
        for (path, title, description) in SAFETY_FIELDS {
            CuratedConfigField {
                key: "{path}",
                snapshot,
                loaded: loaded.clone(),
                profile: profile.clone(),
                saving,
                error,
                path,
                title,
                description,
            }
        }
    }
}

#[component]
fn MemoryConfigFields(
    snapshot: Signal<Option<AgentConfigSnapshot>>,
    loaded: AgentConfigSnapshot,
    profile: Option<String>,
    saving: Signal<bool>,
    error: Signal<Option<String>>,
) -> Element {
    rsx! {
        for (path, title, description) in MEMORY_FIELDS {
            CuratedConfigField {
                key: "{path}",
                snapshot,
                loaded: loaded.clone(),
                profile: profile.clone(),
                saving,
                error,
                path,
                title,
                description,
            }
        }
    }
}

#[component]
fn VoiceConfigFields(
    snapshot: Signal<Option<AgentConfigSnapshot>>,
    loaded: AgentConfigSnapshot,
    profile: Option<String>,
    saving: Signal<bool>,
    error: Signal<Option<String>>,
) -> Element {
    rsx! {
        for (path, title, description) in VOICE_FIELDS {
            if voice_config_field_visible(path, &loaded.config) {
                CuratedConfigField {
                    key: "{path}",
                    snapshot,
                    loaded: loaded.clone(),
                    profile: profile.clone(),
                    saving,
                    error,
                    path,
                    title,
                    description,
                }
            }
        }
    }
}

#[component]
fn AdvancedConfigFields(
    snapshot: Signal<Option<AgentConfigSnapshot>>,
    loaded: AgentConfigSnapshot,
    profile: Option<String>,
    saving: Signal<bool>,
    error: Signal<Option<String>>,
) -> Element {
    rsx! {
        for (path, title, description) in ADVANCED_FIELDS {
            CuratedConfigField {
                key: "{path}",
                snapshot,
                loaded: loaded.clone(),
                profile: profile.clone(),
                saving,
                error,
                path,
                title,
                description,
            }
        }
    }
}

fn voice_config_field_visible(path: &str, config: &BTreeMap<String, Value>) -> bool {
    let mut parts = path.split('.');
    let Some(domain) = parts.next() else {
        return true;
    };
    let Some(provider) = parts.next() else {
        return true;
    };
    if parts.next().is_none() || !matches!(domain, "tts" | "stt") {
        return true;
    }
    if domain == "stt"
        && !config_value(config, "stt.enabled")
            .and_then(Value::as_bool)
            .unwrap_or_default()
    {
        return false;
    }
    config_value(config, &format!("{domain}.provider")).and_then(Value::as_str) == Some(provider)
}

#[component]
fn CuratedConfigField(
    snapshot: Signal<Option<AgentConfigSnapshot>>,
    loaded: AgentConfigSnapshot,
    profile: Option<String>,
    saving: Signal<bool>,
    error: Signal<Option<String>>,
    path: &'static str,
    title: &'static str,
    description: &'static str,
) -> Element {
    let service = use_context::<AppServices>().agent_config.clone();
    let schema = loaded.schema.fields.get(path).cloned();
    let value = config_value(&loaded.config, path)
        .cloned()
        .unwrap_or(Value::Null);
    if schema.is_none() && value.is_null() {
        return rsx! {};
    }
    let field_type = schema
        .as_ref()
        .and_then(|field| field.field_type.clone())
        .unwrap_or_else(|| {
            match value {
                Value::Bool(_) => "boolean",
                Value::Number(_) => "number",
                Value::Array(_) => "list",
                _ => "string",
            }
            .to_owned()
        });
    let current_text = config_display_value(&value);
    let options = curated_config_options(
        path,
        &current_text,
        schema
            .as_ref()
            .map(|field| field.options.as_slice())
            .unwrap_or_default(),
    );
    let description = if description.is_empty() {
        schema
            .as_ref()
            .and_then(|field| field.description.clone())
            .unwrap_or_default()
    } else {
        description.to_owned()
    };
    let free_input = voice_free_input_field(path);
    let suggestions_id = format!("config-suggestions-{}", path.replace('.', "-"));
    let mut draft = use_signal(|| current_text.clone());

    rsx! {
        section { class: "settings-list-row",
            div { class: "settings-row-copy",
                strong { "{title}" }
                if !description.is_empty() { p { "{description}" } }
            }
            div { class: "settings-row-action",
                if field_type == "boolean" {
                    label { class: "settings-switch",
                        input {
                            r#type: "checkbox",
                            checked: value.as_bool().unwrap_or_default(),
                            disabled: saving(),
                            onchange: {
                                let config = loaded.config.clone();
                                let profile = profile.clone();
                                let service = service.clone();
                                move |event| commit_agent_config(snapshot, saving, error, service.clone(), profile.clone(), set_config_value(&config, path, json!(event.checked())))
                            }
                        }
                        span {}
                    }
                } else if free_input {
                    div { class: "settings-suggested-input",
                        input {
                            class: "settings-input",
                            list: "{suggestions_id}",
                            disabled: saving(),
                            value: "{draft}",
                            placeholder: "Not set",
                            oninput: move |event| draft.set(event.value()),
                            onblur: {
                                let config = loaded.config.clone();
                                let profile = profile.clone();
                                let service = service.clone();
                                move |_| {
                                    let next = json!(draft());
                                    if config_value(&config, path) != Some(&next) {
                                        commit_agent_config(snapshot, saving, error, service.clone(), profile.clone(), set_config_value(&config, path, next));
                                    }
                                }
                            }
                        }
                        datalist { id: "{suggestions_id}",
                            for option in &options {
                                option { value: "{config_display_value(option)}" }
                            }
                        }
                    }
                } else if !options.is_empty() {
                    select {
                        class: "settings-select",
                        disabled: saving(),
                        value: "{current_text}",
                        onchange: {
                            let config = loaded.config.clone();
                            let profile = profile.clone();
                            let service = service.clone();
                            let options = options.clone();
                            move |event| {
                                let selected = event.value();
                                let next = options
                                    .iter()
                                    .find(|option| config_display_value(option) == selected)
                                    .cloned()
                                    .unwrap_or_else(|| json!(selected));
                                commit_agent_config(snapshot, saving, error, service.clone(), profile.clone(), set_config_value(&config, path, next));
                            }
                        },
                        for option in &options {
                            {
                                let label = config_display_value(option);
                                rsx! { option { value: "{label}", selected: label == current_text, "{label}" } }
                            }
                        }
                    }
                } else {
                    input {
                        class: if field_type == "number" { "settings-input number" } else { "settings-input" },
                        r#type: if field_type == "number" { "number" } else { "text" },
                        disabled: saving(),
                        value: "{draft}",
                        placeholder: if field_type == "list" { "Comma separated" } else { "Not set" },
                        oninput: move |event| draft.set(event.value()),
                        onblur: {
                            let config = loaded.config.clone();
                            let profile = profile.clone();
                            let service = service.clone();
                            let field_type = field_type.clone();
                            move |_| {
                                let text = draft();
                                let next = if field_type == "number" {
                                    match text.parse::<i64>() {
                                        Ok(value) => json!(value),
                                        Err(_) => return,
                                    }
                                } else if field_type == "list" {
                                    Value::Array(text.split(',').map(str::trim).filter(|part| !part.is_empty()).map(|part| json!(part)).collect())
                                } else {
                                    json!(text)
                                };
                                if config_value(&config, path) != Some(&next) {
                                    commit_agent_config(snapshot, saving, error, service.clone(), profile.clone(), set_config_value(&config, path, next));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn config_display_value(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(value) => value.clone(),
        Value::Array(values) => values
            .iter()
            .map(config_display_value)
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
            .join(", "),
        _ => value.to_string(),
    }
}

fn curated_config_options(path: &str, current: &str, schema_options: &[Value]) -> Vec<Value> {
    let mut options = match path {
        "approvals.mode" => vec![json!("manual"), json!("smart"), json!("off")],
        "code_execution.mode" => vec![json!("project"), json!("strict")],
        "context.engine" => vec![json!("compressor"), json!("default"), json!("custom")],
        "tts.provider" => vec![
            json!("edge"),
            json!("elevenlabs"),
            json!("openai"),
            json!("xai"),
            json!("minimax"),
            json!("mistral"),
            json!("gemini"),
            json!("neutts"),
            json!("kittentts"),
            json!("piper"),
        ],
        "stt.provider" => vec![
            json!("local"),
            json!("groq"),
            json!("openai"),
            json!("mistral"),
            json!("xai"),
            json!("elevenlabs"),
        ],
        "stt.local.model" => vec![
            json!("tiny"),
            json!("base"),
            json!("small"),
            json!("medium"),
            json!("large-v3"),
        ],
        "tts.openai.model" => vec![json!("gpt-4o-mini-tts"), json!("tts-1"), json!("tts-1-hd")],
        "tts.openai.voice" => vec![
            json!("alloy"),
            json!("ash"),
            json!("ballad"),
            json!("cedar"),
            json!("coral"),
            json!("echo"),
            json!("fable"),
            json!("marin"),
            json!("nova"),
            json!("onyx"),
            json!("sage"),
            json!("shimmer"),
            json!("verse"),
        ],
        "tts.neutts.device" => vec![json!("cpu"), json!("cuda"), json!("mps")],
        "terminal.backend" => vec![
            json!("local"),
            json!("docker"),
            json!("singularity"),
            json!("modal"),
            json!("daytona"),
            json!("ssh"),
        ],
        "delegation.reasoning_effort" => vec![
            json!(""),
            json!("none"),
            json!("minimal"),
            json!("low"),
            json!("medium"),
            json!("high"),
            json!("xhigh"),
            json!("max"),
            json!("ultra"),
        ],
        "updates.non_interactive_local_changes" => vec![json!("stash"), json!("discard")],
        _ => schema_options.to_vec(),
    };
    if !options.is_empty()
        && !current.is_empty()
        && !options
            .iter()
            .any(|option| config_display_value(option) == current)
    {
        options.push(json!(current));
    }
    options
}

fn voice_free_input_field(path: &str) -> bool {
    matches!(
        path,
        "tts.edge.voice"
            | "tts.openai.model"
            | "tts.openai.voice"
            | "tts.elevenlabs.voice_id"
            | "tts.gemini.model"
            | "tts.gemini.voice"
            | "tts.xai.voice_id"
            | "tts.minimax.model"
            | "tts.minimax.voice_id"
            | "tts.mistral.model"
            | "tts.mistral.voice_id"
            | "tts.neutts.model"
            | "tts.kittentts.model"
            | "tts.kittentts.voice"
            | "tts.piper.voice"
            | "tts.deepinfra.model"
            | "tts.deepinfra.voice"
    )
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

fn provider_title(provider: &OAuthProvider) -> String {
    match provider.id.as_str() {
        "nous" => "Nous Portal".into(),
        "openai-codex" => "OpenAI OAuth (ChatGPT)".into(),
        "minimax-oauth" => "MiniMax".into(),
        "qwen-oauth" => "Qwen Code".into(),
        "xai-oauth" => "xAI Grok".into(),
        "anthropic" => "Anthropic API Key".into(),
        "claude-code" => "Anthropic OAuth: Required Extra Usage Credits to Use Subscription".into(),
        _ => provider.name.clone(),
    }
}

fn provider_order(provider: &OAuthProvider) -> u8 {
    match provider.id.as_str() {
        "nous" => 0,
        "openai-codex" => 1,
        "minimax-oauth" => 2,
        "qwen-oauth" => 3,
        "xai-oauth" => 4,
        "anthropic" => 5,
        "claude-code" => 6,
        _ => 99,
    }
}

fn provider_flow_subtitle(flow: &str) -> &'static str {
    match flow {
        "pkce" => "Opens your browser to sign in, then continues here",
        "device_code" => {
            "Opens a verification page in your browser — Hermes connects automatically"
        }
        "external" => "Sign in once in your terminal, then come back to chat",
        _ => "Connect this provider to Hermes",
    }
}

const PROVIDER_PREFIXES: &[(&str, &str, u8)] = &[
    ("NOUS_", "Nous Portal", 0),
    ("FIREWORKS_", "Fireworks AI", 1),
    ("OPENROUTER_", "OpenRouter", 1),
    ("ANTHROPIC_", "Anthropic", 2),
    ("XAI_", "xAI", 3),
    ("GOOGLE_", "Gemini", 4),
    ("GEMINI_", "Gemini", 4),
    ("DEEPSEEK_", "DeepSeek", 5),
    ("DASHSCOPE_", "DashScope (Qwen)", 6),
    ("HERMES_QWEN_", "DashScope (Qwen)", 6),
    ("GLM_", "GLM / Z.AI", 7),
    ("ZAI_", "GLM / Z.AI", 7),
    ("Z_AI_", "GLM / Z.AI", 7),
    ("KIMI_", "Kimi / Moonshot", 8),
    ("KIMI_CN_", "Kimi (China)", 9),
    ("MINIMAX_", "MiniMax", 10),
    ("MINIMAX_CN_", "MiniMax (China)", 11),
    ("HF_", "Hugging Face", 12),
    ("OPENCODE_ZEN_", "OpenCode Zen", 13),
    ("OPENCODE_GO_", "OpenCode Go", 14),
    ("NVIDIA_", "NVIDIA NIM", 15),
    ("OLLAMA_", "Ollama Cloud", 16),
    ("LM_", "LM Studio", 17),
    ("STEPFUN_", "StepFun", 18),
    ("XIAOMI_", "Xiaomi MiMo", 19),
    ("ARCEEAI_", "Arcee AI", 20),
    ("ARCEE_", "Arcee AI", 20),
    ("GMI_", "GMI Cloud", 21),
    ("AZURE_FOUNDRY_", "Azure Foundry", 22),
    ("AWS_", "AWS Bedrock", 23),
];

#[derive(Clone, Debug, PartialEq)]
struct ProviderKeyGroup {
    advanced: Vec<(String, EnvVarInfo)>,
    description: String,
    docs_url: Option<String>,
    has_any_set: bool,
    name: String,
    primary: (String, EnvVarInfo),
    priority: u8,
}

fn provider_group_for_key(key: &str) -> Option<(&'static str, u8)> {
    PROVIDER_PREFIXES
        .iter()
        .filter(|(prefix, _, _)| key.starts_with(prefix))
        .max_by_key(|(prefix, _, _)| prefix.len())
        .map(|(_, name, priority)| (*name, *priority))
}

fn provider_priority(name: &str) -> u8 {
    PROVIDER_PREFIXES
        .iter()
        .find_map(|(_, candidate, priority)| (*candidate == name).then_some(*priority))
        .unwrap_or(99)
}

fn is_provider_key(key: &str, info: &EnvVarInfo) -> bool {
    info.is_password
        || key.ends_with("_API_KEY")
        || key.ends_with("_TOKEN")
        || key.ends_with("_KEY")
}

fn build_provider_key_groups(vars: &BTreeMap<String, EnvVarInfo>) -> Vec<ProviderKeyGroup> {
    let mut buckets = BTreeMap::<String, Vec<(String, EnvVarInfo)>>::new();
    for (key, info) in vars {
        if info.category != "provider" {
            continue;
        }
        let fallback = provider_group_for_key(key);
        let name = info
            .provider_label
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                info.provider
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
            })
            .map(str::to_owned)
            .or_else(|| fallback.map(|(name, _)| name.to_owned()));
        if let Some(name) = name {
            buckets
                .entry(name)
                .or_default()
                .push((key.clone(), info.clone()));
        }
    }
    let mut groups = Vec::new();
    for (name, entries) in buckets {
        let primary = entries
            .iter()
            .find(|(key, info)| !info.advanced && is_provider_key(key, info))
            .or_else(|| {
                entries
                    .iter()
                    .find(|(key, info)| is_provider_key(key, info))
            })
            .cloned();
        let Some(primary) = primary else { continue };
        let mut advanced = entries
            .iter()
            .filter(|(key, info)| key != &primary.0 && (!is_provider_key(key, info) || info.is_set))
            .cloned()
            .collect::<Vec<_>>();
        advanced.sort_by(|left, right| left.0.cmp(&right.0));
        groups.push(ProviderKeyGroup {
            description: primary.1.description.clone(),
            docs_url: primary.1.url.clone(),
            has_any_set: entries.iter().any(|(_, info)| info.is_set),
            priority: provider_priority(&name),
            name,
            primary,
            advanced,
        });
    }
    groups.sort_by(|left, right| {
        left.priority
            .cmp(&right.priority)
            .then_with(|| left.name.cmp(&right.name))
    });
    groups
}

fn credential_field_label(key: &str) -> String {
    let trimmed = key
        .strip_suffix("_API_KEY")
        .or_else(|| key.strip_suffix("_TOKEN"))
        .or_else(|| key.strip_suffix("_KEY"))
        .unwrap_or(key);
    trimmed
        .split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Clone, Debug, PartialEq)]
enum ProviderAuthFlow {
    Starting(OAuthProvider),
    External(OAuthProvider),
    Pkce {
        code: String,
        provider: OAuthProvider,
        start: OAuthStart,
        submitting: bool,
    },
    Device {
        provider: OAuthProvider,
        start: OAuthStart,
    },
    Success(OAuthProvider),
    Error {
        message: String,
        provider: OAuthProvider,
        session_id: Option<String>,
    },
}

impl ProviderAuthFlow {
    fn provider(&self) -> &OAuthProvider {
        match self {
            Self::Starting(provider)
            | Self::External(provider)
            | Self::Success(provider)
            | Self::Error { provider, .. }
            | Self::Pkce { provider, .. }
            | Self::Device { provider, .. } => provider,
        }
    }

    fn session_id(&self) -> Option<&str> {
        match self {
            Self::Pkce { start, .. } | Self::Device { start, .. } => Some(start.session_id()),
            Self::Error {
                session_id: Some(session_id),
                ..
            } => Some(session_id),
            _ => None,
        }
    }
}

async fn poll_provider_oauth(
    service: Arc<dyn hermes_core::ProviderService>,
    profile: Option<String>,
    provider: OAuthProvider,
    start: OAuthStart,
    mut flow: Signal<Option<ProviderAuthFlow>>,
    mut refresh: Signal<u64>,
) {
    let OAuthStart::DeviceCode {
        expires_in,
        poll_interval,
        session_id,
        ..
    } = &start
    else {
        return;
    };
    let session_id = session_id.clone();
    let poll_seconds = (*poll_interval).clamp(1, 30);
    let attempts = (*expires_in / poll_seconds).clamp(1, 600);
    for _ in 0..attempts {
        tokio::time::sleep(std::time::Duration::from_secs(poll_seconds)).await;
        let still_active = flow().is_some_and(|current| {
            matches!(current, ProviderAuthFlow::Device { start, .. } if start.session_id() == session_id)
        });
        if !still_active {
            return;
        }
        match service
            .poll_oauth(profile.as_deref(), &provider.id, &session_id)
            .await
        {
            Ok(result) if result.status == "approved" => {
                flow.set(Some(ProviderAuthFlow::Success(provider)));
                refresh += 1;
                return;
            }
            Ok(result) if result.status == "pending" => {}
            Ok(result) => {
                flow.set(Some(ProviderAuthFlow::Error {
                    message: result
                        .error_message
                        .unwrap_or_else(|| format!("Sign-in {}.", result.status)),
                    provider,
                    session_id: Some(session_id),
                }));
                return;
            }
            Err(problem) => {
                flow.set(Some(ProviderAuthFlow::Error {
                    message: format!("Polling failed: {problem}"),
                    provider,
                    session_id: Some(session_id),
                }));
                return;
            }
        }
    }
    flow.set(Some(ProviderAuthFlow::Error {
        message: "Sign-in expired. Try again.".into(),
        provider,
        session_id: Some(session_id),
    }));
}

fn redacted_credential(value: &str) -> String {
    let chars = value.chars().collect::<Vec<_>>();
    if chars.len() <= 8 {
        "••••".into()
    } else {
        format!(
            "{}...{}",
            chars[..4].iter().collect::<String>(),
            chars[chars.len() - 4..].iter().collect::<String>()
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
struct CustomEndpointForm {
    api_key: String,
    base_url: String,
    context_length: String,
    discover_models: bool,
    id: String,
    make_default: bool,
    model: String,
    name: String,
}

impl Default for CustomEndpointForm {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: String::new(),
            context_length: String::new(),
            discover_models: true,
            id: String::new(),
            make_default: true,
            model: String::new(),
            name: String::new(),
        }
    }
}

fn custom_endpoint_form(endpoint: &CustomEndpoint) -> CustomEndpointForm {
    CustomEndpointForm {
        api_key: String::new(),
        base_url: endpoint.base_url.clone(),
        context_length: endpoint
            .context_length
            .map_or_else(String::new, |value| value.to_string()),
        discover_models: endpoint.discover_models,
        id: endpoint.id.clone(),
        make_default: endpoint.is_current,
        model: endpoint.model.clone(),
        name: endpoint.name.clone(),
    }
}

fn custom_endpoint_payload(form: &CustomEndpointForm, models: &[String]) -> CustomEndpointUpdate {
    CustomEndpointUpdate {
        api_key: (!form.api_key.trim().is_empty()).then(|| form.api_key.trim().to_owned()),
        base_url: form.base_url.trim().to_owned(),
        context_length: form
            .context_length
            .trim()
            .parse::<u64>()
            .ok()
            .filter(|value| *value > 0),
        discover_models: form.discover_models,
        id: (!form.id.trim().is_empty()).then(|| form.id.trim().to_owned()),
        make_default: form.make_default,
        model: form.model.trim().to_owned(),
        models: models.to_vec(),
        name: form.name.trim().to_owned(),
    }
}

#[component]
fn CustomEndpointsPanel() -> Element {
    let services = use_context::<AppServices>();
    let load_service = services.providers.clone();
    let mut endpoints = use_signal(Vec::<CustomEndpoint>::new);
    let mut form = use_signal(CustomEndpointForm::default);
    let mut discovered_models = use_signal(Vec::<String>::new);
    let mut loading = use_signal(|| true);
    let mut busy = use_signal(|| None::<String>);
    let mut error = use_signal(|| None::<String>);
    let mut validation = use_signal(|| None::<(String, bool)>);
    let mut delete_target = use_signal(|| None::<CustomEndpoint>);
    let _load = use_resource(move || {
        let service = load_service.clone();
        async move {
            loading.set(true);
            match service.custom_endpoints().await {
                Ok(data) => {
                    let current = data
                        .endpoints
                        .iter()
                        .find(|endpoint| endpoint.is_current)
                        .or_else(|| data.endpoints.first())
                        .cloned();
                    endpoints.set(data.endpoints);
                    if let Some(current) = current {
                        form.set(custom_endpoint_form(&current));
                        discovered_models.set(current.models);
                    }
                    error.set(None);
                }
                Err(problem) => error.set(Some(problem.to_string())),
            }
            loading.set(false);
        }
    });
    let current_form = form();
    let can_save = !current_form.name.trim().is_empty()
        && !current_form.base_url.trim().is_empty()
        && !current_form.model.trim().is_empty();
    let mut model_options = discovered_models();
    if !current_form.model.is_empty() && !model_options.contains(&current_form.model) {
        model_options.push(current_form.model.clone());
    }

    rsx! {
        section { class: "provider-settings custom-endpoints-settings",
            if loading() {
                div { class: "provider-list provider-loading", for _ in 0..4 { div { class: "provider-skeleton" } } }
            } else {
                div { class: "custom-endpoint-heading",
                    div { class: "settings-section-title", Codicon { name: "globe" } h1 { "Custom Endpoints" } }
                    span { class: "custom-endpoint-count", "{endpoints().len()}" }
                }
                div { class: "custom-endpoint-list",
                    if endpoints().is_empty() {
                        div { class: "provider-empty", strong { "No custom endpoints" } span { "Add an OpenAI-compatible endpoint below." } }
                    }
                    for endpoint in endpoints() {
                        article { class: "custom-endpoint-row",
                            button { class: "custom-endpoint-copy", onclick: {
                                let endpoint = endpoint.clone();
                                move |_| {
                                    form.set(custom_endpoint_form(&endpoint));
                                    discovered_models.set(endpoint.models.clone());
                                    validation.set(None);
                                }
                            },
                                div { class: "custom-endpoint-title",
                                    strong { "{endpoint.name}" }
                                    if endpoint.is_current { span { class: "provider-connected", Codicon { name: "check" } "Active" } }
                                    if endpoint.source.as_deref() == Some("direct-config") { span { class: "custom-endpoint-source", "config.yaml" } }
                                }
                                code { "{endpoint.base_url}" }
                                p { "{endpoint.model}"
                                    if endpoint.has_api_key {
                                        if let Some(preview) = &endpoint.api_key_preview { span { " · {preview}" } }
                                        else { span { " · API key set" } }
                                    }
                                }
                            }
                            div { class: "custom-endpoint-actions",
                                button { class: "button", disabled: endpoint.is_current || busy().is_some(), onclick: {
                                    let service = services.providers.clone();
                                    let id = endpoint.id.clone();
                                    move |_| {
                                        busy.set(Some(format!("activate:{id}")));
                                        let service = service.clone();
                                        let id = id.clone();
                                        spawn(async move {
                                            match service.activate_custom_endpoint(&id).await {
                                                Ok(_) => match service.custom_endpoints().await {
                                                    Ok(data) => { endpoints.set(data.endpoints); error.set(None); }
                                                    Err(problem) => error.set(Some(problem.to_string())),
                                                },
                                                Err(problem) => error.set(Some(problem.to_string())),
                                            }
                                            busy.set(None);
                                        });
                                    }
                                }, Codicon { name: "zap" } if busy().as_deref() == Some(format!("activate:{}", endpoint.id).as_str()) { "Using…" } else { "Use" } }
                                if endpoint.source.as_deref() != Some("direct-config") {
                                    button { class: "provider-remove", disabled: busy().is_some(), aria_label: "Delete {endpoint.name}", title: "Delete endpoint", onclick: {
                                        let endpoint = endpoint.clone();
                                        move |_| delete_target.set(Some(endpoint.clone()))
                                    }, Codicon { name: "trash" } }
                                }
                            }
                        }
                    }
                }
                div { class: "custom-endpoint-form-heading",
                    div { class: "settings-section-title", Codicon { name: "add" } h1 { if current_form.id.is_empty() { "Add Endpoint" } else { "Edit Endpoint" } } }
                }
                div { class: "custom-endpoint-form",
                    div { class: "custom-endpoint-two-columns",
                        label { class: "dialog-field", span { "Name" } input { placeholder: "Axet Proxy", value: "{current_form.name}", oninput: move |event| form.write().name = event.value() } }
                        label { class: "dialog-field", span { "Provider ID" } input { placeholder: "axet-proxy", value: "{current_form.id}", oninput: move |event| form.write().id = event.value() } }
                    }
                    label { class: "dialog-field", span { "Endpoint URL" } input { placeholder: "http://127.0.0.1:8081/v1", value: "{current_form.base_url}", oninput: move |event| form.write().base_url = event.value() } }
                    div { class: "custom-endpoint-model-row",
                        label { class: "dialog-field", span { "Default Model" }
                            input { list: "custom-endpoint-models", placeholder: "gpt-5.4", value: "{current_form.model}", oninput: move |event| form.write().model = event.value() }
                            datalist { id: "custom-endpoint-models", for model in model_options { option { value: "{model}" } } }
                        }
                        label { class: "dialog-field", span { "Context" } input { inputmode: "numeric", placeholder: "Auto", value: "{current_form.context_length}", oninput: move |event| form.write().context_length = event.value() } }
                    }
                    label { class: "dialog-field", span { "API Key" } input { r#type: "password", placeholder: if current_form.id.is_empty() { "Optional" } else { "Leave blank to keep current key" }, value: "{current_form.api_key}", oninput: move |event| form.write().api_key = event.value() } }
                    div { class: "custom-endpoint-checks",
                        label { input { r#type: "checkbox", checked: current_form.make_default, onchange: move |event| form.write().make_default = event.checked() } "Use for new chats" }
                        label { input { r#type: "checkbox", checked: current_form.discover_models, onchange: move |event| form.write().discover_models = event.checked() } "Discover models" }
                    }
                    if let Some((message, failed)) = validation() {
                        p { class: if failed { "custom-endpoint-validation error" } else { "custom-endpoint-validation" }, role: "status", "{message}" }
                    }
                    if let Some(problem) = error() { p { class: "inline-error", role: "alert", "{problem}" } }
                    div { class: "custom-endpoint-form-actions",
                        button { class: "button", disabled: busy().is_some() || current_form.base_url.trim().is_empty(), onclick: {
                            let service = services.providers.clone();
                            let payload = custom_endpoint_payload(&current_form, &[]);
                            move |_| {
                                busy.set(Some("test".into())); validation.set(None);
                                let service = service.clone(); let payload = payload.clone();
                                spawn(async move {
                                    match service.validate_custom_endpoint(&payload).await {
                                        Ok(result) => {
                                            discovered_models.set(result.models.clone());
                                            if form().model.is_empty() && let Some(model) = result.models.first() { form.write().model.clone_from(model); }
                                            let message = if result.ok && !result.models.is_empty() { format!("Endpoint is reachable. Found {} models.", result.models.len()) } else if result.ok { "Endpoint is reachable.".into() } else if result.message.is_empty() { "Endpoint validation failed.".into() } else { result.message };
                                            validation.set(Some((message, !result.ok)));
                                        }
                                        Err(problem) => validation.set(Some((problem.to_string(), true))),
                                    }
                                    busy.set(None);
                                });
                            }
                        }, Codicon { name: "zap" } if busy().as_deref() == Some("test") { "Testing…" } else { "Test" } }
                        button { class: "button primary", disabled: busy().is_some() || !can_save, onclick: {
                            let service = services.providers.clone();
                            let payload = custom_endpoint_payload(&current_form, &discovered_models());
                            move |_| {
                                busy.set(Some("save".into()));
                                let service = service.clone(); let payload = payload.clone();
                                spawn(async move {
                                    match service.save_custom_endpoint(&payload).await {
                                        Ok(data) => {
                                            let saved = data.id.as_deref().and_then(|id| data.endpoints.iter().find(|endpoint| endpoint.id == id)).cloned();
                                            endpoints.set(data.endpoints);
                                            if let Some(saved) = saved { form.set(custom_endpoint_form(&saved)); discovered_models.set(saved.models); }
                                            error.set(None);
                                        }
                                        Err(problem) => error.set(Some(problem.to_string())),
                                    }
                                    busy.set(None);
                                });
                            }
                        }, Codicon { name: "save" } if busy().as_deref() == Some("save") { "Saving…" } else { "Save" } }
                        if !current_form.id.is_empty() {
                            button { class: "button", disabled: busy().is_some(), onclick: move |_| { form.set(CustomEndpointForm::default()); discovered_models.set(Vec::new()); validation.set(None); }, "New endpoint" }
                        }
                    }
                }
            }
            if let Some(endpoint) = delete_target() {
                div { class: "dialog-backdrop", role: "presentation",
                    section { class: "hermes-dialog compact", role: "alertdialog", aria_modal: "true", aria_label: "Delete custom endpoint",
                        header { h2 { "Delete {endpoint.name}?" } p { "This removes the saved custom endpoint. It does not delete the server or its models." } }
                        footer {
                            button { class: "button", disabled: busy().is_some(), onclick: move |_| delete_target.set(None), "Cancel" }
                            button { class: "button danger", disabled: busy().is_some(), onclick: {
                                let service = services.providers.clone(); let id = endpoint.id.clone();
                                move |_| { busy.set(Some(format!("delete:{id}"))); let service = service.clone(); let id = id.clone(); spawn(async move {
                                    match service.delete_custom_endpoint(&id).await {
                                        Ok(data) => { endpoints.set(data.endpoints); if form().id == id { form.set(CustomEndpointForm::default()); discovered_models.set(Vec::new()); } delete_target.set(None); error.set(None); }
                                        Err(problem) => error.set(Some(problem.to_string())),
                                    }
                                    busy.set(None);
                                }); }
                            }, if busy().is_some() { "Deleting…" } else { "Delete" } }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ProviderKeysPanel(on_custom_endpoint: Callback<()>) -> Element {
    let services = use_context::<AppServices>();
    let settings = use_context::<SettingsUiState>();
    let profile = (settings.settings)().profile;
    let load_service = services.providers.clone();
    let mut vars = use_signal(|| None::<BTreeMap<String, EnvVarInfo>>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut query = use_signal(String::new);
    let mut expanded = use_signal(|| None::<String>);
    let mut drafts = use_signal(BTreeMap::<String, String>::new);
    let mut busy = use_signal(|| None::<String>);
    let mut remove_target = use_signal(|| None::<String>);
    let load_profile = profile.clone();
    let _load = use_resource(move || {
        let service = load_service.clone();
        let profile = load_profile.clone();
        async move {
            loading.set(true);
            match service.env(profile.as_deref()).await {
                Ok(next) => {
                    vars.set(Some(next));
                    error.set(None);
                }
                Err(problem) => error.set(Some(problem.to_string())),
            }
            loading.set(false);
        }
    });
    let groups = vars()
        .as_ref()
        .map(build_provider_key_groups)
        .unwrap_or_default();
    let needle = query().trim().to_lowercase();
    let visible = groups
        .iter()
        .filter(|group| {
            needle.is_empty()
                || group.name.to_lowercase().contains(&needle)
                || group.description.to_lowercase().contains(&needle)
                || group.primary.0.to_lowercase().contains(&needle)
                || group
                    .advanced
                    .iter()
                    .any(|(key, _)| key.to_lowercase().contains(&needle))
        })
        .cloned()
        .collect::<Vec<_>>();

    rsx! {
        section { class: "provider-settings provider-keys-settings",
            button { class: "provider-row provider-local-endpoint", onclick: move |_| on_custom_endpoint.call(()),
                div { strong { "Local / custom endpoint" } p { "Point Hermes at any OpenAI-compatible endpoint (Zyphra, vLLM, llama.cpp, Ollama, etc)." } }
                Codicon { name: "chevron-right" }
            }
            if loading() {
                div { class: "provider-list provider-loading", for _ in 0..6 { div { class: "provider-skeleton" } } }
            } else if let Some(problem) = error() {
                div { class: "settings-load-error compact", role: "alert", Codicon { name: "warning" } strong { "Could not load provider keys" } p { "{problem}" } }
            } else if groups.is_empty() {
                div { class: "provider-empty", "No provider API keys available." }
            } else {
                label { class: "provider-search",
                    Codicon { name: "search" }
                    input { aria_label: "Search providers", placeholder: "Search providers…", value: "{query}", oninput: move |event| query.set(event.value()) }
                }
                if visible.is_empty() {
                    div { class: "provider-empty", "No providers match your search." }
                } else {
                    div { class: "provider-key-list",
                        for group in visible {
                            article { class: if expanded().as_deref() == Some(group.name.as_str()) { "provider-key-card expanded" } else { "provider-key-card" },
                                button { class: "provider-key-card-head", onclick: {
                                    let name = group.name.clone();
                                    move |_| expanded.set((expanded().as_deref() != Some(name.as_str())).then_some(name.clone()))
                                },
                                    span { class: if group.has_any_set { "provider-key-dot set" } else { "provider-key-dot" } }
                                    div { strong { "{group.name}" } p { "{group.description}" } }
                                    Codicon { name: if expanded().as_deref() == Some(group.name.as_str()) { "chevron-up" } else { "chevron-down" } }
                                }
                                div { class: "provider-key-primary",
                                    ProviderCredentialField {
                                        var_key: group.primary.0.clone(),
                                        info: group.primary.1.clone(),
                                        drafts,
                                        busy,
                                        vars,
                                        remove_target,
                                        error,
                                        profile: profile.clone(),
                                    }
                                }
                                if expanded().as_deref() == Some(group.name.as_str()) {
                                    div { class: "provider-key-advanced",
                                        if !group.advanced.is_empty() { p { class: "provider-group-label", "Configuration" } }
                                        for (key, info) in group.advanced.clone() {
                                            label { class: "provider-advanced-row",
                                                span { "{credential_field_label(&key)}" }
                                                ProviderCredentialField { var_key: key, info, drafts, busy, vars, remove_target, error, profile: profile.clone() }
                                            }
                                        }
                                        if let Some(url) = group.docs_url.clone() {
                                            button { class: "provider-reopen", onclick: {
                                                let platform = services.platform.clone();
                                                move |_| { let platform = platform.clone(); let url = url.clone(); spawn(async move { let _ = platform.open_external(&url).await; }); }
                                            }, Codicon { name: "link-external" } "Get a key" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if let Some(key) = remove_target() {
                div { class: "dialog-backdrop", role: "presentation",
                    section { class: "hermes-dialog compact", role: "alertdialog", aria_modal: "true", aria_label: "Remove provider credential",
                        header { h2 { "Remove {key} from .env?" } p { "Hermes will remove this credential from the active profile." } }
                        footer {
                            button { class: "button", disabled: busy().is_some(), onclick: move |_| remove_target.set(None), "Cancel" }
                            button { class: "button danger", disabled: busy().is_some(), onclick: {
                                let service = services.providers.clone();
                                let profile = profile.clone();
                                let key = key.clone();
                                move |_| {
                                    busy.set(Some(key.clone()));
                                    let service = service.clone();
                                    let profile = profile.clone();
                                    let key = key.clone();
                                    spawn(async move {
                                        match service.delete_env(profile.as_deref(), &key).await {
                                            Ok(()) => {
                                                if let Some(info) = vars.write().as_mut().and_then(|rows| rows.get_mut(&key)) {
                                                    info.is_set = false;
                                                    info.redacted_value = None;
                                                }
                                                drafts.write().remove(&key);
                                                remove_target.set(None);
                                                error.set(None);
                                            }
                                            Err(problem) => error.set(Some(problem.to_string())),
                                        }
                                        busy.set(None);
                                    });
                                }
                            }, if busy().is_some() { "Removing…" } else { "Remove" } }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ProviderCredentialField(
    var_key: String,
    info: EnvVarInfo,
    mut drafts: Signal<BTreeMap<String, String>>,
    mut busy: Signal<Option<String>>,
    mut vars: Signal<Option<BTreeMap<String, EnvVarInfo>>>,
    mut remove_target: Signal<Option<String>>,
    mut error: Signal<Option<String>>,
    profile: Option<String>,
) -> Element {
    let services = use_context::<AppServices>();
    let editing = drafts().contains_key(&var_key);
    let draft = drafts().get(&var_key).cloned().unwrap_or_default();
    let shown = if editing {
        draft.clone()
    } else if info.is_set {
        info.redacted_value
            .clone()
            .unwrap_or_else(|| "••••••••".into())
    } else {
        String::new()
    };
    let is_busy = busy().as_deref() == Some(var_key.as_str());
    rsx! {
        div { class: "provider-credential-field",
            input {
                class: "settings-input",
                r#type: if info.is_password && editing { "password" } else { "text" },
                readonly: info.is_set && !editing,
                disabled: is_busy,
                placeholder: if info.is_set { "Replace current value" } else if info.is_password { "Paste API key" } else { "Optional" },
                value: "{shown}",
                onfocus: {
                    let key = var_key.clone();
                    move |_| { if !drafts().contains_key(&key) { drafts.write().insert(key.clone(), String::new()); } }
                },
                oninput: {
                    let key = var_key.clone();
                    move |event| { drafts.write().insert(key.clone(), event.value()); }
                },
                onkeydown: {
                    let key = var_key.clone();
                    move |event| if event.key() == Key::Escape { drafts.write().remove(&key); }
                }
            }
            if editing && info.is_set {
                button { class: "provider-remove", disabled: is_busy, aria_label: "Remove {var_key}", title: "Remove", onclick: {
                    let key = var_key.clone();
                    move |_| remove_target.set(Some(key.clone()))
                }, Codicon { name: "trash" } }
            }
            if editing && !draft.trim().is_empty() {
                button { class: "button primary provider-key-save", disabled: is_busy, onclick: {
                    let service = services.providers.clone();
                    let profile = profile.clone();
                    let key = var_key.clone();
                    let value = draft.trim().to_owned();
                    move |_| {
                        busy.set(Some(key.clone()));
                        let service = service.clone();
                        let profile = profile.clone();
                        let key = key.clone();
                        let value = value.clone();
                        spawn(async move {
                            match service.set_env(profile.as_deref(), &key, &value).await {
                                Ok(()) => {
                                    if let Some(info) = vars.write().as_mut().and_then(|rows| rows.get_mut(&key)) {
                                        info.is_set = true;
                                        info.redacted_value = Some(redacted_credential(&value));
                                    }
                                    drafts.write().remove(&key);
                                    error.set(None);
                                }
                                Err(problem) => error.set(Some(problem.to_string())),
                            }
                            busy.set(None);
                        });
                    }
                }, if is_busy { "Saving…" } else { "Save" } }
            }
        }
    }
}

#[component]
fn ProviderAccountsPanel(on_want_api_key: Callback<()>) -> Element {
    let services = use_context::<AppServices>();
    let settings = use_context::<SettingsUiState>();
    let profile = (settings.settings)().profile;
    let load_service = services.providers.clone();
    let mut providers = use_signal(Vec::<OAuthProvider>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut refresh = use_signal(|| 0_u64);
    let mut show_all = use_signal(|| false);
    let mut disconnect_target = use_signal(|| None::<OAuthProvider>);
    let mut disconnecting = use_signal(|| None::<String>);
    let mut auth_flow = use_signal(|| None::<ProviderAuthFlow>);
    let load_profile = profile.clone();
    let _load = use_resource(move || {
        let service = load_service.clone();
        let profile = load_profile.clone();
        let _revision = refresh();
        async move {
            loading.set(true);
            match service.list_oauth(profile.as_deref()).await {
                Ok(mut next) => {
                    next.sort_by(|left, right| {
                        provider_order(left)
                            .cmp(&provider_order(right))
                            .then_with(|| left.name.cmp(&right.name))
                    });
                    providers.set(next);
                    error.set(None);
                }
                Err(problem) => error.set(Some(problem.to_string())),
            }
            loading.set(false);
        }
    });

    let rows = providers();
    let featured = rows
        .iter()
        .find(|provider| provider.id == "nous" && !provider.status.logged_in)
        .cloned();
    let rest = rows
        .iter()
        .filter(|provider| featured.as_ref().is_none_or(|row| row.id != provider.id))
        .cloned()
        .collect::<Vec<_>>();
    let connected = rest
        .iter()
        .filter(|provider| provider.status.logged_in)
        .cloned()
        .collect::<Vec<_>>();
    let others = rest
        .iter()
        .filter(|provider| !provider.status.logged_in)
        .cloned()
        .collect::<Vec<_>>();
    let auth_service = services.providers.clone();
    let auth_platform = services.platform.clone();
    let auth_profile = profile.clone();
    let start_auth = Callback::new(move |provider: OAuthProvider| {
        if provider.flow == "external" {
            auth_flow.set(Some(ProviderAuthFlow::External(provider)));
            return;
        }
        auth_flow.set(Some(ProviderAuthFlow::Starting(provider.clone())));
        let service = auth_service.clone();
        let platform = auth_platform.clone();
        let profile = auth_profile.clone();
        spawn(async move {
            let start = match service.start_oauth(profile.as_deref(), &provider.id).await {
                Ok(start) => start,
                Err(problem) => {
                    auth_flow.set(Some(ProviderAuthFlow::Error {
                        message: format!("Could not start sign-in: {problem}"),
                        provider,
                        session_id: None,
                    }));
                    return;
                }
            };
            if let Err(problem) = platform.open_external(start.browser_url()).await {
                let _ = service
                    .cancel_oauth(profile.as_deref(), start.session_id())
                    .await;
                auth_flow.set(Some(ProviderAuthFlow::Error {
                    message: format!("Could not open the sign-in page: {problem}"),
                    provider,
                    session_id: None,
                }));
                return;
            }
            match &start {
                OAuthStart::Pkce { .. } => auth_flow.set(Some(ProviderAuthFlow::Pkce {
                    code: String::new(),
                    provider,
                    start,
                    submitting: false,
                })),
                OAuthStart::DeviceCode { .. } => {
                    auth_flow.set(Some(ProviderAuthFlow::Device {
                        provider: provider.clone(),
                        start: start.clone(),
                    }));
                    poll_provider_oauth(service, profile, provider, start, auth_flow, refresh)
                        .await;
                }
            }
        });
    });

    rsx! {
        section { class: "provider-settings",
            div { class: "provider-heading",
                div { class: "settings-section-title", Codicon { name: "key" } h1 { "Connect an account" } }
                button { class: "provider-key-link", onclick: move |_| on_want_api_key.call(()), "Have an API key instead?" }
            }
            p { class: "settings-intro", "Sign in with a subscription — no API key to copy. Hermes runs the browser sign-in for you, right here in the app." }
            if loading() {
                div { class: "provider-list provider-loading",
                    for _ in 0..5 { div { class: "provider-skeleton" } }
                }
            } else if let Some(problem) = error() {
                div { class: "settings-load-error compact", role: "alert",
                    Codicon { name: "warning" }
                    strong { "Could not load providers" }
                    p { "{problem}" }
                    button { class: "button", onclick: move |_| refresh += 1, "Retry" }
                }
            } else if rows.is_empty() {
                div { class: "provider-empty", "No account providers are available from the connected Agent." }
            } else {
                div { class: "provider-list",
                    if let Some(provider) = featured {
                        ProviderAccountRow { provider, featured: true, disconnecting: false, on_disconnect: move |_| {}, on_select: start_auth }
                    }
                    ProviderKeyShortcut { title: "Fireworks AI", description: "Fast open models through a Fireworks API key", on_select: move |()| on_want_api_key.call(()) }
                    if !connected.is_empty() {
                        p { class: "provider-group-label", "Connected" }
                        for provider in connected.clone() {
                            ProviderAccountRow {
                                disconnecting: disconnecting().as_deref() == Some(provider.id.as_str()),
                                provider,
                                featured: false,
                                on_disconnect: move |provider| disconnect_target.set(Some(provider)),
                                on_select: start_auth,
                            }
                        }
                    }
                    if show_all() || others.is_empty() {
                        if !connected.is_empty() && !others.is_empty() {
                            p { class: "provider-group-label", "Other providers" }
                        }
                        for provider in others.clone() {
                            ProviderAccountRow { provider, featured: false, disconnecting: false, on_disconnect: move |_| {}, on_select: start_auth }
                        }
                        ProviderKeyShortcut { title: "OpenRouter", description: "One API key for models from many providers", on_select: move |()| on_want_api_key.call(()) }
                    }
                    if !others.is_empty() {
                        button { class: "provider-disclosure", onclick: move |_| show_all.toggle(),
                            if show_all() { "Collapse" } else if connected.is_empty() { "Other providers" } else { "Connect another provider" }
                            Codicon { name: if show_all() { "chevron-up" } else { "chevron-down" } }
                        }
                    }
                }
            }
            if let Some(provider) = disconnect_target() {
                div { class: "dialog-backdrop", role: "presentation",
                    section { class: "hermes-dialog compact", role: "alertdialog", aria_modal: "true", aria_label: "Remove provider account",
                        header { h2 { "Remove {provider_title(&provider)}?" } p { "Hermes will remove this account credential from the active profile." } }
                        footer {
                            button { class: "button", disabled: disconnecting().is_some(), onclick: move |_| disconnect_target.set(None), "Cancel" }
                            button { class: "button danger", disabled: disconnecting().is_some(), onclick: {
                                let service = services.providers.clone();
                                let profile = profile.clone();
                                let provider_id = provider.id.clone();
                                move |_| {
                                    disconnecting.set(Some(provider_id.clone()));
                                    let service = service.clone();
                                    let profile = profile.clone();
                                    let provider_id = provider_id.clone();
                                    spawn(async move {
                                        match service.disconnect_oauth(profile.as_deref(), &provider_id).await {
                                            Ok(()) => {
                                                disconnect_target.set(None);
                                                refresh += 1;
                                                error.set(None);
                                            }
                                            Err(problem) => error.set(Some(problem.to_string())),
                                        }
                                        disconnecting.set(None);
                                    });
                                }
                            }, if disconnecting().is_some() { "Removing…" } else { "Remove" } }
                        }
                    }
                }
            }
            if auth_flow().is_some() {
                ProviderOAuthOverlay { flow: auth_flow, profile: profile.clone(), refresh }
            }
        }
    }
}

#[component]
fn ProviderKeyShortcut(
    title: &'static str,
    description: &'static str,
    on_select: Callback<()>,
) -> Element {
    rsx! {
        button { class: "provider-row", onclick: move |_| on_select.call(()),
            div { strong { "{title}" } p { "{description}" } }
            Codicon { name: "chevron-right" }
        }
    }
}

#[component]
fn ProviderAccountRow(
    provider: OAuthProvider,
    featured: bool,
    disconnecting: bool,
    on_disconnect: Callback<OAuthProvider>,
    on_select: Callback<OAuthProvider>,
) -> Element {
    let title = provider_title(&provider);
    let can_disconnect = provider
        .disconnectable
        .unwrap_or(provider.flow != "external");
    let show_hint = provider.status.logged_in && !can_disconnect;
    let row_class = if featured {
        "provider-row featured"
    } else {
        "provider-row"
    };
    rsx! {
        div { class: "provider-row-shell",
            button { class: row_class, disabled: provider.status.logged_in, onclick: {
                    let provider = provider.clone();
                    move |_| on_select.call(provider.clone())
                },
                div {
                    div { class: "provider-title-line",
                        strong { "{title}" }
                        if provider.status.logged_in { span { class: "provider-connected", Codicon { name: "check" } "Connected" } }
                        if featured && !provider.status.logged_in { span { class: "provider-recommended", "Recommended" } }
                    }
                    p { "{provider_flow_subtitle(&provider.flow)}" }
                    if show_hint { small { "{title} is managed by its own CLI — remove it there." } }
                }
                Codicon { name: if provider.flow == "external" { "terminal" } else { "chevron-right" } }
            }
            if provider.status.logged_in && can_disconnect {
                button {
                    class: "provider-remove",
                    disabled: disconnecting,
                    aria_label: "Remove {title}",
                    title: "Remove {title}",
                    onclick: {
                        let provider = provider.clone();
                        move |_| on_disconnect.call(provider.clone())
                    },
                    Codicon { name: if disconnecting { "loading" } else { "trash" } }
                }
            }
        }
    }
}

#[component]
fn ProviderOAuthOverlay(
    mut flow: Signal<Option<ProviderAuthFlow>>,
    profile: Option<String>,
    mut refresh: Signal<u64>,
) -> Element {
    let services = use_context::<AppServices>();
    let Some(current) = flow() else {
        return rsx! {};
    };
    let title = provider_title(current.provider());
    let session_id = current.session_id().map(str::to_owned);
    let close_service = services.providers.clone();
    let close_profile = profile.clone();
    let close = Callback::new(move |()| {
        flow.set(None);
        if let Some(session_id) = session_id.clone() {
            let service = close_service.clone();
            let profile = close_profile.clone();
            spawn(async move {
                let _ = service.cancel_oauth(profile.as_deref(), &session_id).await;
            });
        }
    });

    rsx! {
        div { class: "dialog-backdrop provider-auth-backdrop", role: "presentation",
            section { class: "hermes-dialog provider-auth-dialog", role: "dialog", aria_modal: "true", aria_label: "Sign in with {title}",
                match current {
                    ProviderAuthFlow::Starting(_) => rsx! {
                        div { class: "provider-auth-state starting",
                            Codicon { name: "loading" }
                            h2 { "Starting sign-in for {title}…" }
                            p { "Preparing a secure authorization session with the connected Agent." }
                        }
                    },
                    ProviderAuthFlow::External(provider) => {
                        let check_service = services.providers.clone();
                        let check_profile = profile.clone();
                        rsx! {
                            header { h2 { "Sign in with {title}" } p { "{title} signs in through its own CLI. Run this command in a terminal, then come back and check the connection." } }
                            code { class: "provider-auth-command", "{provider.cli_command}" }
                            footer {
                                button { class: "button", onclick: move |_| close.call(()), "Cancel" }
                                button { class: "button primary", onclick: move |_| {
                                    let service = check_service.clone();
                                    let profile = check_profile.clone();
                                    let provider = provider.clone();
                                    spawn(async move {
                                        match service.list_oauth(profile.as_deref()).await {
                                            Ok(rows) if rows.iter().any(|row| row.id == provider.id && row.status.logged_in) => {
                                                flow.set(Some(ProviderAuthFlow::Success(provider)));
                                                refresh += 1;
                                            }
                                            Ok(_) => flow.set(Some(ProviderAuthFlow::Error {
                                                message: "Hermes cannot see that account yet. Complete the CLI sign-in, then try again.".into(),
                                                provider,
                                                session_id: None,
                                            })),
                                            Err(problem) => flow.set(Some(ProviderAuthFlow::Error {
                                                message: format!("Could not check sign-in: {problem}"),
                                                provider,
                                                session_id: None,
                                            })),
                                        }
                                    });
                                }, "I've signed in" }
                            }
                        }
                    },
                    ProviderAuthFlow::Pkce { code, provider, start, submitting } => {
                        let OAuthStart::Pkce { auth_url, session_id, .. } = &start else { unreachable!() };
                        let reopen_platform = services.platform.clone();
                        let reopen_url = auth_url.clone();
                        let submit_service = services.providers.clone();
                        let submit_profile = profile.clone();
                        let submit_provider = provider.clone();
                        let submit_start = start.clone();
                        let submit_session = session_id.clone();
                        let submit_code = code.clone();
                        rsx! {
                            header { h2 { "We opened {title} in your browser." } p { "Authorize Hermes there. Copy the authorization code and paste it below." } }
                            label { class: "dialog-field provider-auth-code", span { "Authorization code" }
                                input {
                                    autofocus: true,
                                    placeholder: "Paste authorization code",
                                    value: "{code}",
                                    disabled: submitting,
                                    oninput: move |event| flow.set(Some(ProviderAuthFlow::Pkce {
                                        code: event.value(),
                                        provider: provider.clone(),
                                        start: start.clone(),
                                        submitting,
                                    }))
                                }
                            }
                            button { class: "provider-reopen", onclick: move |_| {
                                let platform = reopen_platform.clone();
                                let url = reopen_url.clone();
                                spawn(async move { let _ = platform.open_external(&url).await; });
                            }, Codicon { name: "link-external" } "Re-open authorization page" }
                            footer {
                                button { class: "button", disabled: submitting, onclick: move |_| close.call(()), "Cancel" }
                                button { class: "button primary", disabled: submitting || submit_code.trim().is_empty(), onclick: move |_| {
                                    flow.set(Some(ProviderAuthFlow::Pkce {
                                        code: submit_code.clone(),
                                        provider: submit_provider.clone(),
                                        start: submit_start.clone(),
                                        submitting: true,
                                    }));
                                    let service = submit_service.clone();
                                    let profile = submit_profile.clone();
                                    let provider = submit_provider.clone();
                                    let session = submit_session.clone();
                                    let code = submit_code.clone();
                                    spawn(async move {
                                        match service.submit_oauth(profile.as_deref(), &provider.id, &session, &code).await {
                                            Ok(result) if result.ok && result.status == "approved" => {
                                                flow.set(Some(ProviderAuthFlow::Success(provider)));
                                                refresh += 1;
                                            }
                                            Ok(result) => flow.set(Some(ProviderAuthFlow::Error {
                                                message: result.message.unwrap_or_else(|| "Token exchange failed.".into()),
                                                provider,
                                                session_id: Some(session),
                                            })),
                                            Err(problem) => flow.set(Some(ProviderAuthFlow::Error {
                                                message: problem.to_string(),
                                                provider,
                                                session_id: Some(session),
                                            })),
                                        }
                                    });
                                }, if submitting { "Verifying…" } else { "Connect" } }
                            }
                        }
                    },
                    ProviderAuthFlow::Device { provider: _, start } => {
                        let OAuthStart::DeviceCode { user_code, verification_url, .. } = start else { unreachable!() };
                        let reopen_platform = services.platform.clone();
                        rsx! {
                            header { h2 { "We opened {title} in your browser." } p { "Enter this code there. Hermes will connect automatically after you authorize." } }
                            div { class: "provider-device-code", "{user_code}" }
                            button { class: "provider-reopen", onclick: move |_| {
                                let platform = reopen_platform.clone();
                                let url = verification_url.clone();
                                spawn(async move { let _ = platform.open_external(&url).await; });
                            }, Codicon { name: "link-external" } "Re-open verification page" }
                            div { class: "provider-auth-waiting", Codicon { name: "loading" } "Waiting for you to authorize…" }
                            footer { button { class: "button", onclick: move |_| close.call(()), "Cancel" } }
                        }
                    },
                    ProviderAuthFlow::Success(_) => rsx! {
                        div { class: "provider-auth-state success",
                            Codicon { name: "check" }
                            h2 { "{title} connected" }
                            p { "The account is available to the active Hermes profile." }
                            button { class: "button primary", onclick: move |_| flow.set(None), "Done" }
                        }
                    },
                    ProviderAuthFlow::Error { message, .. } => rsx! {
                        div { class: "provider-auth-state error",
                            Codicon { name: "warning" }
                            h2 { "Sign-in failed" }
                            p { "{message}" }
                            div { class: "provider-auth-error-actions",
                                button { class: "button", onclick: move |_| close.call(()), "Close" }
                                button { class: "button primary", onclick: move |_| close.call(()), "Pick a different provider" }
                            }
                        }
                    },
                }
            }
        }
    }
}

const NOTIFICATION_KINDS: &[(NativeNotificationKind, &str, &str)] = &[
    (
        NativeNotificationKind::Approval,
        "Approval needed",
        "A command is waiting for you to approve or reject it.",
    ),
    (
        NativeNotificationKind::Input,
        "Input needed",
        "Hermes asked a question or needs a password or secret.",
    ),
    (
        NativeNotificationKind::TurnDone,
        "Response ready",
        "A turn finished while Hermes was in the background.",
    ),
    (
        NativeNotificationKind::TurnError,
        "Turn failed",
        "Background turn errors.",
    ),
    (
        NativeNotificationKind::BackgroundDone,
        "Background task finished",
        "A backgrounded terminal command completed.",
    ),
    (
        NativeNotificationKind::Credits,
        "Credit alerts",
        "Credit access is paused or restored.",
    ),
];

const COMPLETION_SOUND_VARIANTS: &[(u8, &str)] = &[
    (1, "Two-note comfort"),
    (2, "Glass ping"),
    (3, "Soft marimba"),
    (4, "Tri-tone message"),
    (5, "Airy whoosh"),
    (6, "Discovery cluster"),
    (7, "Systems online"),
    (8, "IBM terminal"),
    (9, "Modem chirp"),
    (10, "Wind chimes"),
    (11, "Singing bowl"),
    (12, "Harp lift"),
    (13, "Sonar ping"),
    (14, "Music box"),
];

#[component]
fn NotificationsSettingsPanel() -> Element {
    let services = use_context::<AppServices>();
    let state = use_context::<SettingsUiState>();
    let save_service = services.settings.clone();
    let platform = services.platform.clone();
    let current = (state.settings)();
    let variant_id = completion_sound_variant_id(current.completion_sound_variant_id);
    let mut preview_nonce = use_signal(|| 0_u64);
    let mut preview_variant = use_signal(|| variant_id);
    let mut testing = use_signal(|| false);
    let mut test_result = use_signal(|| None::<(&'static str, bool)>);

    rsx! {
        section { class: "notifications-settings",
            div { class: "settings-section-title", Codicon { name: "bell" } h1 { "Notifications" } }
            p { class: "settings-intro", "OS notifications (not in-app toasts). Per device." }
            if (state.loading)() {
                p { class: "settings-intro", "Loading notification preferences…" }
            } else {
                section { class: "settings-list-row",
                    div { class: "settings-row-copy", strong { "Enable notifications" } p { "Off silences every notification below." } }
                    label { class: "settings-switch",
                        input {
                            r#type: "checkbox",
                            checked: current.notifications,
                            aria_label: "Enable notifications",
                            onchange: {
                                let service = save_service.clone();
                                let before = current.clone();
                                let mut next = current.clone();
                                move |event| {
                                    next.notifications = event.checked();
                                    save_app_settings(state, service.clone(), before.clone(), next.clone());
                                }
                            }
                        }
                        span {}
                    }
                }
                for (kind, label, description) in NOTIFICATION_KINDS {
                    section { class: "settings-list-row",
                        div { class: "settings-row-copy", strong { "{label}" } p { "{description}" } }
                        label { class: "settings-switch",
                            input {
                                r#type: "checkbox",
                                disabled: !current.notifications,
                                checked: current.notifications && current.notification_kinds.enabled(*kind),
                                aria_label: "{label}",
                                onchange: {
                                    let service = save_service.clone();
                                    let before = current.clone();
                                    let mut next = current.clone();
                                    let kind = *kind;
                                    move |event| {
                                        next.notification_kinds.set(kind, event.checked());
                                        save_app_settings(state, service.clone(), before.clone(), next.clone());
                                    }
                                }
                            }
                            span {}
                        }
                    }
                }
                section { class: "settings-list-row notification-sound-row",
                    div { class: "settings-row-copy", strong { "Completion Sound" } p { "Plays when an agent turn finishes. Pick a preset and preview it here." } }
                    div { class: "notification-sound-action",
                        select {
                            class: "settings-select",
                            aria_label: "Completion sound",
                            value: "{variant_id}",
                            onchange: {
                                let service = save_service.clone();
                                let before = current.clone();
                                let mut next = current.clone();
                                move |event| {
                                    let selected = event.value().parse::<u8>().map_or(1, completion_sound_variant_id);
                                    next.completion_sound_variant_id = selected;
                                    preview_variant.set(selected);
                                    preview_nonce += 1;
                                    save_app_settings(state, service.clone(), before.clone(), next.clone());
                                }
                            },
                            for (id, name) in COMPLETION_SOUND_VARIANTS {
                                option { value: "{id}", "{name}" }
                            }
                        }
                        button {
                            class: "button notification-preview",
                            onclick: move |_| {
                                preview_variant.set(variant_id);
                                preview_nonce += 1;
                            },
                            Codicon { name: "play" }
                            "Preview"
                        }
                    }
                }
                div { class: "notification-test",
                    button {
                        class: "button",
                        disabled: testing(),
                        onclick: {
                            let platform = platform.clone();
                            move |_| {
                                if testing() { return; }
                                testing.set(true);
                                test_result.set(None);
                                let platform = platform.clone();
                                spawn(async move {
                                    let accepted = platform.notify("Hermes", "Notifications are working.").await.unwrap_or(false);
                                    test_result.set(Some(if accepted {
                                        ("Test sent. If nothing appears, check your OS notification permissions and Focus/Do Not Disturb.", false)
                                    } else {
                                        ("This system does not support native notifications.", true)
                                    }));
                                    testing.set(false);
                                });
                            }
                        },
                        Codicon { name: "bell" }
                        if testing() { "Sending…" } else { "Send test notification" }
                    }
                    p { "Completion alerts only fire while Hermes is in the background." }
                    if let Some((message, is_error)) = test_result() {
                        p { class: if is_error { "notification-test-result error" } else { "notification-test-result" }, role: "status", "{message}" }
                    }
                }
                if preview_nonce() > 0 {
                    audio {
                        key: "{preview_nonce}",
                        autoplay: true,
                        src: "{completion_sound_data_uri(preview_variant())}",
                    }
                }
                if let Some(error) = (state.error)() {
                    p { class: "inline-error", role: "alert", "{error}" }
                }
            }
        }
    }
}

fn save_app_settings(
    state: SettingsUiState,
    service: Arc<dyn hermes_core::SettingsService>,
    before: AppSettings,
    next: AppSettings,
) {
    let mut settings_signal = state.settings;
    let mut error_signal = state.error;
    settings_signal.set(next.clone());
    error_signal.set(None);
    spawn(async move {
        if let Err(error) = service.save(&next).await {
            settings_signal.set(before);
            error_signal.set(Some(error.to_string()));
        }
    });
}

const fn completion_sound_variant_id(value: u8) -> u8 {
    if value >= 1 && value <= 14 { value } else { 1 }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
fn completion_sound_data_uri(variant: u8) -> String {
    const SAMPLE_RATE: u32 = 12_000;
    const SAMPLE_COUNT: usize = 8_640;
    let tones: Vec<(f32, f32, f32)> = match completion_sound_variant_id(variant) {
        1 => vec![(329.63, 0.0, 0.22), (261.63, 0.08, 0.50)],
        2 => vec![(783.99, 0.0, 0.42)],
        3 => vec![(261.63, 0.0, 0.22), (392.0, 0.12, 0.28)],
        4 => vec![
            (261.63, 0.0, 0.20),
            (329.63, 0.08, 0.22),
            (392.0, 0.16, 0.28),
        ],
        5 => vec![(220.0, 0.0, 0.46), (440.0, 0.12, 0.36)],
        6 => vec![
            (261.63, 0.0, 0.38),
            (329.63, 0.02, 0.42),
            (523.25, 0.05, 0.46),
        ],
        7 => vec![
            (130.81, 0.0, 0.22),
            (261.63, 0.11, 0.36),
            (523.25, 0.22, 0.38),
        ],
        8 => vec![(110.0, 0.0, 0.14), (220.0, 0.11, 0.18)],
        9 => vec![
            (987.77, 0.0, 0.16),
            (659.25, 0.13, 0.18),
            (1_318.51, 0.27, 0.18),
        ],
        10 => vec![
            (523.25, 0.0, 0.48),
            (783.99, 0.08, 0.46),
            (659.25, 0.16, 0.44),
        ],
        11 => vec![(261.63, 0.0, 0.62), (523.25, 0.0, 0.48)],
        12 => vec![
            (261.63, 0.0, 0.28),
            (329.63, 0.09, 0.30),
            (392.0, 0.18, 0.32),
            (523.25, 0.27, 0.36),
        ],
        13 => vec![(523.25, 0.0, 0.58)],
        _ => vec![
            (659.25, 0.0, 0.18),
            (523.25, 0.13, 0.20),
            (392.0, 0.26, 0.28),
        ],
    };
    let mut pcm = Vec::with_capacity(SAMPLE_COUNT * 2);
    for index in 0..SAMPLE_COUNT {
        let time = index as f32 / SAMPLE_RATE as f32;
        let mut sample = 0.0_f32;
        for (frequency, start, duration) in &tones {
            if time >= *start && time <= start + duration {
                let local = time - start;
                let envelope = (1.0 - local / duration).max(0.0).powf(2.2);
                sample += (std::f32::consts::TAU * frequency * local).sin() * envelope * 0.22;
            }
        }
        let value = (sample.clamp(-0.9, 0.9) * f32::from(i16::MAX)) as i16;
        pcm.extend_from_slice(&value.to_le_bytes());
    }
    let data_length = u32::try_from(pcm.len()).unwrap_or_default();
    let mut wav = Vec::with_capacity(44 + pcm.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_length).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    wav.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    wav.extend_from_slice(&2_u16.to_le_bytes());
    wav.extend_from_slice(&16_u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_length.to_le_bytes());
    wav.extend_from_slice(&pcm);
    format!("data:audio/wav;base64,{}", base64_encode(&wav))
}

fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or_default();
        let third = chunk.get(2).copied().unwrap_or_default();
        encoded.push(char::from(ALPHABET[usize::from(first >> 2)]));
        encoded.push(char::from(
            ALPHABET[usize::from(((first & 0x03) << 4) | (second >> 4))],
        ));
        encoded.push(if chunk.len() > 1 {
            char::from(ALPHABET[usize::from(((second & 0x0f) << 2) | (third >> 6))])
        } else {
            '='
        });
        encoded.push(if chunk.len() > 2 {
            char::from(ALPHABET[usize::from(third & 0x3f)])
        } else {
            '='
        });
    }
    encoded
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use hermes_protocol::{
        ConnectionConfig, ConnectionMode, CustomEndpoint, EnvVarInfo, MoaConfig, MoaModelSlot,
        MoaPreset, OAuthProvider, OAuthStart, RemoteAuthMode,
    };
    use serde_json::json;

    use super::{
        ProviderAuthFlow, build_provider_key_groups, completion_sound_data_uri,
        completion_sound_variant_id, config_display_value, config_value, curated_config_options,
        custom_endpoint_form, custom_endpoint_payload, gateway_input, gateway_mode_copy,
        moa_complete, provider_group_for_key, provider_order, provider_title, redacted_credential,
        set_config_value, voice_config_field_visible, voice_free_input_field,
    };

    #[test]
    fn voice_fields_follow_the_selected_provider_and_stt_state() {
        let config = serde_json::from_value(json!({
            "tts": { "provider": "openai" },
            "stt": { "enabled": true, "provider": "local" },
            "voice": { "auto_tts": false }
        }))
        .expect("voice config");
        assert!(voice_config_field_visible("tts.provider", &config));
        assert!(voice_config_field_visible("voice.auto_tts", &config));
        assert!(voice_config_field_visible("tts.openai.voice", &config));
        assert!(!voice_config_field_visible("tts.edge.voice", &config));
        assert!(voice_config_field_visible("stt.local.model", &config));
        assert!(!voice_config_field_visible("stt.groq.model", &config));

        let disabled = set_config_value(&config, "stt.enabled", json!(false));
        assert!(voice_config_field_visible("stt.enabled", &disabled));
        assert!(voice_config_field_visible("stt.provider", &disabled));
        assert!(!voice_config_field_visible("stt.local.model", &disabled));
    }

    #[test]
    fn voice_name_fields_remain_open_to_custom_values() {
        for path in [
            "tts.openai.voice",
            "tts.elevenlabs.voice_id",
            "tts.edge.voice",
            "tts.xai.voice_id",
            "tts.piper.voice",
        ] {
            assert!(voice_free_input_field(path), "{path}");
        }
        assert!(!voice_free_input_field("tts.provider"));
        assert!(!voice_free_input_field("tts.neutts.device"));
        assert!(!voice_free_input_field("stt.provider"));
    }

    #[test]
    fn safety_and_workspace_enum_overrides_match_the_source() {
        assert_eq!(
            curated_config_options("approvals.mode", "smart", &[]),
            [json!("manual"), json!("smart"), json!("off")]
        );
        assert_eq!(
            curated_config_options("code_execution.mode", "legacy", &[]),
            [json!("project"), json!("strict"), json!("legacy")]
        );
        assert!(curated_config_options("approvals.timeout", "120", &[]).is_empty());
        assert!(curated_config_options("command_allowlist", "git status", &[]).is_empty());
        assert_eq!(
            curated_config_options(
                "memory.provider",
                "hindsight",
                &[json!("builtin"), json!("honcho"), json!("hindsight")],
            ),
            [json!("builtin"), json!("honcho"), json!("hindsight")]
        );
        assert_eq!(
            curated_config_options("context.engine", "default", &[json!("stale")]),
            [json!("compressor"), json!("default"), json!("custom")]
        );
    }

    #[test]
    fn advanced_enum_overrides_match_the_source() {
        assert_eq!(
            curated_config_options("terminal.backend", "docker", &[]),
            [
                json!("local"),
                json!("docker"),
                json!("singularity"),
                json!("modal"),
                json!("daytona"),
                json!("ssh"),
            ]
        );
        assert_eq!(
            curated_config_options("delegation.reasoning_effort", "high", &[]),
            [
                json!(""),
                json!("none"),
                json!("minimal"),
                json!("low"),
                json!("medium"),
                json!("high"),
                json!("xhigh"),
                json!("max"),
                json!("ultra"),
            ]
        );
        assert_eq!(
            curated_config_options("updates.non_interactive_local_changes", "stash", &[]),
            [json!("stash"), json!("discard")]
        );
    }

    #[test]
    fn workspace_config_edits_preserve_unrelated_nested_keys() {
        let config = serde_json::from_value(json!({
            "desktop": { "repo_scan_enabled": false, "future": "keep" },
            "terminal": { "cwd": "C:\\Code", "timeout": 90 },
            "unrelated": { "enabled": true }
        }))
        .expect("config record");
        let updated = set_config_value(&config, "desktop.repo_scan_enabled", json!(true));
        assert_eq!(
            config_value(&updated, "desktop.repo_scan_enabled"),
            Some(&json!(true))
        );
        assert_eq!(
            config_value(&updated, "desktop.future"),
            Some(&json!("keep"))
        );
        assert_eq!(config_value(&updated, "terminal.timeout"), Some(&json!(90)));
        assert_eq!(
            config_value(&updated, "unrelated.enabled"),
            Some(&json!(true))
        );
    }

    #[test]
    fn workspace_list_fields_use_the_source_comma_separated_shape() {
        assert_eq!(
            config_display_value(&json!(["C:\\Code", "D:\\Projects"])),
            r"C:\Code, D:\Projects"
        );
    }

    #[test]
    fn completion_sound_preview_clamps_legacy_values_and_emits_wav_data() {
        assert_eq!(completion_sound_variant_id(0), 1);
        assert_eq!(completion_sound_variant_id(14), 14);
        assert_eq!(completion_sound_variant_id(15), 1);
        let data_uri = completion_sound_data_uri(2);
        assert!(data_uri.starts_with("data:audio/wav;base64,UklGR"));
        assert!(data_uri.len() > 10_000);
    }

    #[test]
    fn provider_account_order_and_titles_match_the_shared_og_picker() {
        let provider = |id: &str, name: &str| OAuthProvider {
            id: id.into(),
            name: name.into(),
            ..OAuthProvider::default()
        };
        assert_eq!(provider_order(&provider("nous", "Nous")), 0);
        assert_eq!(provider_order(&provider("claude-code", "Claude")), 6);
        assert_eq!(provider_order(&provider("future-provider", "Future")), 99);
        assert_eq!(
            provider_title(&provider("openai-codex", "OpenAI")),
            "OpenAI OAuth (ChatGPT)"
        );
        assert_eq!(
            provider_title(&provider("future-provider", "Future")),
            "Future"
        );
    }

    #[test]
    fn provider_auth_flow_retains_the_session_needed_for_cancellation() {
        let provider = OAuthProvider {
            id: "nous".into(),
            name: "Nous".into(),
            ..OAuthProvider::default()
        };
        let start = OAuthStart::DeviceCode {
            expires_in: 600,
            poll_interval: 5,
            session_id: "session-1".into(),
            user_code: "ABCD-EFGH".into(),
            verification_url: "https://auth.example/device".into(),
        };
        let active = ProviderAuthFlow::Device {
            provider: provider.clone(),
            start,
        };
        assert_eq!(active.session_id(), Some("session-1"));
        let failed = ProviderAuthFlow::Error {
            message: "network".into(),
            provider,
            session_id: Some("session-1".into()),
        };
        assert_eq!(failed.session_id(), Some("session-1"));
    }

    #[test]
    fn provider_key_groups_prefer_backend_identity_and_longest_prefix_fallback() {
        assert_eq!(
            provider_group_for_key("MINIMAX_CN_API_KEY"),
            Some(("MiniMax (China)", 11))
        );
        let key = EnvVarInfo {
            category: "provider".into(),
            description: "Provider key".into(),
            is_password: true,
            provider: Some("future-provider".into()),
            provider_label: Some("Future Provider".into()),
            ..EnvVarInfo::default()
        };
        let endpoint = EnvVarInfo {
            advanced: true,
            category: "provider".into(),
            provider_label: Some("Future Provider".into()),
            ..EnvVarInfo::default()
        };
        let groups = build_provider_key_groups(&BTreeMap::from([
            ("FUTURE_API_KEY".into(), key),
            ("FUTURE_BASE_URL".into(), endpoint),
        ]));
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].name, "Future Provider");
        assert_eq!(groups[0].primary.0, "FUTURE_API_KEY");
        assert_eq!(groups[0].advanced[0].0, "FUTURE_BASE_URL");
        assert_eq!(redacted_credential("abcdefghijkl"), "abcd...ijkl");
    }

    #[test]
    fn custom_endpoint_form_preserves_source_defaults_and_omits_blank_secrets() {
        let endpoint = CustomEndpoint {
            base_url: "http://127.0.0.1:8081/v1".into(),
            context_length: Some(32_768),
            discover_models: true,
            has_api_key: true,
            id: "local".into(),
            is_current: true,
            model: "hermes".into(),
            name: " Local ".into(),
            ..CustomEndpoint::default()
        };
        let form = custom_endpoint_form(&endpoint);
        assert!(form.api_key.is_empty());
        assert_eq!(form.context_length, "32768");
        assert!(form.make_default);
        let payload = custom_endpoint_payload(&form, &["hermes".into(), "qwen".into()]);
        assert_eq!(payload.api_key, None);
        assert_eq!(payload.context_length, Some(32_768));
        assert_eq!(payload.name, "Local");
        assert_eq!(payload.models, ["hermes", "qwen"]);
    }

    #[test]
    fn holds_moa_saves_while_a_slot_is_incomplete() {
        let slot = MoaModelSlot {
            provider: "nous".into(),
            model: "Hermes-4".into(),
            enabled: Some(true),
            ..MoaModelSlot::default()
        };
        let preset = MoaPreset {
            aggregator: slot.clone(),
            enabled: true,
            reference_models: vec![slot],
            ..MoaPreset::default()
        };
        let mut config = MoaConfig {
            default_preset: "default".into(),
            presets: [("default".into(), preset)].into_iter().collect(),
            ..MoaConfig::default()
        };
        assert!(moa_complete(&config));
        let preset = config.presets.get_mut("default").expect("preset");
        preset.reference_models[0].model.clear();
        assert!(!moa_complete(&config));
        let preset = config.presets.get_mut("default").expect("preset");
        preset.reference_models[0].model = "Hermes-4".into();
        preset.reference_models.clear();
        assert!(!moa_complete(&config));
    }

    #[test]
    fn gateway_form_preserves_scope_and_omits_an_unchanged_secret() {
        let config = ConnectionConfig {
            mode: ConnectionMode::Ssh,
            profile: Some("work".into()),
            remote_auth_mode: RemoteAuthMode::Token,
            remote_token_set: true,
            ssh_host: "devbox".into(),
            ssh_port: None,
            ..ConnectionConfig::default()
        };
        let unchanged = gateway_input(&config, "   ");
        assert_eq!(unchanged.profile.as_deref(), Some("work"));
        assert_eq!(unchanged.remote_token, None);
        assert_eq!(unchanged.ssh_port, Some(None));
        assert_eq!(unchanged.ssh_host.as_deref(), Some("devbox"));

        let replaced = gateway_input(&config, "  new-secret  ");
        assert_eq!(replaced.remote_token.as_deref(), Some("new-secret"));
    }

    #[test]
    fn gateway_local_card_switches_copy_for_profile_scope() {
        assert_eq!(
            gateway_mode_copy(ConnectionMode::Local, false).1,
            "Local gateway"
        );
        assert_eq!(
            gateway_mode_copy(ConnectionMode::Local, true).1,
            "Use default gateway"
        );
        assert_eq!(
            gateway_mode_copy(ConnectionMode::Cloud, false).1,
            "Hermes Cloud"
        );
    }
}
