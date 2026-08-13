use std::sync::{Arc, RwLock};

use hermes_core::{EventStream, ServiceFuture, ServiceResult, SessionService};
use hermes_protocol::{
    ChatMessage, SelectedAttachment, SessionAttachmentResult, SessionCreateRequest,
    SessionDirectiveResult, SessionResumeResponse, SessionSummary,
};

#[derive(Clone, Debug, Default)]
pub(in crate::chat) struct DraftRuntimeOverrides {
    inner: Arc<RwLock<DraftRuntimeSelection>>,
}

#[derive(Clone, Debug, Default)]
struct DraftRuntimeSelection {
    provider: String,
    model: String,
    yolo: bool,
}

impl DraftRuntimeOverrides {
    pub(in crate::chat) fn set_model(&self, provider: String, model: String) {
        let mut state = self
            .inner
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.provider = provider;
        state.model = model;
    }

    pub(in crate::chat) fn model(&self) -> Option<(String, String)> {
        let state = self
            .inner
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        (!state.provider.is_empty() && !state.model.is_empty())
            .then(|| (state.provider.clone(), state.model.clone()))
    }

    pub(in crate::chat) fn set_yolo(&self, enabled: bool) {
        self.inner
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .yolo = enabled;
    }

    pub(in crate::chat) fn yolo(&self) -> bool {
        self.inner
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .yolo
    }
}

#[derive(Clone)]
pub(super) struct DraftSessionService {
    inner: Arc<dyn SessionService>,
    overrides: DraftRuntimeOverrides,
}

impl DraftSessionService {
    pub(super) fn new(inner: Arc<dyn SessionService>, overrides: DraftRuntimeOverrides) -> Self {
        Self { inner, overrides }
    }
}

impl SessionService for DraftSessionService {
    fn list(&self) -> ServiceFuture<'_, Vec<SessionSummary>> {
        self.inner.list()
    }

    fn create(&self, request: SessionCreateRequest) -> ServiceFuture<'_, SessionSummary> {
        let inner = self.inner.clone();
        let overrides = self.overrides.clone();
        Box::pin(async move {
            let created = inner.create(request).await?;
            let runtime_id = created
                .runtime_id
                .as_deref()
                .unwrap_or(created.id.as_str())
                .to_owned();

            if let Some((provider, model)) = overrides.model() {
                inner
                    .execute_directive(
                        &runtime_id,
                        &format!("/model {model} --provider {provider} --session"),
                    )
                    .await?;
            }

            if overrides.yolo() {
                inner.execute_directive(&runtime_id, "/yolo").await?;
                overrides.set_yolo(false);
            }

            Ok(created)
        })
    }

    fn resume(&self, session_id: &str) -> ServiceFuture<'_, SessionResumeResponse> {
        self.inner.resume(session_id)
    }

    fn history(&self, session_id: &str) -> ServiceFuture<'_, Vec<ChatMessage>> {
        self.inner.history(session_id)
    }

    fn execute_directive(
        &self,
        session_id: &str,
        command: &str,
    ) -> ServiceFuture<'_, SessionDirectiveResult> {
        self.inner.execute_directive(session_id, command)
    }

    fn attach(
        &self,
        session_id: &str,
        attachment: &SelectedAttachment,
    ) -> ServiceFuture<'_, SessionAttachmentResult> {
        self.inner.attach(session_id, attachment)
    }

    fn detach_image(&self, session_id: &str, path: &str) -> ServiceFuture<'_, ()> {
        self.inner.detach_image(session_id, path)
    }

    fn submit(&self, session_id: &str, text: &str) -> ServiceFuture<'_, ()> {
        self.inner.submit(session_id, text)
    }

    fn interrupt(&self, session_id: &str) -> ServiceFuture<'_, ()> {
        self.inner.interrupt(session_id)
    }

    fn set_pinned(&self, session_id: &str, pinned: bool) -> ServiceFuture<'_, ()> {
        self.inner.set_pinned(session_id, pinned)
    }

    fn set_archived(&self, session_id: &str, archived: bool) -> ServiceFuture<'_, ()> {
        self.inner.set_archived(session_id, archived)
    }

    fn rename(
        &self,
        session_id: &str,
        runtime_id: Option<&str>,
        title: &str,
    ) -> ServiceFuture<'_, ()> {
        self.inner.rename(session_id, runtime_id, title)
    }

    fn delete(&self, session_id: &str) -> ServiceFuture<'_, ()> {
        self.inner.delete(session_id)
    }

    fn events(&self) -> ServiceResult<EventStream> {
        self.inner.events()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draft_model_is_sticky_and_yolo_is_explicit() {
        let state = DraftRuntimeOverrides::default();
        assert_eq!(state.model(), None);
        assert!(!state.yolo());
        state.set_model("provider".into(), "model".into());
        state.set_yolo(true);
        assert_eq!(state.model(), Some(("provider".into(), "model".into())));
        assert!(state.yolo());
    }
}
