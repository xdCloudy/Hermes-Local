use dioxus::prelude::*;
use hermes_core::AppServices;
use hermes_protocol::{CuratorStatus, MemoryResetTarget, MemoryStatus};

use super::Surface;

fn reset_label(target: MemoryResetTarget) -> &'static str {
    match target {
        MemoryResetTarget::All => "all memory files",
        MemoryResetTarget::Memory => "MEMORY.md",
        MemoryResetTarget::User => "USER.md",
    }
}

#[component]
pub(super) fn Memory() -> Element {
    let services = use_context::<AppServices>();
    let mut status = use_signal(|| None::<MemoryStatus>);
    let mut curator = use_signal(|| None::<CuratorStatus>);
    let mut loading = use_signal(|| true);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut notice = use_signal(|| None::<String>);
    let mut refresh = use_signal(|| 0_u64);
    let mut confirm_reset = use_signal(|| None::<MemoryResetTarget>);

    let load_services = services.clone();
    let _load = use_resource(move || {
        let services = load_services.clone();
        let _revision = refresh();
        async move {
            loading.set(true);
            let next_status = services.memory.status().await;
            let next_curator = services.memory.curator_status().await;
            match (next_status, next_curator) {
                (Ok(next_status), Ok(next_curator)) => {
                    status.set(Some(next_status));
                    curator.set(Some(next_curator));
                    error.set(None);
                }
                (status_result, curator_result) => {
                    error.set(Some(
                        status_result
                            .err()
                            .or_else(|| curator_result.err())
                            .map_or_else(
                                || "Memory unavailable".into(),
                                |problem| problem.to_string(),
                            ),
                    ));
                }
            }
            loading.set(false);
        }
    });

    let pause_services = services.clone();
    let toggle_pause = Callback::new(move |paused: bool| {
        if busy() {
            return;
        }
        busy.set(true);
        error.set(None);
        notice.set(None);
        let services = pause_services.clone();
        spawn(async move {
            match services.memory.set_curator_paused(paused).await {
                Ok(result) if result.ok => {
                    notice.set(Some(
                        if result.paused {
                            "Curator paused"
                        } else {
                            "Curator resumed"
                        }
                        .into(),
                    ));
                    refresh.set(refresh() + 1);
                }
                Ok(_) => error.set(Some("Agent did not confirm the curator change".into())),
                Err(problem) => error.set(Some(problem.to_string())),
            }
            busy.set(false);
        });
    });

    let run_services = services.clone();
    let run_curator = Callback::new(move |()| {
        if busy() {
            return;
        }
        busy.set(true);
        error.set(None);
        notice.set(None);
        let services = run_services.clone();
        spawn(async move {
            match services.memory.run_curator().await {
                Ok(result) if result.ok => {
                    notice.set(Some(format!(
                        "Curator run started{}",
                        if result.pid == 0 {
                            String::new()
                        } else {
                            format!(" (PID {})", result.pid)
                        }
                    )));
                    refresh.set(refresh() + 1);
                }
                Ok(_) => error.set(Some("Agent did not start the curator".into())),
                Err(problem) => error.set(Some(problem.to_string())),
            }
            busy.set(false);
        });
    });

    let reset_services = services.clone();
    let reset = Callback::new(move |target: MemoryResetTarget| {
        if busy() {
            return;
        }
        busy.set(true);
        confirm_reset.set(None);
        error.set(None);
        notice.set(None);
        let services = reset_services.clone();
        spawn(async move {
            match services.memory.reset(target).await {
                Ok(result) if result.ok => {
                    notice.set(Some(format!(
                        "Reset complete: {} item(s) removed",
                        result.deleted.len()
                    )));
                    refresh.set(refresh() + 1);
                }
                Ok(_) => error.set(Some("Agent did not confirm the memory reset".into())),
                Err(problem) => error.set(Some(problem.to_string())),
            }
            busy.set(false);
        });
    });

    let current_status = status();
    let current_curator = curator();
    rsx! {
        Surface { eyebrow: "Intelligence", title: "Memory", subtitle: "Inspect profile-bound memory providers and control the local curator.",
            div { class: "settings-toolbar",
                button { class: "button", disabled: loading() || busy(), onclick: move |_| refresh.set(refresh() + 1), "Refresh" }
            }
            if loading() { div { class: "loading-state", role: "status", "◌ Loading memory state" } }
            if let Some(problem) = error() { div { class: "error-state", role: "alert", "{problem}" } }
            if let Some(message) = notice() { div { class: "success-state", role: "status", "{message}" } }
            if let Some(current) = current_status {
                section { class: "panel",
                    header { class: "panel-title", "Memory providers" }
                    div { class: "integrity-grid",
                        div { class: "integrity-item", span { "Active" } strong { "{current.active}" } }
                        div { class: "integrity-item", span { "MEMORY.md bytes" } strong { "{current.builtin_files.memory}" } }
                        div { class: "integrity-item", span { "USER.md bytes" } strong { "{current.builtin_files.user}" } }
                    }
                    div { class: "settings-list",
                        for provider in current.providers {
                            div { class: "settings-list-row", key: "{provider.name}",
                                div { class: "settings-row-copy", strong { "{provider.name}" } p { "{provider.description}" } }
                                span { class: "badge", if provider.configured { "Configured" } else { "Needs setup" } }
                            }
                        }
                    }
                }
            }
            if let Some(current) = current_curator {
                section { class: "panel",
                    header { class: "panel-title", "Curator" }
                    p { class: "muted", "Enabled: {current.enabled} · Paused: {current.paused}" }
                    if let Some(last_run) = current.last_run_at.as_deref() { p { class: "muted", "Last run: {last_run}" } }
                    div { class: "settings-toolbar",
                        button { class: "primary-button", disabled: busy() || !current.enabled, onclick: move |_| run_curator.call(()), "Run now" }
                        button { class: "button", disabled: busy() || !current.enabled, onclick: move |_| toggle_pause.call(!current.paused), if current.paused { "Resume" } else { "Pause" } }
                    }
                }
            }
            section { class: "panel",
                header { class: "panel-title", "Reset memory" }
                p { class: "muted", "Reset is destructive and requires an explicit second confirmation." }
                div { class: "settings-toolbar",
                    for (target, label) in [
                        (MemoryResetTarget::Memory, "Reset MEMORY.md"),
                        (MemoryResetTarget::User, "Reset USER.md"),
                        (MemoryResetTarget::All, "Reset all"),
                    ] {
                        button { class: "button danger", disabled: busy(), onclick: move |_| confirm_reset.set(Some(target)), "{label}" }
                    }
                }
            }
            if let Some(target) = confirm_reset() {
                div { class: "dialog-backdrop", role: "presentation",
                    section { class: "dialog-card", role: "dialog", aria_modal: "true", aria_label: "Confirm memory reset",
                        h2 { "Reset {reset_label(target)}?" }
                        p { "This removes persisted memory content and cannot be undone." }
                        div { class: "dialog-actions",
                            button { class: "button", onclick: move |_| confirm_reset.set(None), "Cancel" }
                            button { class: "button danger", onclick: move |_| reset.call(target), "Confirm reset" }
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

    #[test]
    fn reset_copy_is_explicit_for_every_target() {
        assert_eq!(reset_label(MemoryResetTarget::Memory), "MEMORY.md");
        assert_eq!(reset_label(MemoryResetTarget::User), "USER.md");
        assert_eq!(reset_label(MemoryResetTarget::All), "all memory files");
    }
}
