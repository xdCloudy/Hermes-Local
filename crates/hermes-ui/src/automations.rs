use dioxus::prelude::*;
use hermes_core::AppServices;
use hermes_protocol::{CronJob, CronJobCreate, CronJobUpdate, SessionSummary};

use super::Surface;

fn label(job: &CronJob) -> String {
    job.name
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&job.id)
        .to_owned()
}

fn schedule(job: &CronJob) -> String {
    job.schedule_display
        .clone()
        .or_else(|| {
            job.schedule
                .as_ref()
                .and_then(|value| value.display.clone())
        })
        .or_else(|| job.schedule.as_ref().and_then(|value| value.expr.clone()))
        .unwrap_or_else(|| "Schedule unavailable".into())
}

#[component]
pub(super) fn Automations() -> Element {
    let services = use_context::<AppServices>();
    let mut jobs = use_signal(Vec::<CronJob>::new);
    let mut selected = use_signal(|| None::<String>);
    let mut runs = use_signal(Vec::<SessionSummary>::new);
    let mut loading = use_signal(|| true);
    let mut busy = use_signal(|| None::<String>);
    let mut error = use_signal(|| None::<String>);
    let mut notice = use_signal(|| None::<String>);
    let mut refresh = use_signal(|| 0_u64);
    let mut editing = use_signal(|| None::<String>);
    let mut name = use_signal(String::new);
    let mut prompt = use_signal(String::new);
    let mut schedule_text = use_signal(String::new);
    let mut pending_delete = use_signal(|| None::<String>);

    let load_services = services.clone();
    let _load = use_resource(move || {
        let services = load_services.clone();
        let _revision = refresh();
        async move {
            loading.set(true);
            match services.cron.list().await {
                Ok(next) => {
                    if selected()
                        .as_ref()
                        .is_none_or(|id| !next.iter().any(|job| &job.id == id))
                    {
                        selected.set(next.first().map(|job| job.id.clone()));
                    }
                    jobs.set(next);
                    error.set(None);
                }
                Err(problem) => error.set(Some(problem.to_string())),
            }
            loading.set(false);
        }
    });

    let run_services = services.clone();
    let _runs = use_resource(move || {
        let services = run_services.clone();
        let id = selected();
        let _revision = refresh();
        async move {
            let Some(id) = id else {
                runs.set(Vec::new());
                return;
            };
            match services.cron.runs(&id, 20).await {
                Ok(next) => runs.set(next),
                Err(problem) => error.set(Some(problem.to_string())),
            }
        }
    });

    let mutation_services = services.clone();
    let mutate = Callback::new(move |(id, action): (String, String)| {
        if busy().is_some() {
            return;
        }
        busy.set(Some(id.clone()));
        error.set(None);
        notice.set(None);
        let services = mutation_services.clone();
        spawn(async move {
            let result = match action.as_str() {
                "pause" => services.cron.pause(&id).await.map(|_| ()),
                "resume" => services.cron.resume(&id).await.map(|_| ()),
                "trigger" => services.cron.trigger(&id).await.map(|_| ()),
                _ => Err(hermes_core::ServiceError::InvalidInput(
                    "unknown Cron action".into(),
                )),
            };
            match result {
                Ok(()) => {
                    notice.set(Some(format!("Cron action completed: {action}")));
                    refresh.set(refresh() + 1);
                }
                Err(problem) => error.set(Some(problem.to_string())),
            }
            busy.set(None);
        });
    });

    let save_services = services.clone();
    let save = Callback::new(move |()| {
        if busy().is_some() {
            return;
        }
        let current_name = name().trim().to_owned();
        let current_prompt = prompt().trim().to_owned();
        let current_schedule = schedule_text().trim().to_owned();
        let edit_id = editing();
        busy.set(Some(edit_id.clone().unwrap_or_else(|| "create".into())));
        error.set(None);
        notice.set(None);
        let services = save_services.clone();
        spawn(async move {
            let result = if let Some(id) = edit_id.as_deref() {
                services
                    .cron
                    .update(
                        id,
                        &CronJobUpdate {
                            name: Some(current_name),
                            prompt: Some(current_prompt),
                            schedule: Some(current_schedule),
                            ..CronJobUpdate::default()
                        },
                    )
                    .await
            } else {
                services
                    .cron
                    .create(&CronJobCreate {
                        name: (!current_name.is_empty()).then_some(current_name),
                        prompt: current_prompt,
                        schedule: current_schedule,
                        ..CronJobCreate::default()
                    })
                    .await
            };
            match result {
                Ok(job) => {
                    selected.set(Some(job.id));
                    editing.set(None);
                    name.set(String::new());
                    prompt.set(String::new());
                    schedule_text.set(String::new());
                    notice.set(Some("Cron job saved".into()));
                    refresh.set(refresh() + 1);
                }
                Err(problem) => error.set(Some(problem.to_string())),
            }
            busy.set(None);
        });
    });

    let delete_services = services.clone();
    let delete = Callback::new(move |id: String| {
        if busy().is_some() {
            return;
        }
        busy.set(Some(id.clone()));
        pending_delete.set(None);
        error.set(None);
        let services = delete_services.clone();
        spawn(async move {
            match services.cron.delete(&id).await {
                Ok(()) => {
                    notice.set(Some("Cron job deleted".into()));
                    refresh.set(refresh() + 1);
                }
                Err(problem) => error.set(Some(problem.to_string())),
            }
            busy.set(None);
        });
    });

    let current = selected().and_then(|id| jobs().into_iter().find(|job| job.id == id));
    rsx! {
        Surface { eyebrow: "Activity", title: "Automations", subtitle: "Create, inspect and control profile-bound scheduled work.",
            div { class: "settings-toolbar",
                button { class: "button", disabled: loading() || busy().is_some(), onclick: move |_| refresh.set(refresh() + 1), "Refresh" }
            }
            if loading() { div { class: "loading-state", role: "status", "◌ Loading scheduled work" } }
            if let Some(problem) = error() { div { class: "error-state", role: "alert", "{problem}" } }
            if let Some(message) = notice() { div { class: "success-state", role: "status", "{message}" } }
            section { class: "panel",
                header { class: "panel-title", "Scheduled jobs" }
                if jobs().is_empty() {
                    div { class: "settings-empty", h2 { "No scheduled jobs" } p { "Create one below to run work on a recurring schedule." } }
                } else {
                    div { class: "settings-list",
                        for job in jobs() {
                            {
                                let id = job.id.clone();
                                let select_id = id.clone();
                                let edit_job = job.clone();
                                let paused = !job.enabled || job.state.as_deref() == Some("paused");
                                let state_label = if paused { "Paused" } else { "Enabled" };
                                rsx! { div { class: "settings-list-row", key: "{id}",
                                    button { class: "settings-row-copy", onclick: move |_| selected.set(Some(select_id.clone())),
                                        strong { "{label(&job)}" }
                                        p { "{schedule(&job)} · {state_label}" }
                                        if let Some(last_error) = job.last_error.as_deref() { p { class: "muted", "Last error: {last_error}" } }
                                    }
                                    div { class: "settings-row-action",
                                        button { class: "button", disabled: busy().is_some(), onclick: move |_| {
                                            editing.set(Some(edit_job.id.clone()));
                                            name.set(edit_job.name.clone().unwrap_or_default());
                                            prompt.set(edit_job.prompt.clone().unwrap_or_default());
                                            schedule_text.set(edit_job.schedule.as_ref().and_then(|value| value.expr.clone()).or_else(|| edit_job.schedule_display.clone()).unwrap_or_default());
                                        }, "Edit" }
                                        button { class: "button", disabled: busy().is_some(), onclick: move |_| mutate.call((id.clone(), if paused { "resume".into() } else { "pause".into() })), if paused { "Resume" } else { "Pause" } }
                                    }
                                } }
                            }
                        }
                    }
                }
            }
            if let Some(job) = current {
                section { class: "panel",
                    header { class: "panel-title", "{label(&job)}" }
                    if let Some(job_prompt) = job.prompt.as_deref() { p { class: "muted", "{job_prompt}" } } else { p { class: "muted", "No prompt" } }
                    if let Some(next) = job.next_run_at.as_deref() { p { class: "muted", "Next run: {next}" } }
                    div { class: "settings-toolbar",
                        {
                            let id = job.id.clone();
                            rsx! { button { class: "primary-button", disabled: busy().is_some(), onclick: move |_| mutate.call((id.clone(), "trigger".into())), "Run now" } }
                        }
                        {
                            let id = job.id.clone();
                            rsx! { button { class: "button danger", disabled: busy().is_some(), onclick: move |_| pending_delete.set(Some(id.clone())), "Delete" } }
                        }
                    }
                    header { class: "panel-title", "Run history" }
                    if runs().is_empty() { p { class: "muted", "No recorded runs." } }
                    for run in runs() {
                        {
                            let run_state = run.model.clone().unwrap_or_else(|| if run.running { "Running".into() } else { "Completed".into() });
                            rsx! { div { class: "settings-list-row", key: "{run.id}",
                                div { class: "settings-row-copy", strong { "{run.title}" } p { "{run_state}" } }
                            } }
                        }
                    }
                }
            }
            section { class: "panel",
                header { class: "panel-title", if editing().is_some() { "Edit scheduled job" } else { "Create scheduled job" } }
                label { class: "field-stack", span { "Name" } input { value: "{name}", maxlength: "256", oninput: move |event| name.set(event.value()) } }
                label { class: "field-stack", span { "Schedule" } input { value: "{schedule_text}", placeholder: "0 9 * * 1-5", maxlength: "4096", oninput: move |event| schedule_text.set(event.value()) } }
                label { class: "field-stack", span { "Prompt" } textarea { value: "{prompt}", rows: "5", maxlength: "1048576", oninput: move |event| prompt.set(event.value()) } }
                div { class: "settings-toolbar",
                    button { class: "primary-button", disabled: busy().is_some() || prompt().trim().is_empty() || schedule_text().trim().is_empty(), onclick: move |_| save.call(()), if editing().is_some() { "Save changes" } else { "Create job" } }
                    if editing().is_some() { button { class: "button", onclick: move |_| { editing.set(None); name.set(String::new()); prompt.set(String::new()); schedule_text.set(String::new()); }, "Cancel edit" } }
                }
            }
            if let Some(id) = pending_delete() {
                div { class: "dialog-backdrop", role: "presentation",
                    section { class: "dialog-card", role: "dialog", aria_modal: "true", aria_label: "Confirm Cron deletion",
                        h2 { "Delete scheduled job?" }
                        p { "The job and its future schedule will be removed." }
                        div { class: "dialog-actions",
                            button { class: "button", onclick: move |_| pending_delete.set(None), "Cancel" }
                            button { class: "button danger", onclick: move |_| delete.call(id.clone()), "Delete job" }
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
    fn automation_labels_fall_back_safely() {
        let job = CronJob {
            id: "job-1".into(),
            ..CronJob::default()
        };
        assert_eq!(label(&job), "job-1");
        assert_eq!(schedule(&job), "Schedule unavailable");
    }
}
