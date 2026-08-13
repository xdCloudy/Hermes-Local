from pathlib import Path

path = Path("crates/hermes-ui/src/chat.rs")
text = path.read_text(encoding="utf-8")

old = '''        if attachment.kind == AttachmentKind::Image {
            if let (Some(session_id), Some(path)) = (
                attachment.attached_session_id.clone(),
                attachment.staged_path.clone(),
            ) {
                let service = remove_attachment_service.clone();
                spawn(async move {
                    let _ = service.detach_image(&session_id, &path).await;
                });
            }
        }
'''
new = '''        if attachment.kind == AttachmentKind::Image
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
'''

if old not in text:
    raise SystemExit("expected CH-11 nested image-detach block was not found")

path.write_text(text.replace(old, new, 1), encoding="utf-8")
print("CH-11 Clippy collapsible-if repair applied")
