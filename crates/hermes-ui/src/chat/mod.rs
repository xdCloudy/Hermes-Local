use dioxus::prelude::*;

use super::{
    Codicon, ErrorState, LoadingState, ProjectPicker, ProjectUiState, Route, SettingsUiState,
};

mod chat_controls;
mod fresh_model;
mod legacy;
mod reaction_panel;
mod reaction_store;
mod reaction_view;

#[component]
pub(super) fn ChatRuntimeProvider() -> Element {
    rsx! {
        style { ".transcript > article { content-visibility: auto; contain-intrinsic-size: auto 128px; }" }
        legacy::ChatRuntimeProvider {}
    }
}

#[component]
pub(super) fn Chat() -> Element {
    rsx! {
        div { class: "chat-runtime-wrapper",
            fresh_model::FreshModelControl {}
            legacy::Chat {}
        }
    }
}

#[component]
pub(super) fn Session(id: String) -> Element {
    let control_id = id.clone();
    let reaction_id = id.clone();
    rsx! {
        div { class: "chat-runtime-wrapper",
            chat_controls::ChatControls { session: Some(control_id) }
            legacy::Session { id }
            reaction_panel::ReactionPanel { session_id: reaction_id }
        }
    }
}
