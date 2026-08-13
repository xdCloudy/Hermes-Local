use dioxus::prelude::*;
use hermes_core::AppServices;
use hermes_protocol::{ModelSettingsSnapshot, SessionCreateRequest};

use super::{ProjectUiState, Route};

fn model_pair(value: &str) -> Option<(String, String)> {
    let (provider, model) = value.split_once('\u{1f}')?;
    (!provider.is_empty() && !model.is_empty()).then(|| (provider.into(), model.into()))
}

fn valid_approval(value: &str) -> bool {
    matches!(value, "manual" | "smart")
}

#[component]
pub(super) fn FreshModelControl() -> Element {
    let services = use_context::<AppServices>();
    let projects = use_context::<ProjectUiState>();
    let navigator = use_navigator();
    let mut catalog = use_signal(|| None::<ModelSettingsSnapshot>);
    let mut selected = use_signal(String::new);
    let mut approval = use_signal(|| "manual".to_owned());
    let mut tools = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(String::new);

    let loader = services.models.clone();
    let _load = use_resource(move || {
        let loader = loader.clone();
        async move {
            match loader.load(None).await {
                Ok(snapshot) => {
                    if selected().is_empty() {
                        selected.set(format!(
                            "{}\u{1f}{}",
                            snapshot.info.provider, snapshot.info.model
                        ));
                    }
                    catalog.set(Some(snapshot));
                }
                Err(problem) => error.set(problem.to_string()),
            }
        }
    });

    let session_service = services.sessions.clone();
    let start = Callback::new(move |()| {
        let Some((provider, model)) = model_pair(&selected()) else {
            error.set("Choose a model first".into());
            return;
        };
        let approval_mode = approval();
        if !valid_approval(&approval_mode) {
            error.set("Choose a valid approval mode".into());
            return;
        }
        let tool_control = tools().trim().to_owned();
        if tool_control.len() > 256 {
            error.set("Tool control must be 256 characters or fewer".into());
            return;
        }
        if busy() {
            return;
        }
        let snapshot = (projects.snapshot)();
        let project_id = snapshot.active_id.clone();
        let cwd = project_id.as_ref().and_then(|active_id| {
            snapshot
                .projects
                .iter()
                .find(|project| &project.id == active_id)
                .and_then(|project| project.primary_path.clone())
        });
        let service = session_service.clone();
        busy.set(true);
        error.set(String::new());
        spawn(async move {
            let result = async {
                let session = service
                    .create(SessionCreateRequest {
                        cwd,
                        project_id,
                        ..SessionCreateRequest::default()
                    })
                    .await?;
                let stored_id = session.id.clone();
                let runtime_id = session.runtime_id.unwrap_or_else(|| stored_id.clone());

                let configure = async {
                    service
                        .execute_directive(
                            &runtime_id,
                            &format!("/model {model} --provider {provider} --session"),
                        )
                        .await?;
                    service
                        .execute_directive(&runtime_id, &format!("/approvals {approval_mode}"))
                        .await?;
                    if !tool_control.is_empty() {
                        service
                            .execute_directive(&runtime_id, &format!("/tools {tool_control}"))
                            .await?;
                    }
                    Ok::<(), hermes_core::ServiceError>(())
                }
                .await;

                if let Err(problem) = configure {
                    let _ = service.delete(&stored_id).await;
                    return Err(problem);
                }
                Ok::<_, hermes_core::ServiceError>(stored_id)
            }
            .await;
            busy.set(false);
            match result {
                Ok(id) => {
                    let _ = navigator.push(Route::Session { id });
                }
                Err(problem) => error.set(problem.to_string()),
            }
        });
    });

    rsx! {
        if let Some(models) = catalog() {
            div { class: "chat-runtime-controls fresh-model", aria_label: "New chat runtime controls",
                select {
                    value: "{selected}",
                    disabled: busy(),
                    aria_label: "Model for new chat",
                    onchange: move |event| selected.set(event.value()),
                    for provider in models.options.providers {
                        for model in provider.models {
                            option {
                                value: "{provider.slug}\u{1f}{model}",
                                "{provider.name} · {model}"
                            }
                        }
                    }
                }
                select {
                    value: "{approval}",
                    disabled: busy(),
                    aria_label: "Approval mode for new chat",
                    onchange: move |event| approval.set(event.value()),
                    option { value: "manual", "Approvals: manual" }
                    option { value: "smart", "Approvals: smart" }
                }
                input {
                    value: "{tools}",
                    disabled: busy(),
                    aria_label: "Tool control for new chat",
                    placeholder: "tools: +name | -name",
                    oninput: move |event| tools.set(event.value()),
                }
                button {
                    class: "secondary-button",
                    disabled: busy() || selected().is_empty(),
                    onclick: move |_| start.call(()),
                    if busy() { "Starting…" } else { "Start configured chat" }
                }
            }
        }
        if !error().is_empty() {
            small { class: "inline-error", role: "alert", "{error}" }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_provider_and_model() {
        assert_eq!(
            model_pair("provider\u{1f}model"),
            Some(("provider".into(), "model".into()))
        );
        assert_eq!(model_pair("model"), None);
    }

    #[test]
    fn fresh_approval_modes_exclude_bypass() {
        assert!(valid_approval("manual"));
        assert!(valid_approval("smart"));
        assert!(!valid_approval("off"));
    }
}
