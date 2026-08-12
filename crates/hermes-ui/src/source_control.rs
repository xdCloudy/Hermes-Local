use std::path::Path;

use dioxus::prelude::*;
use hermes_core::{AppServices, GitBranchInfo, GitWorktreeInfo};
use hermes_protocol::ProjectsSnapshot;

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
        Surface { eyebrow: "Source control", title: "Branches", subtitle: "Switch the home checkout or isolate non-default branches in linked worktrees.",
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
                (Err(next_error), _) | (_, Err(next_error)) => {
                    error.set(Some(next_error.to_string()))
                }
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
        Surface { eyebrow: "Source control", title: "Worktrees", subtitle: "Create isolated branch checkouts and remove only Hermes-managed linked worktrees.",
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
                        for tree in rows.clone() {
                            {
                                let target = tree.clone();
                                let branch_label = tree
                                    .branch
                                    .clone()
                                    .unwrap_or_else(|| "detached".to_owned());
                                rsx! {
                                    div { class: "settings-row", style: "align-items:flex-start;gap:.75rem;",
                                        div { style: "min-width:0;flex:1;",
                                            div { style: "display:flex;gap:.4rem;align-items:center;flex-wrap:wrap;",
                                                strong { "{branch_label}" }
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
