use dioxus::prelude::*;

use super::{
    Codicon, ErrorState, LoadingState, ProjectPicker, ProjectUiState, Route, SettingsUiState,
};

mod legacy;
mod runtime_controls;

#[component]
pub(super) fn ChatRuntimeProvider() -> Element {
    rsx! { legacy::ChatRuntimeProvider {} }
}

#[component]
pub(super) fn Chat() -> Element {
    rsx! {
        div { class: "chat-runtime-wrapper",
            runtime_controls::RuntimeControls { session_id: None }
            legacy::Chat {}
        }
    }
}

#[component]
pub(super) fn Session(id: String) -> Element {
    let control_id = id.clone();
    rsx! {
        div { class: "chat-runtime-wrapper",
            runtime_controls::RuntimeControls { session_id: Some(control_id) }
            legacy::Session { id }
        }
    }
}
