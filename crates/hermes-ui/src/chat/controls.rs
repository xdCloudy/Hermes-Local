use dioxus::prelude::*;
use hermes_core::{AppServices, ServiceResult, SessionService};
use hermes_protocol::ModelSettingsSnapshot;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ModelChoice {
    pub provider: String,
    pub model: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct PendingChatControls {
    pub model: Option<ModelChoice>,
    pub approval: Option<String>,
    pub tools: Option<String>,
    pub yolo: bool,
}

fn encode_choice(provider: &str, model: &str) -> String {
    format!("{provider}\u{1f}{model}")
}

fn decode_choice(value: &str) -> Option<ModelChoice> {
    let (provider, model) = value.split_once('\u{1f}')?;
    (!provider.is_empty() && !model.is_empty()).then(|| ModelChoice {
        provider: provider.to_owned(),
        model: model.to_owned(),
    })
}

pub(super) async fn apply_pending_controls(
    service: &dyn SessionService,
    session_id: &str,
    controls: &PendingChatControls,
) -> ServiceResult<()> {
    if let Some(model) = &controls.model {
        let _ = service
            .set_model(session_id, &model.provider, &model.model)
            .await?;
    }
    if let Some(mode) = controls.approval.as_deref() {
        let _ = service
            .execute_directive(session_id, &format!("/approvals {mode}"))
            .await?;
    }
    if let Some(tools) = controls.tools.as_deref() {
        let tools = tools.trim();
        if !tools.is_empty() {
            let _ = service
                .execute_directive(session_id, &format!("/tools {tools}"))
                .await?;
        }
    }
    if controls.yolo {
        let _ = service.set_yolo(session_id, true).await?;
    }
    Ok(())
}

#[component]
pub(super) fn ChatControls(
    session_id: Option<String>,
    mut pending: Signal<PendingChatControls>,
) -> Element {
    let services = use_context::<AppServices>();
    let mut catalog = use_signal(|| None::<ModelSettingsSnapshot>);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut notice = use_signal(|| None::<String>);

    let model_service = services.models.clone();
    let _models = use_resource(move || {
        let service = model_service.clone();
        async move {
            match service.load(None).await {
                Ok(value) => {
                    catalog.set(Some(value));
                    error.set(None);
                }
                Err(problem) => error.set(Some(problem.to_string())),
            }
        }
    });

    let default_choice = catalog().as_ref().map(|snapshot| ModelChoice {
        provider: snapshot.info.provider.clone(),
        model: snapshot.info.model.clone(),
    });
    let selected = pending().model.or(default_choice);
    let selected_value = selected
        .as_ref()
        .map(|choice| encode_choice(&choice.provider, &choice.model))
        .unwrap_or_default();

    let model_session = session_id.clone();
    let model_sessions = services.sessions.clone();
    let on_model_change = move |event: Event<FormData>| {
        let Some(choice) = decode_choice(&event.value()) else {
            return;
        };
        let mut next = pending();
        next.model = Some(choice.clone());
        pending.set(next);
        let Some(session_id) = model_session.clone() else {
            notice.set(Some("Model staged for the new session.".into()));
            return;
        };
        busy.set(true);
        error.set(None);
        notice.set(None);
        let service = model_sessions.clone();
        spawn(async move {
            match service
                .set_model(&session_id, &choice.provider, &choice.model)
                .await
            {
                Ok(true) => notice.set(Some("Model queued for the next turn.".into())),
                Ok(false) => notice.set(Some("Session model updated.".into())),
                Err(problem) => error.set(Some(problem.to_string())),
            }
            busy.set(false);
        });
    };

    let approval_session = session_id.clone();
    let approval_sessions = services.sessions.clone();
    let on_approval_change = move |event: Event<FormData>| {
        let value = event.value();
        let mode = match value.as_str() {
            "manual" | "smart" | "off" => Some(value.clone()),
            _ => None,
        };
        let mut next = pending();
        next.approval = mode.clone();
        pending.set(next);
        let (Some(session_id), Some(mode)) = (approval_session.clone(), mode) else {
            notice.set(Some("Approval mode staged for the new session.".into()));
            return;
        };
        busy.set(true);
        error.set(None);
        let service = approval_sessions.clone();
        spawn(async move {
            match service
                .execute_directive(&session_id, &format!("/approvals {mode}"))
                .await
            {
                Ok(_) => notice.set(Some(format!("Approval mode: {mode}"))),
                Err(problem) => error.set(Some(problem.to_string())),
            }
            busy.set(false);
        });
    };

    let tools_session = session_id.clone();
    let tools_sessions = services.sessions.clone();
    let apply_tools = move |_| {
        let tools = pending().tools.unwrap_or_default();
        let tools = tools.trim().to_owned();
        if tools.is_empty() || tools.len() > 256 {
            error.set(Some("Enter a bounded tool argument first.".into()));
            return;
        }
        let Some(session_id) = tools_session.clone() else {
            notice.set(Some("Tool selection staged for the new session.".into()));
            return;
        };
        busy.set(true);
        error.set(None);
        let service = tools_sessions.clone();
        spawn(async move {
            match service
                .execute_directive(&session_id, &format!("/tools {tools}"))
                .await
            {
                Ok(result) => notice.set(Some(
                    result
                        .display
                        .or(result.output)
                        .or(result.message)
                        .unwrap_or_else(|| "Tool availability updated.".into()),
                )),
                Err(problem) => error.set(Some(problem.to_string())),
            }
            busy.set(false);
        });
    };

    let yolo_session = session_id.clone();
    let yolo_sessions = services.sessions.clone();
    let toggle_yolo = move |_| {
        let next_value = !pending().yolo;
        let mut next = pending();
        next.yolo = next_value;
        pending.set(next);
        let Some(session_id) = yolo_session.clone() else {
            notice.set(Some(if next_value {
                "YOLO staged for this new session only.".into()
            } else {
                "YOLO staging cleared.".into()
            }));
            return;
        };
        busy.set(true);
        error.set(None);
        let service = yolo_sessions.clone();
        spawn(async move {
            match service.set_yolo(&session_id, next_value).await {
                Ok(active) => {
                    let mut state = pending();
                    state.yolo = active;
                    pending.set(state);
                    notice.set(Some(if active {
                        "YOLO enabled for this session only.".into()
                    } else {
                        "YOLO disabled for this session.".into()
                    }));
                }
                Err(problem) => {
                    let mut state = pending();
                    state.yolo = !next_value;
                    pending.set(state);
                    error.set(Some(problem.to_string()));
                }
            }
            busy.set(false);
        });
    };

    let staged = pending();
    rsx! {
        div { class: "chat-runtime-controls", aria_label: "Chat runtime controls",
            if let Some(snapshot) = catalog() {
                label { class: "composer-model-picker",
                    span { class: "sr-only", "Model" }
                    select {
                        value: "{selected_value}",
                        disabled: busy(),
                        onchange: on_model_change,
                        for provider in snapshot.options.providers {
                            optgroup { label: "{provider.name}",
                                for model in provider.models {
                                    option {
                                        value: "{encode_choice(&provider.slug, &model)}",
                                        "{provider.name} · {model}"
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                span { class: "composer-model", "Loading models…" }
            }
            label { class: "composer-approval-picker",
                span { class: "sr-only", "Approval mode" }
                select {
                    value: staged.approval.clone().unwrap_or_else(|| "inherit".into()),
                    disabled: busy(),
                    onchange: on_approval_change,
                    option { value: "inherit", "Approvals: inherit" }
                    option { value: "manual", "Approvals: manual" }
                    option { value: "smart", "Approvals: smart" }
                    option { value: "off", "Approvals: off" }
                }
            }
            input {
                class: "composer-tool-input",
                aria_label: "Tool control argument",
                placeholder: "tools: list | +name | -name",
                value: staged.tools.clone().unwrap_or_default(),
                disabled: busy(),
                oninput: move |event| {
                    let mut next = pending();
                    next.tools = Some(event.value());
                    pending.set(next);
                },
            }
            button {
                class: "composer-tool",
                disabled: busy(),
                onclick: apply_tools,
                "Apply tools"
            }
            button {
                class: if staged.yolo { "composer-tool active danger" } else { "composer-tool" },
                disabled: busy(),
                title: "Session-scoped approval bypass",
                aria_pressed: staged.yolo,
                onclick: toggle_yolo,
                if staged.yolo { "YOLO on" } else { "YOLO off" }
            }
        }
        if let Some(message) = notice() {
            small { class: "composer-control-notice", role: "status", "{message}" }
        }
        if let Some(message) = error() {
            small { class: "inline-error", role: "alert", "{message}" }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_choice_round_trips() {
        let encoded = encode_choice("provider", "model-name");
        assert_eq!(
            decode_choice(&encoded),
            Some(ModelChoice {
                provider: "provider".into(),
                model: "model-name".into(),
            })
        );
    }

    #[test]
    fn malformed_model_choice_is_rejected() {
        assert_eq!(decode_choice("provider-only"), None);
    }
}
