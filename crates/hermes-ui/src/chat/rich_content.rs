use dioxus::prelude::*;

#[component]
pub(super) fn RichContent(text: String) -> Element {
    rsx! { p { class: "rich-content", "{text}" } }
}
