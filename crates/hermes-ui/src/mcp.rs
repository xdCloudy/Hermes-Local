use std::collections::{BTreeMap, BTreeSet};

use dioxus::prelude::*;
use hermes_core::AppServices;
use hermes_protocol::{
    McpCatalogEntry, McpCatalogResponse, McpServerSummary, McpServerTestResult, McpServerUpsert,
};

use super::{SettingsUiState, Surface};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum McpMode {
    Servers,
    Catalog,
}

fn args_from_draft(value: &str) -> Vec<String> {
    value
        .lines()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}

fn env_from_draft(value: &str) -> Result<BTreeMap<String, String>, String> {
    let mut env = BTreeMap::new();
    for (index, line) in value.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(format!(
                "Environment line {} must use KEY=value.",
                index + 1
            ));
        };
        let key = key.trim();
        if key.is_empty() {
            return Err(format!("Environment line {} has an empty key.", index + 1));
        }
        env.insert(key.to_owned(), value.to_owned());
    }
    Ok(env)
}

fn blank_server() -> McpServerUpsert {
    McpServerUpsert {
        name: "my-server".into(),
        transport: "stdio".into(),
        enabled: true,
        ..McpServerUpsert::default()
    }
}

fn edit_server(server: &McpServerSummary) -> McpServerUpsert {
    McpServerUpsert {
        name: server.name.clone(),
        previous_name: Some(server.name.clone()),
        transport: if server.transport.is_empty() {
            "stdio".into()
        } else {
            server.transport.clone()
        },
        command: server.command.clone(),
        args: server.args.clone(),
        url: server.url.clone(),
        enabled: server.enabled,
        ..McpServerUpsert::default()
    }
}

fn transport_label(value: &str) -> &str {
    match value {
        "stdio" => "Local stdio process",
        "sse" => "HTTP server-sent events",
        "streamable-http" => "Streamable HTTP",
        "http" => "HTTP",
        _ => "MCP transport",
    }
}

#[component]
pub(super) fn Mcp() -> Element {
    let services = use_context::<AppServices>();
    let settings = use_context::<SettingsUiState>();
    let settings_signal = settings.settings;

    let mut mode = use_signal(|| McpMode::Servers);
    let mut query = use_signal(String::new);
    let mut servers = use_signal(Vec::<McpServerSummary>::new);
    let mut catalog = use_signal(McpCatalogResponse::default);
    let mut loading = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut notice = use_signal(|| None::<String>);
    let mut busy = use_signal(BTreeSet::<String>::new);
    let mut tests = use_signal(BTreeMap::<String, McpServerTestResult>::new);
    let mut refresh = use_signal(|| 0_u64);

    let mut editor = use_signal(|| None::<McpServerUpsert>);
    let mut args_draft = use_signal(String::new);
    let mut env_draft = use_signal(String::new);
    let mut remove_target = use_signal(|| None::<String>);
    let mut install_target = use_signal(|| None::<McpCatalogEntry>);
    let mut install_env = use_signal(BTreeMap::<String, String>::new);
    let mut install_confirmed = use_signal(|| false);

    let load_service = services.mcp.clone();
    let last_profile = use_signal(|| settings_signal().profile);
    use_effect(move || {
        let profile = settings_signal().profile;
        if last_profile() != profile {
            servers.set(Vec::new());
            catalog.set(McpCatalogResponse::default());
            tests.set(BTreeMap::new());
            editor.set(None);
            remove_target.set(None);
            install_target.set(None);
            install_env.set(BTreeMap::new());
            install_confirmed.set(false);
            busy.set(BTreeSet::new());
            error.set(None);
            notice.set(None);
            let mut previous = last_profile;
            previous.set(profile);
            refresh += 1;
        }
    });
    let _load = use_resource(move || {
        let _revision = refresh();
        let service = load_service.clone();
        let profile = settings_signal().profile;
        async move {
            loading.set(true);
            error.set(None);
            let listed = service.list(profile.as_deref()).await;
            let available = service.catalog(profile.as_deref()).await;
            if settings_signal().profile != profile {
                return;
            }
            match listed {
                Ok(rows) => servers.set(rows),
                Err(problem) => error.set(Some(problem.to_string())),
            }
            match available {
                Ok(entries) => catalog.set(entries),
                Err(problem) if error().is_none() => error.set(Some(problem.to_string())),
                Err(_) => {}
            }
            loading.set(false);
        }
    });

    let normalized_query = query().trim().to_ascii_lowercase();
    let filtered_servers = servers()
        .into_iter()
        .filter(|server| {
            normalized_query.is_empty()
                || server.name.to_ascii_lowercase().contains(&normalized_query)
                || server
                    .command
                    .as_deref()
                    .unwrap_or_default()
                    .to_ascii_lowercase()
                    .contains(&normalized_query)
                || server
                    .url
                    .as_deref()
                    .unwrap_or_default()
                    .to_ascii_lowercase()
                    .contains(&normalized_query)
        })
        .collect::<Vec<_>>();
    let filtered_catalog = catalog()
        .entries
        .into_iter()
        .filter(|entry| {
            normalized_query.is_empty()
                || entry.name.to_ascii_lowercase().contains(&normalized_query)
                || entry
                    .description
                    .to_ascii_lowercase()
                    .contains(&normalized_query)
        })
        .collect::<Vec<_>>();

    rsx! {
        Surface {
            eyebrow: "Extensions",
            title: "MCP servers",
            subtitle: "Configure explicit tool providers through a profile-scoped native boundary. Stored environment secrets are never read back into this page.",
            div { class: "toolbar-row",
                div { class: "segmented-control", role: "tablist", aria_label: "MCP view",
                    button { class: if mode() == McpMode::Servers { "active" } else { "" }, onclick: move |_| mode.set(McpMode::Servers), "Configured" }
                    button { class: if mode() == McpMode::Catalog { "active" } else { "" }, onclick: move |_| mode.set(McpMode::Catalog), "Catalog" }
                }
                input { class: "settings-input", aria_label: "Search MCP servers", placeholder: "Search MCP servers…", value: "{query}", oninput: move |event| query.set(event.value()) }
                button { class: "button", disabled: loading(), onclick: move |_| refresh += 1, "Refresh" }
                button { class: "button", disabled: !busy().is_empty(), onclick: {
                    let service = services.mcp.clone();
                    move |_| {
                        let service = service.clone();
                        let profile = settings_signal().profile;
                        error.set(None);
                        spawn(async move {
                            let result = service.reload(profile.as_deref()).await;
                            if settings_signal().profile != profile {
                                return;
                            }
                            match result {
                                Ok(()) => notice.set(Some("MCP tools reloaded for live sessions.".into())),
                                Err(problem) => error.set(Some(problem.to_string())),
                            }
                        });
                    }
                }, "Reload tools" }
                if mode() == McpMode::Servers {
                    button { class: "primary-button", onclick: move |_| { let draft = blank_server(); args_draft.set(String::new()); env_draft.set(String::new()); editor.set(Some(draft)); }, "Add server" }
                }
            }

            if loading() && servers().is_empty() { p { class: "muted", "Loading MCP configuration…" } }
            if let Some(problem) = error() { p { class: "inline-error", role: "alert", "{problem}" } }
            if let Some(message) = notice() { p { class: "success-message", role: "status", "{message}" } }

            if mode() == McpMode::Servers {
                if !loading() && filtered_servers.is_empty() {
                    div { class: "settings-card", p { "No configured MCP servers match this view." } }
                }
                div { class: "settings-list",
                    for server in filtered_servers {
                        {
                            let name = server.name.clone();
                            let toggle_name = name.clone();
                            let test_name = name.clone();
                            let edit_value = server.clone();
                            let remove_name = name.clone();
                            let is_busy = busy().contains(&name);
                            let result = tests().get(&name).cloned();
                            let result_error = result.as_ref().and_then(|value| value.error.clone()).unwrap_or_else(|| "MCP test failed.".into());
                            rsx! {
                                article { class: "settings-card mcp-server-row", id: "mcp-server-{name}",
                                    header { class: "settings-row",
                                        div { class: "settings-row-copy",
                                            strong { "{name}" }
                                            p { "{transport_label(&server.transport)}" }
                                            if let Some(command) = server.command.as_deref() { code { "{command}" } }
                                            if let Some(url) = server.url.as_deref() { code { "{url}" } }
                                        }
                                        span { class: if server.enabled { "scope-pill" } else { "scope-pill muted" }, if server.enabled { "Enabled" } else { "Disabled" } }
                                    }
                                    div { class: "toolbar-row",
                                        button { class: "button", disabled: is_busy, onclick: {
                                            let service = services.mcp.clone();
                                            move |_| {
                                                let service = service.clone();
                                                let profile = settings_signal().profile;
                                                let name = toggle_name.clone();
                                                let enabled = !server.enabled;
                                                busy.write().insert(name.clone());
                                                error.set(None);
                                                spawn(async move {
                                                    let result = service.set_enabled(profile.as_deref(), &name, enabled).await;
                                                    if settings_signal().profile != profile {
                                                        return;
                                                    }
                                                    match result {
                                                        Ok(()) => { notice.set(Some(format!("{} {}.", name, if enabled { "enabled" } else { "disabled" }))); refresh += 1; },
                                                        Err(problem) => error.set(Some(problem.to_string())),
                                                    }
                                                    busy.write().remove(&name);
                                                });
                                            }
                                        }, if server.enabled { "Disable" } else { "Enable" } }
                                        button { class: "button", disabled: is_busy || !server.enabled, onclick: {
                                            let service = services.mcp.clone();
                                            move |_| {
                                                let service = service.clone();
                                                let profile = settings_signal().profile;
                                                let name = test_name.clone();
                                                busy.write().insert(name.clone());
                                                error.set(None);
                                                spawn(async move {
                                                    let result = service.test(profile.as_deref(), &name).await;
                                                    if settings_signal().profile != profile {
                                                        return;
                                                    }
                                                    match result {
                                                        Ok(result) => { tests.write().insert(name.clone(), result); },
                                                        Err(problem) => error.set(Some(problem.to_string())),
                                                    }
                                                    busy.write().remove(&name);
                                                });
                                            }
                                        }, if is_busy { "Working…" } else { "Test" } }
                                        button { class: "button", disabled: is_busy, onclick: move |_| { let draft = edit_server(&edit_value); args_draft.set(draft.args.join("\n")); env_draft.set(String::new()); editor.set(Some(draft)); }, "Edit" }
                                        button { class: "button danger", disabled: is_busy, onclick: move |_| remove_target.set(Some(remove_name.clone())), "Remove" }
                                    }
                                    if let Some(result) = result {
                                        div { class: if result.ok { "success-message" } else { "inline-error" }, role: "status",
                                            if result.ok {
                                                "Connected · {result.tools.len()} tools · {result.prompts.unwrap_or(0)} prompts · {result.resources.unwrap_or(0)} resources"
                                            } else {
                                                "{result_error}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                if !loading() && filtered_catalog.is_empty() {
                    div { class: "settings-card", p { "No catalog entries match this search." } }
                }
                div { class: "settings-list",
                    for entry in filtered_catalog {
                        {
                            let install_entry = entry.clone();
                            rsx! {
                                article { class: "settings-card",
                                    header { class: "settings-row",
                                        div { class: "settings-row-copy", strong { "{entry.name}" } p { "{entry.description}" } small { "{entry.source} · {transport_label(&entry.transport)} · {entry.auth_type}" } }
                                        span { class: "scope-pill", if entry.installed { "Installed" } else if entry.needs_install { "Setup required" } else { "Available" } }
                                    }
                                    if !entry.required_env.is_empty() { p { class: "muted", "Requires {entry.required_env.len()} environment value(s). Values are write-only." } }
                                    button { class: "primary-button", disabled: entry.installed || busy().contains(&entry.name), onclick: move |_| { install_env.set(BTreeMap::new()); install_confirmed.set(false); install_target.set(Some(install_entry.clone())); }, if entry.installed { "Installed" } else { "Install" } }
                                }
                            }
                        }
                    }
                }
                for diagnostic in catalog().diagnostics {
                    p { class: "inline-error", "{diagnostic.name}: {diagnostic.message}" }
                }
            }
        }

        if let Some(draft) = editor() {
            div { class: "modal-backdrop", role: "presentation",
                section { class: "modal-card", role: "dialog", aria_modal: "true", aria_label: "MCP server editor",
                    header { h2 { if draft.previous_name.is_some() { "Edit MCP server" } else { "Add MCP server" } } p { "Blank environment values preserve existing stored secrets." } }
                    label { class: "field-stack", span { "Name" } input { class: "settings-input", value: "{draft.name}", oninput: move |event| { let mut next = editor().unwrap_or_default(); next.name = event.value(); editor.set(Some(next)); } } }
                    label { class: "field-stack", span { "Transport" }
                        select { class: "settings-input", value: "{draft.transport}", onchange: move |event| { let mut next = editor().unwrap_or_default(); next.transport = event.value(); editor.set(Some(next)); },
                            option { value: "stdio", "stdio" }
                            option { value: "http", "HTTP" }
                            option { value: "sse", "SSE" }
                            option { value: "streamable-http", "Streamable HTTP" }
                        }
                    }
                    if draft.transport == "stdio" {
                        label { class: "field-stack", span { "Command" } input { class: "settings-input mono", value: "{draft.command.clone().unwrap_or_default()}", oninput: move |event| { let mut next = editor().unwrap_or_default(); next.command = Some(event.value()); editor.set(Some(next)); } } }
                        label { class: "field-stack", span { "Arguments (one per line)" } textarea { class: "settings-input mono", rows: "5", value: "{args_draft}", oninput: move |event| args_draft.set(event.value()) } }
                    } else {
                        label { class: "field-stack", span { "URL" } input { class: "settings-input mono", value: "{draft.url.clone().unwrap_or_default()}", oninput: move |event| { let mut next = editor().unwrap_or_default(); next.url = Some(event.value()); editor.set(Some(next)); } } }
                    }
                    label { class: "field-stack", span { "Environment (KEY=value, one per line)" } textarea { class: "settings-input mono", rows: "5", value: "{env_draft}", placeholder: "TOKEN=…", oninput: move |event| env_draft.set(event.value()) } small { "Stored values are never loaded into this editor. Leave a value blank to keep the existing secret." } }
                    label { class: "settings-row", input { r#type: "checkbox", checked: draft.enabled, onchange: move |event| { let mut next = editor().unwrap_or_default(); next.enabled = event.checked(); editor.set(Some(next)); } } span { "Enabled" } }
                    footer { class: "modal-actions",
                        button { class: "button", onclick: move |_| editor.set(None), "Cancel" }
                        button { class: "primary-button", disabled: busy().contains("__save__"), onclick: {
                            let service = services.mcp.clone();
                            move |_| {
                                let service = service.clone();
                                let profile = settings_signal().profile;
                                let Some(mut input) = editor() else { return; };
                                input.args = args_from_draft(&args_draft());
                                input.env = match env_from_draft(&env_draft()) { Ok(env) => env, Err(problem) => { error.set(Some(problem)); return; } };
                                busy.write().insert("__save__".into());
                                error.set(None);
                                spawn(async move {
                                    let saved = service.upsert(profile.as_deref(), &input).await;
                                    if settings_signal().profile != profile {
                                        return;
                                    }
                                    match saved {
                                        Ok(()) => {
                                            let reloaded = service.reload(profile.as_deref()).await;
                                            if settings_signal().profile != profile { return; }
                                            match reloaded {
                                                Ok(()) => { editor.set(None); notice.set(Some(format!("{} saved and MCP tools reloaded.", input.name))); refresh += 1; },
                                                Err(problem) => { editor.set(None); notice.set(Some(format!("{} saved, but live sessions were not reloaded.", input.name))); error.set(Some(problem.to_string())); refresh += 1; },
                                            }
                                        },
                                        Err(problem) => error.set(Some(problem.to_string())),
                                    }
                                    busy.write().remove("__save__");
                                });
                            }
                        }, if busy().contains("__save__") { "Saving…" } else { "Save server" } }
                    }
                }
            }
        }

        if let Some(name) = remove_target() {
            div { class: "modal-backdrop", role: "presentation",
                section { class: "modal-card", role: "dialog", aria_modal: "true", aria_label: "Remove MCP server",
                    header { h2 { "Remove {name}?" } p { "This removes the server configuration from the active profile and reloads MCP tools." } }
                    footer { class: "modal-actions",
                        button { class: "button", onclick: move |_| remove_target.set(None), "Cancel" }
                        button { class: "button danger", onclick: {
                            let service = services.mcp.clone();
                            move |_| {
                                let service = service.clone();
                                let profile = settings_signal().profile;
                                let name = name.clone();
                                busy.write().insert(name.clone());
                                spawn(async move {
                                    let removed = service.remove(profile.as_deref(), &name).await;
                                    if settings_signal().profile != profile {
                                        return;
                                    }
                                    match removed {
                                        Ok(()) => {
                                            let reloaded = service.reload(profile.as_deref()).await;
                                            if settings_signal().profile != profile { return; }
                                            match reloaded {
                                                Ok(()) => { remove_target.set(None); tests.write().remove(&name); notice.set(Some(format!("{name} removed and MCP tools reloaded."))); refresh += 1; },
                                                Err(problem) => { remove_target.set(None); tests.write().remove(&name); notice.set(Some(format!("{name} removed, but live sessions were not reloaded."))); error.set(Some(problem.to_string())); refresh += 1; },
                                            }
                                        },
                                        Err(problem) => error.set(Some(problem.to_string())),
                                    }
                                    busy.write().remove(&name);
                                });
                            }
                        }, "Remove" }
                    }
                }
            }
        }

        if let Some(entry) = install_target() {
            div { class: "modal-backdrop", role: "presentation",
                section { class: "modal-card", role: "dialog", aria_modal: "true", aria_label: "Install MCP catalog entry",
                    header { h2 { "Install {entry.name}" } p { "This adds a tool provider that can execute commands or contact a remote service. Review its source and connection before trusting it." } }
                    p { class: "muted", "Source: {entry.source} · {transport_label(&entry.transport)}" }
                    if let Some(command) = entry.command.as_deref() { code { "{command}" } }
                    if let Some(url) = entry.url.as_deref() { code { "{url}" } }
                    for required in entry.required_env.clone() {
                        label { class: "field-stack", span { "{required.prompt}" }
                            input { class: "settings-input mono", r#type: "password", value: "{install_env().get(&required.name).cloned().unwrap_or_default()}", placeholder: "{required.name}", oninput: move |event| { install_env.write().insert(required.name.clone(), event.value()); } }
                            if required.required { small { "Required" } }
                        }
                    }
                    label { class: "settings-row", input { r#type: "checkbox", checked: install_confirmed(), onchange: move |event| install_confirmed.set(event.checked()) } span { "I reviewed this provider and trust its source and requested access." } }
                    footer { class: "modal-actions",
                        button { class: "button", onclick: move |_| { install_confirmed.set(false); install_target.set(None); }, "Cancel" }
                        button { class: "primary-button", disabled: busy().contains(&entry.name) || !install_confirmed(), onclick: {
                            let service = services.mcp.clone();
                            move |_| {
                                let service = service.clone();
                                let profile = settings_signal().profile;
                                let name = entry.name.clone();
                                let values = install_env();
                                if entry.required_env.iter().any(|field| field.required && values.get(&field.name).is_none_or(String::is_empty)) { error.set(Some("Enter every required environment value.".into())); return; }
                                busy.write().insert(name.clone());
                                spawn(async move {
                                    let installed = service.install_catalog(profile.as_deref(), &name, &values).await;
                                    if settings_signal().profile != profile {
                                        return;
                                    }
                                    match installed {
                                        Ok(result) if result.ok => {
                                            let reloaded = service.reload(profile.as_deref()).await;
                                            if settings_signal().profile != profile { return; }
                                            match reloaded {
                                                Ok(()) => { install_target.set(None); install_env.set(BTreeMap::new()); install_confirmed.set(false); notice.set(Some(if result.background { format!("{name} installation started; MCP tools will refresh as setup completes.") } else { format!("{name} installed and MCP tools reloaded.") })); refresh += 1; },
                                                Err(problem) => { install_target.set(None); install_env.set(BTreeMap::new()); install_confirmed.set(false); notice.set(Some(format!("{name} was accepted, but live sessions were not reloaded."))); error.set(Some(problem.to_string())); refresh += 1; },
                                            }
                                        },
                                        Ok(_) => error.set(Some("Agent did not confirm the MCP catalog install.".into())),
                                        Err(problem) => error.set(Some(problem.to_string())),
                                    }
                                    busy.write().remove(&name);
                                });
                            }
                        }, "Install" }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{args_from_draft, env_from_draft};

    #[test]
    fn editor_parses_bounded_line_oriented_inputs() {
        assert_eq!(args_from_draft("--one\n\n value "), vec!["--one", "value"]);
        let env = env_from_draft("TOKEN=secret\nEMPTY=").expect("environment draft");
        assert_eq!(env.get("TOKEN").map(String::as_str), Some("secret"));
        assert_eq!(env.get("EMPTY").map(String::as_str), Some(""));
        assert!(env_from_draft("BROKEN").is_err());
    }
}
