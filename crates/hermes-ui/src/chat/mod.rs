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

const CHAT_RUNTIME_CSS: &str = r#"
.transcript > article { content-visibility: auto; contain-intrinsic-size: auto 128px; }
.ansi-bold { font-weight: 700; }
.ansi-black { color: #3f3f46; }
.ansi-red { color: #b91c1c; }
.ansi-green { color: #047857; }
.ansi-yellow { color: #b45309; }
.ansi-blue { color: #1d4ed8; }
.ansi-magenta { color: #a21caf; }
.ansi-cyan { color: #0e7490; }
.ansi-white { color: #52525b; }
.ansi-bright-black { color: #71717a; }
.ansi-bright-red { color: #e11d48; }
.ansi-bright-green { color: #059669; }
.ansi-bright-yellow { color: #d97706; }
.ansi-bright-blue { color: #0284c7; }
.ansi-bright-magenta { color: #db2777; }
.ansi-bright-cyan { color: #0d9488; }
.ansi-bright-white { color: #71717a; }
.theme-dark .ansi-black { color: #d4d4d8; }
.theme-dark .ansi-red { color: #fca5a5; }
.theme-dark .ansi-green { color: #6ee7b7; }
.theme-dark .ansi-yellow { color: #fcd34d; }
.theme-dark .ansi-blue { color: #93c5fd; }
.theme-dark .ansi-magenta { color: #f0abfc; }
.theme-dark .ansi-cyan { color: #67e8f9; }
.theme-dark .ansi-white { color: #e4e4e7; }
.theme-dark .ansi-bright-black { color: #a1a1aa; }
.theme-dark .ansi-bright-red { color: #fda4af; }
.theme-dark .ansi-bright-green { color: #a7f3d0; }
.theme-dark .ansi-bright-yellow { color: #fde68a; }
.theme-dark .ansi-bright-blue { color: #7dd3fc; }
.theme-dark .ansi-bright-magenta { color: #f9a8d4; }
.theme-dark .ansi-bright-cyan { color: #99f6e4; }
.theme-dark .ansi-bright-white { color: #f4f4f5; }
@media (prefers-color-scheme: dark) {
  .window-root:not(.theme-light) .ansi-black { color: #d4d4d8; }
  .window-root:not(.theme-light) .ansi-red { color: #fca5a5; }
  .window-root:not(.theme-light) .ansi-green { color: #6ee7b7; }
  .window-root:not(.theme-light) .ansi-yellow { color: #fcd34d; }
  .window-root:not(.theme-light) .ansi-blue { color: #93c5fd; }
  .window-root:not(.theme-light) .ansi-magenta { color: #f0abfc; }
  .window-root:not(.theme-light) .ansi-cyan { color: #67e8f9; }
  .window-root:not(.theme-light) .ansi-white { color: #e4e4e7; }
  .window-root:not(.theme-light) .ansi-bright-black { color: #a1a1aa; }
  .window-root:not(.theme-light) .ansi-bright-red { color: #fda4af; }
  .window-root:not(.theme-light) .ansi-bright-green { color: #a7f3d0; }
  .window-root:not(.theme-light) .ansi-bright-yellow { color: #fde68a; }
  .window-root:not(.theme-light) .ansi-bright-blue { color: #7dd3fc; }
  .window-root:not(.theme-light) .ansi-bright-magenta { color: #f9a8d4; }
  .window-root:not(.theme-light) .ansi-bright-cyan { color: #99f6e4; }
  .window-root:not(.theme-light) .ansi-bright-white { color: #f4f4f5; }
}
"#;

#[component]
pub(super) fn ChatRuntimeProvider() -> Element {
    rsx! {
        style { "{CHAT_RUNTIME_CSS}" }
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
