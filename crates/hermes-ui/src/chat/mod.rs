use dioxus::prelude::*;

use super::{
    Codicon, ErrorState, LoadingState, ProjectPicker, ProjectUiState, Route, SettingsUiState,
};

mod chat_controls;
mod legacy;

#[component]
pub(super) fn ChatRuntimeProvider() -> Element {
    rsx! { legacy::ChatRuntimeProvider {} }
}

#[component]
pub(super) fn Chat() -> Element {
    rsx! {
        div { class: "chat-runtime-wrapper",
            chat_controls::ChatControls { session: None }
            legacy::Chat {}
        }
    }
}

#[component]
pub(super) fn Session(id: String) -> Element {
    let control_id = id.clone();
    rsx! {
        div { class: "chat-runtime-wrapper",
            chat_controls::ChatControls { session: Some(control_id) }
            legacy::Session { id }
        }
    }
}
