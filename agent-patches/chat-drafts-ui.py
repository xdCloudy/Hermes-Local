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
    "use hermes_core::{AppServices, PromptQueueCoordinator, SessionTranscript};",
    "use hermes_core::{AppServices, ComposerDraftStore, PromptQueueCoordinator, SessionTranscript};",
    "draft store import",
)
text = replace_once(
    text,
    "use super::{Codicon, ErrorState, LoadingState, ProjectPicker, ProjectUiState, Route};",
    "use super::{\n    Codicon, ErrorState, LoadingState, ProjectPicker, ProjectUiState, Route, SettingsUiState,\n};",
    "settings UI import",
)
text = replace_once(
    text,
    "const TRANSCRIPT_WINDOW: usize = 80;\n",
    "const TRANSCRIPT_WINDOW: usize = 80;\nconst DRAFTS_SETTINGS_KEY: &str = \"hermes.chat.drafts.v1\";\nconst NEW_CHAT_DRAFT_KEY: &str = \"__new_chat__\";\n\nfn mark_draft_changed(mut revision: Signal<u64>) {\n    let next = revision().wrapping_add(1);\n    revision.set(next);\n}\n",
    "draft constants",
)
text = replace_once(
    text,
    "#[derive(Clone, Copy)]\nstruct ChatRuntimeState {\n    queue: Signal<PromptQueueCoordinator>,\n}\n",
    "#[derive(Clone, Copy)]\nstruct ChatRuntimeState {\n    queue: Signal<PromptQueueCoordinator>,\n    drafts: Signal<ComposerDraftStore>,\n    drafts_hydrated: Signal<bool>,\n    draft_revision: Signal<u64>,\n}\n",
    "runtime state fields",
)
text = replace_once(
    text,
    "    let events_service = services.sessions.clone();\n    let submit_service = services.sessions.clone();\n    let mut queue = use_signal(PromptQueueCoordinator::default);\n    use_context_provider(|| ChatRuntimeState { queue });\n",
    "    let events_service = services.sessions.clone();\n    let submit_service = services.sessions.clone();\n    let settings_service = services.settings.clone();\n    let settings_ui = use_context::<SettingsUiState>();\n    let mut queue = use_signal(PromptQueueCoordinator::default);\n    let mut drafts = use_signal(ComposerDraftStore::default);\n    let mut drafts_hydrated = use_signal(|| false);\n    let mut draft_revision = use_signal(|| 0_u64);\n    let mut draft_saved_revision = use_signal(|| 0_u64);\n    use_context_provider(|| ChatRuntimeState {\n        queue,\n        drafts,\n        drafts_hydrated,\n        draft_revision,\n    });\n",
    "provider state",
)
text = replace_once(
    text,
    "    rsx! { Router::<Route> {} }\n}\n\n#[component]\npub(super) fn Chat() -> Element {",
    "    let _draft_hydration = use_resource(move || {\n        let settings_loading = (settings_ui.loading)();\n        let settings = (settings_ui.settings)();\n        async move {\n            if settings_loading || drafts_hydrated() {\n                return;\n            }\n            if let Some(value) = settings.extra.get(DRAFTS_SETTINGS_KEY) {\n                drafts.set(ComposerDraftStore::hydrate(value));\n            }\n            drafts_hydrated.set(true);\n        }\n    });\n\n    let _draft_persistence = use_resource(move || {\n        let revision = draft_revision();\n        let hydrated = drafts_hydrated();\n        let saved_revision = draft_saved_revision();\n        let settings_service = settings_service.clone();\n        async move {\n            if !hydrated || revision == 0 || revision == saved_revision {\n                return;\n            }\n            tokio::time::sleep(std::time::Duration::from_millis(400)).await;\n            if draft_revision() != revision {\n                return;\n            }\n            let mut settings = match settings_service.load().await {\n                Ok(settings) => settings,\n                Err(error) => {\n                    let mut error_signal = settings_ui.error;\n                    error_signal.set(Some(error.to_string()));\n                    return;\n                }\n            };\n            settings\n                .extra\n                .insert(DRAFTS_SETTINGS_KEY.into(), drafts().persisted_value());\n            match settings_service.save(&settings).await {\n                Ok(()) => {\n                    draft_saved_revision.set(revision);\n                    let mut settings_signal = settings_ui.settings;\n                    settings_signal.set(settings);\n                    let mut error_signal = settings_ui.error;\n                    error_signal.set(None);\n                }\n                Err(error) => {\n                    let mut error_signal = settings_ui.error;\n                    error_signal.set(Some(error.to_string()));\n                }\n            }\n        }\n    });\n\n    rsx! { Router::<Route> {} }\n}\n\n#[component]\npub(super) fn Chat() -> Element {",
    "provider persistence",
)
text = replace_once(
    text,
    "    let services = use_context::<AppServices>();\n    let projects = use_context::<ProjectUiState>();\n",
    "    let services = use_context::<AppServices>();\n    let projects = use_context::<ProjectUiState>();\n    let chat_runtime = use_context::<ChatRuntimeState>();\n",
    "new chat runtime",
)
text = replace_once(
    text,
    "    let navigator = use_navigator();\n    let mut prompt = use_signal(String::new);\n    let mut submitting = use_signal(|| false);\n",
    "    let navigator = use_navigator();\n    let mut prompt = use_signal(String::new);\n    let mut prompt_bound = use_signal(|| false);\n    let _restore_prompt = use_resource(move || {\n        let hydrated = (chat_runtime.drafts_hydrated)();\n        async move {\n            if hydrated && !prompt_bound() {\n                prompt.set((chat_runtime.drafts)().text(NEW_CHAT_DRAFT_KEY));\n                prompt_bound.set(true);\n            }\n        }\n    });\n    let mut submitting = use_signal(|| false);\n",
    "new chat restore",
)
text = replace_once(
    text,
    "                Ok(id) => {\n                    prompt.set(String::new());\n                    navigator.push(Route::Session { id });\n                }\n",
    "                Ok(id) => {\n                    prompt.set(String::new());\n                    chat_runtime.drafts.write().clear(NEW_CHAT_DRAFT_KEY);\n                    mark_draft_changed(chat_runtime.draft_revision);\n                    navigator.push(Route::Session { id });\n                }\n",
    "new chat clear",
)
text = replace_once(
    text,
    "                        oninput: move |event| prompt.set(event.value()),\n",
    "                        oninput: move |event| {\n                            let value = event.value();\n                            prompt.set(value.clone());\n                            chat_runtime.drafts.write().edit(NEW_CHAT_DRAFT_KEY, value);\n                            mark_draft_changed(chat_runtime.draft_revision);\n                        },\n",
    "new chat input persistence",
)
text = replace_once(
    text,
    "                    div { class: \"composer-actions\",\n                        span { class: \"composer-model\", \"Agents A1\" }\n",
    "                    div { class: \"composer-actions\",\n                        button {\n                            class: \"composer-tool\", title: \"Undo\", aria_label: \"Undo draft\",\n                            onclick: move |_| {\n                                let restored = chat_runtime.drafts.write().undo(NEW_CHAT_DRAFT_KEY);\n                                if let Some(value) = restored {\n                                    prompt.set(value);\n                                    mark_draft_changed(chat_runtime.draft_revision);\n                                }\n                            },\n                            Codicon { name: \"discard\" }\n                        }\n                        button {\n                            class: \"composer-tool\", title: \"Redo\", aria_label: \"Redo draft\",\n                            onclick: move |_| {\n                                let restored = chat_runtime.drafts.write().redo(NEW_CHAT_DRAFT_KEY);\n                                if let Some(value) = restored {\n                                    prompt.set(value);\n                                    mark_draft_changed(chat_runtime.draft_revision);\n                                }\n                            },\n                            Codicon { name: \"redo\" }\n                        }\n                        span { class: \"composer-model\", \"Agents A1\" }\n",
    "new chat undo controls",
)

text = replace_once(
    text,
    "    let mut draft = use_signal(String::new);\n    let mut send_error = use_signal(|| None::<String>);\n",
    "    let mut draft = use_signal(String::new);\n    let mut draft_bound = use_signal(|| false);\n    let restore_draft_id = id.clone();\n    let _restore_draft = use_resource(move || {\n        let hydrated = (chat_runtime.drafts_hydrated)();\n        let restore_draft_id = restore_draft_id.clone();\n        async move {\n            if hydrated && !draft_bound() {\n                draft.set((chat_runtime.drafts)().text(&restore_draft_id));\n                draft_bound.set(true);\n            }\n        }\n    });\n    let mut send_error = use_signal(|| None::<String>);\n",
    "session draft restore",
)
text = replace_once(
    text,
    "        if before.busy {\n            chat_runtime.queue.write().enqueue(&stored_id, text);\n            draft.set(String::new());\n            send_error.set(None);\n            return;\n        }\n",
    "        if before.busy {\n            chat_runtime.queue.write().enqueue(&stored_id, text);\n            draft.set(String::new());\n            chat_runtime.drafts.write().clear(&stored_id);\n            mark_draft_changed(chat_runtime.draft_revision);\n            send_error.set(None);\n            return;\n        }\n",
    "queued send draft clear",
)
text = replace_once(
    text,
    "        draft.set(String::new());\n        send_error.set(None);\n        let service = submit_service.clone();\n",
    "        draft.set(String::new());\n        chat_runtime.drafts.write().clear(&stored_id);\n        mark_draft_changed(chat_runtime.draft_revision);\n        send_error.set(None);\n        let service = submit_service.clone();\n",
    "normal send draft clear",
)
text = replace_once(
    text,
    "                transcript.set(Some(before));\n                draft.set(text);\n                send_error.set(Some(error.to_string()));\n",
    "                transcript.set(Some(before));\n                draft.set(text.clone());\n                chat_runtime\n                    .drafts\n                    .write()\n                    .replace_without_history(&stored_id, text);\n                mark_draft_changed(chat_runtime.draft_revision);\n                send_error.set(Some(error.to_string()));\n",
    "failed send draft restore",
)
text = replace_once(
    text,
    "                        oninput: move |event| draft.set(event.value()),\n",
    "                        oninput: move |event| {\n                            let value = event.value();\n                            draft.set(value.clone());\n                            chat_runtime.drafts.write().edit(&id, value);\n                            mark_draft_changed(chat_runtime.draft_revision);\n                        },\n",
    "session input persistence",
)
text = replace_once(
    text,
    "                    div { class: \"composer-actions\",\n                        span { class: \"composer-model\", if busy { \"Running · Enter queues\" } else { \"Private session\" } }\n",
    "                    div { class: \"composer-actions\",\n                        button {\n                            class: \"composer-tool\", title: \"Undo\", aria_label: \"Undo draft\",\n                            onclick: move |_| {\n                                let restored = chat_runtime.drafts.write().undo(&id);\n                                if let Some(value) = restored {\n                                    draft.set(value);\n                                    mark_draft_changed(chat_runtime.draft_revision);\n                                }\n                            },\n                            Codicon { name: \"discard\" }\n                        }\n                        button {\n                            class: \"composer-tool\", title: \"Redo\", aria_label: \"Redo draft\",\n                            onclick: move |_| {\n                                let restored = chat_runtime.drafts.write().redo(&id);\n                                if let Some(value) = restored {\n                                    draft.set(value);\n                                    mark_draft_changed(chat_runtime.draft_revision);\n                                }\n                            },\n                            Codicon { name: \"redo\" }\n                        }\n                        span { class: \"composer-model\", if busy { \"Running · Enter queues\" } else { \"Private session\" } }\n",
    "session undo controls",
)
CHAT.write_text(text, encoding="utf-8")
