use dioxus::prelude::*;
use hermes_core::AppServices;
use hermes_protocol::{ModelAssignmentRequest, ModelSettingsSnapshot};

fn split_model(value: &str) -> Option<(String, String)> {
    let (provider, model) = value.split_once('\u{1f}')?;
    (!provider.is_empty() && !model.is_empty())
        .then(|| (provider.to_owned(), model.to_owned()))
}

async fn runtime_id(services: &AppServices, stored_id: &str) -> Result<String, String> {
    services
        .sessions
        .resume(stored_id)
        .await
        .map(|value| value.session_id)
        .map_err(|error| error.to_string())
}

#[component]
pub(super) fn RuntimeControls(session_id: Option<String>) -> Element {
    let services = use_context::<AppServices>();
    let mut catalog = use_signal(|| None::<ModelSettingsSnapshot>);
    let mut selected = use_signal(String::new);
    let mut tools = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut yolo = use_signal(|| false);
    let mut message = use_signal(|| None::<String>);

    let model_service = services.models.clone();
    let _models = use_resource(move || {
        let model_service = model_service.clone();
        async move {
            if let Ok(value) = model_service.load(None).await {
                if selected().is_empty() {
                    selected.set(format!("{}\u{1f}{}", value.info.provider, value.info.model));
                }
                catalog.set(Some(value));
            }
        }
    });

    let model_services = services.clone();
    let model_session = session_id.clone();
    let change_model = move |event: Event<FormData>| {
        let Some((provider, model)) = split_model(&event.value()) else { return };
        selected.set(format!("{provider}\u{1f}{model}"));
        busy.set(true);
        message.set(None);
        let services = model_services.clone();
        let stored_id = model_session.clone();
        spawn(async move {
            let result = if let Some(stored_id) = stored_id {
                match runtime_id(&services, &stored_id).await {
                    Ok(id) => services.sessions.execute_directive(
                        &id,
                        &format!("/model {model} --provider {provider} --session"),
                    ).await.map(|_| "Session model updated.".to_owned()),
                    Err(error) => Err(hermes_core::ServiceError::Platform(error)),
                }
            } else {
                services.models.assign(None, &ModelAssignmentRequest {
                    model,
                    provider,
                    scope: "main".into(),
                    task: None,
                    base_url: None,
                }).await.map(|_| "Default model updated for new chats.".to_owned())
            };
            message.set(Some(result.unwrap_or_else(|error| error.to_string())));
            busy.set(false);
        });
    };

    let directive_services = services.clone();
    let directive_session = session_id.clone();
    let run_directive = move |command: String| {
        let Some(stored_id) = directive_session.clone() else { return };
        busy.set(true);
        message.set(None);
        let services = directive_services.clone();
        spawn(async move {
            let result = match runtime_id(&services, &stored_id).await {
                Ok(id) => services.sessions.execute_directive(&id, &command).await
                    .map(|value| value.display.or(value.output).or(value.message).unwrap_or(command)),
                Err(error) => Err(hermes_core::ServiceError::Platform(error)),
            };
            message.set(Some(result.unwrap_or_else(|error| error.to_string())));
            busy.set(false);
        });
    };

    rsx! {
        div { class: "chat-runtime-controls", aria_label: "Chat runtime controls",
            if let Some(snapshot) = catalog() {
                select { value: "{selected}", disabled: busy(), aria_label: "Model", onchange: change_model,
                    for provider in snapshot.options.providers {
                        optgroup { label: "{provider.name}",
                            for model in provider.models {
                                option { value: "{provider.slug}\u{1f}{model}", "{provider.name} · {model}" }
                            }
                        }
                    }
                }
            }
            if session_id.is_some() {
                select { disabled: busy(), aria_label: "Approval mode", onchange: move |event| run_directive(format!("/approvals {}", event.value())),
                    option { value: "manual", "Approvals: manual" }
                    option { value: "smart", "Approvals: smart" }
                    option { value: "off", "Approvals: off" }
                }
                input { value: "{tools}", disabled: busy(), aria_label: "Tool control", placeholder: "tools: list | +name | -name", oninput: move |event| tools.set(event.value()) }
                button { disabled: busy(), onclick: move |_| {
                    let value = tools().trim().to_owned();
                    if !value.is_empty() && value.len() <= 256 { run_directive(format!("/tools {value}")); }
                }, "Apply tools" }
                button { disabled: busy(), aria_pressed: yolo(), onclick: move |_| {
                    let next = !yolo(); yolo.set(next);
                    run_directive(format!("/approvals {}", if next { "off" } else { "manual" }));
                }, if yolo() { "YOLO on" } else { "YOLO off" } }
            }
        }
        if let Some(value) = message() { small { role: "status", "{value}" } }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn model_value_is_unambiguous() {
        assert_eq!(split_model("p\u{1f}m"), Some(("p".into(), "m".into())));
        assert_eq!(split_model("p-only"), None);
    }
}
