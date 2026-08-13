use dioxus::prelude::*;

mod ansi;
mod base;

#[component]
pub(super) fn RichContent(text: String, on_open_link: Callback<String>) -> Element {
    if ansi::has_ansi(&text) {
        return rsx! { ansi::AnsiContent { text } };
    }
    rsx! { base::RichContent { text, on_open_link } }
}
