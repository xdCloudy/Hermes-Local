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
    """    pin::Pin,
    sync::Arc,
};""",
    """    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};""",
)
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

#[derive(Clone, Debug, Default)]
pub struct RepoScanCancellation {
    cancelled: Arc<AtomicBool>,
}

impl RepoScanCancellation {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DiscoveredGitRepository {
    pub root: PathBuf,
    pub label: String,
}

pub trait GitRepoScanService: Send + Sync {
    fn scan(
        &self,
        roots: &[PathBuf],
        exclude_paths: &[PathBuf],
        enabled: bool,
        cancellation: RepoScanCancellation,
    ) -> ServiceFuture<'_, Vec<DiscoveredGitRepository>>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableGitRepoScanService;

impl GitRepoScanService for UnavailableGitRepoScanService {
    fn scan(
        &self,
        _roots: &[PathBuf],
        _exclude_paths: &[PathBuf],
        _enabled: bool,
        _cancellation: RepoScanCancellation,
    ) -> ServiceFuture<'_, Vec<DiscoveredGitRepository>> {
        Box::pin(async move {
            Err(ServiceError::Unavailable(
                "Git repository discovery is unavailable on this platform".into(),
            ))
        })
    }
}

pub trait TerminalService: Send + Sync {""",
)
replace_once(
    core,
    "    pub git_discard: Arc<dyn GitDiscardService>,\n    pub terminal: Arc<dyn TerminalService>,",
    "    pub git_discard: Arc<dyn GitDiscardService>,\n    pub git_repo_scan: Arc<dyn GitRepoScanService>,\n    pub terminal: Arc<dyn TerminalService>,",
)


desktop = "crates/hermes-desktop/src/lib.rs"
replace_once(
    desktop,
    """    UnavailableGitDiscardService, UnavailablePreviewService, UpdateService, validate_identifier,
    validate_relative_path,""",
    """    UnavailableGitDiscardService, UnavailableGitRepoScanService, UnavailablePreviewService,
    UpdateService, validate_identifier, validate_relative_path,""",
)
replace_once(
    desktop,
    """                git: Arc::new(DesktopGit),
                git_discard: Arc::new(UnavailableGitDiscardService),
                terminal: Arc::new(DesktopTerminals::default()),""",
    """                git: Arc::new(DesktopGit),
                git_discard: Arc::new(UnavailableGitDiscardService),
                git_repo_scan: Arc::new(UnavailableGitRepoScanService),
                terminal: Arc::new(DesktopTerminals::default()),""",
)

scan = "apps/desktop/src/git_repo_scan_service.rs"
replace_once(
    scan,
    "#![allow(dead_code)] // GT-07 service foundation; Project discovery UI is a later stage.\n\n",
    "",
)
replace_once(
    scan,
    """    path::{Component, Path, PathBuf},
};

const DEFAULT_MAX_DEPTH""",
    """    path::{Component, Path, PathBuf},
    sync::Arc,
};

use hermes_core::{
    AppServices, DiscoveredGitRepository, GitRepoScanService as GitRepoScanServiceContract,
    RepoScanCancellation, ServiceError, ServiceFuture,
};

const DEFAULT_MAX_DEPTH""",
)
replace_once(
    scan,
    """#[derive(Clone, Copy, Debug, Default)]
pub struct GitRepoScanService;

impl GitRepoScanService {""",
    """#[derive(Clone, Copy, Debug, Default)]
pub struct GitRepoScanService;

pub fn install(services: &mut AppServices) {
    services.git_repo_scan = Arc::new(GitRepoScanService);
}

impl GitRepoScanServiceContract for GitRepoScanService {
    fn scan(
        &self,
        roots: &[PathBuf],
        exclude_paths: &[PathBuf],
        enabled: bool,
        cancellation: RepoScanCancellation,
    ) -> ServiceFuture<'_, Vec<DiscoveredGitRepository>> {
        let roots = roots.to_vec();
        let exclude_paths = exclude_paths.to_vec();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || {
                let options = RepoScanOptions {
                    enabled,
                    exclude_paths,
                    ..RepoScanOptions::default()
                };
                GitRepoScanService.scan_with_cancel(&roots, &options, || cancellation.is_cancelled())
            })
            .await
            .map_err(|error| ServiceError::Platform(format!("repository scan worker failed: {error}")))?
            .map(|repositories| {
                repositories
                    .into_iter()
                    .map(|repository| DiscoveredGitRepository {
                        root: repository.root,
                        label: repository.label,
                    })
                    .collect()
            })
            .map_err(|error| {
                if error.contains("cancelled") {
                    ServiceError::Unavailable(error)
                } else {
                    ServiceError::Platform(error)
                }
            })
        })
    }
}

impl GitRepoScanService {""",
)
replace_once(
    scan,
    """    pub fn scan(
        &self,
        roots: &[PathBuf],
        options: &RepoScanOptions,
    ) -> Result<Vec<DiscoveredRepository>, String> {
        if !options.enabled {""",
    """    pub fn scan(
        &self,
        roots: &[PathBuf],
        options: &RepoScanOptions,
    ) -> Result<Vec<DiscoveredRepository>, String> {
        self.scan_with_cancel(roots, options, || false)
    }

    fn scan_with_cancel<F>(
        &self,
        roots: &[PathBuf],
        options: &RepoScanOptions,
        cancelled: F,
    ) -> Result<Vec<DiscoveredRepository>, String>
    where
        F: Fn() -> bool,
    {
        if cancelled() {
            return Err("Repository discovery was cancelled.".to_owned());
        }
        if !options.enabled {""",
)
replace_once(
    scan,
    """        while let Some((directory, depth)) = queue.pop_front() {
            if depth > options.max_depth {""",
    """        while let Some((directory, depth)) = queue.pop_front() {
            if cancelled() {
                return Err("Repository discovery was cancelled.".to_owned());
            }
            if depth > options.max_depth {""",
)
replace_once(
    scan,
    """            for entry in entries {
                let Ok(file_type) = entry.file_type() else {""",
    """            for entry in entries {
                if cancelled() {
                    return Err("Repository discovery was cancelled.".to_owned());
                }
                let Ok(file_type) = entry.file_type() else {""",
)
replace_once(
    scan,
    """    #[test]
    fn scans_configured_root_and_excludes_complete_subtree() {""",
    """    #[test]
    fn cooperative_cancellation_stops_before_touching_roots() {
        let error = GitRepoScanService
            .scan_with_cancel(
                &[PathBuf::from("definitely-relative-and-unused")],
                &RepoScanOptions::default(),
                || true,
            )
            .expect_err("cancelled scan");
        assert!(error.contains("cancelled"));
    }

    #[test]
    fn scans_configured_root_and_excludes_complete_subtree() {""",
)

main = "apps/desktop/src/main.rs"
replace_once(
    main,
    """    notification_service::install(&mut native.services);
    git_discard_service::install(&mut native.services);
    preview_service::install(&mut native.services);""",
    """    notification_service::install(&mut native.services);
    git_discard_service::install(&mut native.services);
    git_repo_scan_service::install(&mut native.services);
    preview_service::install(&mut native.services);""",
)

ui = "crates/hermes-ui/src/lib.rs"
replace_once(
    ui,
    """use std::{collections::BTreeMap, sync::Arc};""",
    """use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::Arc,
};""",
)
replace_once(
    ui,
    """use hermes_core::{AgentConfigService, AppServices, ModelService, SessionTranscript};""",
    """use hermes_core::{
    AgentConfigService, AppServices, DiscoveredGitRepository, ModelService, RepoScanCancellation,
    SessionTranscript,
};""",
)
replace_once(
    ui,
    """fn project_repair_folder(project: &hermes_protocol::ProjectSummary) -> Option<ProjectFolder> {
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
fn Projects() -> Element {""",
    """fn project_repair_folder(project: &hermes_protocol::ProjectSummary) -> Option<ProjectFolder> {
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

#[derive(Clone, Debug)]
struct RepoDiscoveryPolicy {
    enabled: bool,
    roots: Vec<PathBuf>,
    exclude_paths: Vec<PathBuf>,
}

fn repo_discovery_policy(config: &BTreeMap<String, Value>) -> RepoDiscoveryPolicy {
    let desktop = config.get("desktop").and_then(Value::as_object);
    let strings = |key: &str| {
        desktop
            .and_then(|desktop| desktop.get(key))
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(PathBuf::from)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    RepoDiscoveryPolicy {
        enabled: desktop
            .and_then(|desktop| desktop.get("repo_scan_enabled"))
            .and_then(Value::as_bool)
            .unwrap_or(true),
        roots: strings("repo_scan_roots"),
        exclude_paths: strings("repo_scan_exclude_paths"),
    }
}

fn repo_path_key(path: &Path) -> String {
    let value = path.to_string_lossy().replace('\\\\', "/").trim_end_matches('/').to_owned();
    if cfg!(windows) {
        value.to_lowercase()
    } else {
        value
    }
}

fn registered_repo_keys(snapshot: &ProjectsSnapshot) -> BTreeSet<String> {
    snapshot
        .projects
        .iter()
        .flat_map(|project| {
            project
                .folders
                .iter()
                .map(|folder| folder.path.as_str())
                .chain(project.primary_path.iter().map(String::as_str))
        })
        .map(Path::new)
        .map(repo_path_key)
        .collect()
}

#[component]
fn Projects() -> Element {""",
)
replace_once(
    ui,
    """    let mut delete_target = use_signal(|| None::<String>);
    let mut delete_confirmation = use_signal(String::new);

    let needle = query().trim().to_lowercase();""",
    """    let mut delete_target = use_signal(|| None::<String>);
    let mut delete_confirmation = use_signal(String::new);
    let mut discovered_repositories = use_signal(Vec::<DiscoveredGitRepository>::new);
    let mut repo_scanning = use_signal(|| false);
    let mut repo_scan_enabled = use_signal(|| true);
    let mut repo_scan_error = use_signal(|| None::<String>);
    let mut repo_scan_refresh = use_signal(|| 0_u64);
    let mut repo_scan_cancel = use_signal(|| None::<RepoScanCancellation>);
    let mut repo_registering = use_signal(|| None::<String>);

    let scan_service = services.git_repo_scan.clone();
    let config_service = services.agent_config.clone();
    let settings_signal = settings_state.settings;
    let project_snapshot_signal = state.snapshot;
    let _repo_discovery = use_resource(move || {
        let _revision = repo_scan_refresh();
        let snapshot = project_snapshot_signal();
        let profile = settings_signal().profile;
        let scan_service = scan_service.clone();
        let config_service = config_service.clone();
        async move {
            repo_scan_error.set(None);
            let loaded = match config_service.load(profile.as_deref()).await {
                Ok(loaded) => loaded,
                Err(problem) => {
                    repo_scanning.set(false);
                    repo_scan_cancel.set(None);
                    repo_scan_error.set(Some(problem.to_string()));
                    return;
                }
            };
            let policy = repo_discovery_policy(&loaded.config);
            repo_scan_enabled.set(policy.enabled);
            if !policy.enabled {
                discovered_repositories.set(Vec::new());
                repo_scanning.set(false);
                repo_scan_cancel.set(None);
                return;
            }

            let cancellation = RepoScanCancellation::default();
            repo_scan_cancel.set(Some(cancellation.clone()));
            repo_scanning.set(true);
            let result = scan_service
                .scan(
                    &policy.roots,
                    &policy.exclude_paths,
                    true,
                    cancellation.clone(),
                )
                .await;
            if cancellation.is_cancelled() {
                repo_scan_error.set(None);
            } else {
                match result {
                    Ok(repositories) => {
                        let registered = registered_repo_keys(&snapshot);
                        discovered_repositories.set(
                            repositories
                                .into_iter()
                                .filter(|repository| !registered.contains(&repo_path_key(&repository.root)))
                                .collect(),
                        );
                    }
                    Err(problem) => repo_scan_error.set(Some(problem.to_string())),
                }
            }
            repo_scanning.set(false);
            repo_scan_cancel.set(None);
        }
    });

    let needle = query().trim().to_lowercase();""",
)
replace_once(
    ui,
    """                    button { class: "button project-create-button", onclick: move |_| create_open.set(true),
                        Codicon { name: "add" }
                        "New project"
                    }
                }
                div { class: "project-filters", aria_label: "Project filters",""",
    """                    if repo_scanning() {
                        button {
                            class: "button",
                            onclick: move |_| {
                                if let Some(cancellation) = repo_scan_cancel() {
                                    cancellation.cancel();
                                }
                            },
                            "Cancel scan"
                        }
                    } else {
                        button {
                            class: "button",
                            disabled: !repo_scan_enabled(),
                            onclick: move |_| repo_scan_refresh.set(repo_scan_refresh() + 1),
                            Codicon { name: "search" }
                            "Scan repositories"
                        }
                    }
                    button { class: "button project-create-button", onclick: move |_| create_open.set(true),
                        Codicon { name: "add" }
                        "New project"
                    }
                }
                if !repo_scan_enabled() {
                    p { class: "muted", "Repository discovery is disabled for the active profile in Workspace settings." }
                }
                if let Some(problem) = repo_scan_error() {
                    p { class: "inline-error", role: "alert", "{problem}" }
                }
                if !discovered_repositories().is_empty() {
                    section { class: "settings-card", style: "display:grid;gap:.6rem;margin-bottom:1rem;",
                        div { style: "display:flex;align-items:center;gap:.5rem;",
                            div { style: "min-width:0;flex:1;",
                                strong { "Discovered repositories" }
                                div { class: "muted", "Found under the active profile's configured discovery roots. Registering keeps files in place." }
                            }
                            span { class: "scope-pill", "{discovered_repositories().len()} found" }
                        }
                        for repository in discovered_repositories() {
                            {
                                let root = repository.root.to_string_lossy().into_owned();
                                let label = repository.label.clone();
                                let busy_key = root.clone();
                                let project_service = services.projects.clone();
                                rsx! {
                                    div { class: "settings-row", style: "align-items:flex-start;gap:.75rem;",
                                        div { style: "min-width:0;flex:1;",
                                            strong { "{label}" }
                                            div { class: "muted", title: "{root}", "{root}" }
                                        }
                                        button {
                                            class: "button",
                                            disabled: repo_registering().is_some(),
                                            onclick: move |_| {
                                                let project_service = project_service.clone();
                                                let root = root.clone();
                                                let label = label.clone();
                                                let busy_key = busy_key.clone();
                                                repo_registering.set(Some(busy_key));
                                                repo_scan_error.set(None);
                                                let mut refresh = state.refresh;
                                                spawn(async move {
                                                    let result = async {
                                                        let project = project_service.create(&label, std::slice::from_ref(&root)).await?;
                                                        project_service.set_active(Some(&project.id)).await?;
                                                        Ok::<_, hermes_core::ServiceError>(())
                                                    }.await;
                                                    match result {
                                                        Ok(()) => {
                                                            refresh += 1;
                                                            repo_scan_refresh.set(repo_scan_refresh() + 1);
                                                        }
                                                        Err(problem) => repo_scan_error.set(Some(problem.to_string())),
                                                    }
                                                    repo_registering.set(None);
                                                });
                                            },
                                            if repo_registering().as_deref() == Some(root.as_str()) { "Registering…" } else { "Register project" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                div { class: "project-filters", aria_label: "Project filters",""",
)

Path("crates/hermes-desktop/tests/git_repo_discovery_ui_contract.rs").write_text(
    r'''#[test]
fn project_centre_discovery_uses_profile_policy_typed_scan_and_cancellation() {
    let ui = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../hermes-ui/src/lib.rs"));
    let main = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../apps/desktop/src/main.rs"));
    let native = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../apps/desktop/src/git_repo_scan_service.rs"));

    assert!(ui.contains("config_service.load(profile.as_deref()).await"));
    assert!(ui.contains("repo_scan_enabled"));
    assert!(ui.contains("repo_scan_roots"));
    assert!(ui.contains("repo_scan_exclude_paths"));
    assert!(ui.contains("services.git_repo_scan.clone()"));
    assert!(ui.contains("cancellation.cancel()"));
    assert!(ui.contains("Register project"));
    assert!(ui.contains("project_service.create(&label, std::slice::from_ref(&root)).await"));
    assert!(!ui.contains("std::fs"));
    assert!(!ui.contains("Command::new"));
    assert!(!ui.contains("std::process"));
    assert!(main.contains("git_repo_scan_service::install(&mut native.services)"));
    assert!(native.contains("services.git_repo_scan = Arc::new(GitRepoScanService)"));
    assert!(native.contains("tokio::task::spawn_blocking"));
    assert!(native.contains("cancellation.is_cancelled()"));
}
''',
    encoding="utf-8",
)
