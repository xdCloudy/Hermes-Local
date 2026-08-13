//! Chat/session presentation surfaces extracted from the shell so the A4 chat
//! migration can evolve behind the existing typed service boundary.

use dioxus::prelude::*;
use futures_util::StreamExt;
use hermes_core::{AppServices, PromptQueueCoordinator, SessionTranscript};
use hermes_protocol::{MessageRole, SessionCreateRequest};

use super::{Codicon, ProjectPicker, ProjectUiState, Route};

const TRANSCRIPT_WINDOW: usize = 80;

fn visible_message_start(total: usize, requested: usize) -> usize {
    total.saturating_sub(requested.max(1))
}

#[derive(Clone, Copy)]
struct ChatRuntimeState {
    queue: Signal<PromptQueueCoordinator>,
}

/// Route-persistent chat runtime. Prompt queues live above the Router so a busy
/// session can finish and drain its queue even after the user navigates to a
/// different session or product surface.
#[component]
pub(super) fn ChatRuntimeProvider() -> Element {
    let services = use_context::<AppServices>();
    let events_service = services.sessions.clone();
    let submit_service = services.sessions.clone();
    let mut queue = use_signal(PromptQueueCoordinator::default);
    use_context_provider(|| ChatRuntimeState { queue });

    let _queue_events = use_resource(move || {
        let events_service = events_service.clone();
        let submit_service = submit_service.clone();
        async move {
            let Ok(mut events) = events_service.events() else {
                return;
            };
            while let Some(event) = events.next().await {
                let Some(runtime_id) = event.session_id.as_deref() else {
                    continue;
                };
                if event.kind == "message.start" {
                    queue.write().mark_runtime_busy(runtime_id);
                    continue;
                }
                if event.kind != "message.complete" {
                    continue;
                }
                let next = queue.write().next_after_completion(runtime_id);
                let Some((stored_id, runtime_id, text)) = next else {
                    continue;
                };
                if let Err(error) = submit_service.submit(&runtime_id, &text).await {
                    queue
                        .write()
                        .mark_submit_failed(&stored_id, text, error.to_string());
                }
            }
        }
    });

    rsx! { Router::<Route> {} }
}

#[component]
pub(super) fn Chat() -> Element {
    let services = use_context::<AppServices>();
    let projects = use_context::<ProjectUiState>();
    let create_service = services.sessions.clone();
    let navigator = use_navigator();
    let mut prompt = use_signal(String::new);
    let mut submitting = use_signal(|| false);
    let mut submit_error = use_signal(|| None::<String>);
    let send = Callback::new(move |()| {
        let service = create_service.clone();
        let text = prompt().trim().to_owned();
        if text.is_empty() || submitting() {
            return;
        }
        submitting.set(true);
        submit_error.set(None);
        let snapshot = (projects.snapshot)();
        let project_id = snapshot.active_id.clone();
        let cwd = project_id.as_ref().and_then(|active_id| {
            snapshot
                .projects
                .iter()
                .find(|project| &project.id == active_id)
                .and_then(|project| project.primary_path.clone())
        });
        spawn(async move {
            let result = async {
                let session = service
                    .create(SessionCreateRequest {
                        cwd,
                        project_id,
                        ..SessionCreateRequest::default()
                    })
                    .await?;
                service
                    .submit(session.runtime_id.as_deref().unwrap_or(&session.id), &text)
                    .await?;
                Ok::<_, hermes_core::ServiceError>(session.id)
            }
            .await;
            submitting.set(false);
            match result {
                Ok(id) => {
                    prompt.set(String::new());
                    navigator.push(Route::Session { id });
                }
                Err(error) => submit_error.set(Some(error.to_string())),
            }
        });
    });

    rsx! {
        section { class: "new-chat-surface",
            div { class: "new-chat-hero",
                h1 { "HERMES AGENT" }
                p { "Send a prompt to trigger tool calls. Supports multi-file edits, test runs, git ops, and web fetches." }
            }
            div { class: "chat-composer-dock",
                ProjectPicker {}
                div { class: "composer-card",
                    button { class: "composer-tool", title: "Attach", aria_label: "Attach", Codicon { name: "add" } }
                    textarea {
                        aria_label: "Start a conversation",
                        placeholder: "What are we building?",
                        rows: "1",
                        value: "{prompt}",
                        oninput: move |event| prompt.set(event.value()),
                        onkeydown: move |event| {
                            if event.key() == Key::Enter && !event.modifiers().contains(Modifiers::SHIFT) {
                                event.prevent_default();
                                send.call(());
                            }
                        }
                    }
                    div { class: "composer-actions",
                        span { class: "composer-model", "Agents A1" }
                        button { class: "composer-tool", title: "Voice", aria_label: "Voice", Codicon { name: "mic" } }
                        button {
                            class: "send-button",
                            aria_label: "Send message",
                            disabled: submitting() || prompt().trim().is_empty(),
                            onclick: move |_| send.call(()),
                            if submitting() { "…" } else { "↑" }
                        }
                    }
                }
                if let Some(error) = submit_error() { p { class: "inline-error composer-error", role: "alert", "{error}" } }
            }
        }
    }
}

#[component]
pub(super) fn Session(id: String) -> Element {
    let services = use_context::<AppServices>();
    let chat_runtime = use_context::<ChatRuntimeState>();
    let load_service = services.sessions.clone();
    let history_service = services.sessions.clone();
    let submit_service = services.sessions.clone();
    let interrupt_service = services.sessions.clone();
    let queue_resume_service = services.sessions.clone();
    let events_service = services.sessions.clone();
    let session_id = id.clone();
    let history_stored_id = id.clone();
    let mut transcript = use_signal(|| None::<SessionTranscript>);
    let mut loading = use_signal(|| true);
    let mut load_error = use_signal(|| None::<String>);
    let mut events_ready = use_signal(|| false);
    let mut visible_limit = use_signal(|| TRANSCRIPT_WINDOW);
    let mut history_loading = use_signal(|| false);
    let mut history_error = use_signal(|| None::<String>);
    let _load = use_resource(move || {
        let load_service = load_service.clone();
        let session_id = session_id.clone();
        async move {
            loading.set(true);
            match load_service.resume(&session_id).await {
                Ok(response) => {
                    let state = SessionTranscript::load(session_id, response);
                    chat_runtime
                        .queue
                        .write()
                        .bind(&state.stored_id, &state.runtime_id, state.busy);
                    transcript.set(Some(state));
                    visible_limit.set(TRANSCRIPT_WINDOW);
                    load_error.set(None);
                    history_error.set(None);
                    events_ready.set(true);
                }
                Err(error) => load_error.set(Some(error.to_string())),
            }
            loading.set(false);
        }
    });
    let _events = use_resource(move || {
        let ready = events_ready();
        let events_service = events_service.clone();
        async move {
            if !ready {
                return;
            }
            let Ok(mut events) = events_service.events() else {
                return;
            };
            while let Some(event) = events.next().await {
                if let Some(state) = transcript.write().as_mut() {
                    state.apply_event(&event);
                }
            }
        }
    });
    let load_older = Callback::new(move |()| {
        let Some(state) = transcript() else {
            return;
        };
        if history_loading() {
            return;
        }
        if !state.messages_omitted {
            visible_limit.set(visible_limit().saturating_add(TRANSCRIPT_WINDOW));
            return;
        }
        history_loading.set(true);
        history_error.set(None);
        let service = history_service.clone();
        let stored_id = history_stored_id.clone();
        spawn(async move {
            match service.history(&stored_id).await {
                Ok(messages) => {
                    if let Some(state) = transcript.write().as_mut() {
                        state.merge_history(messages);
                    }
                    visible_limit.set(visible_limit().saturating_add(TRANSCRIPT_WINDOW));
                }
                Err(error) => history_error.set(Some(error.to_string())),
            }
            history_loading.set(false);
        });
    });
    let mut draft = use_signal(String::new);
    let mut send_error = use_signal(|| None::<String>);
    let send = Callback::new(move |()| {
        let text = draft().trim().to_owned();
        let Some(before) = transcript() else {
            return;
        };
        if text.is_empty() {
            return;
        }
        let stored_id = before.stored_id.clone();
        if before.busy {
            chat_runtime.queue.write().enqueue(&stored_id, text);
            draft.set(String::new());
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
        send_error.set(None);
        let service = submit_service.clone();
        spawn(async move {
            if let Err(error) = service.submit(&runtime_id, &text).await {
                chat_runtime.queue.write().mark_busy(&stored_id, false);
                transcript.set(Some(before));
                draft.set(text);
                send_error.set(Some(error.to_string()));
            }
        });
    });

    let remove_queue_id = id.clone();
    let remove_queued = Callback::new(move |index: usize| {
        chat_runtime.queue.write().remove(&remove_queue_id, index);
    });
    let clear_queue_id = id.clone();
    let clear_queued = Callback::new(move |()| {
        chat_runtime.queue.write().clear(&clear_queue_id);
    });
    let resume_queue_id = id.clone();
    let resume_queue = Callback::new(move |()| {
        let next = {
            let mut queue = chat_runtime.queue.write();
            queue.resume(&resume_queue_id);
            queue.next_if_idle(&resume_queue_id)
        };
        let Some((runtime_id, text)) = next else {
            return;
        };
        let service = queue_resume_service.clone();
        let stored_id = resume_queue_id.clone();
        spawn(async move {
            if let Err(error) = service.submit(&runtime_id, &text).await {
                chat_runtime
                    .queue
                    .write()
                    .mark_submit_failed(&stored_id, text, error.to_string());
            }
        });
    });

    let busy = transcript().as_ref().is_some_and(|state| state.busy);
    let queued = (chat_runtime.queue)().items(&id);
    let queue_parked = (chat_runtime.queue)().is_parked(&id);
    let queue_error = (chat_runtime.queue)().error(&id);
    let header_interrupt = interrupt_service.clone();
    let composer_interrupt = interrupt_service;
    let header_queue_id = id.clone();
    let composer_queue_id = id.clone();
    rsx! {
        section { class: "conversation-surface",
            header { class: "conversation-header",
                div { span { class: if busy { "session-dot running" } else { "session-dot" } } strong { "Session" } small { "{id}" } }
                if busy {
                    button {
                        class: "stop-button",
                        onclick: move |_| {
                            let service = header_interrupt.clone();
                            let runtime_id = transcript().map(|state| state.runtime_id).unwrap_or_default();
                            if runtime_id.is_empty() { return; }
                            chat_runtime.queue.write().park(&header_queue_id);
                            spawn(async move {
                                if let Err(error) = service.interrupt(&runtime_id).await {
                                    send_error.set(Some(error.to_string()));
                                }
                            });
                        },
                        Codicon { name: "primitive-square" }
                        "Stop"
                    }
                }
            }
            div { class: "conversation-scroll",
                div { class: "transcript",
                    if loading() {
                        LoadingState { label: "Loading conversation" }
                    } else if let Some(error) = load_error() {
                        ErrorState { error }
                    } else if let Some(state) = transcript() {
                        if state.messages.is_empty() {
                            div { class: "conversation-empty", "Write a message below to continue this conversation." }
                        }
                        {
                            let total = state.messages.len();
                            let start = visible_message_start(total, visible_limit());
                            let has_local_earlier = start > 0;
                            let has_server_earlier = state.messages_omitted;
                            rsx! {
                                if has_local_earlier || has_server_earlier {
                                    div { class: "history-window-controls",
                                        button {
                                            class: "secondary-button",
                                            disabled: history_loading(),
                                            onclick: move |_| load_older.call(()),
                                            if history_loading() {
                                                "Loading earlier history…"
                                            } else if has_server_earlier {
                                                "Load earlier history"
                                            } else {
                                                "Show {TRANSCRIPT_WINDOW.min(start)} earlier messages"
                                            }
                                        }
                                        small {
                                            if state.message_count > total {
                                                "Showing a bounded window from {total} loaded / {state.message_count} total messages"
                                            } else {
                                                "Rendering {total.saturating_sub(start)} of {total} loaded messages"
                                            }
                                        }
                                    }
                                }
                                if let Some(error) = history_error() {
                                    p { class: "inline-error transcript-error", role: "alert", "{error}" }
                                }
                                for message in state.messages.into_iter().skip(start) {
                                    if message.role == MessageRole::Tool {
                                        article { class: "tool-message",
                                            div { class: "tool-message-head", Codicon { name: "tools" } strong { if let Some(name) = message.tool_name.as_deref() { "{name}" } else { "Tool" } } span { if message.streaming { "Running" } else { "Done" } } }
                                            if !message.text.is_empty() { pre { "{message.text}" } }
                                        }
                                    } else {
                                        article { class: if message.role == MessageRole::User { "message user" } else { "message assistant" },
                                            div { class: "message-role", if message.role == MessageRole::User { "You" } else { "Hermes" } }
                                            if let Some(reasoning) = message.metadata.get("reasoning").and_then(serde_json::Value::as_str) {
                                                details { class: "reasoning", summary { "Thinking" } p { "{reasoning}" } }
                                            }
                                            if !message.text.is_empty() { p { "{message.text}" } }
                                            if message.streaming { span { class: "stream-cursor", aria_label: "Hermes is responding" } }
                                        }
                                    }
                                }
                            }
                        }
                        if state.needs_input { div { class: "needs-input", Codicon { name: "question" } "Hermes is waiting for input in this session." } }
                        if let Some(error) = state.error { p { class: "inline-error transcript-error", role: "alert", "{error}" } }
                    }
                }
            }
            div { class: "session-composer-dock",
                ProjectPicker {}
                if !queued.is_empty() {
                    section { class: "prompt-queue", aria_label: "Queued prompts",
                        div { class: "prompt-queue-head",
                            strong { "Queued prompts ({queued.len()})" }
                            if queue_parked {
                                button { class: "secondary-button", onclick: move |_| resume_queue.call(()), "Resume queue" }
                            }
                            button { class: "secondary-button", onclick: move |_| clear_queued.call(()), "Clear" }
                        }
                        for (index, text) in queued.iter().enumerate() {
                            div { class: "prompt-queue-row",
                                span { "{index + 1}. {text}" }
                                button {
                                    class: "composer-tool",
                                    aria_label: "Remove queued prompt {index + 1}",
                                    title: "Remove queued prompt",
                                    onclick: move |_| remove_queued.call(index),
                                    Codicon { name: "close" }
                                }
                            }
                        }
                    }
                }
                if let Some(error) = queue_error {
                    p { class: "inline-error composer-error", role: "alert", "Queued prompt failed: {error}" }
                }
                div { class: "composer-card",
                    button { class: "composer-tool", title: "Attach", aria_label: "Attach", Codicon { name: "add" } }
                    textarea {
                        aria_label: "Message Hermes",
                        placeholder: if busy { "Type and press Enter to queue another prompt…" } else { "What are we building?" },
                        rows: "1",
                        value: "{draft}",
                        disabled: loading(),
                        oninput: move |event| draft.set(event.value()),
                        onkeydown: move |event| {
                            if event.key() == Key::Enter && !event.modifiers().contains(Modifiers::SHIFT) {
                                event.prevent_default();
                                send.call(());
                            }
                        }
                    }
                    div { class: "composer-actions",
                        span { class: "composer-model", if busy { "Running · Enter queues" } else { "Private session" } }
                        if busy && !draft().trim().is_empty() {
                            button {
                                class: "secondary-button",
                                aria_label: "Queue prompt",
                                onclick: move |_| send.call(()),
                                "Queue"
                            }
                        }
                        button {
                            class: if busy { "send-button stop" } else { "send-button" },
                            aria_label: if busy { "Stop response" } else { "Send message" },
                            disabled: !busy && draft().trim().is_empty(),
                            onclick: move |_| {
                                if busy {
                                    let service = composer_interrupt.clone();
                                    let runtime_id = transcript().map(|state| state.runtime_id).unwrap_or_default();
                                    chat_runtime.queue.write().park(&composer_queue_id);
                                    spawn(async move { let _ = service.interrupt(&runtime_id).await; });
                                } else {
                                    send.call(());
                                }
                            },
                            if busy { Codicon { name: "primitive-square" } } else { "↑" }
                        }
                    }
                }
                if let Some(error) = send_error() { p { class: "inline-error composer-error", role: "alert", "{error}" } }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{TRANSCRIPT_WINDOW, visible_message_start};

    #[test]
    fn transcript_window_stays_bounded_for_very_large_histories() {
        let total = 100_000;
        let start = visible_message_start(total, TRANSCRIPT_WINDOW);
        assert_eq!(start, 99_920);
        assert_eq!(total - start, TRANSCRIPT_WINDOW);
    }

    #[test]
    fn transcript_window_expands_in_fixed_chunks_without_underflow() {
        assert_eq!(visible_message_start(25, TRANSCRIPT_WINDOW), 0);
        assert_eq!(visible_message_start(500, TRANSCRIPT_WINDOW * 2), 340);
        assert_eq!(visible_message_start(0, TRANSCRIPT_WINDOW), 0);
    }
}
