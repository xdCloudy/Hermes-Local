use dioxus::prelude::*;
use futures_util::StreamExt as _;
use hermes_core::AppServices;
use hermes_protocol::{GatewayEvent, SessionCreateRequest};

const MAX_TASK_BYTES: usize = 32 * 1024;
const MAX_ROWS: usize = 128;
const MAX_TRANSCRIPT_MESSAGES: usize = 40;
const MAX_TRANSCRIPT_BYTES: usize = 96 * 1024;

#[derive(Clone, Debug, Default, PartialEq)]
struct SubagentView {
    id: String,
    parent_id: Option<String>,
    parent_runtime_id: String,
    child_session_id: Option<String>,
    goal: String,
    model: Option<String>,
    status: String,
    task_index: u64,
    task_count: u64,
    summary: Option<String>,
    last_line: Option<String>,
}

fn text_field(payload: &serde_json::Value, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.chars().take(2_048).collect())
}

fn number_field(payload: &serde_json::Value, key: &str) -> Option<u64> {
    payload.get(key).and_then(serde_json::Value::as_u64)
}

fn event_status(event: &GatewayEvent) -> String {
    if let Some(status) = text_field(&event.payload, "status") {
        return status;
    }
    if event.kind.ends_with("complete") || event.kind.ends_with("completed") {
        "completed".into()
    } else if event.kind.ends_with("failed") || event.kind.ends_with("error") {
        "failed".into()
    } else if event.kind.ends_with("interrupted") || event.kind.ends_with("cancelled") {
        "interrupted".into()
    } else if event.kind.ends_with("queued") {
        "queued".into()
    } else {
        "running".into()
    }
}

fn merge_subagent_event(rows: &mut Vec<SubagentView>, event: &GatewayEvent) -> bool {
    if !event.kind.starts_with("subagent.") {
        return false;
    }
    let Some(parent_runtime_id) = event
        .session_id
        .as_deref()
        .filter(|value| !value.is_empty())
    else {
        // Legacy parity rule: an unscoped subagent event must never be attached to
        // whichever chat happens to be focused.
        return false;
    };

    let goal = text_field(&event.payload, "goal").unwrap_or_else(|| "Subagent".into());
    let parent_id = text_field(&event.payload, "parent_id");
    let task_index = number_field(&event.payload, "task_index").unwrap_or_default();
    let id = text_field(&event.payload, "subagent_id").unwrap_or_else(|| {
        format!(
            "{}:{}:{}",
            parent_id.as_deref().unwrap_or(parent_runtime_id),
            task_index,
            goal
        )
    });
    let status = event_status(event);
    let child_session_id = text_field(&event.payload, "child_session_id");
    let model = text_field(&event.payload, "model");
    let summary = text_field(&event.payload, "summary");
    let last_line = text_field(&event.payload, "text")
        .or_else(|| text_field(&event.payload, "tool_preview"))
        .or_else(|| text_field(&event.payload, "tool_name"));

    if let Some(existing) = rows.iter_mut().find(|row| row.id == id) {
        existing.parent_runtime_id = parent_runtime_id.to_owned();
        if parent_id.is_some() {
            existing.parent_id = parent_id;
        }
        if child_session_id.is_some() {
            existing.child_session_id = child_session_id;
        }
        if model.is_some() {
            existing.model = model;
        }
        if summary.is_some() {
            existing.summary = summary;
        }
        if last_line.is_some() {
            existing.last_line = last_line;
        }
        existing.goal = goal;
        existing.status = status;
        existing.task_index = task_index;
        existing.task_count =
            number_field(&event.payload, "task_count").unwrap_or(existing.task_count.max(1));
    } else {
        rows.push(SubagentView {
            id,
            parent_id,
            parent_runtime_id: parent_runtime_id.to_owned(),
            child_session_id,
            goal,
            model,
            status,
            task_index,
            task_count: number_field(&event.payload, "task_count").unwrap_or(1),
            summary,
            last_line,
        });
        if rows.len() > MAX_ROWS {
            rows.drain(..rows.len() - MAX_ROWS);
        }
    }
    true
}

fn bounded_task(raw: &str) -> Result<String, String> {
    let task = raw.trim();
    if task.is_empty() {
        return Err("Delegated task is required.".into());
    }
    if task.len() > MAX_TASK_BYTES || task.contains('\0') {
        return Err(format!(
            "Delegated task must be valid text no larger than {} KiB.",
            MAX_TASK_BYTES / 1024
        ));
    }
    Ok(task.to_owned())
}

fn delegation_prompt(task: &str) -> String {
    format!(
        "Delegate the task below to a Hermes subagent using the native delegation tool. Keep the delegated worker bounded to this task, preserve its progress events, and report the final result back to this parent session.\n\nTASK:\n{task}"
    )
}

#[component]
pub fn SubagentBridge(children: Element) -> Element {
    let services = use_context::<AppServices>();
    let mut open = use_signal(|| false);
    let mut task = use_signal(String::new);
    let mut rows = use_signal(Vec::<SubagentView>::new);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut notice = use_signal(|| None::<String>);
    let mut transcript = use_signal(|| None::<String>);

    let stream_services = services.clone();
    use_effect(move || {
        let sessions = stream_services.sessions.clone();
        match sessions.events() {
            Ok(mut events) => {
                spawn(async move {
                    while let Some(event) = events.next().await {
                        let mut current = rows();
                        if merge_subagent_event(&mut current, &event) {
                            rows.set(current);
                        }
                    }
                });
            }
            Err(problem) => error.set(Some(problem.to_string())),
        }
    });

    let launch_services = services.clone();
    let launch = move |_| {
        if busy() {
            return;
        }
        let delegated = match bounded_task(&task()) {
            Ok(value) => value,
            Err(problem) => {
                error.set(Some(problem));
                return;
            }
        };
        busy.set(true);
        error.set(None);
        notice.set(None);
        let sessions = launch_services.sessions.clone();
        spawn(async move {
            let result = async {
                let created = sessions.create(SessionCreateRequest::default()).await?;
                let title = delegated
                    .split_whitespace()
                    .take(8)
                    .collect::<Vec<_>>()
                    .join(" ");
                let title = format!("Delegation · {title}");
                sessions
                    .rename(&created.id, created.runtime_id.as_deref(), &title)
                    .await?;
                let runtime_id = created.runtime_id.as_deref().unwrap_or(created.id.as_str());
                sessions
                    .submit(runtime_id, &delegation_prompt(&delegated))
                    .await?;
                Ok::<_, hermes_core::ServiceError>(())
            }
            .await;
            match result {
                Ok(()) => {
                    task.set(String::new());
                    notice.set(Some("Delegation launched; native subagent events will appear here even while another session is focused.".into()));
                }
                Err(problem) => error.set(Some(problem.to_string())),
            }
            busy.set(false);
        });
    };

    let stop_services = services.clone();
    let stop = Callback::new(move |row: SubagentView| {
        if busy() {
            return;
        }
        busy.set(true);
        error.set(None);
        notice.set(None);
        let sessions = stop_services.sessions.clone();
        spawn(async move {
            let result = if let Some(child_id) = row.child_session_id.as_deref() {
                match sessions.resume(child_id).await {
                    Ok(child) => sessions.interrupt(&child.session_id).await,
                    Err(problem) => Err(problem),
                }
            } else {
                sessions.interrupt(&row.parent_runtime_id).await
            };
            match result {
                Ok(()) => notice.set(Some(format!("Interrupt requested for {}.", row.goal))),
                Err(problem) => error.set(Some(problem.to_string())),
            }
            busy.set(false);
        });
    });

    let inspect_services = services.clone();
    let inspect = Callback::new(move |row: SubagentView| {
        let Some(child_id) = row.child_session_id.clone() else {
            error.set(Some(
                "This subagent has not published a child session id yet.".into(),
            ));
            return;
        };
        busy.set(true);
        error.set(None);
        let sessions = inspect_services.sessions.clone();
        spawn(async move {
            match sessions.history(&child_id).await {
                Ok(messages) => {
                    let mut rendered = String::new();
                    for message in messages
                        .into_iter()
                        .rev()
                        .take(MAX_TRANSCRIPT_MESSAGES)
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                    {
                        let text = if message.text.is_empty() {
                            message.content_text
                        } else {
                            message.text
                        };
                        if text.trim().is_empty() {
                            continue;
                        }
                        let line = format!("{:?}: {}\n\n", message.role, text.trim());
                        if rendered.len() + line.len() > MAX_TRANSCRIPT_BYTES {
                            rendered.push_str("… transcript truncated …\n");
                            break;
                        }
                        rendered.push_str(&line);
                    }
                    transcript.set(Some(if rendered.is_empty() {
                        "No child transcript yet.".into()
                    } else {
                        rendered
                    }));
                }
                Err(problem) => error.set(Some(problem.to_string())),
            }
            busy.set(false);
        });
    });

    let active = rows()
        .iter()
        .filter(|row| matches!(row.status.as_str(), "running" | "queued"))
        .count();

    rsx! {
        {children}
        div {
            style: "position:fixed;right:104px;bottom:14px;z-index:2147483000;font:12px system-ui,sans-serif;",
            button {
                style: "border:1px solid rgb(71 85 105);border-radius:6px;background:rgb(15 23 42);color:rgb(226 232 240);padding:7px 10px;box-shadow:0 8px 24px rgb(0 0 0 / 28%);cursor:pointer;",
                title: "Hermes subagents",
                onclick: move |_| open.set(!open()),
                "Agents ({active})"
            }
            if open() {
                div {
                    style: "position:absolute;right:0;bottom:40px;width:min(620px,calc(100vw - 28px));max-height:min(720px,calc(100vh - 90px));overflow:auto;display:grid;gap:10px;padding:12px;border:1px solid rgb(71 85 105);border-radius:8px;background:rgb(9 11 16);color:rgb(226 232 240);box-shadow:0 18px 48px rgb(0 0 0 / 45%);",
                    div { style: "display:flex;align-items:center;justify-content:space-between;gap:8px;",
                        strong { "Agents / subagents" }
                        span { style: "color:rgb(148 163 184);", "{active} active · {rows().len()} observed" }
                    }
                    textarea {
                        value: "{task}",
                        rows: 4,
                        placeholder: "Task to delegate",
                        style: "width:100%;box-sizing:border-box;resize:vertical;border:1px solid rgb(51 65 85);border-radius:5px;background:rgb(15 23 42);color:rgb(241 245 249);padding:8px;font:12px system-ui,sans-serif;",
                        oninput: move |event| task.set(event.value()),
                    }
                    button {
                        style: "justify-self:start;border:1px solid rgb(71 85 105);border-radius:5px;background:rgb(30 41 59);color:rgb(241 245 249);padding:6px 9px;cursor:pointer;",
                        disabled: busy(),
                        onclick: launch,
                        "Launch delegated task"
                    }
                    if let Some(problem) = error() { div { role: "alert", style: "color:rgb(248 113 113);", "{problem}" } }
                    if let Some(message) = notice() { div { role: "status", style: "color:rgb(148 163 184);", "{message}" } }
                    if rows().is_empty() {
                        div { style: "padding:14px 0;color:rgb(148 163 184);", "No subagent events observed yet." }
                    } else {
                        for row in rows().into_iter().rev() {
                            div { key: "{row.id}", style: "display:grid;gap:5px;padding:9px;border:1px solid rgb(51 65 85);border-radius:6px;background:rgb(15 23 42);",
                                div { style: "display:flex;align-items:flex-start;justify-content:space-between;gap:8px;",
                                    div { style: "min-width:0;",
                                        strong { style: "display:block;overflow-wrap:anywhere;", "{row.goal}" }
                                        span { style: "color:rgb(148 163 184);", "{row.status} · worker {row.task_index + 1}/{row.task_count.max(1)}" }
                                        if let Some(model) = row.model.as_deref() { span { style: "color:rgb(100 116 139);", " · {model}" } }
                                    }
                                    div { style: "display:flex;gap:6px;",
                                        button { disabled: busy() || row.child_session_id.is_none(), onclick: { let row = row.clone(); move |_| inspect.call(row.clone()) }, "Transcript" }
                                        button { disabled: busy() || !matches!(row.status.as_str(), "running" | "queued"), onclick: { let row = row.clone(); move |_| stop.call(row.clone()) }, "Stop" }
                                    }
                                }
                                if let Some(line) = row.last_line.as_deref() { span { style: "color:rgb(203 213 225);overflow-wrap:anywhere;", "{line}" } }
                                if let Some(summary) = row.summary.as_deref() { span { style: "color:rgb(148 163 184);overflow-wrap:anywhere;", "{summary}" } }
                            }
                        }
                    }
                    if let Some(text) = transcript() {
                        div { style: "display:grid;gap:6px;",
                            div { style: "display:flex;justify-content:space-between;", strong { "Child transcript" } button { onclick: move |_| transcript.set(None), "Close" } }
                            pre { style: "margin:0;max-height:260px;overflow:auto;white-space:pre-wrap;overflow-wrap:anywhere;border:1px solid rgb(51 65 85);border-radius:5px;background:rgb(2 6 23);padding:9px;color:rgb(203 213 225);font:11px ui-monospace,monospace;", "{text}" }
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
    use serde_json::json;

    fn event(kind: &str, session_id: Option<&str>, payload: serde_json::Value) -> GatewayEvent {
        GatewayEvent {
            kind: kind.into(),
            session_id: session_id.map(str::to_owned),
            profile: None,
            payload,
            extra: Default::default(),
        }
    }

    #[test]
    fn unscoped_subagent_events_are_rejected() {
        let mut rows = Vec::new();
        assert!(!merge_subagent_event(
            &mut rows,
            &event("subagent.progress", None, json!({"subagent_id":"worker"}))
        ));
        assert!(rows.is_empty());
    }

    #[test]
    fn lifecycle_events_merge_without_losing_child_session() {
        let mut rows = Vec::new();
        assert!(merge_subagent_event(
            &mut rows,
            &event(
                "subagent.progress",
                Some("parent-runtime"),
                json!({
                    "subagent_id":"worker-1",
                    "goal":"Audit project",
                    "child_session_id":"child-stored",
                    "status":"running",
                    "task_index":0,
                    "task_count":2,
                    "tool_name":"grep"
                })
            )
        ));
        assert!(merge_subagent_event(
            &mut rows,
            &event(
                "subagent.complete",
                Some("parent-runtime"),
                json!({"subagent_id":"worker-1","goal":"Audit project","summary":"done"})
            )
        ));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, "completed");
        assert_eq!(rows[0].child_session_id.as_deref(), Some("child-stored"));
        assert_eq!(rows[0].summary.as_deref(), Some("done"));
    }

    #[test]
    fn delegated_task_validation_is_bounded() {
        assert!(bounded_task("do work").is_ok());
        assert!(bounded_task(" ").is_err());
        assert!(bounded_task("bad\0task").is_err());
        assert!(bounded_task(&"x".repeat(MAX_TASK_BYTES + 1)).is_err());
    }
}
