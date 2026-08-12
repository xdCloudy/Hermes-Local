from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one match, got {count}: {old[:120]!r}")
    p.write_text(text.replace(old, new), encoding="utf-8")


core = "crates/hermes-core/src/lib.rs"
replace_once(
    core,
    """impl GitDiscardService for UnavailableGitDiscardService {
    fn discard_path(&self, _repository: &Path, _relative: &Path) -> ServiceFuture<'_, ()> {
        Box::pin(async move {
            Err(ServiceError::Unavailable(
                "Git discard is unavailable on this platform".into(),
            ))
        })
    }

    fn discard_all(&self, _repository: &Path) -> ServiceFuture<'_, ()> {
        Box::pin(async move {
            Err(ServiceError::Unavailable(
                "Git discard is unavailable on this platform".into(),
            ))
        })
    }
}

pub trait TerminalService: Send + Sync {""",
    """impl GitDiscardService for UnavailableGitDiscardService {
    fn discard_path(&self, _repository: &Path, _relative: &Path) -> ServiceFuture<'_, ()> {
        Box::pin(async move {
            Err(ServiceError::Unavailable(
                "Git discard is unavailable on this platform".into(),
            ))
        })
    }

    fn discard_all(&self, _repository: &Path) -> ServiceFuture<'_, ()> {
        Box::pin(async move {
            Err(ServiceError::Unavailable(
                "Git discard is unavailable on this platform".into(),
            ))
        })
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GitPullRequestInfo {
    pub url: String,
    pub state: String,
    pub number: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GitShipInfo {
    pub gh_ready: bool,
    pub pull_request: Option<GitPullRequestInfo>,
}

pub trait GitShipService: Send + Sync {
    fn info(&self, repository: &Path) -> ServiceFuture<'_, GitShipInfo>;
    fn commit(
        &self,
        repository: &Path,
        message: &str,
        push_after_commit: bool,
    ) -> ServiceFuture<'_, ()>;
    fn push(&self, repository: &Path) -> ServiceFuture<'_, ()>;
    fn create_pull_request(&self, repository: &Path) -> ServiceFuture<'_, String>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableGitShipService;

impl GitShipService for UnavailableGitShipService {
    fn info(&self, _repository: &Path) -> ServiceFuture<'_, GitShipInfo> {
        Box::pin(async move {
            Err(ServiceError::Unavailable(
                "Git ship actions are unavailable on this platform".into(),
            ))
        })
    }

    fn commit(
        &self,
        _repository: &Path,
        _message: &str,
        _push_after_commit: bool,
    ) -> ServiceFuture<'_, ()> {
        Box::pin(async move {
            Err(ServiceError::Unavailable(
                "Git ship actions are unavailable on this platform".into(),
            ))
        })
    }

    fn push(&self, _repository: &Path) -> ServiceFuture<'_, ()> {
        Box::pin(async move {
            Err(ServiceError::Unavailable(
                "Git ship actions are unavailable on this platform".into(),
            ))
        })
    }

    fn create_pull_request(&self, _repository: &Path) -> ServiceFuture<'_, String> {
        Box::pin(async move {
            Err(ServiceError::Unavailable(
                "Git ship actions are unavailable on this platform".into(),
            ))
        })
    }
}

pub trait TerminalService: Send + Sync {""",
)
replace_once(
    core,
    "    pub git_discard: Arc<dyn GitDiscardService>,\n    pub terminal: Arc<dyn TerminalService>,",
    "    pub git_discard: Arc<dyn GitDiscardService>,\n    pub git_ship: Arc<dyn GitShipService>,\n    pub terminal: Arc<dyn TerminalService>,",
)

desktop = "crates/hermes-desktop/src/lib.rs"
replace_once(
    desktop,
    "    UnavailableGitDiscardService, UnavailablePreviewService, UpdateService, validate_identifier,\n    validate_relative_path,",
    "    UnavailableGitDiscardService, UnavailableGitShipService, UnavailablePreviewService,\n    UpdateService, validate_identifier, validate_relative_path,",
)
replace_once(
    desktop,
    "                git_discard: Arc::new(UnavailableGitDiscardService),\n                terminal: Arc::new(DesktopTerminals::default()),",
    "                git_discard: Arc::new(UnavailableGitDiscardService),\n                git_ship: Arc::new(UnavailableGitShipService),\n                terminal: Arc::new(DesktopTerminals::default()),",
)

ship = "apps/desktop/src/git_ship_service.rs"
replace_once(
    ship,
    "#![allow(dead_code)] // GT-06 service foundation; Review ship UI is a later stage.\n\n",
    "",
)
replace_once(
    ship,
    "    process::{Command, Output, Stdio},\n    thread,",
    "    process::{Command, Output, Stdio},\n    sync::Arc,\n    thread,",
)
replace_once(
    ship,
    "};\n\nconst MAX_GIT_OUTPUT_BYTES",
    """};

use hermes_core::{
    AppServices, GitPullRequestInfo, GitShipInfo, GitShipService as GitShipServiceContract,
    ServiceError, ServiceFuture,
};

const MAX_GIT_OUTPUT_BYTES""",
)
replace_once(
    ship,
    """#[derive(Clone, Debug, Default)]
pub struct GitShipService;

impl GitShipService {""",
    """#[derive(Clone, Debug, Default)]
pub struct GitShipService;

pub fn install(services: &mut AppServices) {
    services.git_ship = Arc::new(GitShipService);
}

impl GitShipServiceContract for GitShipService {
    fn info(&self, repository: &Path) -> ServiceFuture<'_, GitShipInfo> {
        let repository = repository.to_owned();
        Box::pin(async move {
            let result = tokio::task::spawn_blocking(move || GitShipService.ship_info(&repository))
                .await
                .map_err(join_error)?
                .map_err(service_error)?;
            Ok(GitShipInfo {
                gh_ready: result.gh_ready,
                pull_request: result.pull_request.map(|pull_request| GitPullRequestInfo {
                    url: pull_request.url,
                    state: pull_request.state,
                    number: pull_request.number,
                }),
            })
        })
    }

    fn commit(
        &self,
        repository: &Path,
        message: &str,
        push_after_commit: bool,
    ) -> ServiceFuture<'_, ()> {
        let repository = repository.to_owned();
        let message = message.to_owned();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || {
                GitShipService.commit(&repository, &message, push_after_commit)
            })
            .await
            .map_err(join_error)?
            .map(|_| ())
            .map_err(service_error)
        })
    }

    fn push(&self, repository: &Path) -> ServiceFuture<'_, ()> {
        let repository = repository.to_owned();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || GitShipService.push(&repository))
                .await
                .map_err(join_error)?
                .map(|_| ())
                .map_err(service_error)
        })
    }

    fn create_pull_request(&self, repository: &Path) -> ServiceFuture<'_, String> {
        let repository = repository.to_owned();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || GitShipService.create_pull_request(&repository))
                .await
                .map_err(join_error)?
                .map_err(service_error)
        })
    }
}

fn join_error(error: tokio::task::JoinError) -> ServiceError {
    ServiceError::Platform(format!("Git ship worker failed: {error}"))
}

fn service_error(error: String) -> ServiceError {
    if error.contains("not installed") || error.contains("not authenticated") {
        ServiceError::Unavailable(error)
    } else if error.contains("must be")
        || error.contains("required")
        || error.contains("requires")
        || error.contains("NUL character")
        || error.contains("repository root")
    {
        ServiceError::InvalidInput(error)
    } else {
        ServiceError::Platform(error)
    }
}

impl GitShipService {""",
)

main = "apps/desktop/src/main.rs"
replace_once(
    main,
    "    git_discard_service::install(&mut native.services);\n    preview_service::install(&mut native.services);",
    "    git_discard_service::install(&mut native.services);\n    git_ship_service::install(&mut native.services);\n    preview_service::install(&mut native.services);",
)

review = "crates/hermes-ui/src/review.rs"
replace_once(
    review,
    "use hermes_core::AppServices;",
    "use hermes_core::{AppServices, GitShipInfo};",
)
replace_once(
    review,
    """    let mut discard_target = use_signal(|| None::<DiscardTarget>);
    let mut error = use_signal(|| None::<String>);
    let mut refresh = use_signal(|| 0_u64);""",
    """    let mut discard_target = use_signal(|| None::<DiscardTarget>);
    let mut commit_message = use_signal(String::new);
    let mut ship_info = use_signal(GitShipInfo::default);
    let mut ship_loading = use_signal(|| false);
    let mut ship_action = use_signal(|| None::<String>);
    let mut error = use_signal(|| None::<String>);
    let mut refresh = use_signal(|| 0_u64);
    let mut ship_refresh = use_signal(|| 0_u64);""",
)

resource_marker = """    let snapshot = (project_state.snapshot)();
    let active = active_project_root(&snapshot);"""
resource_insert = """    let ship_service = services.git_ship.clone();
    let ship_snapshot = project_state.snapshot;
    let _ship_resource = use_resource(move || {
        let snapshot = ship_snapshot();
        let root = active_project_root(&snapshot).map(|(_, root)| root);
        let _revision = ship_refresh();
        let service = ship_service.clone();
        async move {
            let Some(root) = root else {
                ship_info.set(GitShipInfo::default());
                ship_loading.set(false);
                return;
            };
            ship_loading.set(true);
            match service.info(Path::new(&root)).await {
                Ok(next) => {
                    ship_info.set(next);
                    error.set(None);
                }
                Err(next_error) => {
                    ship_info.set(GitShipInfo::default());
                    error.set(Some(next_error.to_string()));
                }
            }
            ship_loading.set(false);
        }
    });

    let snapshot = (project_state.snapshot)();
    let active = active_project_root(&snapshot);"""
replace_once(review, resource_marker, resource_insert)

replace_once(
    review,
    """    let rows = changes(&current);
    let branch = current.branch.as_deref().unwrap_or("detached / unborn");""",
    """    let rows = changes(&current);
    let branch = current.branch.as_deref().unwrap_or("detached / unborn");
    let current_ship = ship_info();
    let current_pr = current_ship.pull_request.clone();""",
)

ship_ui_marker = """                }
            }
            if let Some(target) = discard_target() {"""
ship_ui = r'''                }
                if !rows.is_empty() {
                    section { class: "settings-card", style: "margin-top:1rem;display:grid;gap:.75rem;",
                        header { style: "display:flex;align-items:center;gap:.5rem;",
                            div { style: "min-width:0;flex:1;",
                                strong { "Ship changes" }
                                div { class: "muted", "Commit the current index, optionally push, then create or open a GitHub pull request." }
                            }
                            button {
                                class: "icon-button",
                                title: "Refresh GitHub status",
                                aria_label: "Refresh GitHub status",
                                disabled: ship_loading() || mutation_busy(),
                                onclick: move |_| ship_refresh.set(ship_refresh() + 1),
                                "↻"
                            }
                        }
                        textarea {
                            aria_label: "Commit message",
                            placeholder: "Commit message",
                            rows: "3",
                            value: "{commit_message}",
                            disabled: mutation_busy(),
                            oninput: move |event| commit_message.set(event.value()),
                        }
                        div { style: "display:flex;flex-wrap:wrap;gap:.5rem;align-items:center;",
                            button {
                                class: "button",
                                disabled: mutation_busy() || commit_message().trim().is_empty(),
                                onclick: {
                                    let service = services.git_ship.clone();
                                    let repo = root.clone().unwrap_or_default();
                                    move |_| {
                                        let service = service.clone();
                                        let repo = repo.clone();
                                        let message = commit_message();
                                        mutation_busy.set(true);
                                        ship_action.set(Some("Committing…".to_owned()));
                                        error.set(None);
                                        spawn(async move {
                                            match service.commit(Path::new(&repo), &message, false).await {
                                                Ok(()) => {
                                                    commit_message.set(String::new());
                                                    refresh.set(refresh() + 1);
                                                    ship_refresh.set(ship_refresh() + 1);
                                                }
                                                Err(next_error) => error.set(Some(next_error.to_string())),
                                            }
                                            ship_action.set(None);
                                            mutation_busy.set(false);
                                        });
                                    }
                                },
                                "Commit"
                            }
                            button {
                                class: "button",
                                disabled: mutation_busy() || commit_message().trim().is_empty(),
                                onclick: {
                                    let service = services.git_ship.clone();
                                    let repo = root.clone().unwrap_or_default();
                                    move |_| {
                                        let service = service.clone();
                                        let repo = repo.clone();
                                        let message = commit_message();
                                        mutation_busy.set(true);
                                        ship_action.set(Some("Committing and pushing…".to_owned()));
                                        error.set(None);
                                        spawn(async move {
                                            match service.commit(Path::new(&repo), &message, true).await {
                                                Ok(()) => {
                                                    commit_message.set(String::new());
                                                    refresh.set(refresh() + 1);
                                                    ship_refresh.set(ship_refresh() + 1);
                                                }
                                                Err(next_error) => error.set(Some(next_error.to_string())),
                                            }
                                            ship_action.set(None);
                                            mutation_busy.set(false);
                                        });
                                    }
                                },
                                "Commit + Push"
                            }
                            button {
                                class: "button",
                                disabled: mutation_busy(),
                                onclick: {
                                    let service = services.git_ship.clone();
                                    let repo = root.clone().unwrap_or_default();
                                    move |_| {
                                        let service = service.clone();
                                        let repo = repo.clone();
                                        mutation_busy.set(true);
                                        ship_action.set(Some("Pushing…".to_owned()));
                                        error.set(None);
                                        spawn(async move {
                                            match service.push(Path::new(&repo)).await {
                                                Ok(()) => ship_refresh.set(ship_refresh() + 1),
                                                Err(next_error) => error.set(Some(next_error.to_string())),
                                            }
                                            ship_action.set(None);
                                            mutation_busy.set(false);
                                        });
                                    }
                                },
                                "Push"
                            }
                            if let Some(pull_request) = current_pr.clone() {
                                button {
                                    class: "button",
                                    disabled: mutation_busy(),
                                    onclick: {
                                        let platform = services.platform.clone();
                                        let url = pull_request.url.clone();
                                        move |_| {
                                            let platform = platform.clone();
                                            let url = url.clone();
                                            mutation_busy.set(true);
                                            ship_action.set(Some("Opening pull request…".to_owned()));
                                            error.set(None);
                                            spawn(async move {
                                                if let Err(next_error) = platform.open_external(&url).await {
                                                    error.set(Some(next_error.to_string()));
                                                }
                                                ship_action.set(None);
                                                mutation_busy.set(false);
                                            });
                                        }
                                    },
                                    "Open PR #{pull_request.number}"
                                }
                            } else {
                                button {
                                    class: "button",
                                    disabled: mutation_busy() || ship_loading() || !current_ship.gh_ready,
                                    title: if current_ship.gh_ready { "Create pull request" } else { "GitHub CLI is unavailable or not authenticated" },
                                    onclick: {
                                        let service = services.git_ship.clone();
                                        let platform = services.platform.clone();
                                        let repo = root.clone().unwrap_or_default();
                                        move |_| {
                                            let service = service.clone();
                                            let platform = platform.clone();
                                            let repo = repo.clone();
                                            mutation_busy.set(true);
                                            ship_action.set(Some("Creating pull request…".to_owned()));
                                            error.set(None);
                                            spawn(async move {
                                                match service.create_pull_request(Path::new(&repo)).await {
                                                    Ok(url) => {
                                                        ship_refresh.set(ship_refresh() + 1);
                                                        if let Err(next_error) = platform.open_external(&url).await {
                                                            error.set(Some(next_error.to_string()));
                                                        }
                                                    }
                                                    Err(next_error) => error.set(Some(next_error.to_string())),
                                                }
                                                ship_action.set(None);
                                                mutation_busy.set(false);
                                            });
                                        }
                                    },
                                    if ship_loading() { "Checking GitHub…" } else if current_ship.gh_ready { "Create PR" } else { "GitHub CLI unavailable" }
                                }
                            }
                            if let Some(action) = ship_action() { span { class: "muted", "{action}" } }
                        }
                    }
                }
            }
            if let Some(target) = discard_target() {'''
replace_once(review, ship_ui_marker, ship_ui)

Path("crates/hermes-desktop/tests/git_ship_ui_contract.rs").write_text(
    r'''#[test]
fn review_ship_uses_typed_services_and_safe_external_opening() {
    let review = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../hermes-ui/src/review.rs"));
    let main = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../apps/desktop/src/main.rs"));
    let native = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../apps/desktop/src/git_ship_service.rs"));

    assert!(review.contains("services.git_ship.clone()"));
    assert!(review.contains("service.commit(Path::new(&repo), &message, false)"));
    assert!(review.contains("service.commit(Path::new(&repo), &message, true)"));
    assert!(review.contains("service.push(Path::new(&repo)).await"));
    assert!(review.contains("service.create_pull_request(Path::new(&repo)).await"));
    assert!(review.contains("platform.open_external(&url).await"));
    assert!(review.contains("Commit + Push"));
    assert!(review.contains("Create PR"));
    assert!(!review.contains("Command::new"));
    assert!(!review.contains("std::process"));
    assert!(main.contains("git_ship_service::install(&mut native.services)"));
    assert!(native.contains("services.git_ship = Arc::new(GitShipService)"));
    assert!(native.contains("tokio::task::spawn_blocking"));
}
''',
    encoding="utf-8",
)
