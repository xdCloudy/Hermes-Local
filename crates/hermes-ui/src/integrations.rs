use std::collections::BTreeMap;

use dioxus::prelude::*;
use hermes_core::AppServices;
use hermes_protocol::{
    MessagingPlatform, MessagingPlatformUpdate, PairingSnapshot, PairingUser, WebhookCreate,
    WebhooksSnapshot,
};

use super::Surface;

fn platform_state(platform: &MessagingPlatform) -> &'static str {
    if !platform.enabled {
        "Disabled"
    } else if platform.gateway_running {
        "Connected"
    } else if platform.configured {
        "Configured"
    } else {
        "Needs setup"
    }
}

fn pairing_label(user: &PairingUser) -> String {
    user.user_name
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&user.user_id)
        .to_owned()
}

#[component]
pub(super) fn Integrations() -> Element {
    let services = use_context::<AppServices>();
    let mut platforms = use_signal(Vec::<MessagingPlatform>::new);
    let mut pairing = use_signal(PairingSnapshot::default);
    let mut webhooks = use_signal(WebhooksSnapshot::default);
    let mut loading = use_signal(|| true);
    let mut busy = use_signal(|| None::<String>);
    let mut error = use_signal(|| None::<String>);
    let mut notice = use_signal(|| None::<String>);
    let mut refresh = use_signal(|| 0_u64);
    let mut selected_platform = use_signal(|| None::<String>);
    let mut env_draft = use_signal(BTreeMap::<String, String>::new);
    let mut webhook_name = use_signal(String::new);
    let mut webhook_description = use_signal(String::new);
    let mut webhook_prompt = use_signal(String::new);
    let mut created_secret = use_signal(|| None::<(String, String)>);
    let mut pending_webhook_delete = use_signal(|| None::<String>);
    let mut pending_revoke = use_signal(|| None::<PairingUser>);

    let load_services = services.clone();
    let _load = use_resource(move || {
        let services = load_services.clone();
        let _revision = refresh();
        async move {
            loading.set(true);
            let next_platforms = services.integrations.messaging_platforms().await;
            let next_pairing = services.integrations.pairing().await;
            let next_webhooks = services.integrations.webhooks().await;
            match (next_platforms, next_pairing, next_webhooks) {
                (Ok(next_platforms), Ok(next_pairing), Ok(next_webhooks)) => {
                    if selected_platform()
                        .as_ref()
                        .is_some_and(|id| !next_platforms.iter().any(|platform| &platform.id == id))
                    {
                        selected_platform.set(None);
                        env_draft.set(BTreeMap::new());
                    }
                    platforms.set(next_platforms);
                    pairing.set(next_pairing);
                    webhooks.set(next_webhooks);
                    error.set(None);
                }
                (platform_result, pairing_result, webhook_result) => {
                    error.set(Some(
                        platform_result
                            .err()
                            .or_else(|| pairing_result.err())
                            .or_else(|| webhook_result.err())
                            .map_or_else(
                                || "Integrations unavailable".into(),
                                |problem| problem.to_string(),
                            ),
                    ));
                }
            }
            loading.set(false);
        }
    });

    let update_services = services.clone();
    let update_platform = Callback::new(
        move |(id, update, message): (String, MessagingPlatformUpdate, String)| {
            if busy().is_some() {
                return;
            }
            busy.set(Some(id.clone()));
            error.set(None);
            notice.set(None);
            let services = update_services.clone();
            spawn(async move {
                match services
                    .integrations
                    .update_messaging_platform(&id, &update)
                    .await
                {
                    Ok(()) => {
                        notice.set(Some(message));
                        env_draft.set(BTreeMap::new());
                        refresh.set(refresh() + 1);
                    }
                    Err(problem) => error.set(Some(problem.to_string())),
                }
                busy.set(None);
            });
        },
    );

    let test_services = services.clone();
    let test_platform = Callback::new(move |id: String| {
        if busy().is_some() {
            return;
        }
        busy.set(Some(id.clone()));
        error.set(None);
        notice.set(None);
        let services = test_services.clone();
        spawn(async move {
            match services.integrations.test_messaging_platform(&id).await {
                Ok(result) if result.ok => notice.set(Some(if result.message.is_empty() {
                    "Messaging connection test passed".into()
                } else {
                    result.message
                })),
                Ok(result) => error.set(Some(if result.message.is_empty() {
                    "Messaging connection test failed".into()
                } else {
                    result.message
                })),
                Err(problem) => error.set(Some(problem.to_string())),
            }
            busy.set(None);
        });
    });

    let pairing_services = services.clone();
    let approve = Callback::new(move |user: PairingUser| {
        let Some(request_id) = user.request_id.clone() else {
            error.set(Some("Pairing request has no approval identifier".into()));
            return;
        };
        if busy().is_some() {
            return;
        }
        busy.set(Some(user.user_id.clone()));
        error.set(None);
        let services = pairing_services.clone();
        spawn(async move {
            match services
                .integrations
                .approve_pairing(&user.platform, &request_id)
                .await
            {
                Ok(approved) => {
                    notice.set(Some(format!("Approved {}", pairing_label(&approved))));
                    refresh.set(refresh() + 1);
                }
                Err(problem) => error.set(Some(problem.to_string())),
            }
            busy.set(None);
        });
    });

    let revoke_services = services.clone();
    let revoke = Callback::new(move |user: PairingUser| {
        if busy().is_some() {
            return;
        }
        busy.set(Some(user.user_id.clone()));
        pending_revoke.set(None);
        error.set(None);
        let services = revoke_services.clone();
        spawn(async move {
            match services
                .integrations
                .revoke_pairing(&user.platform, &user.user_id)
                .await
            {
                Ok(()) => {
                    notice.set(Some("Messaging access revoked".into()));
                    refresh.set(refresh() + 1);
                }
                Err(problem) => error.set(Some(problem.to_string())),
            }
            busy.set(None);
        });
    });

    let webhook_services = services.clone();
    let enable_webhooks = Callback::new(move |()| {
        if busy().is_some() {
            return;
        }
        busy.set(Some("webhooks".into()));
        error.set(None);
        let services = webhook_services.clone();
        spawn(async move {
            match services.integrations.enable_webhooks().await {
                Ok(true) => {
                    notice.set(Some("Webhook gateway enabled".into()));
                    refresh.set(refresh() + 1);
                }
                Ok(false) => error.set(Some("Agent did not enable the webhook gateway".into())),
                Err(problem) => error.set(Some(problem.to_string())),
            }
            busy.set(None);
        });
    });

    let create_services = services.clone();
    let create_webhook = Callback::new(move |()| {
        if busy().is_some() {
            return;
        }
        let input = WebhookCreate {
            name: webhook_name().trim().to_owned(),
            description: Some(webhook_description().trim().to_owned()),
            prompt: Some(webhook_prompt().trim().to_owned()),
            ..WebhookCreate::default()
        };
        busy.set(Some("webhook-create".into()));
        error.set(None);
        let services = create_services.clone();
        spawn(async move {
            match services.integrations.create_webhook(&input).await {
                Ok(created) => {
                    created_secret.set(Some((created.route.name, created.secret)));
                    webhook_name.set(String::new());
                    webhook_description.set(String::new());
                    webhook_prompt.set(String::new());
                    notice.set(Some("Webhook created; save the one-time secret now".into()));
                    refresh.set(refresh() + 1);
                }
                Err(problem) => error.set(Some(problem.to_string())),
            }
            busy.set(None);
        });
    });

    let toggle_services = services.clone();
    let toggle_webhook = Callback::new(move |(name, enabled): (String, bool)| {
        if busy().is_some() {
            return;
        }
        busy.set(Some(name.clone()));
        error.set(None);
        let services = toggle_services.clone();
        spawn(async move {
            match services
                .integrations
                .set_webhook_enabled(&name, enabled)
                .await
            {
                Ok(_) => {
                    notice.set(Some("Webhook state updated".into()));
                    refresh.set(refresh() + 1);
                }
                Err(problem) => error.set(Some(problem.to_string())),
            }
            busy.set(None);
        });
    });

    let delete_services = services.clone();
    let delete_webhook = Callback::new(move |name: String| {
        if busy().is_some() {
            return;
        }
        busy.set(Some(name.clone()));
        pending_webhook_delete.set(None);
        error.set(None);
        let services = delete_services.clone();
        spawn(async move {
            match services.integrations.delete_webhook(&name).await {
                Ok(()) => {
                    notice.set(Some("Webhook deleted".into()));
                    refresh.set(refresh() + 1);
                }
                Err(problem) => error.set(Some(problem.to_string())),
            }
            busy.set(None);
        });
    });

    let selected = selected_platform()
        .and_then(|id| platforms().into_iter().find(|platform| platform.id == id));
    let webhook_gateway = if webhooks().enabled {
        "Enabled"
    } else {
        "Disabled"
    };
    rsx! {
        Surface { eyebrow: "Connectivity", title: "Integrations", subtitle: "Configure messaging access and secret-safe webhook subscriptions.",
            div { class: "settings-toolbar",
                button { class: "button", disabled: loading() || busy().is_some(), onclick: move |_| refresh.set(refresh() + 1), "Refresh" }
            }
            if loading() { div { class: "loading-state", role: "status", "◌ Loading integrations" } }
            if let Some(problem) = error() { div { class: "error-state", role: "alert", "{problem}" } }
            if let Some(message) = notice() { div { class: "success-state", role: "status", "{message}" } }
            section { class: "panel",
                header { class: "panel-title", "Messaging platforms" }
                if platforms().is_empty() { p { class: "muted", "No messaging platforms were reported by the Agent." } }
                div { class: "settings-list",
                    for platform in platforms() {
                        {
                            let select_id = platform.id.clone();
                            let toggle_id = platform.id.clone();
                            let test_id = platform.id.clone();
                            let enabled = platform.enabled;
                            rsx! { div { class: "settings-list-row", key: "{platform.id}",
                                button { class: "settings-row-copy", onclick: move |_| { selected_platform.set(Some(select_id.clone())); env_draft.set(BTreeMap::new()); },
                                    strong { "{platform.name}" }
                                    p { "{platform.description}" }
                                    if let Some(problem) = platform.error_message.as_deref() { p { class: "muted", "{problem}" } }
                                }
                                div { class: "settings-row-action",
                                    span { class: "badge", "{platform_state(&platform)}" }
                                    button { class: "button", disabled: busy().is_some(), onclick: move |_| test_platform.call(test_id.clone()), "Test" }
                                    button { class: "button", disabled: busy().is_some(), onclick: move |_| update_platform.call((toggle_id.clone(), MessagingPlatformUpdate { enabled: Some(!enabled), ..MessagingPlatformUpdate::default() }, if enabled { "Messaging platform disabled".into() } else { "Messaging platform enabled".into() })), if enabled { "Disable" } else { "Enable" } }
                                }
                            } }
                        }
                    }
                }
                if let Some(platform) = selected {
                    div { class: "panel",
                        header { class: "panel-title", "Configure {platform.name}" }
                        p { class: "muted", "Secret values are write-only; existing values are shown only as redacted state." }
                        for field in platform.env_vars {
                            {
                                let key = field.key.clone();
                                let clear_key = key.clone();
                                let input_key = key.clone();
                                let selected_id = platform.id.clone();
                                let field_label = format!(
                                    "{}{}",
                                    if field.prompt.is_empty() { &field.key } else { &field.prompt },
                                    if field.required { " · required" } else { "" },
                                );
                                rsx! { label { class: "field-stack", key: "{key}",
                                    span { "{field_label}" }
                                    input { r#type: if field.is_password { "password" } else { "text" }, autocomplete: "off", value: env_draft().get(&key).cloned().unwrap_or_default(), placeholder: field.redacted_value.clone().unwrap_or_else(|| "Not set".into()), oninput: move |event| { let mut next = env_draft(); next.insert(input_key.clone(), event.value()); env_draft.set(next); } }
                                    if field.is_set { button { class: "button", disabled: busy().is_some(), onclick: move |_| update_platform.call((selected_id.clone(), MessagingPlatformUpdate { clear_env: Some(vec![clear_key.clone()]), ..MessagingPlatformUpdate::default() }, "Stored credential cleared".into())), "Clear stored value" } }
                                } }
                            }
                        }
                        {
                            let id = platform.id.clone();
                            rsx! { button { class: "primary-button", disabled: busy().is_some() || env_draft().is_empty(), onclick: move |_| update_platform.call((id.clone(), MessagingPlatformUpdate { env: Some(env_draft()), ..MessagingPlatformUpdate::default() }, "Messaging credentials saved".into())), "Save values" } }
                        }
                    }
                }
            }
            section { class: "panel",
                header { class: "panel-title", "Messaging access" }
                p { class: "muted", "Approve pending users or explicitly revoke existing access." }
                for user in pairing().pending {
                    {
                        let approve_user = user.clone();
                        rsx! { div { class: "settings-list-row", key: "pending-{user.platform}-{user.user_id}",
                            div { class: "settings-row-copy", strong { "{pairing_label(&user)}" } p { "Pending · {user.platform}" } }
                            button { class: "primary-button", disabled: busy().is_some(), onclick: move |_| approve.call(approve_user.clone()), "Approve" }
                        } }
                    }
                }
                for user in pairing().approved {
                    {
                        let revoke_user = user.clone();
                        rsx! { div { class: "settings-list-row", key: "approved-{user.platform}-{user.user_id}",
                            div { class: "settings-row-copy", strong { "{pairing_label(&user)}" } p { "Approved · {user.platform}" } }
                            button { class: "button danger", disabled: busy().is_some(), onclick: move |_| pending_revoke.set(Some(revoke_user.clone())), "Revoke" }
                        } }
                    }
                }
            }
            section { class: "panel",
                header { class: "panel-title", "Webhooks" }
                p { class: "muted", "Gateway: {webhook_gateway} · {webhooks().base_url}" }
                if !webhooks().enabled { button { class: "primary-button", disabled: busy().is_some(), onclick: move |_| enable_webhooks.call(()), "Enable webhook gateway" } }
                if let Some((name, secret)) = created_secret() {
                    div { class: "success-state", role: "status",
                        strong { "One-time secret for {name}" }
                        p { class: "mono", "{secret}" }
                        p { "Save this now. It will not be shown again after dismissal." }
                        button { class: "button", onclick: move |_| created_secret.set(None), "I saved it" }
                    }
                }
                div { class: "settings-list",
                    for hook in webhooks().subscriptions {
                        {
                            let toggle_name = hook.name.clone();
                            let delete_name = hook.name.clone();
                            let enabled = hook.enabled;
                            rsx! { div { class: "settings-list-row", key: "{hook.name}",
                                div { class: "settings-row-copy", strong { "{hook.name}" } p { "{hook.description} · {hook.url}" } span { class: "badge", if hook.secret_set { "Secret set" } else { "No secret" } } }
                                div { class: "settings-row-action",
                                    button { class: "button", disabled: busy().is_some(), onclick: move |_| toggle_webhook.call((toggle_name.clone(), !enabled)), if enabled { "Disable" } else { "Enable" } }
                                    button { class: "button danger", disabled: busy().is_some(), onclick: move |_| pending_webhook_delete.set(Some(delete_name.clone())), "Delete" }
                                }
                            } }
                        }
                    }
                }
                header { class: "panel-title", "Create webhook" }
                label { class: "field-stack", span { "Name" } input { value: "{webhook_name}", maxlength: "256", oninput: move |event| webhook_name.set(event.value()) } }
                label { class: "field-stack", span { "Description" } input { value: "{webhook_description}", oninput: move |event| webhook_description.set(event.value()) } }
                label { class: "field-stack", span { "Prompt" } textarea { value: "{webhook_prompt}", rows: "4", maxlength: "1048576", oninput: move |event| webhook_prompt.set(event.value()) } }
                button { class: "primary-button", disabled: busy().is_some() || webhook_name().trim().is_empty(), onclick: move |_| create_webhook.call(()), "Create webhook" }
            }
            if let Some(name) = pending_webhook_delete() {
                div { class: "dialog-backdrop", role: "presentation",
                    section { class: "dialog-card", role: "dialog", aria_modal: "true", aria_label: "Confirm webhook deletion",
                        h2 { "Delete webhook {name}?" }
                        p { "The route and its secret will stop accepting events." }
                        div { class: "dialog-actions", button { class: "button", onclick: move |_| pending_webhook_delete.set(None), "Cancel" } button { class: "button danger", onclick: move |_| delete_webhook.call(name.clone()), "Delete webhook" } }
                    }
                }
            }
            if let Some(user) = pending_revoke() {
                div { class: "dialog-backdrop", role: "presentation",
                    section { class: "dialog-card", role: "dialog", aria_modal: "true", aria_label: "Confirm messaging access revocation",
                        h2 { "Revoke {pairing_label(&user)}?" }
                        p { "This user will no longer be allowed to message the Agent." }
                        div { class: "dialog-actions", button { class: "button", onclick: move |_| pending_revoke.set(None), "Cancel" } button { class: "button danger", onclick: move |_| revoke.call(user.clone()), "Revoke access" } }
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
    fn platform_state_is_explicit() {
        let platform = MessagingPlatform::default();
        assert_eq!(platform_state(&platform), "Disabled");
        assert_eq!(
            pairing_label(&PairingUser {
                user_id: "user-1".into(),
                ..PairingUser::default()
            }),
            "user-1"
        );
    }
}
