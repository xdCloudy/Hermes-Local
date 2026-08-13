use dioxus::prelude::*;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ReactionItem {
    pub key: String,
    pub label: String,
    pub current: Option<String>,
    pub agent: Vec<String>,
}

const OPTIONS: &[(&str, &str)] = &[
    ("❤️", "Heart"),
    ("👍", "Like"),
    ("👎", "Dislike"),
    ("😂", "Laugh"),
    ("‼️", "Emphasize"),
    ("❓", "Question"),
];

#[component]
pub(super) fn ReactionView(
    items: Vec<ReactionItem>,
    on_react: Callback<(String, String)>,
) -> Element {
    rsx! {
        if !items.is_empty() {
            details { class: "chat-reaction-panel",
                summary { "Message reactions" }
                for item in items {
                    div { class: "reaction-message", key: "{item.key}",
                        small { "{item.label}" }
                        if !item.agent.is_empty() {
                            div { class: "reaction-agent", aria_label: "Agent reactions",
                                for (index, emoji) in item.agent.iter().enumerate() {
                                    span { key: "{index}", title: "Agent reaction", "{emoji}" }
                                }
                            }
                        }
                        div { class: "reaction-buttons", role: "group", aria_label: "React to message",
                            for (reaction, label) in OPTIONS {
                                button {
                                    class: if item.current.as_deref() == Some(*reaction) { "reaction active" } else { "reaction" },
                                    aria_pressed: item.current.as_deref() == Some(*reaction),
                                    title: "{label}",
                                    onclick: {
                                        let key = item.key.clone();
                                        let reaction = (*reaction).to_owned();
                                        move |_| on_react.call((key.clone(), reaction.clone()))
                                    },
                                    "{reaction}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
