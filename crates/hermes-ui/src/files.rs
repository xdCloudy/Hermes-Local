use std::path::Path;

use dioxus::prelude::*;
use futures_util::StreamExt;
use hermes_core::{AppServices, PreviewDocument, PreviewDocumentKind, ServiceError};
use hermes_protocol::{FileEntry, ProjectsSnapshot};

use super::{ProjectUiState, Surface};

fn active_project_root(snapshot: &ProjectsSnapshot) -> Option<(String, String)> {
    let active_id = snapshot.active_id.as_deref()?;
    let project = snapshot
        .projects
        .iter()
        .find(|project| project.id == active_id)?;
    let folder = project
        .folders
        .iter()
        .find(|folder| folder.is_primary)
        .or_else(|| project.folders.first())?;
    Some((project.name.clone(), folder.path.clone()))
}

fn parent_dir(path: &str) -> String {
    Path::new(path)
        .parent()
        .map(|parent| parent.to_string_lossy().replace('\\', "/"))
        .filter(|parent| parent != ".")
        .unwrap_or_default()
}

fn file_size(entry: &FileEntry) -> String {
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
pub(super) fn SafePreview(document: PreviewDocument) -> Element {
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
                        "sandbox": "allow-scripts",
                        "loading": "lazy",
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

#[component]
pub(super) fn Files() -> Element {
    let services = use_context::<AppServices>();
    let project_state = use_context::<ProjectUiState>();
    let mut current_dir = use_signal(String::new);
    let mut entries = use_signal(Vec::<FileEntry>::new);
    let mut selected_path = use_signal(|| None::<String>);
    let mut editor_text = use_signal(String::new);
    let mut action_target = use_signal(|| None::<FileEntry>);
    let mut rename_name = use_signal(String::new);
    let mut delete_confirm = use_signal(|| false);
    let mut list_loading = use_signal(|| false);
    let mut editor_loading = use_signal(|| false);
    let mut saving = use_signal(|| false);
    let mut action_busy = use_signal(|| false);
    let mut dirty = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut message = use_signal(|| None::<String>);
    let mut refresh = use_signal(|| 0_u64);
    let mut preview_mode = use_signal(|| false);
    let mut preview_input = use_signal(String::new);
    let mut preview_target = use_signal(String::new);
    let mut preview_document = use_signal(|| None::<PreviewDocument>);
    let mut preview_loading = use_signal(|| false);
    let mut preview_error = use_signal(|| None::<String>);
    let mut preview_revision = use_signal(|| 0_u64);

    let snapshot_signal = project_state.snapshot;
    let list_service = services.files.clone();
    let _listing = use_resource(move || {
        let snapshot = snapshot_signal();
        let root = active_project_root(&snapshot).map(|(_, root)| root);
        let directory = current_dir();
        let _revision = refresh();
        let service = list_service.clone();
        async move {
            let Some(root) = root else {
                entries.set(Vec::new());
                list_loading.set(false);
                return;
            };
            list_loading.set(true);
            match service
                .read_dir(Path::new(&root), Path::new(&directory))
                .await
            {
                Ok(mut rows) => {
                    rows.sort_by_key(|entry| (!entry.is_dir, entry.name.to_lowercase()));
                    entries.set(rows);
                    error.set(None);
                }
                Err(next_error) => error.set(Some(next_error.to_string())),
            }
            list_loading.set(false);
        }
    });

    let watch_service = services.files.clone();
    let watch_snapshot = project_state.snapshot;
    let _watching = use_resource(move || {
        let snapshot = watch_snapshot();
        let root = active_project_root(&snapshot).map(|(_, root)| root);
        let directory = current_dir();
        let service = watch_service.clone();
        async move {
            let Some(root) = root else {
                return;
            };
            match service.watch_directory(Path::new(&root), Path::new(&directory)) {
                Ok(mut events) => {
                    while events.next().await.is_some() {
                        refresh.set(refresh() + 1);
                    }
                }
                Err(ServiceError::Unavailable(_)) => {}
                Err(next_error) => error.set(Some(next_error.to_string())),
            }
        }
    });

    let preview_service = services.preview.clone();
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
                    preview_error.set(Some(
                        "Preview target was not found or is unsupported.".into(),
                    ));
                }
                Err(next_error) => {
                    preview_document.set(None);
                    preview_error.set(Some(next_error.to_string()));
                }
            }
            preview_loading.set(false);
        }
    });

    let snapshot = (project_state.snapshot)();
    let active = active_project_root(&snapshot);
    let project_name = active
        .as_ref()
        .map(|(name, _)| name.as_str())
        .unwrap_or("No active project");
    let root = active.as_ref().map(|(_, root)| root.clone());
    let directory_label = if current_dir().is_empty() {
        "/".to_owned()
    } else {
        format!("/{}", current_dir())
    };

    rsx! {
        Surface {
            eyebrow: "Workspace",
            title: "Files",
            subtitle: "Browse and edit files inside the active project root.",
            if root.is_none() {
                section { class: "settings-empty",
                    h2 { "Select a project first" }
                    p { "Files are scoped to the active Project Centre root." }
                }
            } else {
                div { style: "display:grid;grid-template-columns:minmax(15rem,0.8fr) minmax(22rem,1.6fr);gap:1rem;min-height:32rem;",
                    section { class: "settings-card", style: "min-width:0;",
                        header { style: "display:flex;align-items:center;gap:.5rem;margin-bottom:.75rem;",
                            div { style: "min-width:0;flex:1;",
                                strong { "{project_name}" }
                                div { class: "muted", title: "{root.clone().unwrap_or_default()}", "{directory_label}" }
                            }
                            button {
                                class: "icon-button",
                                title: "Up one folder",
                                aria_label: "Up one folder",
                                disabled: current_dir().is_empty() || list_loading(),
                                onclick: move |_| {
                                    current_dir.set(parent_dir(&current_dir()));
                                    selected_path.set(None);
                                    action_target.set(None);
                                    delete_confirm.set(false);
                                    editor_text.set(String::new());
                                    dirty.set(false);
                                    message.set(None);
                                },
                                "↑"
                            }
                            button {
                                class: "icon-button",
                                title: "Refresh files",
                                aria_label: "Refresh files",
                                disabled: list_loading(),
                                onclick: move |_| refresh.set(refresh() + 1),
                                "↻"
                            }
                        }
                        if list_loading() {
                            p { class: "muted", "Loading files…" }
                        } else if entries().is_empty() {
                            p { class: "muted", "This folder is empty." }
                        } else {
                            div { style: "display:flex;flex-direction:column;gap:.2rem;max-height:31rem;overflow:auto;",
                                for entry in entries() {
                                    {
                                        let entry_path = entry.path.clone();
                                        let entry_name = entry.name.clone();
                                        let action_entry = entry.clone();
                                        let is_dir = entry.is_dir;
                                        let size = file_size(&entry);
                                        let root_for_open = root.clone().unwrap_or_default();
                                        let file_service = services.files.clone();
                                        rsx! {
                                            div {
                                                key: "{entry_path}",
                                                style: "display:grid;grid-template-columns:minmax(0,1fr) auto;gap:.25rem;",
                                                button {
                                                    class: "button",
                                                    style: "display:grid;grid-template-columns:auto minmax(0,1fr) auto;align-items:center;gap:.5rem;text-align:left;width:100%;",
                                                    title: "{entry_path}",
                                                    onclick: move |_| {
                                                        if is_dir {
                                                            current_dir.set(entry_path.clone());
                                                            selected_path.set(None);
                                                            action_target.set(None);
                                                            delete_confirm.set(false);
                                                            editor_text.set(String::new());
                                                            dirty.set(false);
                                                            message.set(None);
                                                            error.set(None);
                                                            return;
                                                        }
                                                        let service = file_service.clone();
                                                        let root = root_for_open.clone();
                                                        let path = entry_path.clone();
                                                        editor_loading.set(true);
                                                        message.set(None);
                                                        error.set(None);
                                                        spawn(async move {
                                                            match service.read_text(Path::new(&root), Path::new(&path)).await {
                                                                Ok(text) => {
                                                                    preview_input.set(path.clone());
                                                                    preview_target.set(path.clone());
                                                                    selected_path.set(Some(path));
                                                                    editor_text.set(text);
                                                                    dirty.set(false);
                                                                    error.set(None);
                                                                }
                                                                Err(next_error) => error.set(Some(next_error.to_string())),
                                                            }
                                                            editor_loading.set(false);
                                                        });
                                                    },
                                                    span { aria_hidden: "true", if is_dir { "▸" } else { "·" } }
                                                    span { style: "overflow:hidden;text-overflow:ellipsis;white-space:nowrap;", "{entry_name}" }
                                                    span { class: "muted", "{size}" }
                                                }
                                                button {
                                                    class: "icon-button",
                                                    title: "File actions",
                                                    aria_label: "Actions for {action_entry.name}",
                                                    onclick: move |_| {
                                                        rename_name.set(action_entry.name.clone());
                                                        action_target.set(Some(action_entry.clone()));
                                                        delete_confirm.set(false);
                                                        message.set(None);
                                                        error.set(None);
                                                    },
                                                    "⋯"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        if let Some(target) = action_target() {
                            {
                                let target_path = target.path.clone();
                                let target_name = target.name.clone();
                                let target_is_open_dirty = selected_path().as_deref() == Some(target_path.as_str()) && dirty();
                                let root_for_actions = root.clone().unwrap_or_default();
                                let rename_service = services.files.clone();
                                let reveal_service = services.files.clone();
                                let open_service = services.files.clone();
                                let trash_service = services.files.clone();
                                rsx! {
                                    div { class: "settings-list", style: "margin-top:.75rem;padding-top:.75rem;border-top:1px solid var(--border-subtle,rgba(127,127,127,.2));",
                                        div {
                                            strong { "{target_name}" }
                                            div { class: "muted", title: "{target_path}", "{target_path}" }
                                        }
                                        if target_is_open_dirty {
                                            p { class: "muted", "Save the open editor changes before renaming or deleting this file." }
                                        }
                                        div { style: "display:flex;gap:.4rem;flex-wrap:wrap;align-items:center;",
                                            input {
                                                class: "settings-input",
                                                style: "min-width:10rem;flex:1;",
                                                aria_label: "Rename file or folder",
                                                disabled: action_busy(),
                                                value: "{rename_name}",
                                                oninput: move |event| rename_name.set(event.value())
                                            }
                                            button {
                                                class: "button",
                                                disabled: action_busy()
                                                    || target_is_open_dirty
                                                    || rename_name().trim().is_empty()
                                                    || rename_name().trim() == target_name,
                                                onclick: {
                                                    let root = root_for_actions.clone();
                                                    let path = target_path.clone();
                                                    let old_path = target_path.clone();
                                                    let service = rename_service.clone();
                                                    move |_| {
                                                        let root = root.clone();
                                                        let path = path.clone();
                                                        let old_path = old_path.clone();
                                                        let name = rename_name();
                                                        let service = service.clone();
                                                        action_busy.set(true);
                                                        message.set(None);
                                                        error.set(None);
                                                        spawn(async move {
                                                            match service.rename(Path::new(&root), Path::new(&path), &name).await {
                                                                Ok(new_path) => {
                                                                    if selected_path().as_deref() == Some(old_path.as_str()) {
                                                                        selected_path.set(Some(new_path));
                                                                    }
                                                                    action_target.set(None);
                                                                    delete_confirm.set(false);
                                                                    refresh.set(refresh() + 1);
                                                                    message.set(Some("Renamed.".into()));
                                                                }
                                                                Err(next_error) => error.set(Some(next_error.to_string())),
                                                            }
                                                            action_busy.set(false);
                                                        });
                                                    }
                                                },
                                                "Rename"
                                            }
                                            button {
                                                class: "button",
                                                disabled: action_busy(),
                                                onclick: {
                                                    let root = root_for_actions.clone();
                                                    let path = target_path.clone();
                                                    let service = reveal_service.clone();
                                                    move |_| {
                                                        let root = root.clone();
                                                        let path = path.clone();
                                                        let service = service.clone();
                                                        action_busy.set(true);
                                                        message.set(None);
                                                        error.set(None);
                                                        spawn(async move {
                                                            match service.reveal(Path::new(&root), Path::new(&path)).await {
                                                                Ok(()) => message.set(Some("Revealed in file manager.".into())),
                                                                Err(next_error) => error.set(Some(next_error.to_string())),
                                                            }
                                                            action_busy.set(false);
                                                        });
                                                    }
                                                },
                                                "Reveal"
                                            }
                                            button {
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
                                                    let service = open_service.clone();
                                                    move |_| {
                                                        let root = root.clone();
                                                        let path = path.clone();
                                                        let service = service.clone();
                                                        action_busy.set(true);
                                                        message.set(None);
                                                        error.set(None);
                                                        spawn(async move {
                                                            match service.open(Path::new(&root), Path::new(&path)).await {
                                                                Ok(()) => message.set(Some("Opened with the system handler.".into())),
                                                                Err(next_error) => error.set(Some(next_error.to_string())),
                                                            }
                                                            action_busy.set(false);
                                                        });
                                                    }
                                                },
                                                "Open"
                                            }
                                            if !delete_confirm() {
                                                button {
                                                    class: "button",
                                                    disabled: action_busy() || target_is_open_dirty,
                                                    onclick: move |_| delete_confirm.set(true),
                                                    "Delete…"
                                                }
                                            }
                                        }
                                        if delete_confirm() {
                                            div { class: "settings-error",
                                                strong { "Move {target_name} to the OS trash?" }
                                                p { "This uses the recoverable system trash instead of permanently deleting the item." }
                                                div { style: "display:flex;gap:.4rem;",
                                                    button {
                                                        class: "button",
                                                        disabled: action_busy(),
                                                        onclick: move |_| delete_confirm.set(false),
                                                        "Cancel"
                                                    }
                                                    button {
                                                        class: "button",
                                                        disabled: action_busy(),
                                                        onclick: {
                                                            let root = root_for_actions.clone();
                                                            let path = target_path.clone();
                                                            let deleted_path = target_path.clone();
                                                            let service = trash_service.clone();
                                                            move |_| {
                                                                let root = root.clone();
                                                                let path = path.clone();
                                                                let deleted_path = deleted_path.clone();
                                                                let service = service.clone();
                                                                action_busy.set(true);
                                                                message.set(None);
                                                                error.set(None);
                                                                spawn(async move {
                                                                    match service.trash(Path::new(&root), Path::new(&path)).await {
                                                                        Ok(()) => {
                                                                            if selected_path().as_deref() == Some(deleted_path.as_str()) {
                                                                                selected_path.set(None);
                                                                                editor_text.set(String::new());
                                                                                dirty.set(false);
                                                                            }
                                                                            action_target.set(None);
                                                                            delete_confirm.set(false);
                                                                            refresh.set(refresh() + 1);
                                                                            message.set(Some("Moved to trash.".into()));
                                                                        }
                                                                        Err(next_error) => error.set(Some(next_error.to_string())),
                                                                    }
                                                                    action_busy.set(false);
                                                                });
                                                            }
                                                        },
                                                        "Delete"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    section { class: "settings-card", style: "min-width:0;display:flex;flex-direction:column;",
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
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hermes_protocol::{ProjectFolder, ProjectSummary};

    #[test]
    fn active_root_prefers_primary_folder() {
        let snapshot = ProjectsSnapshot {
            active_id: Some("p1".into()),
            projects: vec![ProjectSummary {
                id: "p1".into(),
                name: "Demo".into(),
                folders: vec![
                    ProjectFolder {
                        path: "secondary".into(),
                        ..ProjectFolder::default()
                    },
                    ProjectFolder {
                        path: "primary".into(),
                        is_primary: true,
                        ..ProjectFolder::default()
                    },
                ],
                ..ProjectSummary::default()
            }],
            ..ProjectsSnapshot::default()
        };

        assert_eq!(
            active_project_root(&snapshot),
            Some(("Demo".into(), "primary".into()))
        );
    }

    #[test]
    fn parent_navigation_never_returns_dot() {
        assert_eq!(parent_dir("src/app"), "src");
        assert_eq!(parent_dir("src"), "");
        assert_eq!(parent_dir(""), "");
    }
}
