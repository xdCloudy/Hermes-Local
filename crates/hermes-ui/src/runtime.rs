use std::time::Duration;

use dioxus::prelude::*;
use hermes_core::AppServices;
use hermes_protocol::{ConnectionState, RuntimeStatus, TaskSummary};
use serde_json::Value;

use super::Surface;

const REFRESH_INTERVAL: Duration = Duration::from_millis(1500);

fn display(value: &str, fallback: &str) -> String {
    if value.trim().is_empty() {
        fallback.to_owned()
    } else {
        value.to_owned()
    }
}

fn gateway_label(state: ConnectionState) -> &'static str {
    match state {
        ConnectionState::Idle => "Idle",
        ConnectionState::Connecting => "Connecting",
        ConnectionState::Open => "Connected",
        ConnectionState::Closed => "Closed",
        ConnectionState::Error => "Error",
    }
}

fn progress_label(progress: Option<f64>) -> String {
    progress.map_or_else(
        || "—".into(),
        |value| format!("{:.0}%", value.clamp(0.0, 1.0) * 100.0),
    )
}

fn action_for_route(route: &'static str) -> Option<(&'static str, &'static str)> {
    match route {
        "benchmarks" => Some(("benchmark", "Run benchmark")),
        "security" => Some(("security", "Run security scan")),
        _ => None,
    }
}

#[component]
fn RuntimeSurface(route: &'static str, title: &'static str, subtitle: &'static str) -> Element {
    let services = use_context::<AppServices>();
    let mut status = use_signal(|| None::<RuntimeStatus>);
    let mut tasks = use_signal(Vec::<TaskSummary>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut refresh = use_signal(|| 0_u64);
    let mut busy = use_signal(|| None::<String>);

    let load_services = services.clone();
    let _load = use_resource(move || {
        let services = load_services.clone();
        let _revision = refresh();
        async move {
            loop {
                let next_status = services.runtime.status().await;
                let next_tasks = services.runtime.actions().await;
                match (next_status, next_tasks) {
                    (Ok(next_status), Ok(next_tasks)) => {
                        status.set(Some(next_status));
                        tasks.set(next_tasks);
                        error.set(None);
                    }
                    (status_result, tasks_result) => {
                        error.set(Some(
                            status_result
                                .err()
                                .or_else(|| tasks_result.err())
                                .map_or_else(
                                    || "Runtime unavailable".into(),
                                    |problem| problem.to_string(),
                                ),
                        ));
                    }
                }
                loading.set(false);
                tokio::time::sleep(REFRESH_INTERVAL).await;
            }
        }
    });

    let start_services = services.clone();
    let start = Callback::new(move |action: String| {
        if busy().is_some() {
            return;
        }
        busy.set(Some(action.clone()));
        error.set(None);
        let services = start_services.clone();
        spawn(async move {
            match services.runtime.start_action(&action, Value::Null).await {
                Ok(_) => refresh.set(refresh() + 1),
                Err(problem) => error.set(Some(problem.to_string())),
            }
            busy.set(None);
        });
    });

    let cancel_services = services.clone();
    let cancel = Callback::new(move |id: String| {
        if busy().is_some() {
            return;
        }
        busy.set(Some(id.clone()));
        error.set(None);
        let services = cancel_services.clone();
        spawn(async move {
            match services.runtime.cancel_action(&id).await {
                Ok(()) => refresh.set(refresh() + 1),
                Err(problem) => error.set(Some(problem.to_string())),
            }
            busy.set(None);
        });
    });

    let current = status();
    let phase = current
        .as_ref()
        .map(|value| display(&value.phase, "Unknown"))
        .unwrap_or_else(|| "Unknown".into());
    let model = current
        .as_ref()
        .and_then(|value| value.model.clone())
        .unwrap_or_else(|| "Not reported".into());
    let provider = current
        .as_ref()
        .and_then(|value| value.provider.clone())
        .unwrap_or_else(|| "Not reported".into());
    let agent_version = current
        .as_ref()
        .and_then(|value| value.agent_version.clone())
        .unwrap_or_else(|| "Not reported".into());
    let filtered = tasks()
        .into_iter()
        .filter(|task| match route {
            "benchmarks" => task.name.to_ascii_lowercase().contains("benchmark"),
            "security" => task.name.to_ascii_lowercase().contains("security"),
            _ => true,
        })
        .collect::<Vec<_>>();

    rsx! {
        Surface { eyebrow: "Workstation", title, subtitle,
            if loading() {
                div { class: "loading-state", role: "status", "◌ Loading runtime state" }
            }
            if let Some(problem) = error() {
                div { class: "error-state", role: "alert", h2 { "Runtime request failed" } p { "{problem}" } }
            }
            if let Some(current) = current {
                section { class: "panel",
                    header { class: "panel-title", "Live runtime" }
                    div { class: "integrity-grid",
                        div { class: "integrity-item", span { "Phase" } strong { "{phase}" } }
                        div { class: "integrity-item", span { "Gateway" } strong { "{gateway_label(current.gateway)}" } }
                        div { class: "integrity-item", span { "Model" } strong { "{model}" } }
                        div { class: "integrity-item", span { "Provider" } strong { "{provider}" } }
                        div { class: "integrity-item", span { "Agent" } strong { "{agent_version}" } }
                    }
                    if let Some(detail) = current.detail.as_deref() { p { class: "muted", "{detail}" } }
                }
            }
            section { class: "panel",
                header { class: "panel-title", "Task Centre" }
                if let Some((action, label)) = action_for_route(route) {
                    button {
                        class: "primary-button",
                        disabled: busy().is_some(),
                        onclick: move |_| start.call(action.to_owned()),
                        if busy().as_deref() == Some(action) { "Starting…" } else { "{label}" }
                    }
                }
                if filtered.is_empty() {
                    div { class: "settings-empty", h2 { "No matching tasks" } p { "Task state refreshes automatically." } }
                } else {
                    div { class: "settings-list",
                        for task in filtered {
                            {
                                let active = matches!(task.state.as_str(), "queued" | "running" | "cancelling");
                                let task_id = task.id.clone();
                                let task_name = display(&task.name, &task.id);
                                let task_state = display(&task.state, "unknown");
                                let task_progress = progress_label(task.progress);
                                rsx! { div { class: "settings-list-row", key: "{task.id}",
                                    div { class: "settings-row-copy",
                                        strong { "{task_name}" }
                                        p { "{task_state} · {task_progress}" }
                                        if let Some(detail) = task.detail.as_deref() { p { class: "muted", "{detail}" } }
                                    }
                                    div { class: "settings-row-action",
                                        if active {
                                            button { class: "button danger", disabled: busy().is_some(), onclick: move |_| cancel.call(task_id.clone()), "Cancel" }
                                        }
                                    }
                                } }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn Runtime() -> Element {
    rsx! { RuntimeSurface { route: "runtime", title: "Runtime", subtitle: "Live Agent, model and gateway health." } }
}

#[component]
pub(super) fn Tasks() -> Element {
    rsx! { RuntimeSurface { route: "tasks", title: "Tasks", subtitle: "Track and cancel durable workstation work." } }
}

#[component]
pub(super) fn Benchmarks() -> Element {
    rsx! { RuntimeSurface { route: "benchmarks", title: "Benchmarks", subtitle: "Run and monitor the native benchmark task." } }
}

#[component]
pub(super) fn Security() -> Element {
    rsx! { RuntimeSurface { route: "security", title: "Security", subtitle: "Run and monitor the native security task." } }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_actions_remain_explicit() {
        assert_eq!(
            action_for_route("benchmarks"),
            Some(("benchmark", "Run benchmark"))
        );
        assert_eq!(
            action_for_route("security"),
            Some(("security", "Run security scan"))
        );
        assert_eq!(action_for_route("tasks"), None);
    }

    #[test]
    fn progress_is_bounded() {
        assert_eq!(progress_label(Some(1.4)), "100%");
        assert_eq!(progress_label(Some(-1.0)), "0%");
        assert_eq!(progress_label(None), "—");
    }
}
