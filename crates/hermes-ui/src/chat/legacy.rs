//! Chat/session presentation surfaces extracted from the shell so the A4 chat
//! migration can evolve behind the existing typed service boundary.

use std::{collections::BTreeMap, rc::Rc};

use dioxus::prelude::*;
use futures_util::StreamExt;
use hermes_core::{
    AppServices, ComposerDraftStore, PromptQueueCoordinator, QueuedPrompt, ServiceResult,
    SessionService, SessionTranscript, attachment_context_text,
};
use hermes_protocol::{AttachmentKind, MessageRole, SelectedAttachment, SessionCreateRequest};

use super::{
    Codicon, ErrorState, LoadingState, ProjectPicker, ProjectUiState, Route, SettingsUiState,
};
use crate::{ExternalActivation, use_external_activation_queue};

mod rich_content;
use rich_content::RichContent;

const TRANSCRIPT_WINDOW: usize = 80;
const DRAFTS_SETTINGS_KEY: &str = "hermes.chat.drafts.v1";
const NEW_CHAT_DRAFT_KEY: &str = "__new_chat__";

fn mark_draft_changed(mut revision: Signal<u64>) {
    let next = revision().wrapping_add(1);
    revision.set(next);
}

fn blueprint_command(name: &str, params: &BTreeMap<String, String>) -> String {
    let slots = params
        .iter()
        .map(|(key, value)| {
            let value = if value.chars().any(char::is_whitespace) {
                format!("\"{}\"", value.replace('\"', "\\\""))
            } else {
                value.clone()
            };
            format!("{key}={value}")
        })
        .collect::<Vec<_>>()
        .join(" ");
    if slots.is_empty() {
        format!("/blueprint {name}")
    } else {
        format!("/blueprint {name} {slots}")
    }
}

fn insert_composer_block(existing: &str, block: &str) -> String {
    let existing = existing.trim_end();
    if existing.is_empty() {
        block.to_owned()
    } else {
        format!("{existing}\n\n{block}")
    }
}

fn visible_message_start(total: usize, requested: usize) -> usize {
    total.saturating_sub(requested.max(1))
}

fn attachment_size_label(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

async fn prepare_prompt_attachments(
    service: &dyn SessionService,
    runtime_id: &str,
    visible_text: &str,
    attachments: &[SelectedAttachment],
) -> ServiceResult<(String, Vec<SelectedAttachment>)> {
    let mut results = Vec::with_capacity(attachments.len());
    let mut staged = Vec::with_capacity(attachments.len());
    for attachment in attachments {
        let result = service.attach(runtime_id, attachment).await?;
        let mut next = attachment.clone();
        next.attached_session_id = Some(runtime_id.to_owned());
        next.ref_text.clone_from(&result.ref_text);
        next.staged_path.clone_from(&result.path);
        results.push(result);
        staged.push(next);
    }
    Ok((attachment_context_text(visible_text, &results), staged))
}

async fn detach_staged_images(service: &dyn SessionService, prompt: &QueuedPrompt) {
    for attachment in &prompt.attachments {
        if attachment.kind != AttachmentKind::Image {
            continue;
        }
        let (Some(session_id), Some(path)) = (
            attachment.attached_session_id.as_deref(),
            attachment.staged_path.as_deref(),
        ) else {
            continue;
        };
        let _ = service.detach_image(session_id, path).await;
    }
}

#[component]
fn AttachmentTray(
    attachments: Signal<Vec<SelectedAttachment>>,
    on_remove: Callback<SelectedAttachment>,
) -> Element {
    let items = attachments();
    rsx! {
        if !items.is_empty() {
            div { class: "composer-attachments", aria_label: "Attachments",
                for attachment in items {
                    {
                        let remove = attachment.clone();
                        let size = attachment_size_label(attachment.size);
                        let icon = if attachment.kind == AttachmentKind::Image { "file-media" } else { "file" };
                        rsx! {
                            div { class: "composer-attachment-chip", key: "{attachment.id}",
                                if let Some(preview) = attachment.preview_data_url.as_deref() {
                                    img { class: "composer-attachment-preview", src: "{preview}", alt: "{attachment.label}" }
                                } else {
                                    span { class: "composer-attachment-icon", Codicon { name: icon } }
                                }
                                span { class: "composer-attachment-copy",
                                    strong { "{attachment.label}" }
                                    small { "{size}" }
                                }
                                button {
                                    class: "composer-attachment-remove",
                                    title: "Remove attachment",
                                    aria_label: "Remove {attachment.label}",
                                    onclick: move |_| on_remove.call(remove.clone()),
                                    Codicon { name: "close" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
struct ChatRuntimeState {
    queue: Signal<PromptQueueCoordinator>,
    drafts: Signal<ComposerDraftStore>,
    drafts_hydrated: Signal<bool>,
    draft_revision: Signal<u64>,
}

/// Route-persistent chat runtime. Prompt queues live above the Router so a busy
/// session can finish and drain its queue even after the user navigates to a
/// different session or product surface.
#[component]
pub(super) fn ChatRuntimeProvider() -> Element {
    let services = use_context::<AppServices>();
    let events_service = services.sessions.clone();
    let submit_service = services.sessions.clone();
    let settings_service = services.settings.clone();
    let settings_ui = use_context::<SettingsUiState>();
    let mut queue = use_signal(PromptQueueCoordinator::default);
    let mut drafts = use_signal(ComposerDraftStore::default);
    let mut drafts_hydrated = use_signal(|| false);
    let draft_revision = use_signal(|| 0_u64);
    let mut draft_saved_revision = use_signal(|| 0_u64);
    use_context_provider(|| ChatRuntimeState {
        queue,
        drafts,
        drafts_hydrated,
        draft_revision,
    });

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
                let next = queue.write().next_prompt_after_completion(runtime_id);
                let Some((stored_id, runtime_id, prompt)) = next else {
                    continue;
                };
                let prepared = prepare_prompt_attachments(
                    submit_service.as_ref(),
                    &runtime_id,
                    &prompt.text,
                    &prompt.attachments,
                )
                .await;
                let (result, retry_prompt) = match prepared {
                    Ok((model_text, staged)) => (
                        submit_service.submit(&runtime_id, &model_text).await,
                        QueuedPrompt {
                            text: prompt.text.clone(),
                            attachments: staged,
                        },
                    ),
                    Err(error) => (Err(error), prompt),
                };
                if let Err(error) = result {
                    queue
                        .write()
                        .mark_prompt_failed(&stored_id, retry_prompt, error.to_string());
                }
            }
        }
    });

    let _draft_hydration = use_resource(move || {
        let settings_loading = (settings_ui.loading)();
        let settings = (settings_ui.settings)();
        async move {
            if settings_loading || drafts_hydrated() {
                return;
            }
            if let Some(value) = settings.extra.get(DRAFTS_SETTINGS_KEY) {
                drafts.set(ComposerDraftStore::hydrate(value));
            }
            drafts_hydrated.set(true);
        }
    });

    let _draft_persistence = use_resource(move || {
        let revision = draft_revision();
        let hydrated = drafts_hydrated();
        let saved_revision = draft_saved_revision();
        let settings_service = settings_service.clone();
        async move {
            if !hydrated || revision == 0 || revision == saved_revision {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            if draft_revision() != revision {
                return;
            }
            let mut settings = match settings_service.load().await {
                Ok(settings) => settings,
                Err(error) => {
                    let mut error_signal = settings_ui.error;
                    error_signal.set(Some(error.to_string()));
                    return;
                }
            };
            settings
                .extra
                .insert(DRAFTS_SETTINGS_KEY.into(), drafts().persisted_value());
            match settings_service.save(&settings).await {
                Ok(()) => {
                    draft_saved_revision.set(revision);
                    let mut settings_signal = settings_ui.settings;
                    settings_signal.set(settings);
                    let mut error_signal = settings_ui.error;
                    error_signal.set(None);
                }
                Err(error) => {
                    let mut error_signal = settings_ui.error;
                    error_signal.set(Some(error.to_string()));
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
    let mut chat_runtime = use_context::<ChatRuntimeState>();
    let create_service = services.sessions.clone();
    let navigator = use_navigator();
    let mut prompt = use_signal(String::new);
    let mut prompt_bound = use_signal(|| false);
    let mut composer_element = use_signal(|| None::<Rc<MountedData>>);
    let mut external_activations = use_external_activation_queue();
    let mut attachments = use_signal(Vec::<SelectedAttachment>::new);
    let attachment_picker = services.platform.clone();
    let _restore_prompt = use_resource(move || {
        let hydrated = (chat_runtime.drafts_hydrated)();
        async move {
            if hydrated && !prompt_bound() {
                prompt.set((chat_runtime.drafts)().text(NEW_CHAT_DRAFT_KEY));
                prompt_bound.set(true);
            }
        }
    });
    use_effect(move || {
        if !(chat_runtime.drafts_hydrated)() {
            return;
        }
        let next = external_activations.read().front().cloned();
        let Some(ExternalActivation::Blueprint { name, params }) = next else {
            return;
        };
        let command = blueprint_command(&name, &params);
        let existing = (chat_runtime.drafts)().text(NEW_CHAT_DRAFT_KEY);
        let value = insert_composer_block(&existing, &command);
        prompt.set(value.clone());
        prompt_bound.set(true);
        chat_runtime.drafts.write().edit(NEW_CHAT_DRAFT_KEY, value);
        mark_draft_changed(chat_runtime.draft_revision);
        external_activations.write().pop_front();
        if let Some(element) = composer_element() {
            spawn(async move {
                let _ = element.set_focus(true).await;
            });
        }
    });
    let mut submitting = use_signal(|| false);
    let mut submit_error = use_signal(|| None::<String>);
    let remove_attachment = Callback::new(move |attachment: SelectedAttachment| {
        attachments.write().retain(|item| item.id != attachment.id);
    });
    let pick_attachments = Callback::new(move |()| {
        let service = attachment_picker.clone();
        submit_error.set(None);
        spawn(async move {
            match service.pick_attachments("Attach files", None, false).await {
                Ok(selected) => attachments.write().extend(selected),
                Err(error) => submit_error.set(Some(error.to_string())),
            }
        });
    });
    let send = Callback::new(move |()| {
        let service = create_service.clone();
        let text = prompt().trim().to_owned();
        let pending_attachments = attachments();
        if (text.is_empty() && pending_attachments.is_empty()) || submitting() {
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
                let runtime_id = session.runtime_id.as_deref().unwrap_or(&session.id);
                let (model_text, _) = prepare_prompt_attachments(
                    service.as_ref(),
                    runtime_id,
                    &text,
                    &pending_attachments,
                )
                .await?;
                service.submit(runtime_id, &model_text).await?;
                Ok::<_, hermes_core::ServiceError>(session.id)
            }
            .await;
            submitting.set(false);
            match result {
                Ok(id) => {
                    prompt.set(String::new());
                    attachments.set(Vec::new());
                    chat_runtime.drafts.write().clear(NEW_CHAT_DRAFT_KEY);
                    mark_draft_changed(chat_runtime.draft_revision);
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
                    button {
                        class: "composer-tool", title: "Attach files", aria_label: "Attach files",
                        onclick: move |_| pick_attachments.call(()), Codicon { name: "add" }
                    }
                    AttachmentTray { attachments, on_remove: remove_attachment }
                    textarea {
                        aria_label: "Start a conversation",
                        onmounted: move |element| composer_element.set(Some(element.data())),
                        placeholder: "What are we building?",
                        rows: "1",
                        value: "{prompt}",
                        oninput: move |event| {
                            let value = event.value();
                            prompt.set(value.clone());
                            chat_runtime.drafts.write().edit(NEW_CHAT_DRAFT_KEY, value);
                            mark_draft_changed(chat_runtime.draft_revision);
                        },
                        onkeydown: move |event| {
                            if event.key() == Key::Enter && !event.modifiers().contains(Modifiers::SHIFT) {
                                event.prevent_default();
                                send.call(());
                            }
                        }
                    }
                    div { class: "composer-actions",
                        button {
                            class: "composer-tool", title: "Undo", aria_label: "Undo draft",
                            onclick: move |_| {
                                let restored = chat_runtime.drafts.write().undo(NEW_CHAT_DRAFT_KEY);
                                if let Some(value) = restored {
                                    prompt.set(value);
                                    mark_draft_changed(chat_runtime.draft_revision);
                                }
                            },
                            Codicon { name: "discard" }
                        }
                        button {
                            class: "composer-tool", title: "Redo", aria_label: "Redo draft",
                            onclick: move |_| {
                                let restored = chat_runtime.drafts.write().redo(NEW_CHAT_DRAFT_KEY);
                                if let Some(value) = restored {
                                    prompt.set(value);
                                    mark_draft_changed(chat_runtime.draft_revision);
                                }
                            },
                            Codicon { name: "redo" }
                        }
                        span { class: "composer-model", "Agents A1" }
                        button { class: "composer-tool", title: "Voice", aria_label: "Voice", Codicon { name: "mic" } }
                        button {
                            class: "send-button",
                            aria_label: "Send message",
                            disabled: submitting() || (prompt().trim().is_empty() && attachments().is_empty()),
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
    let mut chat_runtime = use_context::<ChatRuntimeState>();
    let load_service = services.sessions.clone();
    let history_service = services.sessions.clone();
    let submit_service = services.sessions.clone();
    let directive_service = services.sessions.clone();
    let interrupt_service = services.sessions.clone();
    let queue_resume_service = services.sessions.clone();
    let events_service = services.sessions.clone();
    let external_service = services.platform.clone();
    let session_id = id.clone();
    let history_stored_id = id.clone();
    let mut transcript = use_signal(|| None::<SessionTranscript>);
    let mut loading = use_signal(|| true);
    let mut load_error = use_signal(|| None::<String>);
    let mut events_ready = use_signal(|| false);
    let mut visible_limit = use_signal(|| TRANSCRIPT_WINDOW);
    let mut history_loading = use_signal(|| false);
    let mut history_error = use_signal(|| None::<String>);
    let open_link = Callback::new(move |url: String| {
        let service = external_service.clone();
        spawn(async move {
            let _ = service.open_external(&url).await;
        });
    });
    let _load = use_resource(move || {
        let load_service = load_service.clone();
        let session_id = session_id.clone();
        async move {
            loading.set(true);
            match load_service.resume(&session_id).await {
                Ok(response) => {
                    let state = SessionTranscript::load(session_id, response);
                    chat_runtime.queue.write().bind(
                        &state.stored_id,
                        &state.runtime_id,
                        state.busy,
                    );
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
    let mut draft_bound = use_signal(|| false);
    let mut attachments = use_signal(Vec::<SelectedAttachment>::new);
    let attachment_picker = services.platform.clone();
    let restore_draft_id = id.clone();
    let _restore_draft = use_resource(move || {
        let hydrated = (chat_runtime.drafts_hydrated)();
        let restore_draft_id = restore_draft_id.clone();
        async move {
            if hydrated && !draft_bound() {
                draft.set((chat_runtime.drafts)().text(&restore_draft_id));
                draft_bound.set(true);
            }
        }
    });
    let mut send_error = use_signal(|| None::<String>);
    let remove_attachment_service = services.sessions.clone();
    let remove_attachment = Callback::new(move |attachment: SelectedAttachment| {
        attachments.write().retain(|item| item.id != attachment.id);
        if attachment.kind == AttachmentKind::Image
            && let (Some(session_id), Some(path)) = (
                attachment.attached_session_id.clone(),
                attachment.staged_path.clone(),
            )
        {
            let service = remove_attachment_service.clone();
            spawn(async move {
                let _ = service.detach_image(&session_id, &path).await;
            });
        }
    });
    let pick_attachments = Callback::new(move |()| {
        let service = attachment_picker.clone();
        send_error.set(None);
        spawn(async move {
            match service.pick_attachments("Attach files", None, false).await {
                Ok(selected) => attachments.write().extend(selected),
                Err(error) => send_error.set(Some(error.to_string())),
            }
        });
    });
    let send = Callback::new(move |()| {
        let text = draft().trim().to_owned();
        let pending_attachments = attachments();
        let Some(before) = transcript() else {
            return;
        };
        if text.is_empty() && pending_attachments.is_empty() {
            return;
        }
        let stored_id = before.stored_id.clone();
        if text.starts_with('/') && !pending_attachments.is_empty() {
            send_error.set(Some(
                "Send attachments with a normal prompt; slash directives do not accept files."
                    .into(),
            ));
            return;
        }

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
                            let Some(target) =
                                result.target.filter(|target| !target.trim().is_empty())
                            else {
                                send_error
                                    .set(Some("directive alias did not provide a target".into()));
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
                                let optimistic_id =
                                    format!("user-directive-{}", state.messages.len());
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
            chat_runtime.queue.write().enqueue_prompt(
                &stored_id,
                QueuedPrompt {
                    text,
                    attachments: pending_attachments,
                },
            );
            draft.set(String::new());
            attachments.set(Vec::new());
            chat_runtime.drafts.write().clear(&stored_id);
            mark_draft_changed(chat_runtime.draft_revision);
            send_error.set(None);
            return;
        }
        let runtime_id = before.runtime_id.clone();
        let optimistic_id = format!("user-local-{}", before.messages.len());
        let display_text = if text.is_empty() {
            pending_attachments
                .iter()
                .map(|attachment| attachment.label.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        } else {
            text.clone()
        };
        chat_runtime.queue.write().mark_busy(&stored_id, true);
        if let Some(state) = transcript.write().as_mut() {
            state.push_user(optimistic_id, display_text);
        }
        draft.set(String::new());
        attachments.set(Vec::new());
        chat_runtime.drafts.write().clear(&stored_id);
        mark_draft_changed(chat_runtime.draft_revision);
        send_error.set(None);
        let service = submit_service.clone();
        spawn(async move {
            let prepared = prepare_prompt_attachments(
                service.as_ref(),
                &runtime_id,
                &text,
                &pending_attachments,
            )
            .await;
            let (result, restore_attachments) = match prepared {
                Ok((model_text, staged)) => {
                    (service.submit(&runtime_id, &model_text).await, staged)
                }
                Err(error) => (Err(error), pending_attachments.clone()),
            };
            if let Err(error) = result {
                chat_runtime.queue.write().mark_busy(&stored_id, false);
                transcript.set(Some(before));
                draft.set(text.clone());
                attachments.set(restore_attachments);
                chat_runtime
                    .drafts
                    .write()
                    .replace_without_history(&stored_id, text);
                mark_draft_changed(chat_runtime.draft_revision);
                send_error.set(Some(error.to_string()));
            }
        });
    });

    let queue_cleanup_service = services.sessions.clone();
    let remove_queue_id = id.clone();
    let remove_queued = Callback::new(move |index: usize| {
        let removed = chat_runtime
            .queue
            .write()
            .remove_prompt(&remove_queue_id, index);
        if let Some(prompt) = removed {
            let service = queue_cleanup_service.clone();
            spawn(async move {
                detach_staged_images(service.as_ref(), &prompt).await;
            });
        }
    });
    let clear_queue_service = services.sessions.clone();
    let clear_queue_id = id.clone();
    let clear_queued = Callback::new(move |()| {
        let removed = (chat_runtime.queue)().prompts(&clear_queue_id);
        chat_runtime.queue.write().clear(&clear_queue_id);
        if !removed.is_empty() {
            let service = clear_queue_service.clone();
            spawn(async move {
                for prompt in removed {
                    detach_staged_images(service.as_ref(), &prompt).await;
                }
            });
        }
    });
    let resume_queue_id = id.clone();
    let resume_queue = Callback::new(move |()| {
        let next = {
            let mut queue = chat_runtime.queue.write();
            queue.resume(&resume_queue_id);
            queue.next_prompt_if_idle(&resume_queue_id)
        };
        let Some((runtime_id, prompt)) = next else {
            return;
        };
        let service = queue_resume_service.clone();
        let stored_id = resume_queue_id.clone();
        spawn(async move {
            let prepared = prepare_prompt_attachments(
                service.as_ref(),
                &runtime_id,
                &prompt.text,
                &prompt.attachments,
            )
            .await;
            let (result, retry_prompt) = match prepared {
                Ok((model_text, staged)) => (
                    service.submit(&runtime_id, &model_text).await,
                    QueuedPrompt {
                        text: prompt.text.clone(),
                        attachments: staged,
                    },
                ),
                Err(error) => (Err(error), prompt),
            };
            if let Err(error) = result {
                chat_runtime.queue.write().mark_prompt_failed(
                    &stored_id,
                    retry_prompt,
                    error.to_string(),
                );
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
    let input_draft_id = id.clone();
    let undo_draft_id = id.clone();
    let redo_draft_id = id.clone();
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
                                        article {
                                            class: if message.role == MessageRole::User { "message user" } else if message.role == MessageRole::System { "message system" } else { "message assistant" },
                                            div { class: "message-role", if message.role == MessageRole::User { "You" } else if message.role == MessageRole::System { "System" } else { "Hermes" } }
                                            if let Some(reasoning) = message.metadata.get("reasoning").and_then(serde_json::Value::as_str) {
                                                details { class: "reasoning", summary { "Thinking" } p { "{reasoning}" } }
                                            }
                                            if !message.text.is_empty() {
                                                if message.role == MessageRole::User {
                                                    p { "{message.text}" }
                                                } else {
                                                    RichContent { text: message.text.clone(), on_open_link: open_link }
                                                }
                                            }
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
                    button {
                        class: "composer-tool", title: "Attach files", aria_label: "Attach files",
                        onclick: move |_| pick_attachments.call(()), Codicon { name: "add" }
                    }
                    AttachmentTray { attachments, on_remove: remove_attachment }
                    textarea {
                        aria_label: "Message Hermes",
                        placeholder: if busy { "Type and press Enter to queue another prompt…" } else { "What are we building?" },
                        rows: "1",
                        value: "{draft}",
                        disabled: loading(),
                        oninput: move |event| {
                            let value = event.value();
                            draft.set(value.clone());
                            chat_runtime.drafts.write().edit(&input_draft_id, value);
                            mark_draft_changed(chat_runtime.draft_revision);
                        },
                        onkeydown: move |event| {
                            if event.key() == Key::Enter && !event.modifiers().contains(Modifiers::SHIFT) {
                                event.prevent_default();
                                send.call(());
                            }
                        }
                    }
                    div { class: "composer-actions",
                        button {
                            class: "composer-tool", title: "Undo", aria_label: "Undo draft",
                            onclick: move |_| {
                                let restored = chat_runtime.drafts.write().undo(&undo_draft_id);
                                if let Some(value) = restored {
                                    draft.set(value);
                                    mark_draft_changed(chat_runtime.draft_revision);
                                }
                            },
                            Codicon { name: "discard" }
                        }
                        button {
                            class: "composer-tool", title: "Redo", aria_label: "Redo draft",
                            onclick: move |_| {
                                let restored = chat_runtime.drafts.write().redo(&redo_draft_id);
                                if let Some(value) = restored {
                                    draft.set(value);
                                    mark_draft_changed(chat_runtime.draft_revision);
                                }
                            },
                            Codicon { name: "redo" }
                        }
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
                            disabled: !busy && draft().trim().is_empty() && attachments().is_empty(),
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
    use std::collections::BTreeMap;

    use super::{
        TRANSCRIPT_WINDOW, blueprint_command, insert_composer_block, visible_message_start,
    };

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

    #[test]
    fn blueprint_activation_matches_reviewable_electron_command_shape() {
        let params = BTreeMap::from([
            ("mode".to_owned(), "fast".to_owned()),
            ("note".to_owned(), "hello world".to_owned()),
        ]);
        assert_eq!(
            blueprint_command("morning-brief", &params),
            r#"/blueprint morning-brief mode=fast note="hello world""#
        );
    }

    #[test]
    fn external_blueprint_insert_preserves_existing_draft_as_block() {
        assert_eq!(
            insert_composer_block("keep this", "/blueprint daily"),
            "keep this\n\n/blueprint daily"
        );
        assert_eq!(
            insert_composer_block("", "/blueprint daily"),
            "/blueprint daily"
        );
    }
}
