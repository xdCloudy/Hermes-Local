from pathlib import Path


def read(path: str) -> str:
    return Path(path).read_text(encoding="utf-8")


def write(path: str, text: str) -> None:
    Path(path).write_text(text, encoding="utf-8")


def once(path: str, old: str, new: str) -> None:
    text = read(path)
    count = text.count(old)
    if count != 1:
        raise RuntimeError(f"{path}: expected one marker, found {count}: {old[:100]!r}")
    write(path, text.replace(old, new, 1))


core = "crates/hermes-core/src/lib.rs"
marker = "    fn execute_directive(\n        &self,\n        _session_id: &str,\n        _command: &str,\n    ) -> ServiceFuture<'_, SessionDirectiveResult> {\n"
methods = """    fn set_model(
        &self,
        _session_id: &str,
        _provider: &str,
        _model: &str,
    ) -> ServiceFuture<'_, bool> {
        Box::pin(async move {
            Err(ServiceError::Unavailable(
                \"session-scoped model switching is unavailable on this host\".into(),
            ))
        })
    }
    fn set_yolo(&self, _session_id: &str, _enabled: bool) -> ServiceFuture<'_, bool> {
        Box::pin(async move {
            Err(ServiceError::Unavailable(
                \"session-scoped YOLO control is unavailable on this host\".into(),
            ))
        })
    }
"""
once(core, marker, methods + marker)


desktop = "crates/hermes-desktop/src/lib.rs"
impl_marker = "impl SessionService for GatewayServices {\n"
helper = """fn session_model_config_value(provider: &str, model: &str) -> ServiceResult<String> {
    let provider = provider.trim();
    let model = model.trim();
    if provider.is_empty()
        || provider.len() > 256
        || model.is_empty()
        || model.len() > 512
        || provider.chars().any(char::is_whitespace)
        || provider.chars().any(char::is_control)
        || model.chars().any(char::is_control)
        || model.contains(\" --provider \")
    {
        return Err(ServiceError::InvalidInput(
            \"invalid session model selection\".into(),
        ));
    }
    Ok(format!(\"{model} --provider {provider} --session\"))
}

"""
once(desktop, impl_marker, helper + impl_marker)
text = read(desktop)
start = text.index(impl_marker)
directive = "    fn execute_directive(\n        &self,\n        session_id: &str,\n        command: &str,\n    ) -> ServiceFuture<'_, SessionDirectiveResult> {\n"
at = text.index(directive, start)
impl_methods = """    fn set_model(
        &self,
        session_id: &str,
        provider: &str,
        model: &str,
    ) -> ServiceFuture<'_, bool> {
        let session_id = session_id.to_owned();
        let value = session_model_config_value(provider, model);
        Box::pin(async move {
            validate_identifier(&session_id, \"session\")?;
            let value = value?;
            let response = self
                .client()?
                .request::<_, Value>(
                    \"config.set\",
                    json!({
                        \"session_id\": session_id,
                        \"key\": \"model\",
                        \"value\": value,
                    }),
                )
                .await
                .map_err(transport)?;
            Ok(response
                .get(\"deferred\")
                .and_then(Value::as_bool)
                .unwrap_or(false))
        })
    }

    fn set_yolo(&self, session_id: &str, enabled: bool) -> ServiceFuture<'_, bool> {
        let session_id = session_id.to_owned();
        Box::pin(async move {
            validate_identifier(&session_id, \"session\")?;
            let response = self
                .client()?
                .request::<_, Value>(
                    \"config.set\",
                    json!({
                        \"session_id\": session_id,
                        \"key\": \"yolo\",
                        \"value\": if enabled { \"1\" } else { \"0\" },
                    }),
                )
                .await
                .map_err(transport)?;
            Ok(response
                .get(\"value\")
                .and_then(Value::as_str)
                .map_or(enabled, |value| value == \"1\"))
        })
    }

"""
write(desktop, text[:at] + impl_methods + text[at:])


chat = "crates/hermes-ui/src/chat.rs"
once(
    chat,
    "mod rich_content;\nuse rich_content::RichContent;\n",
    "mod controls;\nmod rich_content;\nuse controls::{apply_pending_controls, ChatControls, PendingChatControls};\nuse rich_content::RichContent;\n",
)
once(
    chat,
    "    let mut attachments = use_signal(Vec::<SelectedAttachment>::new);\n",
    "    let mut attachments = use_signal(Vec::<SelectedAttachment>::new);\n    let pending_controls = use_signal(PendingChatControls::default);\n",
)
once(
    chat,
    "        let pending_attachments = attachments();\n",
    "        let pending_attachments = attachments();\n        let pending_runtime_controls = pending_controls();\n",
)
once(
    chat,
    "                let runtime_id = session.runtime_id.as_deref().unwrap_or(&session.id);\n                let (model_text, _) = prepare_prompt_attachments(\n",
    "                let runtime_id = session.runtime_id.as_deref().unwrap_or(&session.id);\n                apply_pending_controls(service.as_ref(), runtime_id, &pending_runtime_controls).await?;\n                let (model_text, _) = prepare_prompt_attachments(\n",
)
once(
    chat,
    '                        span { class: "composer-model", "Agents A1" }\n                        button { class: "composer-tool", title: "Voice", aria_label: "Voice", Codicon { name: "mic" } }\n',
    '                        ChatControls { session_id: None, pending: pending_controls }\n                        button { class: "composer-tool", title: "Voice", aria_label: "Voice", Codicon { name: "mic" } }\n',
)
once(
    chat,
    "    let mut transcript = use_signal(|| None::<SessionTranscript>);\n",
    "    let mut transcript = use_signal(|| None::<SessionTranscript>);\n    let session_controls = use_signal(PendingChatControls::default);\n",
)
once(
    chat,
    '                        span { class: "composer-model", if busy { "Running · Enter queues" } else { "Private session" } }\n',
    '                        ChatControls { session_id: transcript().map(|state| state.runtime_id), pending: session_controls }\n                        span { class: "composer-model", if busy { "Running · Enter queues" } else { "Private session" } }\n',
)

rich = "crates/hermes-ui/src/chat/rich_content.rs"
once(rich, "        && !source.contains('>')\n", "")

expected = {
    core,
    desktop,
    chat,
    rich,
    "crates/hermes-ui/src/chat/controls.rs",
}
for path in expected:
    if not Path(path).exists():
        raise RuntimeError(f"expected source file missing after patch: {path}")
