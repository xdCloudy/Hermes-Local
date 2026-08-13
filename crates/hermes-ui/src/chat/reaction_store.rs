use std::collections::BTreeMap;

use hermes_protocol::{AppSettings, ChatMessage};
use serde_json::Value;

const KEY: &str = "hermes.chat.reactions.v1";

pub(super) fn message_key(message: &ChatMessage) -> String {
    if let Some(value) = message.metadata.get("row_id") {
        if let Some(value) = value.as_str().filter(|value| !value.is_empty()) {
            return format!("row:{value}");
        }
        if let Some(value) = value.as_i64() {
            return format!("row:{value}");
        }
        if let Some(value) = value.as_u64() {
            return format!("row:{value}");
        }
    }
    format!("message:{}", message.id)
}

pub(super) fn load(settings: &AppSettings) -> BTreeMap<String, String> {
    settings
        .extra
        .get(KEY)
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default()
}

pub(super) fn store(settings: &mut AppSettings, reactions: &BTreeMap<String, String>) {
    let value = serde_json::to_value(reactions).unwrap_or(Value::Object(Default::default()));
    settings.extra.insert(KEY.into(), value);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durable_row_identity_wins() {
        let mut message = ChatMessage {
            id: "renderer-9".into(),
            ..ChatMessage::default()
        };
        message.metadata.insert("row_id".into(), Value::from(42));
        assert_eq!(message_key(&message), "row:42");
    }

    #[test]
    fn settings_round_trip() {
        let mut settings = AppSettings::default();
        let mut reactions = BTreeMap::new();
        reactions.insert("row:42".into(), "up".into());
        store(&mut settings, &reactions);
        assert_eq!(load(&settings), reactions);
    }
}
