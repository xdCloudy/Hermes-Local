from pathlib import Path


def rep(path, old, new):
    p = Path(path)
    s = p.read_text(encoding='utf-8')
    if old not in s:
        raise SystemExit(f'missing pattern in {path}: {old[:120]!r}')
    p.write_text(s.replace(old, new, 1), encoding='utf-8')

ui = 'crates/hermes-ui/src/chat.rs'
rep(ui, '''    let mut draft = use_signal(String::new);
    let mut draft_bound = use_signal(|| false);''', '''    let mut draft = use_signal(String::new);
    let mut draft_bound = use_signal(|| false);
    let mut attachments = use_signal(Vec::<SelectedAttachment>::new);
    let attachment_picker = services.platform.clone();''')
rep(ui, '''    let mut send_error = use_signal(|| None::<String>);
    let send = Callback::new(move |()| {''', '''    let mut send_error = use_signal(|| None::<String>);
    let remove_attachment = Callback::new(move |attachment: SelectedAttachment| {
        attachments.write().retain(|item| item.id != attachment.id);
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
    let send = Callback::new(move |()| {''')
rep(ui, '''        let text = draft().trim().to_owned();
        let Some(before) = transcript() else {
            return;
        };
        if text.is_empty() {
            return;
        }
        let stored_id = before.stored_id.clone();''', '''        let text = draft().trim().to_owned();
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
                "Send attachments with a normal prompt; slash directives do not accept files.".into(),
            ));
            return;
        }''')
rep(ui, '''        if before.busy {
            chat_runtime.queue.write().enqueue(&stored_id, text);
            draft.set(String::new());''', '''        if before.busy {
            if !pending_attachments.is_empty() {
                send_error.set(Some(
                    "Wait for the current turn to finish before sending attachments.".into(),
                ));
                return;
            }
            chat_runtime.queue.write().enqueue(&stored_id, text);
            draft.set(String::new());''')
rep(ui, '''        let runtime_id = before.runtime_id.clone();
        let optimistic_id = format!("user-local-{}", before.messages.len());
        chat_runtime.queue.write().mark_busy(&stored_id, true);
        if let Some(state) = transcript.write().as_mut() {
            state.push_user(optimistic_id, text.clone());
        }
        draft.set(String::new());''', '''        let runtime_id = before.runtime_id.clone();
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
        attachments.set(Vec::new());''')
rep(ui, '''        let service = submit_service.clone();
        spawn(async move {
            if let Err(error) = service.submit(&runtime_id, &text).await {
                chat_runtime.queue.write().mark_busy(&stored_id, false);
                transcript.set(Some(before));
                draft.set(text.clone());''', '''        let service = submit_service.clone();
        spawn(async move {
            let prepared = prepare_prompt_attachments(
                service.as_ref(), &runtime_id, &text, &pending_attachments,
            ).await;
            let (result, restore_attachments) = match prepared {
                Ok((model_text, staged)) => (service.submit(&runtime_id, &model_text).await, staged),
                Err(error) => (Err(error), pending_attachments.clone()),
            };
            if let Err(error) = result {
                chat_runtime.queue.write().mark_busy(&stored_id, false);
                transcript.set(Some(before));
                draft.set(text.clone());
                attachments.set(restore_attachments);''')
rep(ui, '''                    button { class: "composer-tool", title: "Attach", aria_label: "Attach", Codicon { name: "add" } }
                    textarea {''', '''                    button {
                        class: "composer-tool", title: "Attach files", aria_label: "Attach files",
                        onclick: move |_| pick_attachments.call(()), Codicon { name: "add" }
                    }
                    AttachmentTray { attachments, on_remove: remove_attachment }
                    textarea {''')
rep(ui, '''                            disabled: !busy && draft().trim().is_empty(),''', '''                            disabled: !busy && draft().trim().is_empty() && attachments().is_empty(),''')

print('session attachment UI transform applied')
