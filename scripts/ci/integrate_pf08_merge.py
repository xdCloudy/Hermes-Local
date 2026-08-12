from pathlib import Path


def replace_once(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one match, found {count}: {old[:140]!r}")
    path.write_text(text.replace(old, new), encoding="utf-8")


main = Path("apps/desktop/src/main.rs")
replace_once(
    main,
    "mod preview_normalization;\nmod preview_watcher;",
    "mod preview_normalization;\nmod preview_service;\nmod preview_watcher;",
)
replace_once(
    main,
    "    notification_service::install(&mut native.services);\n    preview_watcher::install(&mut native.services);",
    "    notification_service::install(&mut native.services);\n    preview_service::install(&mut native.services);\n    preview_watcher::install(&mut native.services);",
)

core = Path("crates/hermes-core/src/lib.rs")
preview_contract = '''#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PreviewDocumentKind {
    Url,
    Html,
    Image,
    Binary,
    #[default]
    Text,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PreviewDocument {
    pub kind: PreviewDocumentKind,
    pub label: String,
    pub source: String,
    pub url: String,
    pub mime_type: Option<String>,
    pub language: Option<String>,
    pub byte_size: Option<u64>,
    pub large: bool,
    pub text: Option<String>,
}

pub trait PreviewService: Send + Sync {
    fn load(
        &self,
        raw_target: &str,
        base_dir: Option<&Path>,
    ) -> ServiceFuture<'_, Option<PreviewDocument>>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailablePreviewService;

impl PreviewService for UnavailablePreviewService {
    fn load(
        &self,
        _raw_target: &str,
        _base_dir: Option<&Path>,
    ) -> ServiceFuture<'_, Option<PreviewDocument>> {
        Box::pin(async move {
            Err(ServiceError::Unavailable(
                "safe preview is unavailable on this platform".into(),
            ))
        })
    }
}

'''
replace_once(core, "pub trait FileService: Send + Sync {\n", preview_contract + "pub trait FileService: Send + Sync {\n")
replace_once(
    core,
    "    pub trust: Arc<dyn TrustService>,\n    pub files: Arc<dyn FileService>,",
    "    pub trust: Arc<dyn TrustService>,\n    pub preview: Arc<dyn PreviewService>,\n    pub files: Arc<dyn FileService>,",
)

desktop = Path("crates/hermes-desktop/src/lib.rs")
replace_once(
    desktop,
    "    ServiceFuture, ServiceResult, SessionService, SettingsService, TerminalService, TrustService,\n    UpdateService, validate_identifier, validate_relative_path,",
    "    ServiceFuture, ServiceResult, SessionService, SettingsService, TerminalService, TrustService,\n    UnavailablePreviewService, UpdateService, validate_identifier, validate_relative_path,",
)
replace_once(
    desktop,
    "                runtime: remote.clone(),\n                trust: remote,\n                files: Arc::new(DesktopFiles),",
    "                runtime: remote.clone(),\n                trust: remote,\n                preview: Arc::new(UnavailablePreviewService),\n                files: Arc::new(DesktopFiles),",
)
