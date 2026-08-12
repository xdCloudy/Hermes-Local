from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one match, got {count}: {old[:140]!r}")
    p.write_text(text.replace(old, new), encoding="utf-8")


core = "crates/hermes-core/src/lib.rs"
replace_once(
    core,
    """pub trait TerminalService: Send + Sync {
    fn start(&self, cwd: &Path, cols: u16, rows: u16) -> ServiceFuture<'_, String>;
    fn write(&self, id: &str, data: &[u8]) -> ServiceFuture<'_, ()>;
    fn read(&self, id: &str) -> ServiceFuture<'_, Vec<u8>>;
    fn resize(&self, id: &str, cols: u16, rows: u16) -> ServiceFuture<'_, ()>;
    fn dispose(&self, id: &str) -> ServiceFuture<'_, ()>;
}""",
    """pub trait TerminalService: Send + Sync {
    fn start(&self, cwd: &Path, cols: u16, rows: u16) -> ServiceFuture<'_, String>;
    fn write(&self, id: &str, data: &[u8]) -> ServiceFuture<'_, ()>;
    fn read(&self, id: &str) -> ServiceFuture<'_, Vec<u8>>;
    fn resize(&self, id: &str, cols: u16, rows: u16) -> ServiceFuture<'_, ()>;
    fn dispose(&self, id: &str) -> ServiceFuture<'_, ()>;
    fn dispose_now(&self, _id: &str) -> ServiceResult<()> {
        Err(ServiceError::Unavailable(
            "synchronous terminal disposal is unavailable on this platform".into(),
        ))
    }
}""",
)

desktop = "crates/hermes-desktop/src/lib.rs"
replace_once(
    desktop,
    """#[derive(Default)]
struct DesktopTerminals {
    processes: Mutex<HashMap<String, TerminalProcess>>,
}

impl TerminalService for DesktopTerminals {""",
    """#[derive(Default)]
struct DesktopTerminals {
    processes: Mutex<HashMap<String, TerminalProcess>>,
}

impl DesktopTerminals {
    fn dispose_process(&self, id: &str) -> ServiceResult<()> {
        validate_identifier(id, "terminal")?;
        let mut process = self
            .processes
            .lock()
            .map_err(|_| ServiceError::Platform("terminal lock was poisoned".into()))?
            .remove(id)
            .ok_or_else(|| ServiceError::NotFound("terminal".into()))?;
        process
            .child
            .kill()
            .map_err(|error| ServiceError::Platform(error.to_string()))
    }
}

impl TerminalService for DesktopTerminals {""",
)
replace_once(
    desktop,
    """    fn dispose(&self, id: &str) -> ServiceFuture<'_, ()> {
        let id = id.to_owned();
        Box::pin(async move {
            validate_identifier(&id, "terminal")?;
            let mut process = self
                .processes
                .lock()
                .map_err(|_| ServiceError::Platform("terminal lock was poisoned".into()))?
                .remove(&id)
                .ok_or_else(|| ServiceError::NotFound("terminal".into()))?;
            process
                .child
                .kill()
                .map_err(|error| ServiceError::Platform(error.to_string()))
        })
    }
}""",
    """    fn dispose(&self, id: &str) -> ServiceFuture<'_, ()> {
        let id = id.to_owned();
        Box::pin(async move { self.dispose_process(&id) })
    }

    fn dispose_now(&self, id: &str) -> ServiceResult<()> {
        self.dispose_process(id)
    }
}""",
)

ui = "crates/hermes-ui/src/lib.rs"
replace_once(
    ui,
    """mod files;
mod review;
use files::Files;
use review::Review;""",
    """mod files;
mod review;
mod terminal;
use files::Files;
use review::Review;
use terminal::Terminal;""",
)
replace_once(
    ui,
    """simple_surface!(
    Terminal,
    "Developer tools",
    "Terminal",
    "A native ConPTY session scoped to your workspace."
);
""",
    "",
)

Path("crates/hermes-ui/src/terminal.rs").write_text(
    r'''use std::path::Path;

use dioxus::prelude::*;
use hermes_core::{AppServices, ServiceError};
use hermes_protocol::ProjectsSnapshot;

use super::{ProjectUiState, Surface};

const MAX_UI_OUTPUT_BYTES: usize = 1024 * 1024;
const READ_INTERVAL_MS: u64 = 50;

fn active_project_root(snapshot: &ProjectsSnapshot) -> Option<(String, String)> {
    let active_id = snapshot.active_id.as_deref()?;
    let project = snapshot.projects.iter().find(|project| project.id == active_id)?;
    let folder = project
        .folders
        .iter()
        .find(|folder| folder.is_primary)
        .or_else(|| project.folders.first())?;
    Some((project.name.clone(), folder.path.clone()))
}

fn dimensions(cols: &str, rows: &str) -> Result<(u16, u16), String> {
    let cols = cols
        .trim()
        .parse::<u16>()
        .map_err(|_| "Terminal columns must be a positive integer.".to_owned())?;
    let rows = rows
        .trim()
        .parse::<u16>()
        .map_err(|_| "Terminal rows must be a positive integer.".to_owned())?;
    if cols == 0 || rows == 0 {
        return Err("Terminal dimensions must be greater than zero.".to_owned());
    }
    Ok((cols, rows))
}

fn append_output(mut output: Signal<Vec<u8>>, bytes: &[u8]) {
    if bytes.is_empty() {
        return;
    }
    let mut current = output.write();
    current.extend_from_slice(bytes);
    if current.len() > MAX_UI_OUTPUT_BYTES {
        let excess = current.len() - MAX_UI_OUTPUT_BYTES;
        current.drain(..excess);
    }
}

#[component]
pub(super) fn Terminal() -> Element {
    let services = use_context::<AppServices>();
    let projects = use_context::<ProjectUiState>();
    let snapshot = (projects.snapshot)();
    let active = active_project_root(&snapshot);

    let mut terminal_id = use_signal(|| None::<String>);
    let mut output = use_signal(Vec::<u8>::new);
    let mut input = use_signal(String::new);
    let mut cols = use_signal(|| "120".to_owned());
    let mut rows = use_signal(|| "30".to_owned());
    let mut starting = use_signal(|| false);
    let mut mutating = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    let cleanup_service = services.terminal.clone();
    let cleanup_id = terminal_id;
    use_drop(move || {
        if let Some(id) = cleanup_id() {
            let _ = cleanup_service.dispose_now(&id);
        }
    });

    let output_text = String::from_utf8_lossy(&output()).into_owned();
    let running = terminal_id().is_some();

    rsx! {
        Surface { eyebrow: "Developer tools", title: "Terminal", subtitle: "A native PTY session scoped to the active project. ANSI rendering and persisted scrollback are tracked separately under TM-02.",
            if let Some((project_name, root)) = active {
                div { style: "display:grid;gap:1rem;min-height:0;",
                    section { class: "settings-card", style: "display:grid;gap:.75rem;",
                        header { style: "display:flex;align-items:flex-start;gap:.75rem;",
                            div { style: "min-width:0;flex:1;",
                                strong { "{project_name}" }
                                div { class: "muted", title: "{root}", "{root}" }
                            }
                            if let Some(id) = terminal_id() {
                                span { class: "scope-pill", title: "{id}", "PTY active" }
                            }
                        }
                        div { style: "display:flex;gap:.5rem;align-items:end;flex-wrap:wrap;",
                            label { class: "field-stack", span { "Columns" }
                                input { r#type: "number", min: "1", max: "1000", value: "{cols}", disabled: mutating(), oninput: move |event| cols.set(event.value()) }
                            }
                            label { class: "field-stack", span { "Rows" }
                                input { r#type: "number", min: "1", max: "1000", value: "{rows}", disabled: mutating(), oninput: move |event| rows.set(event.value()) }
                            }
                            if !running {
                                button { class: "button", disabled: starting(), onclick: {
                                    let start_service = services.terminal.clone();
                                    let read_service = services.terminal.clone();
                                    let cwd = root.clone();
                                    move |_| {
                                        let Ok((next_cols, next_rows)) = dimensions(&cols(), &rows()) else {
                                            error.set(Some("Enter valid non-zero terminal dimensions.".to_owned()));
                                            return;
                                        };
                                        let start_service = start_service.clone();
                                        let read_service = read_service.clone();
                                        let cwd = cwd.clone();
                                        starting.set(true);
                                        error.set(None);
                                        output.set(Vec::new());
                                        spawn(async move {
                                            match start_service.start(Path::new(&cwd), next_cols, next_rows).await {
                                                Ok(id) => {
                                                    terminal_id.set(Some(id.clone()));
                                                    starting.set(false);
                                                    loop {
                                                        if terminal_id().as_deref() != Some(id.as_str()) {
                                                            break;
                                                        }
                                                        match read_service.read(&id).await {
                                                            Ok(bytes) => append_output(output, &bytes),
                                                            Err(ServiceError::NotFound(_)) => {
                                                                terminal_id.set(None);
                                                                break;
                                                            }
                                                            Err(problem) => {
                                                                error.set(Some(problem.to_string()));
                                                                break;
                                                            }
                                                        }
                                                        tokio::time::sleep(std::time::Duration::from_millis(READ_INTERVAL_MS)).await;
                                                    }
                                                }
                                                Err(problem) => {
                                                    error.set(Some(problem.to_string()));
                                                    starting.set(false);
                                                }
                                            }
                                        });
                                    }
                                }, if starting() { "Starting…" } else { "Start terminal" } }
                            } else {
                                button { class: "button", disabled: mutating(), onclick: {
                                    let service = services.terminal.clone();
                                    move |_| {
                                        let Some(id) = terminal_id() else { return; };
                                        let Ok((next_cols, next_rows)) = dimensions(&cols(), &rows()) else {
                                            error.set(Some("Enter valid non-zero terminal dimensions.".to_owned()));
                                            return;
                                        };
                                        let service = service.clone();
                                        mutating.set(true);
                                        error.set(None);
                                        spawn(async move {
                                            if let Err(problem) = service.resize(&id, next_cols, next_rows).await {
                                                error.set(Some(problem.to_string()));
                                            }
                                            mutating.set(false);
                                        });
                                    }
                                }, "Resize" }
                                button { class: "button", disabled: mutating(), onclick: {
                                    let service = services.terminal.clone();
                                    move |_| {
                                        let Some(id) = terminal_id() else { return; };
                                        let service = service.clone();
                                        mutating.set(true);
                                        error.set(None);
                                        spawn(async move {
                                            match service.dispose(&id).await {
                                                Ok(()) => terminal_id.set(None),
                                                Err(problem) => error.set(Some(problem.to_string())),
                                            }
                                            mutating.set(false);
                                        });
                                    }
                                }, "Dispose" }
                            }
                        }
                    }

                    section { class: "settings-card", style: "display:grid;gap:.75rem;min-height:24rem;",
                        strong { "Raw PTY output" }
                        pre { aria_label: "Terminal output", style: "margin:0;min-height:16rem;max-height:34rem;overflow:auto;white-space:pre-wrap;overflow-wrap:anywhere;font-family:var(--font-mono,monospace);", "{output_text}" }
                        textarea { aria_label: "Terminal input", placeholder: if running { "Type raw terminal input…" } else { "Start the terminal first" }, rows: "3", value: "{input}", disabled: !running || mutating(), oninput: move |event| input.set(event.value()) }
                        div { style: "display:flex;gap:.5rem;flex-wrap:wrap;",
                            button { class: "button", disabled: !running || mutating() || input().is_empty(), onclick: {
                                let service = services.terminal.clone();
                                move |_| {
                                    let Some(id) = terminal_id() else { return; };
                                    let bytes = input().into_bytes();
                                    let service = service.clone();
                                    mutating.set(true);
                                    error.set(None);
                                    spawn(async move {
                                        match service.write(&id, &bytes).await {
                                            Ok(()) => input.set(String::new()),
                                            Err(problem) => error.set(Some(problem.to_string())),
                                        }
                                        mutating.set(false);
                                    });
                                }
                            }, "Send" }
                            button { class: "button", disabled: !running || mutating(), onclick: {
                                let service = services.terminal.clone();
                                move |_| {
                                    let Some(id) = terminal_id() else { return; };
                                    let mut bytes = input().into_bytes();
                                    bytes.extend_from_slice(b"\r\n");
                                    let service = service.clone();
                                    mutating.set(true);
                                    error.set(None);
                                    spawn(async move {
                                        match service.write(&id, &bytes).await {
                                            Ok(()) => input.set(String::new()),
                                            Err(problem) => error.set(Some(problem.to_string())),
                                        }
                                        mutating.set(false);
                                    });
                                }
                            }, "Send + Enter" }
                            button { class: "button", disabled: !running || mutating(), onclick: {
                                let service = services.terminal.clone();
                                move |_| {
                                    let Some(id) = terminal_id() else { return; };
                                    let service = service.clone();
                                    mutating.set(true);
                                    error.set(None);
                                    spawn(async move {
                                        if let Err(problem) = service.write(&id, b"\x03").await {
                                            error.set(Some(problem.to_string()));
                                        }
                                        mutating.set(false);
                                    });
                                }
                            }, "Ctrl+C" }
                        }
                        if let Some(problem) = error() { p { class: "inline-error", role: "alert", "{problem}" } }
                    }
                }
            } else {
                div { class: "settings-card", p { "Select an active project before starting a terminal so the PTY has an explicit workspace cwd." } }
            }
        }
    }
}
''',
    encoding="utf-8",
)

Path("crates/hermes-desktop/tests/terminal_lifecycle.rs").write_text(
    r'''use std::{fs, path::Path, time::Duration};

use hermes_core::ServiceError;
use hermes_desktop::NativeApp;
use uuid::Uuid;

fn test_root() -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("hermes-terminal-lifecycle-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).expect("terminal test root");
    root
}

#[tokio::test]
async fn pty_round_trip_resize_and_synchronous_dispose() {
    let root = test_root();
    let app = NativeApp::new(root.clone());
    let terminal = app.services.terminal.clone();

    let id = terminal.start(&root, 80, 24).await.expect("start PTY");
    terminal
        .write(&id, b"echo HERMES_PTY_LIFECYCLE\r\n")
        .await
        .expect("write PTY");

    let mut collected = Vec::new();
    for _ in 0..80 {
        collected.extend(terminal.read(&id).await.expect("read PTY"));
        if String::from_utf8_lossy(&collected).contains("HERMES_PTY_LIFECYCLE") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        String::from_utf8_lossy(&collected).contains("HERMES_PTY_LIFECYCLE"),
        "PTY did not return the echo marker: {}",
        String::from_utf8_lossy(&collected)
    );

    terminal.resize(&id, 100, 35).await.expect("resize PTY");
    terminal.dispose_now(&id).expect("dispose PTY synchronously");
    assert!(matches!(terminal.read(&id).await, Err(ServiceError::NotFound(_))));

    let second = terminal.start(&root, 80, 24).await.expect("restart PTY");
    terminal.dispose(&second).await.expect("async dispose PTY");
    assert!(matches!(terminal.read(&second).await, Err(ServiceError::NotFound(_))));

    fs::remove_dir_all(root).expect("remove terminal test root");
}

#[tokio::test]
async fn rejects_invalid_terminal_dimensions_and_cwd() {
    let root = test_root();
    let app = NativeApp::new(root.clone());
    let terminal = app.services.terminal.clone();

    assert!(matches!(
        terminal.start(&root, 0, 24).await,
        Err(ServiceError::InvalidInput(_))
    ));
    assert!(matches!(
        terminal.start(Path::new("Z:/definitely-not-a-real-hermes-directory"), 80, 24).await,
        Err(ServiceError::InvalidInput(_))
    ));

    fs::remove_dir_all(root).expect("remove terminal test root");
}

#[test]
fn dioxus_terminal_owns_cleanup_through_typed_service() {
    let terminal = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../hermes-ui/src/terminal.rs"));
    let ui = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../hermes-ui/src/lib.rs"));

    assert!(terminal.contains("services.terminal.clone()"));
    assert!(terminal.contains("use_drop(move ||"));
    assert!(terminal.contains("cleanup_service.dispose_now(&id)"));
    assert!(terminal.contains("start(Path::new(&cwd), next_cols, next_rows).await"));
    assert!(terminal.contains("read_service.read(&id).await"));
    assert!(terminal.contains("service.write(&id, &bytes).await"));
    assert!(terminal.contains("service.resize(&id, next_cols, next_rows).await"));
    assert!(terminal.contains("service.dispose(&id).await"));
    assert!(!terminal.contains("Command::new"));
    assert!(!terminal.contains("std::process"));
    assert!(!terminal.contains("portable_pty"));
    assert!(ui.contains("mod terminal;"));
    assert!(ui.contains("use terminal::Terminal;"));
    assert!(!ui.contains("simple_surface!(\n    Terminal,"));
}
''',
    encoding="utf-8",
)
