from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected exactly one GT-07 merge marker, got {count}: {old[:140]!r}")
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

scan_block = '''#[derive(Clone, Debug, Default)]
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

'''
replace_once(
    core,
    "pub trait TerminalService: Send + Sync {",
    scan_block + "pub trait TerminalService: Send + Sync {",
)
replace_once(
    core,
    """    pub git_discard: Arc<dyn GitDiscardService>,
    pub git_ship: Arc<dyn GitShipService>,
    pub terminal: Arc<dyn TerminalService>,""",
    """    pub git_discard: Arc<dyn GitDiscardService>,
    pub git_ship: Arc<dyn GitShipService>,
    pub git_repo_scan: Arc<dyn GitRepoScanService>,
    pub terminal: Arc<dyn TerminalService>,""",
)


desktop = "crates/hermes-desktop/src/lib.rs"
replace_once(
    desktop,
    """    UnavailableGitBranchService, UnavailableGitDiscardService, UnavailableGitShipService,
    UnavailableGitWorktreeService, UnavailablePreviewService, UpdateService, validate_identifier,
    validate_relative_path,""",
    """    UnavailableGitBranchService, UnavailableGitDiscardService, UnavailableGitRepoScanService,
    UnavailableGitShipService, UnavailableGitWorktreeService, UnavailablePreviewService,
    UpdateService, validate_identifier, validate_relative_path,""",
)
replace_once(
    desktop,
    """                git_discard: Arc::new(UnavailableGitDiscardService),
                git_ship: Arc::new(UnavailableGitShipService),
                terminal: Arc::new(DesktopTerminals::default()),""",
    """                git_discard: Arc::new(UnavailableGitDiscardService),
                git_ship: Arc::new(UnavailableGitShipService),
                git_repo_scan: Arc::new(UnavailableGitRepoScanService),
                terminal: Arc::new(DesktopTerminals::default()),""",
)

main = "apps/desktop/src/main.rs"
replace_once(
    main,
    """    git_discard_service::install(&mut native.services);
    git_ship_service::install(&mut native.services);
    git_worktree_service::install(&mut native.services);
    preview_service::install(&mut native.services);""",
    """    git_discard_service::install(&mut native.services);
    git_repo_scan_service::install(&mut native.services);
    git_ship_service::install(&mut native.services);
    git_worktree_service::install(&mut native.services);
    preview_service::install(&mut native.services);""",
)
