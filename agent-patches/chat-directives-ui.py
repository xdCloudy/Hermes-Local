from pathlib import Path

CHAT = Path("crates/hermes-ui/src/chat.rs")


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one match, found {count}")
    return text.replace(old, new, 1)


text = CHAT.read_text(encoding="utf-8")
text = replace_once(
    text,
    "    let submit_service = services.sessions.clone();\n    let interrupt_service = services.sessions.clone();\n",
    "    let submit_service = services.sessions.clone();\n    let directive_service = services.sessions.clone();\n    let interrupt_service = services.sessions.clone();\n",
    "directive service binding",
)

start = text.index("    let mut send_error = use_signal(|| None::<String>);\n    let send = Callback::new(move |()| {")
end = text.index("\n    let remove_queue_id = id.clone();", start)
old = text[start:end]
new = r'''    let mut send_error = use_signal(|| None::<String>);
    let send = Callback::new(move |()| {
        let text = draft().trim().to_owned();
        let Some(before) = transcript() else {
            return;
        };
        if text.is_empty() {
            return;
        }
        let stored_id = before.stored_id.clone();

        if text.starts_with('/') {
            let runtime_id = before.runtime_id.clone();
            let service = directive_service.clone();
            let submit = submit_service.clone();
            draft.set(String::new());
            chat_runtime.drafts.write().clear(&stored_id);
            mark_draft_changed(chat_runtime.draft_revision);
            send_error.set(None);
            spawn(async move {
                let mut command = text.clone();
                for _ in 0..4 {
                    let result = match service.execute_directive(&runtime_id, &command).await {
                        Ok(result) => result,
                        Err(error) => {
                            draft.set(text.clone());
                            chat_runtime
                                .drafts
                                .write()
                                .replace_without_history(&stored_id, text.clone());
                            mark_draft_changed(chat_runtime.draft_revision);
                            send_error.set(Some(error.to_string()));
                            return;
                        }
                    };

                    if let Some(notice) = result.notice.filter(|notice| !notice.trim().is_empty())
                        && let Some(state) = transcript.write().as_mut()
                    {
                        state.push_system(notice);
                    }

                    match result.kind.as_str() {
                        "alias" => {
                            let Some(target) = result.target.filter(|target| !target.trim().is_empty()) else {
                                send_error.set(Some("directive alias did not provide a target".into()));
                                return;
                            };
                            let arg = command
                                .split_once(char::is_whitespace)
                                .map(|(_, arg)| arg.trim())
                                .unwrap_or_default();
                            command = if arg.is_empty() || target.chars().any(char::is_whitespace) {
                                target
                            } else {
                                format!("{target} {arg}")
                            };
                        }
                        "prefill" => {
                            let value = result.message.unwrap_or_default();
                            draft.set(value.clone());
                            chat_runtime.drafts.write().edit(&stored_id, value);
                            mark_draft_changed(chat_runtime.draft_revision);
                            return;
                        }
                        "send" | "skill" => {
                            let message = result.message.unwrap_or_default();
                            if message.trim().is_empty() {
                                send_error.set(Some("directive returned an empty message".into()));
                                return;
                            }
                            if before.busy {
                                chat_runtime.queue.write().enqueue(&stored_id, message);
                                return;
                            }
                            let display = result
                                .display
                                .filter(|display| !display.trim().is_empty())
                                .unwrap_or_else(|| message.clone());
                            chat_runtime.queue.write().mark_busy(&stored_id, true);
                            if let Some(state) = transcript.write().as_mut() {
                                let optimistic_id = format!("user-directive-{}", state.messages.len());
                                state.push_user(optimistic_id, display);
                            }
                            if let Err(error) = submit.submit(&runtime_id, &message).await {
                                chat_runtime.queue.write().mark_busy(&stored_id, false);
                                draft.set(text.clone());
                                chat_runtime
                                    .drafts
                                    .write()
                                    .replace_without_history(&stored_id, text.clone());
                                mark_draft_changed(chat_runtime.draft_revision);
                                send_error.set(Some(error.to_string()));
                            }
                            return;
                        }
                        _ => {
                            let output = result
                                .output
                                .or(result.message)
                                .unwrap_or_else(|| "Command completed.".into());
                            if let Some(state) = transcript.write().as_mut() {
                                state.push_system(output);
                            }
                            return;
                        }
                    }
                }
                draft.set(text.clone());
                chat_runtime
                    .drafts
                    .write()
                    .replace_without_history(&stored_id, text);
                mark_draft_changed(chat_runtime.draft_revision);
                send_error.set(Some("directive alias depth exceeded".into()));
            });
            return;
        }

        if before.busy {
            chat_runtime.queue.write().enqueue(&stored_id, text);
            draft.set(String::new());
            chat_runtime.drafts.write().clear(&stored_id);
            mark_draft_changed(chat_runtime.draft_revision);
            send_error.set(None);
            return;
        }
        let runtime_id = before.runtime_id.clone();
        let optimistic_id = format!("user-local-{}", before.messages.len());
        chat_runtime.queue.write().mark_busy(&stored_id, true);
        if let Some(state) = transcript.write().as_mut() {
            state.push_user(optimistic_id, text.clone());
        }
        draft.set(String::new());
        chat_runtime.drafts.write().clear(&stored_id);
        mark_draft_changed(chat_runtime.draft_revision);
        send_error.set(None);
        let service = submit_service.clone();
        spawn(async move {
            if let Err(error) = service.submit(&runtime_id, &text).await {
                chat_runtime.queue.write().mark_busy(&stored_id, false);
                transcript.set(Some(before));
                draft.set(text.clone());
                chat_runtime
                    .drafts
                    .write()
                    .replace_without_history(&stored_id, text);
                mark_draft_changed(chat_runtime.draft_revision);
                send_error.set(Some(error.to_string()));
            }
        });
    });
'''
text = text[:start] + new + text[end:]

text = replace_once(
    text,
    '''                                    if message.role == MessageRole::Tool {
                                        article { class: "tool-message",
                                            div { class: "tool-message-head", Codicon { name: "tools" } strong { if let Some(name) = message.tool_name.as_deref() { "{name}" } else { "Tool" } } span { if message.streaming { "Running" } else { "Done" } } }
                                            if !message.text.is_empty() { pre { "{message.text}" } }
                                        }
                                    } else {
                                        article { class: if message.role == MessageRole::User { "message user" } else { "message assistant" },
                                            div { class: "message-role", if message.role == MessageRole::User { "You" } else { "Hermes" } }
''',
    '''                                    if message.role == MessageRole::Tool {
                                        article { class: "tool-message",
                                            div { class: "tool-message-head", Codicon { name: "tools" } strong { if let Some(name) = message.tool_name.as_deref() { "{name}" } else { "Tool" } } span { if message.streaming { "Running" } else { "Done" } } }
                                            if !message.text.is_empty() { pre { "{message.text}" } }
                                        }
                                    } else {
                                        article {
                                            class: if message.role == MessageRole::User { "message user" } else if message.role == MessageRole::System { "message system" } else { "message assistant" },
                                            div { class: "message-role", if message.role == MessageRole::User { "You" } else if message.role == MessageRole::System { "System" } else { "Hermes" } }
''',
    "system message rendering",
)

CHAT.write_text(text, encoding="utf-8")
