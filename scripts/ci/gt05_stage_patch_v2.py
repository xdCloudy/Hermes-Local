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
    "    fn unstage(&self, repository: &Path, relative: &Path) -> ServiceFuture<'_, ()>;\n}\n\npub trait TerminalService: Send + Sync {",
    """    fn unstage(&self, repository: &Path, relative: &Path) -> ServiceFuture<'_, ()>;
}

pub trait GitDiscardService: Send + Sync {
    fn discard_path(&self, repository: &Path, relative: &Path) -> ServiceFuture<'_, ()>;
    fn discard_all(&self, repository: &Path) -> ServiceFuture<'_, ()>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableGitDiscardService;

impl GitDiscardService for UnavailableGitDiscardService {
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
)
replace_once(
    core,
    "    pub git: Arc<dyn GitService>,\n    pub terminal: Arc<dyn TerminalService>,",
    "    pub git: Arc<dyn GitService>,\n    pub git_discard: Arc<dyn GitDiscardService>,\n    pub terminal: Arc<dyn TerminalService>,",
)

desktop = "crates/hermes-desktop/src/lib.rs"
replace_once(
    desktop,
    "UnavailablePreviewService, UpdateService, validate_identifier, validate_relative_path,",
    "UnavailableGitDiscardService, UnavailablePreviewService, UpdateService, validate_identifier, validate_relative_path,",
)
replace_once(
    desktop,
    "                git: Arc::new(DesktopGit),\n                terminal: Arc::new(DesktopTerminals::default()),",
    "                git: Arc::new(DesktopGit),\n                git_discard: Arc::new(UnavailableGitDiscardService),\n                terminal: Arc::new(DesktopTerminals::default()),",
)

discard = "apps/desktop/src/git_discard_service.rs"
replace_once(
    discard,
    "#![allow(dead_code)] // GT-05 service foundation; review confirmation/UI is a later stage.\n\n",
    "",
)
replace_once(
    discard,
    "    process::{Command, Output},\n};\n\nconst MAX_GIT_OUTPUT_BYTES",
    """    process::{Command, Output},
    sync::Arc,
};

use hermes_core::{
    AppServices, GitDiscardService as GitDiscardServiceContract, ServiceError, ServiceFuture,
};

const MAX_GIT_OUTPUT_BYTES""",
)
replace_once(
    discard,
    "pub struct GitDiscardService;\n\nimpl GitDiscardService {",
    """pub struct GitDiscardService;

pub fn install(services: &mut AppServices) {
    services.git_discard = Arc::new(GitDiscardService);
}

impl GitDiscardServiceContract for GitDiscardService {
    fn discard_path(&self, repository: &Path, relative: &Path) -> ServiceFuture<'_, ()> {
        let repository = repository.to_owned();
        let relative = relative.to_owned();
        Box::pin(async move {
            GitDiscardService
                .discard_path(&repository, &relative)
                .map_err(service_error)
        })
    }

    fn discard_all(&self, repository: &Path) -> ServiceFuture<'_, ()> {
        let repository = repository.to_owned();
        Box::pin(async move {
            GitDiscardService
                .discard_all(&repository)
                .map_err(service_error)
        })
    }
}

fn service_error(error: String) -> ServiceError {
    if error.contains("cannot escape") || error.contains("metadata cannot") {
        ServiceError::PermissionDenied(error)
    } else if error.contains("must be")
        || error.contains("requires a repository")
        || error.contains("NUL character")
        || error.starts_with("Use discard_all")
    {
        ServiceError::InvalidInput(error)
    } else {
        ServiceError::Platform(error)
    }
}

impl GitDiscardService {""",
)

main = "apps/desktop/src/main.rs"
replace_once(
    main,
    "    notification_service::install(&mut native.services);\n    preview_service::install(&mut native.services);",
    "    notification_service::install(&mut native.services);\n    git_discard_service::install(&mut native.services);\n    preview_service::install(&mut native.services);",
)

review_path = Path("crates/hermes-ui/src/review.rs")
review = review_path.read_text(encoding="utf-8")

marker = "\n#[component]\npub(super) fn Review() -> Element {"
if review.count(marker) != 1:
    raise SystemExit("review.rs: component marker missing or ambiguous")
review = review.replace(
    marker,
    """

#[derive(Clone, Debug, Eq, PartialEq)]
enum DiscardTarget {
    Path(String),
    All,
}

#[component]
pub(super) fn Review() -> Element {""",
)

signal = "    let mut mutation_busy = use_signal(|| false);\n    let mut error = use_signal(|| None::<String>);"
if review.count(signal) != 1:
    raise SystemExit("review.rs: mutation signal marker missing or ambiguous")
review = review.replace(
    signal,
    "    let mut mutation_busy = use_signal(|| false);\n    let mut discard_target = use_signal(|| None::<DiscardTarget>);\n    let mut error = use_signal(|| None::<String>);",
)

lines = review.splitlines()
refresh_title = next((i for i, line in enumerate(lines) if 'title: "Refresh Git status"' in line), None)
if refresh_title is None:
    raise SystemExit("review.rs: refresh button marker not found")
button_start = refresh_title - 1
if lines[button_start].strip() != "button {":
    raise SystemExit("review.rs: refresh button start was not adjacent")
indent = lines[button_start][: len(lines[button_start]) - len(lines[button_start].lstrip())]
insert = [
    indent + "if !rows.is_empty() {",
    indent + "    button {",
    indent + '        class: "button",',
    indent + "        disabled: mutation_busy(),",
    indent + "        onclick: move |_| discard_target.set(Some(DiscardTarget::All)),",
    indent + '        "Discard all"',
    indent + "    }",
    indent + "}",
]
lines[button_start:button_start] = insert
review = "\n".join(lines) + "\n"

header_end = "                        }\n                        if diff_loading() {"
if review.count(header_end) != 1:
    raise SystemExit(f"review.rs: diff header end expected once, got {review.count(header_end)}")
review = review.replace(
    header_end,
    """                        }
                        if let Some(path) = selected_path() {
                            button {
                                class: "button",
                                disabled: mutation_busy(),
                                onclick: move |_| discard_target.set(Some(DiscardTarget::Path(path.clone()))),
                                "Discard file"
                            }
                        }
                        if diff_loading() {""",
)

suffix = "                    }\n                }\n            }\n        }\n    }\n}\n"
if not review.endswith(suffix):
    raise SystemExit("review.rs: unexpected component tail")
modal = r'''                    }
                }
            }
            if let Some(target) = discard_target() {
                {
                    let message = match &target {
                        DiscardTarget::Path(path) => format!(
                            "Discard every staged and unstaged change for {path}, including a non-ignored untracked file if present?"
                        ),
                        DiscardTarget::All => "Discard every staged and unstaged change in this repository and remove non-ignored untracked files?".to_owned(),
                    };
                    let action_target = target.clone();
                    let discard_root = root.clone().unwrap_or_default();
                    let discard_service = services.git_discard.clone();
                    rsx! {
                        div {
                            role: "dialog",
                            "aria-modal": "true",
                            style: "position:fixed;inset:0;z-index:80;background:rgba(0,0,0,.58);display:grid;place-items:center;padding:1rem;",
                            div {
                                class: "settings-card",
                                style: "width:min(32rem,100%);display:grid;gap:.75rem;box-shadow:0 1.25rem 4rem rgba(0,0,0,.45);",
                                strong { "Discard changes?" }
                                p { style: "margin:0;line-height:1.5;", "{message}" }
                                p { class: "muted", style: "margin:0;", "This cannot be undone. Ignored files are preserved." }
                                div { style: "display:flex;justify-content:flex-end;gap:.5rem;",
                                    button {
                                        class: "button",
                                        disabled: mutation_busy(),
                                        onclick: move |_| discard_target.set(None),
                                        "Cancel"
                                    }
                                    button {
                                        class: "button",
                                        disabled: mutation_busy(),
                                        onclick: move |_| {
                                            let service = discard_service.clone();
                                            let repo = discard_root.clone();
                                            let target = action_target.clone();
                                            mutation_busy.set(true);
                                            error.set(None);
                                            spawn(async move {
                                                let result = match target {
                                                    DiscardTarget::Path(path) => service
                                                        .discard_path(Path::new(&repo), Path::new(&path))
                                                        .await,
                                                    DiscardTarget::All => service.discard_all(Path::new(&repo)).await,
                                                };
                                                match result {
                                                    Ok(()) => {
                                                        selected_path.set(None);
                                                        staged_view.set(false);
                                                        diff.set(String::new());
                                                        discard_target.set(None);
                                                        refresh.set(refresh() + 1);
                                                    }
                                                    Err(next_error) => error.set(Some(next_error.to_string())),
                                                }
                                                mutation_busy.set(false);
                                            });
                                        },
                                        "Discard changes"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
'''
review = review[: -len(suffix)] + modal
review_path.write_text(review, encoding="utf-8")

Path("crates/hermes-desktop/tests/git_discard_ui_contract.rs").write_text(
    r'''#[test]
fn review_discard_is_typed_confirmed_and_platform_neutral() {
    let review = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../hermes-ui/src/review.rs"));
    let main = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../apps/desktop/src/main.rs"));
    let native = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../apps/desktop/src/git_discard_service.rs"));

    assert!(review.contains("services.git_discard.clone()"));
    assert!(review.contains("DiscardTarget::Path"));
    assert!(review.contains("DiscardTarget::All"));
    assert!(review.contains("Discard file"));
    assert!(review.contains("Discard all"));
    assert!(review.contains("Discard changes?"));
    assert!(review.contains("This cannot be undone. Ignored files are preserved."));
    assert!(!review.contains("Command::new"));
    assert!(!review.contains("std::process"));
    assert!(main.contains("git_discard_service::install(&mut native.services)"));
    assert!(native.contains("services.git_discard = Arc::new(GitDiscardService)"));
}
''',
    encoding="utf-8",
)
