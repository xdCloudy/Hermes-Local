from pathlib import Path


def rep(path, old, new):
    p = Path(path)
    s = p.read_text(encoding='utf-8')
    if old not in s:
        raise SystemExit(f'missing pattern in {path}: {old[:80]!r}')
    p.write_text(s.replace(old, new, 1), encoding='utf-8')


def app(path, marker, content):
    p = Path(path)
    s = p.read_text(encoding='utf-8')
    if marker not in s:
        p.write_text(s.rstrip() + '\n\n' + content.strip() + '\n', encoding='utf-8')

protocol = 'crates/hermes-protocol/src/lib.rs'
rep(protocol, '#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]\npub struct SessionSummary {', '''#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AttachmentKind {
    #[default]
    File,
    Image,
}

/// Opaque, user-selected Desktop attachment. `id` is a capability token held by
/// Desktop authority; the shared UI never receives an arbitrary filesystem path.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct SelectedAttachment {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub kind: AttachmentKind,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub preview_data_url: Option<String>,
    #[serde(default)]
    pub attached_session_id: Option<String>,
    #[serde(default)]
    pub ref_text: Option<String>,
    #[serde(default)]
    pub staged_path: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct SessionAttachmentResult {
    #[serde(default)]
    pub attached: bool,
    #[serde(default)]
    pub kind: AttachmentKind,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub ref_text: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct SessionSummary {''')

core = 'crates/hermes-core/src/lib.rs'
rep(core, '    CustomEndpointsResponse, EnvVarInfo, FileEntry, GatewayEvent, GitStatus, MessageRole,\n', '    AttachmentKind, CustomEndpointsResponse, EnvVarInfo, FileEntry, GatewayEvent, GitStatus, MessageRole,\n')
rep(core, '    ProjectsSnapshot, ProviderActivation, RuntimeStatus, SessionCreateRequest,\n    SessionDirectiveResult, SessionResumeResponse, SessionSummary, SkillActionStart,\n', '    ProjectsSnapshot, ProviderActivation, RuntimeStatus, SelectedAttachment,\n    SessionAttachmentResult, SessionCreateRequest, SessionDirectiveResult, SessionResumeResponse,\n    SessionSummary, SkillActionStart,\n')
rep(core, '    fn submit(&self, session_id: &str, text: &str) -> ServiceFuture<\'_, ()>;', '''    fn attach(
        &self,
        _session_id: &str,
        _attachment: &SelectedAttachment,
    ) -> ServiceFuture<'_, SessionAttachmentResult> {
        Box::pin(async move {
            Err(ServiceError::Unavailable(
                "session attachments are unavailable on this host".into(),
            ))
        })
    }
    fn detach_image(&self, _session_id: &str, _path: &str) -> ServiceFuture<'_, ()> {
        Box::pin(async move {
            Err(ServiceError::Unavailable(
                "image detach is unavailable on this host".into(),
            ))
        })
    }
    fn submit(&self, session_id: &str, text: &str) -> ServiceFuture<'_, ()>;''')
rep(core, 'pub trait PlatformService: Send + Sync {\n    fn pick_folder(', '''pub trait PlatformService: Send + Sync {
    fn pick_attachments(
        &self,
        _title: &str,
        _starting_directory: Option<&Path>,
        _images_only: bool,
    ) -> ServiceFuture<'_, Vec<SelectedAttachment>> {
        Box::pin(async move {
            Err(ServiceError::Unavailable(
                "native attachment selection is unavailable on this host".into(),
            ))
        })
    }
    fn pick_folder(''')
rep(core, 'const COMPOSER_UNDO_LIMIT: usize = 64;', '''pub fn attachment_context_text(
    visible_text: &str,
    attachments: &[SessionAttachmentResult],
) -> String {
    let mut parts = attachments
        .iter()
        .filter_map(|attachment| attachment.ref_text.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let visible_text = visible_text.trim();
    if !visible_text.is_empty() {
        parts.push(visible_text.to_owned());
    }
    if parts.is_empty()
        && attachments
            .iter()
            .any(|attachment| attachment.kind == AttachmentKind::Image && attachment.attached)
    {
        return "What do you see in this image?".into();
    }
    parts.join("\\n\\n")
}

const COMPOSER_UNDO_LIMIT: usize = 64;''')
app(core, 'mod attachment_context_tests', '''#[cfg(test)]
mod attachment_context_tests {
    use super::*;

    #[test]
    fn file_refs_precede_visible_text() {
        let attachments = vec![SessionAttachmentResult {
            attached: true,
            kind: AttachmentKind::File,
            ref_text: Some("@file:`notes/a b.txt`".into()),
            ..SessionAttachmentResult::default()
        }];
        assert_eq!(attachment_context_text("summarise this", &attachments), "@file:`notes/a b.txt`\\n\\nsummarise this");
    }

    #[test]
    fn image_only_gets_fallback_prompt() {
        let attachments = vec![SessionAttachmentResult {
            attached: true,
            kind: AttachmentKind::Image,
            ..SessionAttachmentResult::default()
        }];
        assert_eq!(attachment_context_text("", &attachments), "What do you see in this image?");
    }
}''')

print('core attachment transform applied')
