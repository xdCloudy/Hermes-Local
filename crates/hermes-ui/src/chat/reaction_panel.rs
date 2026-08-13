use std::collections::BTreeMap;

use dioxus::prelude::*;
use hermes_core::AppServices;
use hermes_protocol::{ChatMessage, MessageRole};

use super::reaction_store;
use super::reaction_view::{ReactionItem, ReactionView};

fn visible(messages: Vec<ChatMessage>, reactions: &BTreeMap<String, String>) -> Vec<ReactionItem> {
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
    let mut messages = use_signal(Vec::<ChatMessage>::new);
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
                Ok(session) => messages.set(session.messages),
                Err(problem) => error.set(problem.to_string()),
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
        ReactionView { items: visible(messages(), &reactions()), on_react }
        if !error().is_empty() {
            small { class: "inline-error", role: "alert", "{error}" }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let items = visible(messages, &BTreeMap::new());
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].key, "message:u");
    }
}
