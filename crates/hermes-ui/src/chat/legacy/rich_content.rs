use dioxus::prelude::*;

mod ansi;
mod base;

#[component]
pub(super) fn RichContent(text: String, on_open_link: Callback<String>) -> Element {
    if let Some(payload) = ansi::renderable_payload(&text) {
        return rsx! { ansi::AnsiContent { text: payload } };
    }
    rsx! { base::RichContent { text, on_open_link } }
}
