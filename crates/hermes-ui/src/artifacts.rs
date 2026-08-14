use std::{cmp::Ordering, collections::BTreeSet, path::Path};

use dioxus::prelude::*;
use futures_util::{StreamExt, stream};
use hermes_core::{AppServices, PreviewDocument};
use hermes_protocol::{ChatMessage, MessageRole, SessionSummary};
use serde_json::Value;

use super::{Route, Surface, files::SafePreview};

const MAX_SESSIONS: usize = 30;
const MAX_MESSAGES_PER_SESSION: usize = 2_000;
const MAX_ARTIFACTS: usize = 1_000;
const MAX_TARGET_BYTES: usize = 16_384;
const HISTORY_CONCURRENCY: usize = 6;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ArtifactKind {
    Image,
    File,
    Link,
}

impl ArtifactKind {
    const fn label(self) -> &'static str {
        match self {
            Self::Image => "Image",
            Self::File => "File",
            Self::Link => "Link",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct ArtifactRecord {
    id: String,
    kind: ArtifactKind,
    target: String,
    label: String,
    session_id: String,
    session_title: String,
    base_dir: Option<String>,
    timestamp: f64,
}

fn normalize_candidate(value: &str) -> Option<String> {
    let value = value
        .trim()
        .trim_matches(['"', '\'', '`', '(', '[', '{', '<'])
        .trim_end_matches(['"', '\'', '`', ')', ']', '}', '>', ',', '.', ';', ':']);
    if value.is_empty() || value.len() > MAX_TARGET_BYTES || value.chars().any(char::is_control) {
        return None;
    }
    Some(value.to_owned())
}

fn starts_http(value: &str) -> bool {
    value
        .get(..7)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("http://"))
        || value
            .get(..8)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("https://"))
}

fn starts_file_url(value: &str) -> bool {
    value
        .get(..5)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("file:"))
}

fn looks_like_windows_path(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() > 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'/' | b'\\')
}

fn extension(value: &str) -> &str {
    value
        .split(['?', '#'])
        .next()
        .unwrap_or(value)
        .rsplit(['/', '\\'])
        .next()
        .and_then(|name| name.rsplit_once('.').map(|(_, extension)| extension))
        .unwrap_or_default()
}

fn has_file_extension(value: &str) -> bool {
    matches!(
        extension(value).to_ascii_lowercase().as_str(),
        "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "webp"
            | "svg"
            | "bmp"
            | "pdf"
            | "txt"
            | "json"
            | "md"
            | "csv"
            | "zip"
            | "tar"
            | "gz"
            | "mp3"
            | "wav"
            | "mp4"
            | "mov"
            | "html"
            | "htm"
    )
}

fn looks_like_target(value: &str) -> bool {
    if starts_http(value) {
        return true;
    }
    let path_like = starts_file_url(value)
        || value.starts_with('/')
        || value.starts_with("~/")
        || value.starts_with("~\\")
        || value.starts_with("./")
        || value.starts_with(".\\")
        || value.starts_with("../")
        || value.starts_with("..\\")
        || looks_like_windows_path(value);
    path_like && has_file_extension(value)
}

fn artifact_kind(value: &str) -> ArtifactKind {
    if matches!(
        extension(value).to_ascii_lowercase().as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp"
    ) {
        ArtifactKind::Image
    } else if starts_http(value) {
        ArtifactKind::Link
    } else {
        ArtifactKind::File
    }
}

fn artifact_label(value: &str) -> String {
    value
        .split(['?', '#'])
        .next()
        .unwrap_or(value)
        .rsplit(['/', '\\'])
        .find(|part| !part.is_empty())
        .unwrap_or(value)
        .to_owned()
}

fn push_candidate(candidates: &mut Vec<String>, value: &str) {
    if let Some(value) = normalize_candidate(value)
        && looks_like_target(&value)
        && !candidates.iter().any(|existing| existing == &value)
    {
        candidates.push(value);
    }
}

fn collect_text_targets(text: &str, candidates: &mut Vec<String>) {
    let mut remainder = text;
    while let Some(open) = remainder.find("](") {
        let after = &remainder[open + 2..];
        let end = after.find(')').unwrap_or(after.len());
        push_candidate(candidates, &after[..end]);
        remainder = &after[end.min(after.len())..];
    }

    for token in text.split_whitespace() {
        push_candidate(candidates, token);
    }
}

fn collect_json_targets(
    value: &Value,
    key_hint: bool,
    depth: usize,
    remaining: &mut usize,
    candidates: &mut Vec<String>,
) {
    if depth > 8 || *remaining == 0 {
        return;
    }
    *remaining -= 1;
    match value {
        Value::String(value) if key_hint || looks_like_target(value) => {
            push_candidate(candidates, value);
        }
        Value::Array(values) => {
            for value in values {
                collect_json_targets(value, key_hint, depth + 1, remaining, candidates);
            }
        }
        Value::Object(values) => {
            for (key, value) in values {
                let key = key.to_ascii_lowercase();
                let hinted = key_hint
                    || [
                        "path", "file", "url", "image", "artifact", "output", "download", "result",
                        "target",
                    ]
                    .iter()
                    .any(|hint| key.contains(hint));
                collect_json_targets(value, hinted, depth + 1, remaining, candidates);
            }
        }
        _ => {}
    }
}

fn message_targets(message: &ChatMessage) -> Vec<String> {
    let mut candidates = Vec::new();
    for text in [&message.text, &message.content_text] {
        collect_text_targets(text, &mut candidates);
        if matches!(message.role, MessageRole::Tool)
            && let Ok(value) = serde_json::from_str::<Value>(text)
        {
            let mut remaining = 2_000;
            collect_json_targets(&value, false, 0, &mut remaining, &mut candidates);
        }
    }
    if let Some(reasoning) = message.reasoning.as_deref() {
        collect_text_targets(reasoning, &mut candidates);
    }
    let mut remaining = 2_000;
    for (key, value) in &message.metadata {
        let key = key.to_ascii_lowercase();
        let hinted = [
            "path", "file", "url", "image", "artifact", "output", "download", "result", "target",
        ]
        .iter()
        .any(|hint| key.contains(hint));
        collect_json_targets(value, hinted, 0, &mut remaining, &mut candidates);
    }
    candidates
}

fn collect_session_artifacts(
    session: &SessionSummary,
    messages: &[ChatMessage],
) -> Vec<ArtifactRecord> {
    let mut seen = BTreeSet::new();
    let mut artifacts = Vec::new();
    let title = if session.title.trim().is_empty() {
        "Untitled session"
    } else {
        session.title.trim()
    };
    for message in messages.iter().rev().take(MAX_MESSAGES_PER_SESSION) {
        if !matches!(message.role, MessageRole::Assistant | MessageRole::Tool) {
            continue;
        }
        for target in message_targets(message) {
            if !seen.insert(target.clone()) {
                continue;
            }
            artifacts.push(ArtifactRecord {
                id: format!("{}:{}", session.id, target),
                kind: artifact_kind(&target),
                label: artifact_label(&target),
                target,
                session_id: session.id.clone(),
                session_title: title.to_owned(),
                base_dir: session.cwd.clone(),
                timestamp: message.timestamp.or(session.updated_at).unwrap_or_default(),
            });
            if artifacts.len() >= MAX_ARTIFACTS {
                return artifacts;
            }
        }
    }
    artifacts
}

fn matches_filter(artifact: &ArtifactRecord, filter: &str, query: &str) -> bool {
    let kind_matches = match filter {
        "image" => artifact.kind == ArtifactKind::Image,
        "file" => artifact.kind == ArtifactKind::File,
        "link" => artifact.kind == ArtifactKind::Link,
        _ => true,
    };
    let query = query.trim().to_ascii_lowercase();
    kind_matches
        && (query.is_empty()
            || artifact.label.to_ascii_lowercase().contains(&query)
            || artifact.target.to_ascii_lowercase().contains(&query)
            || artifact.session_title.to_ascii_lowercase().contains(&query))
}

#[component]
pub(super) fn Artifacts() -> Element {
    let services = use_context::<AppServices>();
    let mut artifacts = use_signal(Vec::<ArtifactRecord>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut partial_failures = use_signal(|| 0_usize);
    let mut refresh = use_signal(|| 0_u64);
    let mut query = use_signal(String::new);
    let mut filter = use_signal(|| "all".to_owned());
    let mut selected = use_signal(|| None::<ArtifactRecord>);
    let mut preview = use_signal(|| None::<PreviewDocument>);
    let mut preview_loading = use_signal(|| false);
    let mut action_busy = use_signal(|| false);

    let load_services = services.clone();
    let _loading = use_resource(move || {
        let services = load_services.clone();
        let _revision = refresh();
        async move {
            loading.set(true);
            error.set(None);
            let sessions = match services.sessions.list().await {
                Ok(mut sessions) => {
                    sessions.sort_by(|left, right| {
                        right
                            .updated_at
                            .partial_cmp(&left.updated_at)
                            .unwrap_or(Ordering::Equal)
                    });
                    sessions.truncate(MAX_SESSIONS);
                    sessions
                }
                Err(problem) => {
                    artifacts.set(Vec::new());
                    error.set(Some(problem.to_string()));
                    loading.set(false);
                    return;
                }
            };
            let history_service = services.sessions.clone();
            let results = stream::iter(sessions.into_iter().map(|session| {
                let service = history_service.clone();
                async move {
                    let result = service.history(&session.id).await;
                    (session, result)
                }
            }))
            .buffer_unordered(HISTORY_CONCURRENCY)
            .collect::<Vec<_>>()
            .await;
            let mut failures = 0;
            let mut collected = Vec::new();
            for (session, result) in results {
                match result {
                    Ok(messages) => {
                        collected.extend(collect_session_artifacts(&session, &messages));
                    }
                    Err(_) => failures += 1,
                }
                if collected.len() >= MAX_ARTIFACTS {
                    collected.truncate(MAX_ARTIFACTS);
                    break;
                }
            }
            collected.sort_by(|left, right| {
                right
                    .timestamp
                    .partial_cmp(&left.timestamp)
                    .unwrap_or(Ordering::Equal)
            });
            artifacts.set(collected);
            partial_failures.set(failures);
            loading.set(false);
        }
    });

    let preview_services = services.clone();
    let select_artifact = Callback::new(move |artifact: ArtifactRecord| {
        selected.set(Some(artifact.clone()));
        preview.set(None);
        preview_loading.set(true);
        error.set(None);
        let services = preview_services.clone();
        spawn(async move {
            match services
                .preview
                .load(
                    &artifact.target,
                    artifact.base_dir.as_deref().map(Path::new),
                )
                .await
            {
                Ok(Some(document)) => preview.set(Some(document)),
                Ok(None) => error.set(Some("Artifact target is missing or unsupported.".into())),
                Err(problem) => error.set(Some(problem.to_string())),
            }
            preview_loading.set(false);
        });
    });

    let open_services = services.clone();
    let open_selected = Callback::new(move |()| {
        let Some(artifact) = selected() else { return };
        if action_busy() {
            return;
        }
        action_busy.set(true);
        error.set(None);
        let services = open_services.clone();
        spawn(async move {
            if let Err(problem) = services
                .preview
                .open(
                    &artifact.target,
                    artifact.base_dir.as_deref().map(Path::new),
                )
                .await
            {
                error.set(Some(problem.to_string()));
            }
            action_busy.set(false);
        });
    });

    let all = artifacts();
    let current_filter = filter();
    let current_query = query();
    let visible = all
        .iter()
        .filter(|artifact| matches_filter(artifact, &current_filter, &current_query))
        .cloned()
        .collect::<Vec<_>>();
    let count = |kind| all.iter().filter(|artifact| artifact.kind == kind).count();

    rsx! {
        Surface { eyebrow: "Agent output", title: "Artifacts", subtitle: "Index bounded artifact references from recent assistant and tool messages, then preview or open them through native safety checks.",
            div { class: "settings-toolbar",
                input {
                    class: "settings-input",
                    aria_label: "Search artifacts",
                    placeholder: "Search label, path, URL, or session",
                    value: "{current_query}",
                    oninput: move |event| query.set(event.value())
                }
                button { class: "button", disabled: loading(), onclick: move |_| refresh.set(refresh() + 1), "Refresh" }
            }
            div { class: "settings-toolbar", role: "tablist", aria_label: "Artifact kind",
                for (id, label, total) in [
                    ("all", "All", all.len()),
                    ("image", "Images", count(ArtifactKind::Image)),
                    ("file", "Files", count(ArtifactKind::File)),
                    ("link", "Links", count(ArtifactKind::Link)),
                ] {
                    button {
                        class: if current_filter == id { "primary-button" } else { "button" },
                        role: "tab",
                        aria_selected: current_filter == id,
                        onclick: move |_| filter.set(id.to_owned()),
                        "{label} ({total})"
                    }
                }
            }
            if loading() {
                div { class: "loading-state", role: "status", "◌ Indexing up to 30 recent sessions" }
            }
            if let Some(problem) = error() {
                div { class: "error-state", role: "alert", h2 { "Artifact action failed" } p { "{problem}" } }
            }
            if partial_failures() > 0 {
                p { class: "muted", role: "status", "Some session histories were unavailable ({partial_failures()}); available artifacts are still shown." }
            }
            div { style: "display:grid;grid-template-columns:minmax(18rem,2fr) minmax(20rem,3fr);gap:1rem;min-height:0;",
                section { class: "panel", style: "min-height:24rem;max-height:calc(100vh - 18rem);overflow:auto;",
                    header { class: "panel-title", "Indexed artifacts ({visible.len()})" }
                    if !loading() && visible.is_empty() {
                        div { class: "settings-empty", h2 { "No matching artifacts" } p { "Assistant and tool output from the 30 most recent sessions is searched." } }
                    }
                    for artifact in visible {
                        {
                            let row = artifact.clone();
                            let session_id = artifact.session_id.clone();
                            rsx! { div { class: "settings-list", key: "{artifact.id}", style: "align-items:flex-start;",
                                button { class: "project-action", style: "text-align:left;min-width:0;flex:1;", onclick: move |_| select_artifact.call(row.clone()),
                                    strong { "{artifact.label}" }
                                    span { class: "muted", "{artifact.kind.label()} · {artifact.session_title}" }
                                    code { title: "{artifact.target}", style: "display:block;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;max-width:32rem;", "{artifact.target}" }
                                }
                                Link { class: "button", to: Route::Session { id: session_id }, "Chat" }
                            } }
                        }
                    }
                }
                section { class: "panel", style: "min-height:24rem;min-width:0;",
                    header { class: "panel-title", "Safe preview" }
                    if let Some(artifact) = selected() {
                        div { class: "settings-toolbar",
                            div { style: "min-width:0;flex:1;", strong { "{artifact.label}" } p { class: "muted", title: "{artifact.target}", "{artifact.target}" } }
                            button { class: "primary-button", disabled: action_busy(), onclick: move |_| open_selected.call(()), if action_busy() { "Opening…" } else { "Open" } }
                        }
                        if preview_loading() {
                            div { class: "loading-state", role: "status", "◌ Validating preview target" }
                        } else if let Some(document) = preview() {
                            SafePreview { document }
                        }
                    } else {
                        div { class: "settings-empty", h2 { "Select an artifact" } p { "Paths and URLs are normalized by the native preview boundary before content is shown or opened." } }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> SessionSummary {
        SessionSummary {
            id: "session-1".into(),
            title: "Build report".into(),
            cwd: Some("C:/workspace".into()),
            updated_at: Some(10.0),
            ..SessionSummary::default()
        }
    }

    #[test]
    fn collects_deduplicated_markdown_urls_and_windows_paths() {
        let messages = [ChatMessage {
            role: MessageRole::Assistant,
            text: "![plot](./outputs/chart.png) report C:\\workspace\\report.pdf https://example.com/result".into(),
            timestamp: Some(20.0),
            ..ChatMessage::default()
        }];
        let artifacts = collect_session_artifacts(&session(), &messages);
        assert_eq!(artifacts.len(), 3);
        assert_eq!(artifacts[0].kind, ArtifactKind::Image);
        assert!(
            artifacts
                .iter()
                .any(|artifact| artifact.kind == ArtifactKind::File)
        );
        assert!(
            artifacts
                .iter()
                .any(|artifact| artifact.kind == ArtifactKind::Link)
        );
    }

    #[test]
    fn collects_hinted_nested_tool_metadata_but_ignores_unsafe_values() {
        let message = ChatMessage {
            role: MessageRole::Tool,
            metadata: [(
                "result".into(),
                serde_json::json!({"output_path": "../outputs/data.csv", "token": "secret", "bad": "javascript:alert(1)\n"}),
            )]
            .into_iter()
            .collect(),
            ..ChatMessage::default()
        };
        let artifacts = collect_session_artifacts(&session(), &[message]);
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].target, "../outputs/data.csv");
    }

    #[test]
    fn filters_by_kind_and_case_insensitive_session_text() {
        let artifact = ArtifactRecord {
            id: "1".into(),
            kind: ArtifactKind::Image,
            target: "./chart.png".into(),
            label: "chart.png".into(),
            session_id: "session-1".into(),
            session_title: "Quarterly Report".into(),
            base_dir: None,
            timestamp: 0.0,
        };
        assert!(matches_filter(&artifact, "image", "QUARTERLY"));
        assert!(!matches_filter(&artifact, "file", ""));
    }
}
