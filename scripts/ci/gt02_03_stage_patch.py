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
    """    fn stage(&self, repository: &Path, relative: &Path) -> ServiceFuture<'_, ()>;
    fn unstage(&self, repository: &Path, relative: &Path) -> ServiceFuture<'_, ()>;
}

pub trait TerminalService: Send + Sync {""",
    """    fn stage(&self, repository: &Path, relative: &Path) -> ServiceFuture<'_, ()>;
    fn unstage(&self, repository: &Path, relative: &Path) -> ServiceFuture<'_, ()>;
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GitBranchInfo {
    pub name: String,
    pub checked_out: bool,
    pub is_default: bool,
    pub worktree_path: Option<String>,
}

pub trait GitBranchService: Send + Sync {
    fn list(&self, repository: &Path) -> ServiceFuture<'_, Vec<GitBranchInfo>>;
    fn switch(&self, repository: &Path, branch: &str) -> ServiceFuture<'_, String>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableGitBranchService;

impl GitBranchService for UnavailableGitBranchService {
    fn list(&self, _repository: &Path) -> ServiceFuture<'_, Vec<GitBranchInfo>> {
        Box::pin(async move {
            Err(ServiceError::Unavailable(
                "Git branch management is unavailable on this platform".into(),
            ))
        })
    }

    fn switch(&self, _repository: &Path, _branch: &str) -> ServiceFuture<'_, String> {
        Box::pin(async move {
            Err(ServiceError::Unavailable(
                "Git branch management is unavailable on this platform".into(),
            ))
        })
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GitWorktreeInfo {
    pub path: String,
    pub branch: Option<String>,
    pub is_main: bool,
    pub detached: bool,
    pub locked: bool,
}

pub trait GitWorktreeService: Send + Sync {
    fn list(&self, repository: &Path) -> ServiceFuture<'_, Vec<GitWorktreeInfo>>;
    fn add_new(
        &self,
        repository: &Path,
        display_name: &str,
        branch: &str,
        base: Option<&str>,
    ) -> ServiceFuture<'_, GitWorktreeInfo>;
    fn add_existing(&self, repository: &Path, branch: &str) -> ServiceFuture<'_, GitWorktreeInfo>;
    fn remove(
        &self,
        repository: &Path,
        worktree_path: &Path,
        force: bool,
    ) -> ServiceFuture<'_, String>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableGitWorktreeService;

impl GitWorktreeService for UnavailableGitWorktreeService {
    fn list(&self, _repository: &Path) -> ServiceFuture<'_, Vec<GitWorktreeInfo>> {
        Box::pin(async move {
            Err(ServiceError::Unavailable(
                "Git worktree management is unavailable on this platform".into(),
            ))
        })
    }

    fn add_new(
        &self,
        _repository: &Path,
        _display_name: &str,
        _branch: &str,
        _base: Option<&str>,
    ) -> ServiceFuture<'_, GitWorktreeInfo> {
        Box::pin(async move {
            Err(ServiceError::Unavailable(
                "Git worktree management is unavailable on this platform".into(),
            ))
        })
    }

    fn add_existing(&self, _repository: &Path, _branch: &str) -> ServiceFuture<'_, GitWorktreeInfo> {
        Box::pin(async move {
            Err(ServiceError::Unavailable(
                "Git worktree management is unavailable on this platform".into(),
            ))
        })
    }

    fn remove(
        &self,
        _repository: &Path,
        _worktree_path: &Path,
        _force: bool,
    ) -> ServiceFuture<'_, String> {
        Box::pin(async move {
            Err(ServiceError::Unavailable(
                "Git worktree management is unavailable on this platform".into(),
            ))
        })
    }
}

pub trait TerminalService: Send + Sync {""",
)
replace_once(
    core,
    "    pub git: Arc<dyn GitService>,\n    pub terminal: Arc<dyn TerminalService>,",
    "    pub git: Arc<dyn GitService>,\n    pub git_branches: Arc<dyn GitBranchService>,\n    pub git_worktrees: Arc<dyn GitWorktreeService>,\n    pub terminal: Arc<dyn TerminalService>,",
)


desktop = "crates/hermes-desktop/src/lib.rs"
replace_once(
    desktop,
    """    ServiceFuture, ServiceResult, SessionService, SettingsService, TerminalService, TrustService,
    UnavailablePreviewService, UpdateService, validate_identifier, validate_relative_path,
};""",
    """    ServiceFuture, ServiceResult, SessionService, SettingsService, TerminalService, TrustService,
    UnavailableGitBranchService, UnavailableGitWorktreeService, UnavailablePreviewService,
    UpdateService, validate_identifier, validate_relative_path,
};""",
)
replace_once(
    desktop,
    """                files: Arc::new(DesktopFiles),
                git: Arc::new(DesktopGit),
                terminal: Arc::new(DesktopTerminals::default()),""",
    """                files: Arc::new(DesktopFiles),
                git: Arc::new(DesktopGit),
                git_branches: Arc::new(UnavailableGitBranchService),
                git_worktrees: Arc::new(UnavailableGitWorktreeService),
                terminal: Arc::new(DesktopTerminals::default()),""",
)

branch = "apps/desktop/src/git_branch_service.rs"
replace_once(
    branch,
    "#![allow(dead_code)] // GT-02 service foundation; Dioxus Project Centre wiring is a later stage.\n\n",
    "",
)
replace_once(
    branch,
    "    process::{Command, Output},\n};\n\nconst MAX_GIT_OUTPUT_BYTES",
    """    process::{Command, Output},
    sync::Arc,
};

use hermes_core::{
    AppServices, GitBranchInfo, GitBranchService as GitBranchServiceContract, ServiceError,
    ServiceFuture,
};

const MAX_GIT_OUTPUT_BYTES""",
)
replace_once(
    branch,
    """#[derive(Clone, Copy, Debug, Default)]
pub struct GitBranchService;

impl GitBranchService {""",
    """#[derive(Clone, Copy, Debug, Default)]
pub struct GitBranchService;

pub fn install(services: &mut AppServices) {
    services.git_branches = Arc::new(GitBranchService);
}

impl GitBranchServiceContract for GitBranchService {
    fn list(&self, repository: &Path) -> ServiceFuture<'_, Vec<GitBranchInfo>> {
        let repository = repository.to_owned();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || GitBranchService.list(&repository))
                .await
                .map_err(join_error)?
                .map(|branches| {
                    branches
                        .into_iter()
                        .map(|branch| GitBranchInfo {
                            name: branch.name,
                            checked_out: branch.checked_out,
                            is_default: branch.is_default,
                            worktree_path: branch
                                .worktree_path
                                .map(|path| path.to_string_lossy().into_owned()),
                        })
                        .collect()
                })
                .map_err(service_error)
        })
    }

    fn switch(&self, repository: &Path, branch: &str) -> ServiceFuture<'_, String> {
        let repository = repository.to_owned();
        let branch = branch.to_owned();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || GitBranchService.switch(&repository, &branch))
                .await
                .map_err(join_error)?
                .map_err(service_error)
        })
    }
}

fn join_error(error: tokio::task::JoinError) -> ServiceError {
    ServiceError::Platform(format!("Git branch worker failed: {error}"))
}

fn service_error(error: String) -> ServiceError {
    if error.contains("required")
        || error.contains("must be")
        || error.contains("not a Git worktree")
        || error.contains("invalid")
        || error.contains("already checked out")
    {
        ServiceError::InvalidInput(error)
    } else {
        ServiceError::Platform(error)
    }
}

impl GitBranchService {""",
)

worktree = "apps/desktop/src/git_worktree_service.rs"
replace_once(
    worktree,
    "#![allow(dead_code)] // GT-03 service foundation; Dioxus worktree UI is a later stage.\n\n",
    "",
)
replace_once(
    worktree,
    "    process::{Command, Output},\n};\n\nconst MAX_GIT_OUTPUT_BYTES",
    """    process::{Command, Output},
    sync::Arc,
};

use hermes_core::{
    AppServices, GitWorktreeInfo, GitWorktreeService as GitWorktreeServiceContract, ServiceError,
    ServiceFuture,
};

const MAX_GIT_OUTPUT_BYTES""",
)
replace_once(
    worktree,
    """#[derive(Clone, Copy, Debug, Default)]
pub struct GitWorktreeService;

impl GitWorktreeService {""",
    """#[derive(Clone, Copy, Debug, Default)]
pub struct GitWorktreeService;

pub fn install(services: &mut AppServices) {
    services.git_worktrees = Arc::new(GitWorktreeService);
}

impl GitWorktreeServiceContract for GitWorktreeService {
    fn list(&self, repository: &Path) -> ServiceFuture<'_, Vec<GitWorktreeInfo>> {
        let repository = repository.to_owned();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || GitWorktreeService.list(&repository))
                .await
                .map_err(join_error)?
                .map(|worktrees| worktrees.into_iter().map(to_info).collect())
                .map_err(service_error)
        })
    }

    fn add_new(
        &self,
        repository: &Path,
        display_name: &str,
        branch: &str,
        base: Option<&str>,
    ) -> ServiceFuture<'_, GitWorktreeInfo> {
        let repository = repository.to_owned();
        let display_name = display_name.to_owned();
        let branch = branch.to_owned();
        let base = base.map(str::to_owned);
        Box::pin(async move {
            tokio::task::spawn_blocking(move || {
                GitWorktreeService.add_new(
                    &repository,
                    &display_name,
                    &branch,
                    base.as_deref(),
                )
            })
            .await
            .map_err(join_error)?
            .map(to_info)
            .map_err(service_error)
        })
    }

    fn add_existing(&self, repository: &Path, branch: &str) -> ServiceFuture<'_, GitWorktreeInfo> {
        let repository = repository.to_owned();
        let branch = branch.to_owned();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || GitWorktreeService.add_existing(&repository, &branch))
                .await
                .map_err(join_error)?
                .map(to_info)
                .map_err(service_error)
        })
    }

    fn remove(
        &self,
        repository: &Path,
        worktree_path: &Path,
        force: bool,
    ) -> ServiceFuture<'_, String> {
        let repository = repository.to_owned();
        let worktree_path = worktree_path.to_owned();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || {
                GitWorktreeService.remove(&repository, &worktree_path, force)
            })
            .await
            .map_err(join_error)?
            .map(|path| path.to_string_lossy().into_owned())
            .map_err(service_error)
        })
    }
}

fn to_info(worktree: GitWorktree) -> GitWorktreeInfo {
    GitWorktreeInfo {
        path: worktree.path.to_string_lossy().into_owned(),
        branch: worktree.branch,
        is_main: worktree.is_main,
        detached: worktree.detached,
        locked: worktree.locked,
    }
}

fn join_error(error: tokio::task::JoinError) -> ServiceError {
    ServiceError::Platform(format!("Git worktree worker failed: {error}"))
}

fn service_error(error: String) -> ServiceError {
    if error.contains("required")
        || error.contains("must be")
        || error.contains("not a Git worktree")
        || error.contains("cannot be removed")
        || error.contains("only removes managed")
        || error.contains("not registered")
        || error.contains("already checked out")
        || error.contains("invalid")
    {
        ServiceError::InvalidInput(error)
    } else {
        ServiceError::Platform(error)
    }
}

impl GitWorktreeService {""",
)

main = "apps/desktop/src/main.rs"
replace_once(
    main,
    """    let mut native = NativeApp::new(data_dir.clone());
    notification_service::install(&mut native.services);
    preview_service::install(&mut native.services);""",
    """    let mut native = NativeApp::new(data_dir.clone());
    notification_service::install(&mut native.services);
    git_branch_service::install(&mut native.services);
    git_worktree_service::install(&mut native.services);
    preview_service::install(&mut native.services);""",
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
mod source_control;
use files::Files;
use review::Review;
use source_control::{Git, Worktrees};""",
)
replace_once(
    ui,
    """simple_surface!(
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
""",
    "",
)

Path("crates/hermes-ui/src/source_control.rs").write_text(
    r'''use std::path::Path;

use dioxus::prelude::*;
use hermes_core::{AppServices, GitBranchInfo, GitWorktreeInfo};
use hermes_protocol::ProjectsSnapshot;

use super::{ProjectUiState, Surface};

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

#[component]
pub(super) fn Git() -> Element {
    let services = use_context::<AppServices>();
    let project_state = use_context::<ProjectUiState>();
    let mut branches = use_signal(Vec::<GitBranchInfo>::new);
    let mut loading = use_signal(|| false);
    let mut busy = use_signal(|| None::<String>);
    let mut error = use_signal(|| None::<String>);
    let mut refresh = use_signal(|| 0_u64);

    let service = services.git_branches.clone();
    let snapshot_signal = project_state.snapshot;
    let _branches = use_resource(move || {
        let snapshot = snapshot_signal();
        let root = active_project_root(&snapshot).map(|(_, root)| root);
        let _revision = refresh();
        let service = service.clone();
        async move {
            let Some(root) = root else {
                branches.set(Vec::new());
                loading.set(false);
                return;
            };
            loading.set(true);
            match service.list(Path::new(&root)).await {
                Ok(next) => {
                    branches.set(next);
                    error.set(None);
                }
                Err(next_error) => error.set(Some(next_error.to_string())),
            }
            loading.set(false);
        }
    });

    let snapshot = (project_state.snapshot)();
    let active = active_project_root(&snapshot);
    let rows = branches();

    rsx! {
        Surface { eyebrow: "Source control", title: "Branches", summary: "Switch the home checkout or isolate non-default branches in linked worktrees.",
            if let Some((project_name, root)) = active {
                div { class: "settings-card", style: "display:grid;gap:.75rem;",
                    header { style: "display:flex;align-items:center;gap:.5rem;",
                        div { style: "min-width:0;flex:1;",
                            strong { "{project_name}" }
                            div { class: "muted", title: "{root}", "{root}" }
                        }
                        button { class: "icon-button", title: "Refresh branches", aria_label: "Refresh branches", disabled: loading() || busy().is_some(), onclick: move |_| refresh.set(refresh() + 1), "↻" }
                    }
                    if loading() && rows.is_empty() { p { class: "muted", "Loading branches…" } }
                    if rows.is_empty() && !loading() { p { class: "muted", "No local branches were found." } }
                    for branch in rows {
                        {
                            let name = branch.name.clone();
                            let switch_name = name.clone();
                            let worktree_name = name.clone();
                            let switch_root = root.clone();
                            let worktree_root = root.clone();
                            let branch_service = services.git_branches.clone();
                            let worktree_service = services.git_worktrees.clone();
                            rsx! {
                                div { class: "settings-row", style: "align-items:flex-start;gap:.75rem;",
                                    div { style: "min-width:0;flex:1;",
                                        div { style: "display:flex;gap:.4rem;align-items:center;flex-wrap:wrap;",
                                            strong { "{branch.name}" }
                                            if branch.is_default { span { class: "scope-pill", "default" } }
                                            if branch.checked_out { span { class: "scope-pill", "checked out" } }
                                        }
                                        if let Some(path) = branch.worktree_path.as_deref() { div { class: "muted", title: "{path}", "{path}" } }
                                    }
                                    if !branch.checked_out && branch.is_default {
                                        button { class: "button", disabled: busy().is_some(), onclick: move |_| {
                                            let service = branch_service.clone();
                                            let repo = switch_root.clone();
                                            let branch = switch_name.clone();
                                            busy.set(Some(format!("Switching to {branch}…")));
                                            error.set(None);
                                            spawn(async move {
                                                match service.switch(Path::new(&repo), &branch).await {
                                                    Ok(_) => refresh.set(refresh() + 1),
                                                    Err(next_error) => error.set(Some(next_error.to_string())),
                                                }
                                                busy.set(None);
                                            });
                                        }, "Switch here" }
                                    } else if !branch.checked_out {
                                        button { class: "button", disabled: busy().is_some(), onclick: move |_| {
                                            let service = worktree_service.clone();
                                            let repo = worktree_root.clone();
                                            let branch = worktree_name.clone();
                                            busy.set(Some(format!("Creating worktree for {branch}…")));
                                            error.set(None);
                                            spawn(async move {
                                                match service.add_existing(Path::new(&repo), &branch).await {
                                                    Ok(_) => refresh.set(refresh() + 1),
                                                    Err(next_error) => error.set(Some(next_error.to_string())),
                                                }
                                                busy.set(None);
                                            });
                                        }, "Create worktree" }
                                    }
                                }
                            }
                        }
                    }
                    if let Some(action) = busy() { p { class: "muted", "{action}" } }
                    if let Some(message) = error() { p { class: "error-text", role: "alert", "{message}" } }
                }
            } else {
                div { class: "settings-card", p { "Select a project to inspect its branches." } }
            }
        }
    }
}

#[component]
pub(super) fn Worktrees() -> Element {
    let services = use_context::<AppServices>();
    let project_state = use_context::<ProjectUiState>();
    let mut worktrees = use_signal(Vec::<GitWorktreeInfo>::new);
    let mut branches = use_signal(Vec::<GitBranchInfo>::new);
    let mut new_branch = use_signal(String::new);
    let mut base_ref = use_signal(String::new);
    let mut existing_branch = use_signal(String::new);
    let mut loading = use_signal(|| false);
    let mut busy = use_signal(|| None::<String>);
    let mut remove_target = use_signal(|| None::<GitWorktreeInfo>);
    let mut force_remove = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut refresh = use_signal(|| 0_u64);

    let worktree_service = services.git_worktrees.clone();
    let branch_service = services.git_branches.clone();
    let snapshot_signal = project_state.snapshot;
    let _worktrees = use_resource(move || {
        let snapshot = snapshot_signal();
        let root = active_project_root(&snapshot).map(|(_, root)| root);
        let _revision = refresh();
        let worktree_service = worktree_service.clone();
        let branch_service = branch_service.clone();
        async move {
            let Some(root) = root else {
                worktrees.set(Vec::new());
                branches.set(Vec::new());
                loading.set(false);
                return;
            };
            loading.set(true);
            let trees = worktree_service.list(Path::new(&root)).await;
            let branch_rows = branch_service.list(Path::new(&root)).await;
            match (trees, branch_rows) {
                (Ok(next_trees), Ok(next_branches)) => {
                    worktrees.set(next_trees);
                    branches.set(next_branches);
                    error.set(None);
                }
                (Err(next_error), _) | (_, Err(next_error)) => error.set(Some(next_error.to_string())),
            }
            loading.set(false);
        }
    });

    let snapshot = (project_state.snapshot)();
    let active = active_project_root(&snapshot);
    let rows = worktrees();
    let available = branches()
        .into_iter()
        .filter(|branch| !branch.checked_out)
        .collect::<Vec<_>>();

    rsx! {
        Surface { eyebrow: "Source control", title: "Worktrees", summary: "Create isolated branch checkouts and remove only Hermes-managed linked worktrees.",
            if let Some((project_name, root)) = active {
                div { style: "display:grid;gap:1rem;",
                    section { class: "settings-card", style: "display:grid;gap:.75rem;",
                        header { style: "display:flex;align-items:center;gap:.5rem;",
                            div { style: "min-width:0;flex:1;", strong { "{project_name}" } div { class: "muted", title: "{root}", "{root}" } }
                            button { class: "icon-button", title: "Refresh worktrees", aria_label: "Refresh worktrees", disabled: loading() || busy().is_some(), onclick: move |_| refresh.set(refresh() + 1), "↻" }
                        }
                        div { style: "display:grid;grid-template-columns:minmax(0,1fr) minmax(0,1fr) auto;gap:.5rem;align-items:end;",
                            label { class: "field-stack", span { "New branch" } input { aria_label: "New worktree branch", placeholder: "feature/my-change", value: "{new_branch}", disabled: busy().is_some(), oninput: move |event| new_branch.set(event.value()) } }
                            label { class: "field-stack", span { "Base ref (optional)" } input { aria_label: "Base ref", placeholder: "main or origin/main", value: "{base_ref}", disabled: busy().is_some(), oninput: move |event| base_ref.set(event.value()) } }
                            button { class: "button", disabled: busy().is_some() || new_branch().trim().is_empty(), onclick: {
                                let service = services.git_worktrees.clone();
                                let repo = root.clone();
                                move |_| {
                                    let service = service.clone();
                                    let repo = repo.clone();
                                    let branch = new_branch().trim().to_owned();
                                    let base = base_ref().trim().to_owned();
                                    busy.set(Some(format!("Creating {branch}…")));
                                    error.set(None);
                                    spawn(async move {
                                        match service.add_new(Path::new(&repo), &branch, &branch, (!base.is_empty()).then_some(base.as_str())).await {
                                            Ok(_) => { new_branch.set(String::new()); base_ref.set(String::new()); refresh.set(refresh() + 1); }
                                            Err(next_error) => error.set(Some(next_error.to_string())),
                                        }
                                        busy.set(None);
                                    });
                                }
                            }, "New worktree" }
                        }
                        div { style: "display:flex;gap:.5rem;align-items:end;",
                            label { class: "field-stack", style: "min-width:14rem;flex:1;", span { "Existing local branch" }
                                select { aria_label: "Existing branch", value: "{existing_branch}", disabled: busy().is_some(), onchange: move |event| existing_branch.set(event.value()),
                                    option { value: "", "Select a branch…" }
                                    for branch in available { option { value: "{branch.name}", "{branch.name}" } }
                                }
                            }
                            button { class: "button", disabled: busy().is_some() || existing_branch().is_empty(), onclick: {
                                let service = services.git_worktrees.clone();
                                let repo = root.clone();
                                move |_| {
                                    let service = service.clone();
                                    let repo = repo.clone();
                                    let branch = existing_branch();
                                    busy.set(Some(format!("Creating worktree for {branch}…")));
                                    error.set(None);
                                    spawn(async move {
                                        match service.add_existing(Path::new(&repo), &branch).await {
                                            Ok(_) => { existing_branch.set(String::new()); refresh.set(refresh() + 1); }
                                            Err(next_error) => error.set(Some(next_error.to_string())),
                                        }
                                        busy.set(None);
                                    });
                                }
                            }, "Add existing" }
                        }
                    }
                    section { class: "settings-card", style: "display:grid;gap:.5rem;",
                        strong { "Registered worktrees" }
                        if loading() && rows.is_empty() { p { class: "muted", "Loading worktrees…" } }
                        for tree in rows {
                            {
                                let target = tree.clone();
                                rsx! {
                                    div { class: "settings-row", style: "align-items:flex-start;gap:.75rem;",
                                        div { style: "min-width:0;flex:1;",
                                            div { style: "display:flex;gap:.4rem;align-items:center;flex-wrap:wrap;",
                                                strong { "{tree.branch.as_deref().unwrap_or("detached")}" }
                                                if tree.is_main { span { class: "scope-pill", "main checkout" } }
                                                if tree.detached { span { class: "scope-pill", "detached" } }
                                                if tree.locked { span { class: "scope-pill", "locked" } }
                                            }
                                            div { class: "muted", title: "{tree.path}", "{tree.path}" }
                                        }
                                        if !tree.is_main {
                                            button { class: "button", disabled: busy().is_some(), onclick: move |_| { force_remove.set(false); remove_target.set(Some(target.clone())); }, "Remove" }
                                        }
                                    }
                                }
                            }
                        }
                        if rows.is_empty() && !loading() { p { class: "muted", "No registered worktrees were found." } }
                        if let Some(action) = busy() { p { class: "muted", "{action}" } }
                        if let Some(message) = error() { p { class: "error-text", role: "alert", "{message}" } }
                    }
                }
                if let Some(target) = remove_target() {
                    {
                        let path = target.path.clone();
                        let remove_path = path.clone();
                        let remove_root = root.clone();
                        let service = services.git_worktrees.clone();
                        rsx! {
                            div { role: "dialog", "aria-modal": "true", style: "position:fixed;inset:0;z-index:80;background:rgba(0,0,0,.58);display:grid;place-items:center;padding:1rem;",
                                div { class: "settings-card", style: "width:min(34rem,100%);display:grid;gap:.75rem;",
                                    strong { "Remove worktree?" }
                                    p { style: "margin:0;line-height:1.5;", "Remove the registered worktree at {path}? The main checkout can never be removed here." }
                                    label { style: "display:flex;gap:.5rem;align-items:center;", input { r#type: "checkbox", checked: force_remove(), disabled: busy().is_some(), onchange: move |event| force_remove.set(event.checked()) } span { "Force removal of a dirty/locked worktree" } }
                                    div { style: "display:flex;justify-content:flex-end;gap:.5rem;",
                                        button { class: "button", disabled: busy().is_some(), onclick: move |_| remove_target.set(None), "Cancel" }
                                        button { class: "button", disabled: busy().is_some(), onclick: move |_| {
                                            let service = service.clone();
                                            let repo = remove_root.clone();
                                            let target_path = remove_path.clone();
                                            let force = force_remove();
                                            busy.set(Some("Removing worktree…".to_owned()));
                                            error.set(None);
                                            spawn(async move {
                                                match service.remove(Path::new(&repo), Path::new(&target_path), force).await {
                                                    Ok(_) => { remove_target.set(None); refresh.set(refresh() + 1); }
                                                    Err(next_error) => error.set(Some(next_error.to_string())),
                                                }
                                                busy.set(None);
                                            });
                                        }, "Remove worktree" }
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                div { class: "settings-card", p { "Select a project to manage Git worktrees." } }
            }
        }
    }
}
''',
    encoding="utf-8",
)

Path("crates/hermes-desktop/tests/git_source_control_ui_contract.rs").write_text(
    r'''#[test]
fn branch_and_worktree_surfaces_use_typed_services_only() {
    let ui = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../hermes-ui/src/source_control.rs"));
    let main = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../apps/desktop/src/main.rs"));
    let branches = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../apps/desktop/src/git_branch_service.rs"));
    let worktrees = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../apps/desktop/src/git_worktree_service.rs"));

    assert!(ui.contains("services.git_branches.clone()"));
    assert!(ui.contains("services.git_worktrees.clone()"));
    assert!(ui.contains("service.switch(Path::new(&repo), &branch).await"));
    assert!(ui.contains("service.add_existing(Path::new(&repo), &branch).await"));
    assert!(ui.contains("service.add_new(Path::new(&repo)"));
    assert!(ui.contains("service.remove(Path::new(&repo), Path::new(&target_path), force).await"));
    assert!(ui.contains("Remove worktree?"));
    assert!(ui.contains("Force removal of a dirty/locked worktree"));
    assert!(!ui.contains("Command::new"));
    assert!(!ui.contains("std::process"));
    assert!(main.contains("git_branch_service::install(&mut native.services)"));
    assert!(main.contains("git_worktree_service::install(&mut native.services)"));
    assert!(branches.contains("services.git_branches = Arc::new(GitBranchService)"));
    assert!(worktrees.contains("services.git_worktrees = Arc::new(GitWorktreeService)"));
    assert!(branches.contains("tokio::task::spawn_blocking"));
    assert!(worktrees.contains("tokio::task::spawn_blocking"));
}
''',
    encoding="utf-8",
)
