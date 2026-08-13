use std::sync::{Arc, RwLock};

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

    pub(in crate::chat) fn toggle_yolo(&self) -> bool {
        let mut state = self
            .inner
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.yolo = !state.yolo;
        state.yolo
    }

    pub(in crate::chat) fn yolo(&self) -> bool {
        self.inner
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .yolo
    }

    pub(in crate::chat) fn consume_yolo(&self) -> bool {
        let mut state = self
            .inner
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        std::mem::take(&mut state.yolo)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draft_model_is_sticky_and_yolo_is_one_shot() {
        let state = DraftRuntimeOverrides::default();
        assert_eq!(state.model(), None);
        assert!(!state.yolo());
        state.set_model("provider".into(), "model".into());
        assert!(state.toggle_yolo());
        assert_eq!(state.model(), Some(("provider".into(), "model".into())));
        assert!(state.consume_yolo());
        assert!(!state.yolo());
    }
}
