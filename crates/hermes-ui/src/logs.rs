use dioxus::prelude::*;
use hermes_core::{AppServices, DiagnosticsExportResult, DiagnosticsSnapshot};

use super::Surface;

fn matches_filter(source: &str, line: &str, query: &str) -> bool {
    let query = query.trim().to_ascii_lowercase();
    query.is_empty()
        || source.to_ascii_lowercase().contains(&query)
        || line.to_ascii_lowercase().contains(&query)
}

#[component]
pub(super) fn Logs() -> Element {
    let services = use_context::<AppServices>();
    let mut snapshot = use_signal(|| None::<DiagnosticsSnapshot>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut refresh = use_signal(|| 0_u64);
    let mut query = use_signal(String::new);
    let mut exporting = use_signal(|| false);
    let mut exported = use_signal(|| None::<DiagnosticsExportResult>);

    let load_services = services.clone();
    let _load = use_resource(move || {
        let services = load_services.clone();
        let _revision = refresh();
        async move {
            loading.set(true);
            match services.diagnostics.snapshot().await {
                Ok(value) => {
                    snapshot.set(Some(value));
                    error.set(None);
                }
                Err(problem) => error.set(Some(problem.to_string())),
            }
            loading.set(false);
        }
    });

    let export_services = services.clone();
    let export = Callback::new(move |()| {
        if exporting() {
            return;
        }
        exporting.set(true);
        error.set(None);
        let services = export_services.clone();
        spawn(async move {
            match services.diagnostics.export().await {
                Ok(Some(value)) => exported.set(Some(value)),
                Ok(None) => {}
                Err(problem) => error.set(Some(problem.to_string())),
            }
            exporting.set(false);
        });
    });

    let current = snapshot();
    let filter = query();
    rsx! {
        Surface { eyebrow: "Diagnostics", title: "Logs", subtitle: "Review bounded redacted service output and export a privacy-safe support bundle.",
            div { class: "settings-toolbar",
                input {
                    class: "settings-input",
                    aria_label: "Filter diagnostic logs",
                    placeholder: "Filter source or redacted text",
                    value: "{filter}",
                    oninput: move |event| query.set(event.value())
                }
                button { class: "button", disabled: loading(), onclick: move |_| refresh.set(refresh() + 1), "Refresh" }
                button { class: "primary-button", disabled: exporting(), onclick: move |_| export.call(()),
                    if exporting() { "Exporting…" } else { "Export redacted bundle" }
                }
            }
            if loading() {
                div { class: "loading-state", role: "status", "◌ Loading redacted diagnostics" }
            }
            if let Some(problem) = error() {
                div { class: "error-state", role: "alert", h2 { "Diagnostics request failed" } p { "{problem}" } }
            }
            if let Some(result) = exported() {
                section { class: "panel", role: "status",
                    header { class: "panel-title", "Export complete" }
                    p { class: "muted", "Report: {result.report_path.display()}" }
                    p { class: "muted", "Checksum: {result.checksum_path.display()}" }
                    code { "SHA-256 {result.sha256}" }
                }
            }
            if let Some(current) = current {
                for log in current.logs {
                    {
                        let visible = log.lines.into_iter()
                            .filter(|line| matches_filter(&log.source, line, &filter))
                            .collect::<Vec<_>>();
                        rsx! { section { class: "panel", key: "{log.source}",
                            header { class: "panel-title", "{log.source}" }
                            if visible.is_empty() {
                                p { class: "muted", "No matching redacted lines." }
                            } else {
                                pre { class: "log-output", aria_label: "{log.source} diagnostic log",
                                    for (index, line) in visible.into_iter().enumerate() {
                                        code { key: "{index}", "{line}\n" }
                                    }
                                }
                            }
                        } }
                    }
                }
                if let Some(crash) = current.crash {
                    section { class: "panel",
                        header { class: "panel-title", "Latest crash record" }
                        pre { class: "log-output", code { "{crash}" } }
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
    fn filter_is_case_insensitive_and_includes_source() {
        assert!(matches_filter("Supervisor", "ready", "super"));
        assert!(matches_filter("security", "TOKEN=[REDACTED]", "redacted"));
        assert!(!matches_filter("setup", "complete", "failure"));
    }
}
