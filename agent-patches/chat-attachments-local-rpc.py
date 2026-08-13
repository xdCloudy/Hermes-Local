from pathlib import Path

p = Path('crates/hermes-desktop/src/lib.rs')
s = p.read_text(encoding='utf-8')
old = '''    fn submit(&self, session_id: &str, text: &str) -> ServiceFuture<'_, ()> {'''
if old not in s:
    raise SystemExit('submit marker missing')
new = '''    fn attach(
        &self,
        session_id: &str,
        attachment: &SelectedAttachment,
    ) -> ServiceFuture<'_, SessionAttachmentResult> {
        let session_id = session_id.to_owned();
        let attachment = attachment.clone();
        Box::pin(async move {
            validate_identifier(&session_id, "session")?;
            if attachment.attached_session_id.as_deref() == Some(session_id.as_str()) {
                return Ok(SessionAttachmentResult {
                    attached: true,
                    kind: attachment.kind,
                    path: attachment.staged_path.clone(),
                    ref_text: attachment.ref_text.clone(),
                    message: None,
                });
            }
            let path = attachment_selections().resolve(&attachment.id)?;
            let metadata = fs::metadata(&path).map_err(platform)?;
            if !metadata.is_file() {
                return Err(ServiceError::InvalidInput("attachment is not a file".into()));
            }
            let limit = if attachment.kind == AttachmentKind::Image {
                64 * 1024 * 1024
            } else {
                256 * 1024 * 1024
            };
            if metadata.len() > limit {
                return Err(ServiceError::InvalidInput(format!(
                    "{} is too large to attach ({} bytes; limit {} bytes)",
                    attachment.label, metadata.len(), limit
                )));
            }
            if self.connection_store.load(None)?.mode != ConnectionMode::Local {
                return Err(ServiceError::Unavailable(
                    "remote attachment byte staging is not enabled in this tranche".into(),
                ));
            }
            let value: Value = match attachment.kind {
                AttachmentKind::Image => self
                    .client()?
                    .request_with_timeout(
                        "image.attach",
                        json!({ "session_id": session_id, "path": path.to_string_lossy() }),
                        std::time::Duration::from_mins(5),
                    )
                    .await
                    .map_err(transport)?,
                AttachmentKind::File => self
                    .client()?
                    .request_with_timeout(
                        "file.attach",
                        json!({
                            "session_id": session_id,
                            "name": attachment.label,
                            "path": path.to_string_lossy(),
                        }),
                        std::time::Duration::from_mins(5),
                    )
                    .await
                    .map_err(transport)?,
            };
            let result = SessionAttachmentResult {
                attached: value.get("attached").and_then(Value::as_bool).unwrap_or(false),
                kind: attachment.kind,
                path: value.get("path").and_then(Value::as_str).map(str::to_owned),
                ref_text: value.get("ref_text").and_then(Value::as_str).map(str::to_owned),
                message: value.get("message").and_then(Value::as_str).map(str::to_owned),
            };
            if !result.attached {
                return Err(ServiceError::Transport(
                    result.message.clone().unwrap_or_else(|| "attachment rejected".into()),
                ));
            }
            Ok(result)
        })
    }

    fn detach_image(&self, session_id: &str, path: &str) -> ServiceFuture<'_, ()> {
        let session_id = session_id.to_owned();
        let path = path.trim().to_owned();
        Box::pin(async move {
            validate_identifier(&session_id, "session")?;
            if path.is_empty() || path.len() > 32_768 {
                return Err(ServiceError::InvalidInput("invalid image path".into()));
            }
            let _: Value = self
                .client()?
                .request_with_timeout(
                    "image.detach",
                    json!({ "session_id": session_id, "path": path }),
                    std::time::Duration::from_secs(30),
                )
                .await
                .map_err(transport)?;
            Ok(())
        })
    }

    fn submit(&self, session_id: &str, text: &str) -> ServiceFuture<'_, ()> {'''
p.write_text(s.replace(old, new, 1), encoding='utf-8')
print('local attachment RPC transform applied')
