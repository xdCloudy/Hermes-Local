from pathlib import Path


def rep(path, old, new):
    p = Path(path)
    s = p.read_text(encoding='utf-8')
    if old not in s:
        raise SystemExit(f'missing pattern in {path}: {old[:100]!r}')
    p.write_text(s.replace(old, new, 1), encoding='utf-8')

ui = 'crates/hermes-ui/src/chat.rs'
rep(ui, '''use hermes_core::{AppServices, ComposerDraftStore, PromptQueueCoordinator, SessionTranscript};
use hermes_protocol::{MessageRole, SessionCreateRequest};''', '''use hermes_core::{
    AppServices, ComposerDraftStore, PromptQueueCoordinator, ServiceResult, SessionService,
    SessionTranscript, attachment_context_text,
};
use hermes_protocol::{AttachmentKind, MessageRole, SelectedAttachment, SessionCreateRequest};''')
rep(ui, '''fn visible_message_start(total: usize, requested: usize) -> usize {
    total.saturating_sub(requested.max(1))
}
''', '''fn visible_message_start(total: usize, requested: usize) -> usize {
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
''')
rep(ui, '''    let navigator = use_navigator();
    let mut prompt = use_signal(String::new);
    let mut prompt_bound = use_signal(|| false);''', '''    let navigator = use_navigator();
    let mut prompt = use_signal(String::new);
    let mut prompt_bound = use_signal(|| false);
    let mut attachments = use_signal(Vec::<SelectedAttachment>::new);
    let attachment_picker = services.platform.clone();''')
rep(ui, '''    let mut submitting = use_signal(|| false);
    let mut submit_error = use_signal(|| None::<String>);
    let send = Callback::new(move |()| {''', '''    let mut submitting = use_signal(|| false);
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
    let send = Callback::new(move |()| {''')
rep(ui, '''        let service = create_service.clone();
        let text = prompt().trim().to_owned();
        if text.is_empty() || submitting() {''', '''        let service = create_service.clone();
        let text = prompt().trim().to_owned();
        let pending_attachments = attachments();
        if (text.is_empty() && pending_attachments.is_empty()) || submitting() {''')
rep(ui, '''                service
                    .submit(session.runtime_id.as_deref().unwrap_or(&session.id), &text)
                    .await?;
                Ok::<_, hermes_core::ServiceError>(session.id)''', '''                let runtime_id = session.runtime_id.as_deref().unwrap_or(&session.id);
                let (model_text, _) = prepare_prompt_attachments(
                    service.as_ref(), runtime_id, &text, &pending_attachments,
                ).await?;
                service.submit(runtime_id, &model_text).await?;
                Ok::<_, hermes_core::ServiceError>(session.id)''')
rep(ui, '''                    prompt.set(String::new());
                    chat_runtime.drafts.write().clear(NEW_CHAT_DRAFT_KEY);''', '''                    prompt.set(String::new());
                    attachments.set(Vec::new());
                    chat_runtime.drafts.write().clear(NEW_CHAT_DRAFT_KEY);''')
rep(ui, '''                    button { class: "composer-tool", title: "Attach", aria_label: "Attach", Codicon { name: "add" } }
                    textarea {''', '''                    button {
                        class: "composer-tool", title: "Attach files", aria_label: "Attach files",
                        onclick: move |_| pick_attachments.call(()), Codicon { name: "add" }
                    }
                    AttachmentTray { attachments, on_remove: remove_attachment }
                    textarea {''')
rep(ui, '''                            disabled: submitting() || prompt().trim().is_empty(),''', '''                            disabled: submitting() || (prompt().trim().is_empty() && attachments().is_empty()),''')

print('new-chat attachment UI transform applied')
