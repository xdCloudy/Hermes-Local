from pathlib import Path


def replace_once(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one match, found {count}: {old[:140]!r}")
    path.write_text(text.replace(old, new), encoding="utf-8")


core = Path("crates/hermes-core/src/lib.rs")
replace_once(
    core,
    "pub trait FileService: Send + Sync {\n",
    '''#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
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

pub trait FileService: Send + Sync {
''',
)
replace_once(
    core,
    "    pub trust: Arc<dyn TrustService>,\n    pub files: Arc<dyn FileService>,",
    "    pub trust: Arc<dyn TrustService>,\n    pub preview: Arc<dyn PreviewService>,\n    pub files: Arc<dyn FileService>,",
)

native = Path("crates/hermes-desktop/src/lib.rs")
replace_once(
    native,
    "    ModelService, PlatformService, ProjectService, ProviderService, RuntimeService, ServiceError,\n    ServiceFuture, ServiceResult, SessionService, SettingsService, TerminalService, TrustService,\n    UpdateService, validate_identifier, validate_relative_path,",
    "    ModelService, PlatformService, ProjectService, ProviderService, RuntimeService, ServiceError,\n    ServiceFuture, ServiceResult, SessionService, SettingsService, TerminalService, TrustService,\n    UnavailablePreviewService, UpdateService, validate_identifier, validate_relative_path,",
)
replace_once(
    native,
    "                runtime: remote.clone(),\n                trust: remote,\n                files: Arc::new(DesktopFiles),",
    "                runtime: remote.clone(),\n                trust: remote,\n                preview: Arc::new(UnavailablePreviewService),\n                files: Arc::new(DesktopFiles),",
)

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

normalizer = Path("apps/desktop/src/preview_normalization.rs")
replace_once(
    normalizer,
    '#![allow(dead_code)] // PF-08 service foundation; preview UI wiring is a later stage.\n\n',
    "",
)
replace_once(
    normalizer,
    "const TEXT_PREVIEW_MAX_BYTES: u64 = 512 * 1024;",
    "pub(crate) const TEXT_PREVIEW_MAX_BYTES: u64 = 512 * 1024;",
)
replace_once(
    normalizer,
    '''    if !matches!(url.scheme(), "http" | "https") {
        return Err("Preview URL must use HTTP or HTTPS.".to_owned());
    }
    if url.host_str() == Some("0.0.0.0") {''',
    '''    if !matches!(url.scheme(), "http" | "https") {
        return Err("Preview URL must use HTTP or HTTPS.".to_owned());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("Preview URLs with embedded credentials are not allowed.".to_owned());
    }
    if url.host_str() == Some("0.0.0.0") {''',
)
replace_once(
    normalizer,
    '''    // Electron performs an explicit readability probe before returning the
    // target. Opening once here provides the same fail-closed contract.
    let mut file = fs::File::open(&resolved)
        .map_err(|error| format!("Preview target is not readable: {error}"))?;

    let extension = extension_key(&resolved);''',
    '''    // Electron performs an explicit readability probe before returning the
    // target. Open the canonical target so the returned path and the sampled
    // file are the same object after the sensitive-path recheck above.
    let mut file = fs::File::open(&real_path)
        .map_err(|error| format!("Preview target is not readable: {error}"))?;

    let extension = extension_key(&real_path);''',
)
replace_once(
    normalizer,
    '''    let url = Url::from_file_path(&resolved)
        .map_err(|_| "Could not convert preview file path to a file URL.".to_owned())?
        .to_string();

    Ok(Some(PreviewTarget::File {
        binary,
        byte_size,
        large: byte_size > TEXT_PREVIEW_MAX_BYTES,
        label: resolved
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_owned(),
        language: language_for_extension(&extension).to_owned(),
        mime_type,
        path: resolved,''',
    '''    let url = Url::from_file_path(&real_path)
        .map_err(|_| "Could not convert preview file path to a file URL.".to_owned())?
        .to_string();

    Ok(Some(PreviewTarget::File {
        binary,
        byte_size,
        large: byte_size > TEXT_PREVIEW_MAX_BYTES,
        label: real_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_owned(),
        language: language_for_extension(&extension).to_owned(),
        mime_type,
        path: real_path,''',
)
replace_once(
    normalizer,
    '''        assert!(
            PreviewNormalizationService
                .normalize("ftp://example.com/file", None)
                .expect("non-http target")
                .is_none()
        );''',
    '''        assert!(
            PreviewNormalizationService
                .normalize("ftp://example.com/file", None)
                .expect("non-http target")
                .is_none()
        );
        assert!(
            PreviewNormalizationService
                .normalize("https://user:secret@example.com/private", None)
                .is_err()
        );''',
)

ui = Path("crates/hermes-ui/src/files.rs")
replace_once(
    ui,
    "use hermes_core::{AppServices, ServiceError};",
    "use hermes_core::{AppServices, PreviewDocument, PreviewDocumentKind, ServiceError};",
)
replace_once(
    ui,
    '''fn file_size(entry: &FileEntry) -> String {
    let Some(bytes) = entry.size else {
        return String::new();
    };
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    if bytes < 1024 * 1024 {
        return format!("{:.1} KB", bytes as f64 / 1024.0);
    }
    format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
}
''',
    '''fn file_size(entry: &FileEntry) -> String {
    let Some(bytes) = entry.size else {
        return String::new();
    };
    format_bytes(bytes)
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    if bytes < 1024 * 1024 {
        return format!("{:.1} KB", bytes as f64 / 1024.0);
    }
    format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
}

#[component]
fn SafePreview(document: PreviewDocument) -> Element {
    let size = document.byte_size.map(format_bytes);
    rsx! {
        div { style: "display:flex;flex-direction:column;gap:.75rem;min-height:0;flex:1;",
            div { class: "settings-list",
                div {
                    strong { "{document.label}" }
                    div { class: "muted", title: "{document.source}", "{document.source}" }
                }
                div { style: "display:flex;gap:.5rem;flex-wrap:wrap;",
                    if let Some(mime) = document.mime_type.as_deref() { span { class: "muted", "{mime}" } }
                    if let Some(language) = document.language.as_deref() { span { class: "muted", "{language}" } }
                    if let Some(size) = size.as_deref() { span { class: "muted", "{size}" } }
                }
            }
            match document.kind {
                PreviewDocumentKind::Url => rsx! {
                    iframe {
                        title: "Preview of {document.label}",
                        src: "{document.url}",
                        sandbox: "allow-scripts",
                        loading: "lazy",
                        style: "width:100%;min-height:25rem;flex:1;border:1px solid var(--border-subtle,rgba(127,127,127,.2));border-radius:.5rem;background:white;",
                    }
                },
                PreviewDocumentKind::Image => rsx! {
                    div { style: "display:grid;place-items:center;min-height:20rem;overflow:auto;",
                        img {
                            src: "{document.url}",
                            alt: "Preview of {document.label}",
                            style: "max-width:100%;max-height:32rem;object-fit:contain;",
                        }
                    }
                },
                PreviewDocumentKind::Binary => rsx! {
                    div { class: "settings-empty",
                        h2 { "Binary file" }
                        p { "Binary contents are intentionally not copied into the Dioxus UI." }
                    }
                },
                PreviewDocumentKind::Html | PreviewDocumentKind::Text if document.large => rsx! {
                    div { class: "settings-empty",
                        h2 { "Preview too large" }
                        p { "Inline text previews are limited to 512 KiB." }
                    }
                },
                PreviewDocumentKind::Html | PreviewDocumentKind::Text => rsx! {
                    pre {
                        style: "margin:0;min-height:20rem;max-height:32rem;overflow:auto;white-space:pre-wrap;word-break:break-word;font-family:var(--font-mono,monospace);font-size:.78rem;line-height:1.5;padding:.75rem;border:1px solid var(--border-subtle,rgba(127,127,127,.2));border-radius:.5rem;",
                        "{document.text.as_deref().unwrap_or_default()}"
                    }
                },
            }
        }
    }
}
''',
)
replace_once(
    ui,
    '''    let mut dirty = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut message = use_signal(|| None::<String>);
    let mut refresh = use_signal(|| 0_u64);''',
    '''    let mut dirty = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut message = use_signal(|| None::<String>);
    let mut refresh = use_signal(|| 0_u64);
    let mut preview_mode = use_signal(|| false);
    let mut preview_input = use_signal(String::new);
    let mut preview_target = use_signal(String::new);
    let mut preview_document = use_signal(|| None::<PreviewDocument>);
    let mut preview_loading = use_signal(|| false);
    let mut preview_error = use_signal(|| None::<String>);
    let mut preview_revision = use_signal(|| 0_u64);''',
)
watch_marker = '''    let snapshot = (project_state.snapshot)();
'''
preview_resource = '''    let preview_service = services.preview.clone();
    let preview_snapshot = project_state.snapshot;
    let _previewing = use_resource(move || {
        let enabled = preview_mode();
        let target = preview_target();
        let _revision = preview_revision();
        let _external_revision = refresh();
        let snapshot = preview_snapshot();
        let base = active_project_root(&snapshot).map(|(_, root)| root);
        let service = preview_service.clone();
        async move {
            if !enabled || target.trim().is_empty() {
                preview_document.set(None);
                preview_error.set(None);
                preview_loading.set(false);
                return;
            }
            preview_loading.set(true);
            match service.load(&target, base.as_deref().map(Path::new)).await {
                Ok(Some(document)) => {
                    preview_document.set(Some(document));
                    preview_error.set(None);
                }
                Ok(None) => {
                    preview_document.set(None);
                    preview_error.set(Some("Preview target was not found or is unsupported.".into()));
                }
                Err(next_error) => {
                    preview_document.set(None);
                    preview_error.set(Some(next_error.to_string()));
                }
            }
            preview_loading.set(false);
        }
    });

'''
text = ui.read_text(encoding="utf-8")
if text.count(watch_marker) != 1:
    raise SystemExit("Files snapshot marker changed")
ui.write_text(text.replace(watch_marker, preview_resource + watch_marker), encoding="utf-8")

# Keep the preview target synchronized with files that successfully open in the editor.
text = ui.read_text(encoding="utf-8")
old = '''                                                                Ok(text) => {
                                                                    selected_path.set(Some(path));
                                                                    editor_text.set(text);'''
new = '''                                                                Ok(text) => {
                                                                    preview_input.set(path.clone());
                                                                    preview_target.set(path.clone());
                                                                    selected_path.set(Some(path));
                                                                    editor_text.set(text);'''
if text.count(old) != 1:
    raise SystemExit("editor-open success marker changed")
ui.write_text(text.replace(old, new), encoding="utf-8")

# Add Preview to the existing per-item action card, including binary files that cannot open in the editor.
text = ui.read_text(encoding="utf-8")
old = '''                                            button {
                                                class: "button",
                                                disabled: action_busy(),
                                                onclick: {
                                                    let root = root_for_actions.clone();
                                                    let path = target_path.clone();
                                                    let service = open_service.clone();'''
new = '''                                            button {
                                                class: "button",
                                                disabled: action_busy(),
                                                onclick: {
                                                    let path = target_path.clone();
                                                    move |_| {
                                                        preview_input.set(path.clone());
                                                        preview_target.set(path.clone());
                                                        preview_mode.set(true);
                                                        preview_error.set(None);
                                                        preview_revision.set(preview_revision() + 1);
                                                    }
                                                },
                                                "Preview"
                                            }
                                            button {
                                                class: "button",
                                                disabled: action_busy(),
                                                onclick: {
                                                    let root = root_for_actions.clone();
                                                    let path = target_path.clone();
                                                    let service = open_service.clone();'''
if text.count(old) != 1:
    raise SystemExit("open action marker changed")
ui.write_text(text.replace(old, new), encoding="utf-8")

# Replace the editor card header/body with a mode-aware Editor / safe Preview surface.
text = ui.read_text(encoding="utf-8")
start_marker = '''                    section { class: "settings-card", style: "min-width:0;display:flex;flex-direction:column;",
                        header { style: "display:flex;align-items:center;gap:.75rem;margin-bottom:.75rem;",
'''
start = text.find(start_marker)
if start == -1:
    raise SystemExit("editor card start marker changed")
end_marker = '''                    }
                }
            }
        }
    }
}
'''
end = text.find(end_marker, start)
if end == -1:
    raise SystemExit("editor card end marker changed")
replacement = '''                    section { class: "settings-card", style: "min-width:0;display:flex;flex-direction:column;",
                        header { style: "display:flex;align-items:center;gap:.75rem;margin-bottom:.75rem;",
                            div { style: "min-width:0;flex:1;",
                                strong { if preview_mode() { "Safe preview" } else { "Editor" } }
                                if preview_mode() {
                                    div { class: "muted", "Native-normalized targets only; remote pages are sandboxed." }
                                } else if let Some(path) = selected_path() {
                                    div { class: "muted", title: "{path}", "{path}" }
                                } else {
                                    div { class: "muted", "Open a text file from the tree." }
                                }
                            }
                            div { style: "display:flex;gap:.35rem;",
                                button {
                                    class: "button",
                                    disabled: !preview_mode(),
                                    onclick: move |_| preview_mode.set(false),
                                    "Editor"
                                }
                                button {
                                    class: "button",
                                    disabled: preview_mode(),
                                    onclick: move |_| {
                                        if preview_input().trim().is_empty()
                                            && let Some(path) = selected_path()
                                        {
                                            preview_input.set(path.clone());
                                            preview_target.set(path);
                                        }
                                        preview_mode.set(true);
                                        preview_revision.set(preview_revision() + 1);
                                    },
                                    "Preview"
                                }
                                if !preview_mode() {
                                    button {
                                        class: "button",
                                        disabled: selected_path().is_none() || !dirty() || saving() || editor_loading(),
                                        onclick: {
                                            let file_service = services.files.clone();
                                            let root_for_save = root.clone().unwrap_or_default();
                                            move |_| {
                                                let Some(path) = selected_path() else { return; };
                                                let content = editor_text();
                                                let service = file_service.clone();
                                                let root = root_for_save.clone();
                                                saving.set(true);
                                                message.set(None);
                                                error.set(None);
                                                spawn(async move {
                                                    match service.write_text(Path::new(&root), Path::new(&path), &content).await {
                                                        Ok(()) => {
                                                            dirty.set(false);
                                                            message.set(Some("Saved.".into()));
                                                            preview_revision.set(preview_revision() + 1);
                                                        }
                                                        Err(next_error) => error.set(Some(next_error.to_string())),
                                                    }
                                                    saving.set(false);
                                                });
                                            }
                                        },
                                        if saving() { "Saving…" } else { "Save" }
                                    }
                                }
                            }
                        }
                        if preview_mode() {
                            div { style: "display:flex;gap:.4rem;margin-bottom:.75rem;",
                                input {
                                    class: "settings-input",
                                    style: "min-width:0;flex:1;",
                                    aria_label: "Preview target",
                                    value: "{preview_input}",
                                    placeholder: "Relative file path, file URL, or https:// URL",
                                    oninput: move |event| preview_input.set(event.value())
                                }
                                button {
                                    class: "button",
                                    disabled: preview_loading() || preview_input().trim().is_empty(),
                                    onclick: move |_| {
                                        preview_target.set(preview_input());
                                        preview_error.set(None);
                                        preview_revision.set(preview_revision() + 1);
                                    },
                                    if preview_loading() { "Loading…" } else { "Load" }
                                }
                            }
                            if let Some(next_error) = preview_error() {
                                div { class: "settings-error", role: "alert", "{next_error}" }
                            } else if preview_loading() {
                                p { class: "muted", "Normalizing preview target…" }
                            } else if let Some(document) = preview_document() {
                                SafePreview { document }
                            } else {
                                div { class: "settings-empty",
                                    h2 { "No preview loaded" }
                                    p { "Select a file or enter an HTTP(S) target. Sensitive local paths fail closed." }
                                }
                            }
                        } else {
                            if let Some(next_error) = error() {
                                div { class: "settings-error", role: "alert", "{next_error}" }
                            }
                            if let Some(next_message) = message() {
                                div { class: "settings-success", role: "status", "{next_message}" }
                            }
                            if editor_loading() {
                                p { class: "muted", "Loading file…" }
                            }
                            textarea {
                                class: "settings-input",
                                style: "width:100%;min-height:28rem;flex:1;resize:vertical;font-family:var(--font-mono,monospace);",
                                aria_label: "File editor",
                                disabled: selected_path().is_none() || editor_loading() || saving(),
                                value: "{editor_text}",
                                placeholder: "Select a UTF-8 text file to edit.",
                                oninput: move |event| {
                                    editor_text.set(event.value());
                                    dirty.set(true);
                                    message.set(None);
                                }
                            }
                        }
                    }
'''
text = text[:start] + replacement + text[end:]
ui.write_text(text, encoding="utf-8")

contract = Path("crates/hermes-desktop/tests/preview_ui_contract.rs")
contract.write_text(r'''use std::{fs, path::PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read_repo_file(relative: &str) -> String {
    let path = repo_root().join(relative);
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

#[test]
fn safe_preview_stays_behind_a_typed_native_service() {
    let core = read_repo_file("crates/hermes-core/src/lib.rs");
    let desktop = read_repo_file("apps/desktop/src/preview_service.rs");
    let main = read_repo_file("apps/desktop/src/main.rs");
    let ui = read_repo_file("crates/hermes-ui/src/files.rs");

    assert!(core.contains("pub trait PreviewService: Send + Sync"));
    assert!(core.contains("pub preview: Arc<dyn PreviewService>"));
    assert!(desktop.contains("impl PreviewService for DesktopPreviewService"));
    assert!(desktop.contains("read_bounded_text(&path)"));
    assert!(main.contains("preview_service::install(&mut native.services);"));
    assert!(ui.contains("service.load(&target, base.as_deref().map(Path::new)).await"));
    assert!(!ui.contains("std::fs") && !ui.contains("File::open") && !ui.contains("canonicalize()"));
}

#[test]
fn remote_preview_is_sandboxed_and_local_html_is_never_injected() {
    let ui = read_repo_file("crates/hermes-ui/src/files.rs");
    assert!(ui.contains("sandbox: \"allow-scripts\""));
    assert!(!ui.contains("allow-top-navigation"));
    assert!(!ui.contains("allow-popups"));
    assert!(!ui.contains("allow-forms"));
    assert!(!ui.contains("allow-same-origin"));
    assert!(!ui.contains("dangerous_inner_html"));
    assert!(ui.contains("PreviewDocumentKind::Html | PreviewDocumentKind::Text"));
}

#[test]
fn preview_contract_keeps_size_and_sensitive_path_guards() {
    let normalization = read_repo_file("apps/desktop/src/preview_normalization.rs");
    let desktop = read_repo_file("apps/desktop/src/preview_service.rs");
    assert!(normalization.contains("TEXT_PREVIEW_MAX_BYTES: u64 = 512 * 1024"));
    assert!(normalization.contains("reject_sensitive_file_path(&real_path)?"));
    assert!(normalization.contains("Preview URLs with embedded credentials are not allowed."));
    assert!(desktop.contains("TEXT_PREVIEW_MAX_BYTES + 1"));
    assert!(desktop.contains("grew beyond the 512 KiB inline limit"));
}
''', encoding="utf-8")
