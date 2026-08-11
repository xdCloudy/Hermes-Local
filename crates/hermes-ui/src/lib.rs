//! Dioxus presentation layer. This crate has no filesystem, process, or OS authority.

use dioxus::prelude::*;
use hermes_core::AppServices;
use hermes_protocol::{MessageRole, SessionCreateRequest};

const APP_CSS: Asset = asset!("/assets/app.css");

#[derive(Clone, Debug, PartialEq, Routable)]
pub enum Route {
    #[layout(AppShell)]
    #[route("/")]
    Overview {},
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
    rsx! {
        document::Link { rel: "stylesheet", href: APP_CSS }
        div { class: "connection-state",
            match &*boot.read_unchecked() {
                Some(Ok(_)) => rsx! { span { class: "online", "● Agent connected" } },
                Some(Err(error)) => rsx! { span { class: "offline", "○ Offline · {error}" } },
                None => rsx! { span { class: "connecting", "◌ Connecting to Agent" } },
            }
        }
        Router::<Route> {}
    }
}

#[component]
fn AppShell() -> Element {
    rsx! {
        div { class: "app-shell",
            aside { class: "rail",
                div { class: "brand-mark", "H" }
                nav { class: "primary-nav", aria_label: "Primary navigation",
                    NavItem { to: Route::Overview {}, glyph: "⌂", label: "Home" }
                    NavItem { to: Route::Projects {}, glyph: "◇", label: "Projects" }
                    NavItem { to: Route::Files {}, glyph: "▱", label: "Files" }
                    NavItem { to: Route::Git {}, glyph: "⑂", label: "Git" }
                    NavItem { to: Route::Terminal {}, glyph: ">_", label: "Terminal" }
                    NavItem { to: Route::Tasks {}, glyph: "✓", label: "Tasks" }
                }
                nav { class: "secondary-nav", aria_label: "Application navigation",
                    NavItem { to: Route::Trust {}, glyph: "◈", label: "Trust" }
                    NavItem { to: Route::Settings {}, glyph: "⚙", label: "Settings" }
                }
            }
            main { class: "workspace", Outlet::<Route> {} }
        }
    }
}

#[component]
fn NavItem(to: Route, glyph: &'static str, label: &'static str) -> Element {
    rsx! {
        Link { class: "nav-item", to, aria_label: label, title: label,
            span { class: "nav-glyph", "{glyph}" }
            span { class: "nav-label", "{label}" }
        }
    }
}

#[component]
fn Overview() -> Element {
    let services = use_context::<AppServices>();
    let list_service = services.sessions.clone();
    let sessions = use_resource(move || {
        let list_service = list_service.clone();
        async move { list_service.list().await }
    });
    let create_service = services.sessions.clone();
    let navigator = use_navigator();
    let mut prompt = use_signal(String::new);
    let mut submitting = use_signal(|| false);
    let mut submit_error = use_signal(|| None::<String>);

    rsx! {
        Surface { eyebrow: "Local intelligence", title: "Good afternoon.", subtitle: "Your private workspace is ready when you are.",
            div { class: "composer-card",
                textarea {
                    aria_label: "Start a conversation",
                    placeholder: "Ask Hermes anything…",
                    rows: "4",
                    value: "{prompt}",
                    oninput: move |event| prompt.set(event.value())
                }
                div { class: "composer-actions",
                    span { class: "privacy-pill", "● On-device" }
                    button {
                        class: "send-button",
                        aria_label: "Send message",
                        disabled: submitting() || prompt().trim().is_empty(),
                        onclick: move |_| {
                            let service = create_service.clone();
                            let text = prompt().trim().to_owned();
                            if text.is_empty() { return; }
                            submitting.set(true);
                            submit_error.set(None);
                            spawn(async move {
                                let result = async {
                                    let session = service.create(SessionCreateRequest::default()).await?;
                                    service.submit(session.runtime_id.as_deref().unwrap_or(&session.id), &text).await?;
                                    Ok::<_, hermes_core::ServiceError>(session.id)
                                }.await;
                                submitting.set(false);
                                match result {
                                    Ok(id) => { navigator.push(Route::Session { id }); }
                                    Err(error) => submit_error.set(Some(error.to_string())),
                                }
                            });
                        },
                        if submitting() { "…" } else { "↑" }
                    }
                }
                if let Some(error) = submit_error() {
                    p { class: "inline-error", role: "alert", "{error}" }
                }
            }
            div { class: "section-heading",
                h2 { "Recent work" }
                Link { to: Route::Projects {}, "View projects" }
            }
            div { class: "card-grid",
                match &*sessions.read_unchecked() {
                    Some(Ok(items)) if !items.is_empty() => rsx! {
                        for session in items.iter().take(6) {
                            Link { to: Route::Session { id: session.id.clone() },
                                article { class: "work-card violet",
                                    div { class: "card-icon", "✦" }
                                    h3 { if session.title.is_empty() { "Untitled session" } else { "{session.title}" } }
                                    p { if session.running { "Running locally" } else { "Ready to resume" } }
                                }
                            }
                        }
                    },
                    Some(Ok(_)) => rsx! { WorkCard { title: "New local session", detail: "Start from a clean context", accent: "violet" } },
                    Some(Err(error)) => rsx! { p { class: "inline-error", "Could not load sessions: {error}" } },
                    None => rsx! { WorkCard { title: "Loading sessions", detail: "Connecting to Hermes Agent", accent: "blue" } },
                }
            }
        }
    }
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
    let project_service = services.projects.clone();
    let projects = use_resource(move || {
        let project_service = project_service.clone();
        async move { project_service.snapshot().await }
    });
    rsx! {
        Surface { eyebrow: "Workspace", title: "Projects", subtitle: "Organise repositories and working folders.",
            match &*projects.read_unchecked() {
                Some(Ok(snapshot)) if !snapshot.projects.is_empty() => rsx! {
                    div { class: "list-stack",
                        for project in &snapshot.projects {
                            Link { to: Route::Project { id: project.id.clone() },
                                article { class: "list-row",
                                    div {
                                        h2 { if project.name.is_empty() { "Unnamed project" } else { "{project.name}" } }
                                        p {
                                            if let Some(path) = &project.primary_path { "{path}" } else { "No primary folder" }
                                        }
                                    }
                                    span { "{project.folders.len()} folders  ›" }
                                }
                            }
                        }
                    }
                },
                Some(Ok(_)) => rsx! { EmptyState { label: "No projects yet", detail: "Create a project from a folder to group sessions and tools." } },
                Some(Err(error)) => rsx! { ErrorState { error: error.to_string() } },
                None => rsx! { LoadingState { label: "Loading projects" } },
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
    let session_id = id.clone();
    let mut revision = use_signal(|| 0_u64);
    let transcript = use_resource(move || {
        revision();
        let load_service = load_service.clone();
        let session_id = session_id.clone();
        async move { load_service.resume(&session_id).await }
    });
    let mut draft = use_signal(String::new);
    let mut sending = use_signal(|| false);
    let mut send_error = use_signal(|| None::<String>);
    rsx! {
        Surface { eyebrow: "Conversation", title: "Session", subtitle: "Session {id}",
            div { class: "transcript",
                match &*transcript.read_unchecked() {
                    Some(Ok(messages)) if !messages.is_empty() => rsx! {
                        for message in messages {
                            article { class: if message.role == MessageRole::User { "message user" } else { "message assistant" },
                                div { class: "message-role", if message.role == MessageRole::User { "You" } else { "Hermes" } }
                                p { "{message.text}" }
                            }
                        }
                    },
                    Some(Ok(_)) => rsx! { EmptyState { label: "A fresh conversation", detail: "Write a message below to begin." } },
                    Some(Err(error)) => rsx! { ErrorState { error: error.to_string() } },
                    None => rsx! { LoadingState { label: "Loading conversation" } },
                }
            }
            div { class: "composer-card session-composer",
                textarea {
                    aria_label: "Message Hermes",
                    placeholder: "Message Hermes…",
                    rows: "3",
                    value: "{draft}",
                    oninput: move |event| draft.set(event.value())
                }
                div { class: "composer-actions",
                    span { class: "privacy-pill", "● Private session" }
                    button {
                        class: "send-button",
                        disabled: sending() || draft().trim().is_empty(),
                        onclick: move |_| {
                            let service = submit_service.clone();
                            let session_id = id.clone();
                            let text = draft().trim().to_owned();
                            if text.is_empty() { return; }
                            sending.set(true);
                            send_error.set(None);
                            spawn(async move {
                                match service.submit(&session_id, &text).await {
                                    Ok(()) => {
                                        draft.set(String::new());
                                        revision += 1;
                                    }
                                    Err(error) => send_error.set(Some(error.to_string())),
                                }
                                sending.set(false);
                            });
                        },
                        if sending() { "…" } else { "↑" }
                    }
                }
                if let Some(error) = send_error() { p { class: "inline-error", role: "alert", "{error}" } }
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
