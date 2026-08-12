use std::path::Path;

use dioxus::prelude::*;
use hermes_core::AppServices;
use hermes_protocol::{GitChange, GitStatus, ProjectsSnapshot};

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

fn changes(status: &GitStatus) -> Vec<GitChange> {
    if !status.entries.is_empty() {
        return status.entries.clone();
    }
    status
        .changed
        .iter()
        .map(|path| GitChange {
            path: path.clone(),
            unstaged: true,
            ..GitChange::default()
        })
        .collect()
}

#[component]
pub(super) fn Review() -> Element {
    let services = use_context::<AppServices>();
    let project_state = use_context::<ProjectUiState>();
    let mut status = use_signal(GitStatus::default);
    let mut selected_path = use_signal(|| None::<String>);
    let mut staged_view = use_signal(|| false);
    let mut diff = use_signal(String::new);
    let mut status_loading = use_signal(|| false);
    let mut diff_loading = use_signal(|| false);
    let mut mutation_busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut refresh = use_signal(|| 0_u64);

    let status_service = services.git.clone();
    let status_snapshot = project_state.snapshot;
    let _status_resource = use_resource(move || {
        let snapshot = status_snapshot();
        let root = active_project_root(&snapshot).map(|(_, root)| root);
        let _revision = refresh();
        let service = status_service.clone();
        async move {
            let Some(root) = root else {
                status.set(GitStatus::default());
                selected_path.set(None);
                diff.set(String::new());
                status_loading.set(false);
                return;
            };
            status_loading.set(true);
            match service.status(Path::new(&root)).await {
                Ok(next_status) => {
                    let rows = changes(&next_status);
                    if let Some(selected) = selected_path()
                        && !rows.iter().any(|change| change.path == selected)
                    {
                        selected_path.set(None);
                        diff.set(String::new());
                    }
                    status.set(next_status);
                    error.set(None);
                }
                Err(next_error) => {
                    status.set(GitStatus::default());
                    error.set(Some(next_error.to_string()));
                }
            }
            status_loading.set(false);
        }
    });

    let diff_service = services.git.clone();
    let diff_snapshot = project_state.snapshot;
    let _diff_resource = use_resource(move || {
        let snapshot = diff_snapshot();
        let root = active_project_root(&snapshot).map(|(_, root)| root);
        let path = selected_path();
        let staged = staged_view();
        let _revision = refresh();
        let service = diff_service.clone();
        async move {
            let (Some(root), Some(path)) = (root, path) else {
                diff.set(String::new());
                diff_loading.set(false);
                return;
            };
            diff_loading.set(true);
            let result = if staged {
                service.diff_staged(Path::new(&root), Path::new(&path)).await
            } else {
                service.diff(Path::new(&root), Path::new(&path)).await
            };
            match result {
                Ok(next_diff) => {
                    diff.set(next_diff);
                    error.set(None);
                }
                Err(next_error) => {
                    diff.set(String::new());
                    error.set(Some(next_error.to_string()));
                }
            }
            diff_loading.set(false);
        }
    });

    let snapshot = (project_state.snapshot)();
    let active = active_project_root(&snapshot);
    let root = active.as_ref().map(|(_, root)| root.clone());
    let project_name = active
        .as_ref()
        .map(|(name, _)| name.as_str())
        .unwrap_or("No active project");
    let current = status();
    let rows = changes(&current);
    let branch = current.branch.as_deref().unwrap_or("detached / unborn");

    rsx! {
        Surface {
            eyebrow: "Source control",
            title: "Review",
            subtitle: "Inspect uncommitted changes and move files between the working tree and index.",
            if root.is_none() {
                section { class: "settings-empty",
                    h2 { "Select a project first" }
                    p { "Review is scoped to the active Project Centre root." }
                }
            } else {
                div { style: "display:grid;grid-template-columns:minmax(16rem,.9fr) minmax(24rem,1.7fr);gap:1rem;min-height:32rem;",
                    section { class: "settings-card", style: "min-width:0;",
                        header { style: "display:flex;align-items:center;gap:.5rem;margin-bottom:.75rem;",
                            div { style: "min-width:0;flex:1;",
                                strong { "{project_name}" }
                                div { class: "muted", "{branch} · ↑{current.ahead} ↓{current.behind}" }
                            }
                            button {
                                class: "icon-button",
                                title: "Refresh Git status",
                                aria_label: "Refresh Git status",
                                disabled: status_loading() || mutation_busy(),
                                onclick: move |_| refresh.set(refresh() + 1),
                                "↻"
                            }
                        }
                        if let Some(next_error) = error() {
                            div { class: "settings-error", role: "alert", "{next_error}" }
                        }
                        if status_loading() {
                            p { class: "muted", "Loading Git status…" }
                        } else if rows.is_empty() {
                            div { class: "settings-empty",
                                h2 { "Working tree clean" }
                                p { "No uncommitted files are reported by Git." }
                            }
                        } else {
                            div { style: "display:flex;flex-direction:column;gap:.25rem;max-height:31rem;overflow:auto;",
                                for change in rows {
                                    {
                                        let path = change.path.clone();
                                        let row_path = path.clone();
                                        let choose_staged = change.staged && !change.unstaged;
                                        let stage_path = path.clone();
                                        let unstage_path = path.clone();
                                        let stage_root = root.clone().unwrap_or_default();
                                        let unstage_root = stage_root.clone();
                                        let stage_service = services.git.clone();
                                        let unstage_service = services.git.clone();
                                        rsx! {
                                            div {
                                                key: "{row_path}",
                                                class: "settings-list",
                                                style: "display:grid;grid-template-columns:minmax(0,1fr) auto;gap:.4rem;align-items:center;",
                                                button {
                                                    class: "button",
                                                    style: "min-width:0;text-align:left;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;",
                                                    title: "{row_path}",
                                                    onclick: move |_| {
                                                        selected_path.set(Some(path.clone()));
                                                        staged_view.set(choose_staged);
                                                    },
                                                    "{row_path}"
                                                }
                                                div { style: "display:flex;gap:.3rem;align-items:center;",
                                                    if change.staged { span { class: "muted", title: "Index contains changes", "S" } }
                                                    if change.unstaged { span { class: "muted", title: "Working tree contains changes", "U" } }
                                                    if change.unstaged {
                                                        button {
                                                            class: "button",
                                                            disabled: mutation_busy(),
                                                            onclick: move |_| {
                                                                let service = stage_service.clone();
                                                                let repo = stage_root.clone();
                                                                let target = stage_path.clone();
                                                                mutation_busy.set(true);
                                                                error.set(None);
                                                                spawn(async move {
                                                                    match service.stage(Path::new(&repo), Path::new(&target)).await {
                                                                        Ok(()) => refresh.set(refresh() + 1),
                                                                        Err(next_error) => error.set(Some(next_error.to_string())),
                                                                    }
                                                                    mutation_busy.set(false);
                                                                });
                                                            },
                                                            "Stage"
                                                        }
                                                    }
                                                    if change.staged {
                                                        button {
                                                            class: "button",
                                                            disabled: mutation_busy(),
                                                            onclick: move |_| {
                                                                let service = unstage_service.clone();
                                                                let repo = unstage_root.clone();
                                                                let target = unstage_path.clone();
                                                                mutation_busy.set(true);
                                                                error.set(None);
                                                                spawn(async move {
                                                                    match service.unstage(Path::new(&repo), Path::new(&target)).await {
                                                                        Ok(()) => refresh.set(refresh() + 1),
                                                                        Err(next_error) => error.set(Some(next_error.to_string())),
                                                                    }
                                                                    mutation_busy.set(false);
                                                                });
                                                            },
                                                            "Unstage"
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
                    section { class: "settings-card", style: "min-width:0;display:flex;flex-direction:column;",
                        header { style: "display:flex;align-items:center;gap:.5rem;margin-bottom:.75rem;",
                            div { style: "min-width:0;flex:1;",
                                strong { "Diff" }
                                if let Some(path) = selected_path() {
                                    div { class: "muted", title: "{path}", "{path}" }
                                } else {
                                    div { class: "muted", "Select a changed file." }
                                }
                            }
                            if let Some(path) = selected_path() {
                                {
                                    let selected_change = changes(&status())
                                        .into_iter()
                                        .find(|change| change.path == path);
                                    rsx! {
                                        if selected_change.as_ref().is_some_and(|change| change.unstaged) {
                                            button {
                                                class: "button",
                                                disabled: !staged_view(),
                                                onclick: move |_| staged_view.set(false),
                                                "Working tree"
                                            }
                                        }
                                        if selected_change.as_ref().is_some_and(|change| change.staged) {
                                            button {
                                                class: "button",
                                                disabled: staged_view(),
                                                onclick: move |_| staged_view.set(true),
                                                "Staged"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if diff_loading() {
                            p { class: "muted", "Loading diff…" }
                        } else if selected_path().is_none() {
                            div { class: "settings-empty",
                                h2 { "No file selected" }
                                p { "Choose a changed file to inspect its Git diff." }
                            }
                        } else if diff().is_empty() {
                            div { class: "settings-empty",
                                h2 { "No diff in this state" }
                                p { "Switch between working-tree and staged views when both are available." }
                            }
                        } else {
                            pre {
                                style: "margin:0;min-height:24rem;max-height:34rem;overflow:auto;white-space:pre;font-family:var(--font-mono,monospace);font-size:.76rem;line-height:1.45;padding:.75rem;border:1px solid var(--border-subtle,rgba(127,127,127,.2));border-radius:.5rem;",
                                "{diff}"
                            }
                        }
                    }
                }
            }
        }
    }
}
