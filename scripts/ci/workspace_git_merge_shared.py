from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected exactly one shared merge marker, got {count}: {old[:140]!r}")
    p.write_text(text.replace(old, new), encoding="utf-8")


core = "crates/hermes-core/src/lib.rs"
branch_worktree_block = '''#[derive(Clone, Debug, Default, Eq, PartialEq)]
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

    fn add_existing(
        &self,
        _repository: &Path,
        _branch: &str,
    ) -> ServiceFuture<'_, GitWorktreeInfo> {
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

'''
replace_once(
    core,
    "pub trait GitDiscardService: Send + Sync {",
    branch_worktree_block + "pub trait GitDiscardService: Send + Sync {",
)
replace_once(
    core,
    """    pub git: Arc<dyn GitService>,
    pub git_discard: Arc<dyn GitDiscardService>,
    pub git_ship: Arc<dyn GitShipService>,
    pub terminal: Arc<dyn TerminalService>,""",
    """    pub git: Arc<dyn GitService>,
    pub git_branches: Arc<dyn GitBranchService>,
    pub git_worktrees: Arc<dyn GitWorktreeService>,
    pub git_discard: Arc<dyn GitDiscardService>,
    pub git_ship: Arc<dyn GitShipService>,
    pub terminal: Arc<dyn TerminalService>,""",
)

desktop = "crates/hermes-desktop/src/lib.rs"
replace_once(
    desktop,
    """    UnavailableGitDiscardService, UnavailableGitShipService, UnavailablePreviewService,
    UpdateService, validate_identifier, validate_relative_path,""",
    """    UnavailableGitBranchService, UnavailableGitDiscardService, UnavailableGitShipService,
    UnavailableGitWorktreeService, UnavailablePreviewService, UpdateService, validate_identifier,
    validate_relative_path,""",
)
replace_once(
    desktop,
    """                git: Arc::new(DesktopGit),
                git_discard: Arc::new(UnavailableGitDiscardService),
                git_ship: Arc::new(UnavailableGitShipService),
                terminal: Arc::new(DesktopTerminals::default()),""",
    """                git: Arc::new(DesktopGit),
                git_branches: Arc::new(UnavailableGitBranchService),
                git_worktrees: Arc::new(UnavailableGitWorktreeService),
                git_discard: Arc::new(UnavailableGitDiscardService),
                git_ship: Arc::new(UnavailableGitShipService),
                terminal: Arc::new(DesktopTerminals::default()),""",
)

main = "apps/desktop/src/main.rs"
replace_once(
    main,
    """    notification_service::install(&mut native.services);
    git_discard_service::install(&mut native.services);
    git_ship_service::install(&mut native.services);
    preview_service::install(&mut native.services);""",
    """    notification_service::install(&mut native.services);
    git_branch_service::install(&mut native.services);
    git_discard_service::install(&mut native.services);
    git_ship_service::install(&mut native.services);
    git_worktree_service::install(&mut native.services);
    preview_service::install(&mut native.services);""",
)
