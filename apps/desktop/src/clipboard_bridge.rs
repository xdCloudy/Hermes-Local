use dioxus::prelude::*;

use crate::clipboard_service::ClipboardService;

#[component]
pub fn ClipboardBridge(children: Element) -> Element {
    let mut open = use_signal(|| false);
    let mut text = use_signal(String::new);
    let mut status = use_signal(|| None::<String>);

    let read_clipboard = move |_| {
        status.set(None);
        match ClipboardService.read_text() {
            Ok(value) => {
                text.set(value);
                status.set(Some("Read native clipboard text.".into()));
            }
            Err(error) => status.set(Some(error)),
        }
    };

    let write_clipboard = move |_| {
        status.set(None);
        match ClipboardService.write_text(&text()) {
            Ok(()) => status.set(Some("Copied text through the native clipboard service.".into())),
            Err(error) => status.set(Some(error)),
        }
    };

    rsx! {
        {children}
        div {
            style: "position:fixed;right:14px;bottom:14px;z-index:2147483000;font:12px system-ui,sans-serif;",
            button {
                style: "border:1px solid rgb(71 85 105);border-radius:6px;background:rgb(15 23 42);color:rgb(226 232 240);padding:7px 10px;box-shadow:0 8px 24px rgb(0 0 0 / 28%);cursor:pointer;",
                title: "Native clipboard text",
                onclick: move |_| open.set(!open()),
                "Clipboard"
            }
            if open() {
                div {
                    style: "position:absolute;right:0;bottom:40px;width:min(420px,calc(100vw - 28px));display:grid;gap:8px;padding:10px;border:1px solid rgb(71 85 105);border-radius:8px;background:rgb(9 11 16);color:rgb(226 232 240);box-shadow:0 18px 48px rgb(0 0 0 / 45%);",
                    strong { "Native clipboard" }
                    span { style: "color:rgb(148 163 184);", "Text operations stay in Desktop authority; the WebView never receives an OS clipboard handle." }
                    textarea {
                        value: "{text}",
                        rows: 7,
                        placeholder: "Clipboard text",
                        style: "width:100%;box-sizing:border-box;resize:vertical;border:1px solid rgb(51 65 85);border-radius:5px;background:rgb(15 23 42);color:rgb(241 245 249);padding:8px;font:12px ui-monospace,monospace;",
                        oninput: move |event| text.set(event.value()),
                    }
                    div { style: "display:flex;gap:7px;",
                        button {
                            style: "border:1px solid rgb(71 85 105);border-radius:5px;background:rgb(30 41 59);color:rgb(241 245 249);padding:6px 9px;cursor:pointer;",
                            onclick: read_clipboard,
                            "Read clipboard"
                        }
                        button {
                            style: "border:1px solid rgb(71 85 105);border-radius:5px;background:rgb(30 41 59);color:rgb(241 245 249);padding:6px 9px;cursor:pointer;",
                            onclick: write_clipboard,
                            "Copy text"
                        }
                    }
                    if let Some(message) = status() {
                        span { role: "status", style: "color:rgb(148 163 184);", "{message}" }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_is_a_dioxus_component() {
        let _ = ClipboardBridge;
    }
}
