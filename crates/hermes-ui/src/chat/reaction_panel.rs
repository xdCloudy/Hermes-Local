use std::collections::BTreeMap;

use dioxus::prelude::*;
use futures_util::StreamExt;
use hermes_core::AppServices;
use hermes_protocol::{ChatMessage, GatewayEvent, MessageRole};
use serde_json::Value;

use super::reaction_store;
use super::reaction_view::{ReactionItem, ReactionView};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct ReactionUpdate {
    key: String,
    role: MessageRole,
    user: Option<String>,
    agent: Vec<String>,
}

fn row_key(value: &Value) -> Option<String> {
    value
        .as_i64()
        .map(|value| format!("row:{value}"))
        .or_else(|| value.as_u64().map(|value| format!("row:{value}")))
        .or_else(|| {
            value
                .as_str()
                .filter(|value| !value.is_empty())
                .map(|value| format!("row:{value}"))
        })
}

fn reaction_update(event: &GatewayEvent) -> Option<ReactionUpdate> {
    if event.kind != "message.reaction" {
        return None;
    }
    let key = row_key(event.payload.get("row_id")?)?;
    let role = if event.payload.get("role").and_then(Value::as_str) == Some("assistant") {
        MessageRole::Assistant
    } else {
        MessageRole::User
    };
    let mut user = None;
    let mut agent = Vec::new();
    for reaction in event
        .payload
        .get("reactions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(emoji) = reaction.get("emoji").and_then(Value::as_str) else {
            continue;
        };
        match reaction.get("author").and_then(Value::as_str) {
            Some("agent") => agent.push(emoji.to_owned()),
            Some("user") => user = Some(emoji.to_owned()),
            _ => {}
        }
    }
    Some(ReactionUpdate {
        key,
        role,
        user,
        agent,
    })
}

fn bind_row_identity(messages: &mut [ChatMessage], update: &ReactionUpdate) {
    if messages
        .iter()
        .any(|message| reaction_store::message_key(message) == update.key)
    {
        return;
    }
    let Some(message) = messages
        .iter_mut()
        .rfind(|message| message.role == update.role && !message.streaming)
    else {
        return;
    };
    let value = update.key.trim_start_matches("row:");
    let row_id = value
        .parse::<i64>()
        .map(Value::from)
        .unwrap_or_else(|_| Value::String(value.to_owned()));
    message.metadata.insert("row_id".into(), row_id);
}

fn visible(
    messages: Vec<ChatMessage>,
    reactions: &BTreeMap<String, String>,
    agent_reactions: &BTreeMap<String, Vec<String>>,
) -> Vec<ReactionItem> {
    messages
        .into_iter()
        .filter(|message| {
            matches!(message.role, MessageRole::Assistant | MessageRole::User) && !message.streaming
        })
        .rev()
        .take(12)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|message| {
            let key = reaction_store::message_key(&message);
            ReactionItem {
                current: reactions.get(&key).cloned(),
                agent: agent_reactions.get(&key).cloned().unwrap_or_default(),
                key,
                label: message.text.chars().take(72).collect(),
            }
        })
        .collect()
}

#[component]
pub(super) fn ReactionPanel(session_id: String) -> Element {
    let services = use_context::<AppServices>();
    let mut reactions = use_signal(BTreeMap::<String, String>::new);
    let mut agent_reactions = use_signal(BTreeMap::<String, Vec<String>>::new);
    let mut messages = use_signal(Vec::<ChatMessage>::new);
    let mut runtime_id = use_signal(String::new);
    let mut error = use_signal(String::new);

    let load_services = services.clone();
    let stored_id = session_id.clone();
    let _load = use_resource(move || {
        let services = load_services.clone();
        let stored_id = stored_id.clone();
        async move {
            if let Ok(settings) = services.settings.load().await {
                reactions.set(reaction_store::load(&settings));
            }
            match services.sessions.resume(&stored_id).await {
                Ok(session) => {
                    runtime_id.set(session.session_id);
                    messages.set(session.messages);
                    error.set(String::new());
                }
                Err(problem) => error.set(problem.to_string()),
            }
        }
    });

    let event_service = services.sessions.clone();
    let _events = use_resource(move || {
        let runtime = runtime_id();
        let service = event_service.clone();
        async move {
            if runtime.is_empty() {
                return;
            }
            let Ok(mut events) = service.events() else {
                return;
            };
            while let Some(event) = events.next().await {
                if event.session_id.as_deref() != Some(runtime.as_str()) {
                    continue;
                }
                let Some(update) = reaction_update(&event) else {
                    continue;
                };
                bind_row_identity(messages.write().as_mut_slice(), &update);
                if let Some(user) = update.user.clone() {
                    reactions.write().insert(update.key.clone(), user);
                } else {
                    reactions.write().remove(&update.key);
                }
                if update.agent.is_empty() {
                    agent_reactions.write().remove(&update.key);
                } else {
                    agent_reactions
                        .write()
                        .insert(update.key.clone(), update.agent);
                }
            }
        }
    });

    let save_services = services.clone();
    let on_react = Callback::new(move |(key, reaction): (String, String)| {
        let previous = reactions();
        let mut next = previous.clone();
        if next.get(&key) == Some(&reaction) {
            next.remove(&key);
        } else {
            next.insert(key, reaction);
        }
        reactions.set(next.clone());
        error.set(String::new());
        let services = save_services.clone();
        spawn(async move {
            let result = async {
                let mut settings = services.settings.load().await?;
                reaction_store::store(&mut settings, &next);
                services.settings.save(&settings).await
            }
            .await;
            if let Err(problem) = result {
                reactions.set(previous);
                error.set(problem.to_string());
            }
        });
    });

    rsx! {
        ReactionView {
            items: visible(messages(), &reactions(), &agent_reactions()),
            on_react,
        }
        if !error().is_empty() {
            small { class: "inline-error", role: "alert", "{error}" }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn hides_streaming_messages() {
        let messages = vec![
            ChatMessage {
                id: "u".into(),
                role: MessageRole::User,
                text: "hello".into(),
                ..ChatMessage::default()
            },
            ChatMessage {
                id: "a".into(),
                role: MessageRole::Assistant,
                text: "hi".into(),
                streaming: true,
                ..ChatMessage::default()
            },
        ];
        let items = visible(messages, &BTreeMap::new(), &BTreeMap::new());
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].key, "message:u");
    }

    #[test]
    fn parses_authoritative_reaction_event_and_binds_live_row() {
        let event = GatewayEvent {
            kind: "message.reaction".into(),
            session_id: Some("runtime".into()),
            profile: None,
            payload: json!({
                "row_id": 42,
                "role": "assistant",
                "reactions": [
                    { "emoji": "👍", "author": "agent", "at": 1.0 },
                    { "emoji": "❤️", "author": "user", "at": 2.0 }
                ]
            }),
            extra: Default::default(),
        };
        let update = reaction_update(&event).expect("reaction update");
        assert_eq!(update.key, "row:42");
        assert_eq!(update.user.as_deref(), Some("❤️"));
        assert_eq!(update.agent, vec!["👍"]);

        let mut messages = vec![ChatMessage {
            id: "optimistic".into(),
            role: MessageRole::Assistant,
            text: "done".into(),
            ..ChatMessage::default()
        }];
        bind_row_identity(&mut messages, &update);
        assert_eq!(reaction_store::message_key(&messages[0]), "row:42");
    }
}
