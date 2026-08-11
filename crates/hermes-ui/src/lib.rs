//! Dioxus presentation layer. This crate has no filesystem, process, or OS authority.

use dioxus::prelude::*;
use futures_util::StreamExt;
use hermes_core::{AppServices, SessionTranscript};
use hermes_protocol::{MessageRole, ProjectsSnapshot, SessionCreateRequest, SessionSummary};

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
    let boot = use_resource(move || {
        let connection = connection.clone();
        async move { connection.initialize().await }
    });
    use_context_provider(|| boot);
    rsx! {
        style { dangerous_inner_html: APP_CSS }
        div { class: "icon-sprite", aria_hidden: "true", dangerous_inner_html: CODICON_SPRITE }
        div { class: "window-root",
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
    Appearance,
    "Preferences",
    "Appearance",
    "Choose theme, density, and motion."
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
    let snapshot = (state.snapshot)();
    let mut query = use_signal(String::new);
    let mut filter = use_signal(|| "all".to_owned());
    let mut create_open = use_signal(|| false);
    let mut create_mode = use_signal(|| "empty".to_owned());
    let mut project_name = use_signal(String::new);
    let mut project_path = use_signal(String::new);
    let mut repository_url = use_signal(String::new);
    let mut creating = use_signal(|| false);
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
                                input { placeholder: "C:\\path\\to\\folder", value: "{project_path}", oninput: move |event| project_path.set(event.value()) }
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
    let services = use_context::<AppServices>();
    let load_service = services.settings.clone();
    let save_service = services.settings.clone();
    let mut revision = use_signal(|| 0_u64);
    let settings = use_resource(move || {
        revision();
        let load_service = load_service.clone();
        async move { load_service.load().await }
    });
    let mut save_error = use_signal(|| None::<String>);
    rsx! {
        Surface { eyebrow: "Preferences", title: "Settings", subtitle: "Make Hermes feel at home on this device.",
            match &*settings.read_unchecked() {
                Some(Ok(current)) => {
                    let current = current.clone();
                    rsx! {
                        div { class: "settings-stack",
                            section { class: "settings-card",
                                div {
                                    h2 { "Appearance" }
                                    p { "Follow the system, or pin a light or dark theme." }
                                }
                                div { class: "segmented",
                                    for (mode, label) in [
                                        (hermes_protocol::ThemeMode::System, "System"),
                                        (hermes_protocol::ThemeMode::Dark, "Dark"),
                                        (hermes_protocol::ThemeMode::Light, "Light"),
                                    ] {
                                        button {
                                            class: if current.theme == mode { "selected" } else { "" },
                                            onclick: {
                                                let service = save_service.clone();
                                                let mut next = current.clone();
                                                let mode = mode.clone();
                                                move |_| {
                                                    next.theme = mode.clone();
                                                    let next = next.clone();
                                                    let service = service.clone();
                                                    spawn(async move {
                                                        match service.save(&next).await {
                                                            Ok(()) => revision += 1,
                                                            Err(error) => save_error.set(Some(error.to_string())),
                                                        }
                                                    });
                                                }
                                            },
                                            "{label}"
                                        }
                                    }
                                }
                            }
                            section { class: "settings-card",
                                div {
                                    h2 { "Local privacy" }
                                    p { "Native authority remains behind typed Rust services; the WebView receives no shell or filesystem bridge." }
                                }
                                span { class: "privacy-pill", "● Enforced" }
                            }
                            if let Some(error) = save_error() {
                                p { class: "inline-error", role: "alert", "{error}" }
                            }
                        }
                    }
                },
                Some(Err(error)) => rsx! { ErrorState { error: error.to_string() } },
                None => rsx! { LoadingState { label: "Loading preferences" } },
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
