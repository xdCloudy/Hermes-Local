use dioxus::prelude::*;
use hermes_core::{AppServices, DiagnosticsExportResult, DiagnosticsSnapshot};

use super::Surface;

fn matches_filter(source: &str, line: &str, query: &str) -> bool {
    let query = query.trim().to_ascii_lowercase();
    query.is_empty()
        || source.to_ascii_lowercase().contains(&query)
        || line.to_ascii_lowercase().contains(&query)
}

fn presence(value: bool) -> &'static str {
    if value { "Detected" } else { "Not detected" }
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
    let mut recovering = use_signal(|| None::<&'static str>);
    let mut notice = use_signal(|| None::<String>);

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

    let clear_services = services.clone();
    let clear_crash = Callback::new(move |()| {
        if recovering().is_some() {
            return;
        }
        recovering.set(Some("crash"));
        error.set(None);
        notice.set(None);
        let services = clear_services.clone();
        spawn(async move {
            match services.diagnostics.clear_crash().await {
                Ok(()) => {
                    notice.set(Some("Crash record cleared.".into()));
                    refresh.set(refresh() + 1);
                }
                Err(problem) => error.set(Some(problem.to_string())),
            }
            recovering.set(None);
        });
    });

    let environment_services = services.clone();
    let open_environment = Callback::new(move |()| {
        if recovering().is_some() {
            return;
        }
        recovering.set(Some("environment"));
        error.set(None);
        notice.set(None);
        let services = environment_services.clone();
        spawn(async move {
            match services.diagnostics.open_environment_settings().await {
                Ok(()) => notice.set(Some("Opened Windows environment settings.".into())),
                Err(problem) => error.set(Some(problem.to_string())),
            }
            recovering.set(None);
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
            if let Some(message) = notice() {
                div { class: "success-state", role: "status", "{message}" }
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
                section { class: "panel",
                    header { class: "panel-title", "Windows environment" }
                    p { class: "muted", "Values and private paths are never exposed; only bounded presence signals are shown." }
                    div { class: "integrity-grid",
                        div { class: "integrity-item", span { "PATH entries" } strong { "{current.environment.path_entry_count}" } }
                        div { class: "integrity-item", span { "Proxy" } strong { "{presence(current.environment.proxy_configured)}" } }
                        div { class: "integrity-item", span { "Custom CA" } strong { "{presence(current.environment.custom_ca_configured)}" } }
                        div { class: "integrity-item", span { "WSL" } strong { "{presence(current.environment.wsl)}" } }
                        div { class: "integrity-item", span { "Display bridge" } strong { "{presence(current.environment.display_configured || current.environment.wayland_configured)}" } }
                        div { class: "integrity-item", span { "App data" } strong { "{presence(current.environment.appdata_configured && current.environment.localappdata_configured)}" } }
                        div { class: "integrity-item", span { "Temporary directory" } strong { "{presence(current.environment.temp_configured)}" } }
                    }
                    button {
                        class: "button",
                        disabled: recovering().is_some(),
                        onclick: move |_| open_environment.call(()),
                        if recovering() == Some("environment") { "Opening…" } else { "Open Windows environment settings" }
                    }
                }
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
                        button {
                            class: "button danger",
                            disabled: recovering().is_some(),
                            onclick: move |_| clear_crash.call(()),
                            if recovering() == Some("crash") { "Clearing…" } else { "Clear recovered crash record" }
                        }
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

    #[test]
    fn environment_presence_never_echoes_a_value() {
        assert_eq!(presence(true), "Detected");
        assert_eq!(presence(false), "Not detected");
    }
}
