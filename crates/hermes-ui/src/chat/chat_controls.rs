use dioxus::prelude::*;
use hermes_core::AppServices;
use hermes_protocol::{ModelAssignmentRequest, ModelSettingsSnapshot};

fn model_pair(value: &str) -> Option<(String, String)> {
    let (provider, model) = value.split_once('\u{1f}')?;
    (!provider.is_empty() && !model.is_empty()).then(|| (provider.into(), model.into()))
}

async fn session_runtime(services: &AppServices, stored: &str) -> Result<String, String> {
    services
        .sessions
        .resume(stored)
        .await
        .map(|value| value.session_id)
        .map_err(|error| error.to_string())
}

#[component]
pub(super) fn ChatControls(session: Option<String>) -> Element {
    let services = use_context::<AppServices>();
    let mut models = use_signal(|| None::<ModelSettingsSnapshot>);
    let mut selected = use_signal(String::new);
    let mut tools = use_signal(String::new);
    let mut status = use_signal(String::new);
    let mut busy = use_signal(|| false);

    let loader = services.models.clone();
    let _load = use_resource(move || {
        let loader = loader.clone();
        async move {
            if let Ok(value) = loader.load(None).await {
                if selected().is_empty() {
                    selected.set(format!("{}\u{1f}{}", value.info.provider, value.info.model));
                }
                models.set(Some(value));
            }
        }
    });

    let change_services = services.clone();
    let change_session = session.clone();
    let change_model = move |event: Event<FormData>| {
        let Some((provider, model)) = model_pair(&event.value()) else {
            return;
        };
        selected.set(format!("{provider}\u{1f}{model}"));
        let services = change_services.clone();
        let session = change_session.clone();
        busy.set(true);
        spawn(async move {
            let result = if let Some(stored) = session {
                match session_runtime(&services, &stored).await {
                    Ok(runtime) => services
                        .sessions
                        .execute_directive(
                            &runtime,
                            &format!("/model {model} --provider {provider} --session"),
                        )
                        .await
                        .map(|_| "Session model updated".to_owned()),
                    Err(error) => Err(hermes_core::ServiceError::Platform(error)),
                }
            } else {
                services
                    .models
                    .assign(
                        None,
                        &ModelAssignmentRequest {
                            model,
                            provider,
                            scope: "main".into(),
                            task: None,
                            base_url: None,
                        },
                    )
                    .await
                    .map(|_| "Default model updated".to_owned())
            };
            status.set(result.unwrap_or_else(|error| error.to_string()));
            busy.set(false);
        });
    };

    let directive = |command: String,
                     services: AppServices,
                     stored: String,
                     mut status: Signal<String>,
                     mut busy: Signal<bool>| {
        spawn(async move {
            let result = match session_runtime(&services, &stored).await {
                Ok(runtime) => {
                    services
                        .sessions
                        .execute_directive(&runtime, &command)
                        .await
                }
                Err(error) => Err(hermes_core::ServiceError::Platform(error)),
            };
            status.set(result.map_or_else(|error| error.to_string(), |_| command));
            busy.set(false);
        });
    };

    rsx! {
        div { class: "chat-runtime-controls", aria_label: "Chat runtime controls",
            if let Some(catalog) = models() {
                select { value: "{selected}", disabled: busy(), aria_label: "Model", onchange: change_model,
                    for provider in catalog.options.providers {
                        for model in provider.models {
                            option { value: "{provider.slug}\u{1f}{model}", "{provider.name} · {model}" }
                        }
                    }
                }
            }
            if let Some(stored) = session.clone() {
                select { disabled: busy(), aria_label: "Approval mode", onchange: {
                    let services = services.clone();
                    let stored = stored.clone();
                    move |event: Event<FormData>| {
                        busy.set(true);
                        directive(
                            format!("/approvals {}", event.value()),
                            services.clone(),
                            stored.clone(),
                            status,
                            busy,
                        );
                    }
                },
                    option { value: "manual", "Approvals: manual" }
                    option { value: "smart", "Approvals: smart" }
                    option { value: "off", "Approvals: off" }
                }
                input {
                    value: "{tools}",
                    aria_label: "Tool control",
                    placeholder: "tools: list | +name | -name",
                    oninput: move |event| tools.set(event.value()),
                }
                button { disabled: busy(), onclick: {
                    let services = services.clone();
                    let stored = stored.clone();
                    move |_| {
                        let value = tools().trim().to_owned();
                        if !value.is_empty() && value.len() <= 256 {
                            busy.set(true);
                            directive(
                                format!("/tools {value}"),
                                services.clone(),
                                stored.clone(),
                                status,
                                busy,
                            );
                        }
                    }
                }, "Apply tools" }
                button { disabled: busy(), onclick: {
                    let services = services.clone();
                    let stored = stored.clone();
                    move |_| {
                        busy.set(true);
                        directive(
                            "/approvals off".into(),
                            services.clone(),
                            stored.clone(),
                            status,
                            busy,
                        );
                    }
                }, "YOLO" }
            }
        }
        if !status().is_empty() {
            small { role: "status", "{status}" }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_model_pair() {
        assert_eq!(model_pair("p\u{1f}m"), Some(("p".into(), "m".into())));
        assert_eq!(model_pair("invalid"), None);
    }
}
