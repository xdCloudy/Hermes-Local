use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Duration,
};

use dioxus::prelude::*;
use futures_util::future::join_all;
use hermes_core::{AppServices, SkillsService};
use hermes_protocol::{
    SkillActionStart, SkillHubInstalledEntry, SkillHubPreview, SkillHubResult, SkillHubScanResult,
    SkillHubSourcesResponse, SkillSummary,
};

use super::{SettingsUiState, Surface};

const UPDATE_ALL_KEY: &str = "__update_all__";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SkillsMode {
    Local,
    Hub,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HubActionKind {
    Install,
    Uninstall,
    Update,
}

impl HubActionKind {
    const fn label(self) -> &'static str {
        match self {
            Self::Install => "Install",
            Self::Uninstall => "Uninstall",
            Self::Update => "Update",
        }
    }
}

#[derive(Clone, Debug)]
struct HubActionState {
    kind: HubActionKind,
    running: bool,
    lines: Vec<String>,
}

#[derive(Clone, Debug)]
enum HubMutation {
    Install { identifier: String },
    Uninstall { key: String, name: String },
    Update,
}

fn trust_rank(value: &str) -> u8 {
    match value {
        "builtin" => 2,
        "trusted" => 1,
        _ => 0,
    }
}

fn bounded_text(value: &str, max_chars: usize) -> String {
    let mut output = value.chars().take(max_chars).collect::<String>();
    if value.chars().count() > max_chars {
        output.push_str("\nâ€¦ preview truncated â€¦");
    }
    output
}

fn strip_ansi(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(character) = chars.next() {
        if character != '\u{1b}' {
            output.push(character);
            continue;
        }
        if chars.peek() == Some(&'[') {
            let _ = chars.next();
            for next in chars.by_ref() {
                if ('@'..='~').contains(&next) {
                    break;
                }
            }
        }
    }
    output
}

#[allow(clippy::too_many_arguments)]
async fn run_hub_action(
    service: Arc<dyn SkillsService>,
    profile: Option<String>,
    mutation: HubMutation,
    epoch: u64,
    action_epoch: Signal<u64>,
    mut actions: Signal<BTreeMap<String, HubActionState>>,
    mut installed_overrides: Signal<BTreeMap<String, bool>>,
    mut active_log: Signal<Option<String>>,
    mut hub_error: Signal<Option<String>>,
    mut hub_refresh: Signal<u64>,
    mut local_refresh: Signal<u64>,
) {
    let (key, kind) = match &mutation {
        HubMutation::Install { identifier } => (identifier.clone(), HubActionKind::Install),
        HubMutation::Uninstall { key, .. } => (key.clone(), HubActionKind::Uninstall),
        HubMutation::Update => (UPDATE_ALL_KEY.to_owned(), HubActionKind::Update),
    };
    actions.write().insert(
        key.clone(),
        HubActionState {
            kind,
            running: true,
            lines: Vec::new(),
        },
    );
    active_log.set(Some(key.clone()));
    hub_error.set(None);

    let started: Result<SkillActionStart, _> = match mutation {
        HubMutation::Install { identifier } => {
            service.hub_install(profile.as_deref(), &identifier).await
        }
        HubMutation::Uninstall { name, .. } => {
            service.hub_uninstall(profile.as_deref(), &name).await
        }
        HubMutation::Update => service.hub_update(profile.as_deref()).await,
    };
    if action_epoch() != epoch {
        return;
    }
    let started = match started {
        Ok(started) => started,
        Err(problem) => {
            if let Some(action) = actions.write().get_mut(&key) {
                action.running = false;
            }
            hub_error.set(Some(problem.to_string()));
            return;
        }
    };

    loop {
        let status = service
            .action_status(profile.as_deref(), &started.name, 200)
            .await;
        if action_epoch() != epoch {
            return;
        }
        let status = match status {
            Ok(status) => status,
            Err(problem) => {
                if let Some(action) = actions.write().get_mut(&key) {
                    action.running = false;
                }
                hub_error.set(Some(problem.to_string()));
                return;
            }
        };
        actions.write().insert(
            key.clone(),
            HubActionState {
                kind,
                running: status.running,
                lines: status.lines,
            },
        );
        if !status.running {
            if status.exit_code == Some(0) && key != UPDATE_ALL_KEY {
                installed_overrides
                    .write()
                    .insert(key.clone(), kind != HubActionKind::Uninstall);
            }
            hub_refresh.set(hub_refresh() + 1);
            local_refresh.set(local_refresh() + 1);
            break;
        }
        tokio::time::sleep(Duration::from_millis(1200)).await;
    }
}

#[component]
pub(super) fn Skills() -> Element {
    let services = use_context::<AppServices>();
    let settings = use_context::<SettingsUiState>();
    let settings_signal = settings.settings;

    let mut mode = use_signal(|| SkillsMode::Local);
    let mut query = use_signal(String::new);
    let mut local_rows = use_signal(Vec::<SkillSummary>::new);
    let mut local_loading = use_signal(|| false);
    let mut local_error = use_signal(|| None::<String>);
    let mut local_busy = use_signal(BTreeSet::<String>::new);
    let mut local_refresh = use_signal(|| 0_u64);

    let mut hub_sources = use_signal(SkillHubSourcesResponse::default);
    let mut hub_installed = use_signal(BTreeMap::<String, SkillHubInstalledEntry>::new);
    let mut hub_results = use_signal(Vec::<SkillHubResult>::new);
    let mut hub_loading = use_signal(|| false);
    let mut hub_searching = use_signal(|| false);
    let mut hub_error = use_signal(|| None::<String>);
    let mut hub_refresh = use_signal(|| 0_u64);

    let mut detail = use_signal(|| None::<SkillHubResult>);
    let mut preview = use_signal(|| None::<SkillHubPreview>);
    let mut preview_loading = use_signal(|| false);
    let mut scan = use_signal(|| None::<SkillHubScanResult>);
    let mut scanning = use_signal(|| false);

    let mut actions = use_signal(BTreeMap::<String, HubActionState>::new);
    let mut installed_overrides = use_signal(BTreeMap::<String, bool>::new);
    let mut active_log = use_signal(|| None::<String>);
    let mut action_epoch = use_signal(|| 0_u64);
    let mut last_profile = use_signal(|| settings_signal().profile);

    use_effect(move || {
        let profile = settings_signal().profile;
        if last_profile() != profile {
            last_profile.set(profile);
            action_epoch.set(action_epoch() + 1);
            actions.set(BTreeMap::new());
            installed_overrides.set(BTreeMap::new());
            active_log.set(None);
            detail.set(None);
            preview.set(None);
            scan.set(None);
            hub_error.set(None);
            local_error.set(None);
        }
    });

    let local_service = services.skills.clone();
    let _local = use_resource(move || {
        let _revision = local_refresh();
        let profile = settings_signal().profile;
        let service = local_service.clone();
        async move {
            local_loading.set(true);
            match service.list(profile.as_deref()).await {
                Ok(rows) => {
                    local_rows.set(rows);
                    local_error.set(None);
                }
                Err(problem) => local_error.set(Some(problem.to_string())),
            }
            local_loading.set(false);
        }
    });

    let source_service = services.skills.clone();
    let _sources = use_resource(move || {
        let _revision = hub_refresh();
        let profile = settings_signal().profile;
        let service = source_service.clone();
        async move {
            hub_loading.set(true);
            match service.hub_sources(profile.as_deref()).await {
                Ok(response) => {
                    hub_installed.set(response.installed.clone());
                    hub_sources.set(response);
                    hub_error.set(None);
                }
                Err(problem) => hub_error.set(Some(problem.to_string())),
            }
            hub_loading.set(false);
        }
    });

    let search_service = services.skills.clone();
    let _search = use_resource(move || {
        let term = query().trim().to_owned();
        let current_mode = mode();
        let profile = settings_signal().profile;
        let sources = hub_sources().sources;
        let service = search_service.clone();
        async move {
            if current_mode != SkillsMode::Hub || term.is_empty() {
                hub_results.set(Vec::new());
                hub_searching.set(false);
                return;
            }
            hub_searching.set(true);
            tokio::time::sleep(Duration::from_millis(350)).await;
            let source_ids = {
                let searchable = sources
                    .into_iter()
                    .filter(|source| source.searchable != Some(false))
                    .map(|source| source.id)
                    .collect::<Vec<_>>();
                if searchable.is_empty() {
                    vec!["all".to_owned()]
                } else {
                    searchable
                }
            };
            let requests = source_ids.into_iter().map(|source| {
                let service = service.clone();
                let profile = profile.clone();
                let term = term.clone();
                async move {
                    service
                        .hub_search(profile.as_deref(), &term, &source, 50)
                        .await
                }
            });
            let responses = join_all(requests).await;
            let mut seen = BTreeMap::<String, SkillHubResult>::new();
            let mut installed = hub_sources().installed;
            let mut failures = Vec::new();
            for response in responses {
                match response {
                    Ok(response) => {
                        installed.extend(response.installed);
                        for result in response.results {
                            let replace = seen.get(&result.identifier).is_none_or(|current| {
                                trust_rank(&result.trust_level) > trust_rank(&current.trust_level)
                            });
                            if replace {
                                seen.insert(result.identifier.clone(), result);
                            }
                        }
                    }
                    Err(problem) => failures.push(problem.to_string()),
                }
            }
            let mut results = seen.into_values().collect::<Vec<_>>();
            results.sort_by(|left, right| {
                trust_rank(&right.trust_level)
                    .cmp(&trust_rank(&left.trust_level))
                    .then_with(|| left.name.cmp(&right.name))
            });
            hub_installed.set(installed);
            hub_results.set(results);
            if failures.is_empty() {
                hub_error.set(None);
            } else if hub_results().is_empty() {
                hub_error.set(Some(failures.join("; ")));
            }
            hub_searching.set(false);
        }
    });

    let normalized = query().trim().to_lowercase();
    let mut visible_local = local_rows()
        .into_iter()
        .filter(|skill| {
            normalized.is_empty()
                || skill.name.to_lowercase().contains(&normalized)
                || skill.description.to_lowercase().contains(&normalized)
                || skill.category.to_lowercase().contains(&normalized)
        })
        .collect::<Vec<_>>();
    visible_local.sort_by(|left, right| {
        right
            .usage
            .unwrap_or_default()
            .cmp(&left.usage.unwrap_or_default())
            .then_with(|| left.name.cmp(&right.name))
    });

    let listed_hub = if query().trim().is_empty() {
        hub_sources().featured
    } else {
        hub_results()
    };
    let current_profile = settings_signal()
        .profile
        .unwrap_or_else(|| "default".to_owned());

    rsx! {
          Surface { eyebrow: "Extensions", title: "Skills", subtitle: "Manage profile-scoped local skills and inspect trusted or community capabilities from connected Hubs.",
    div { style: "display:grid;gap:1rem;",
        section { class: "settings-card", style: "display:grid;gap:.75rem;",
            div { style: "display:flex;align-items:center;gap:.5rem;flex-wrap:wrap;",
                button { class: "button", disabled: mode() == SkillsMode::Local, onclick: move |_| mode.set(SkillsMode::Local), "Local skills" }
                button { class: "button", disabled: mode() == SkillsMode::Hub, onclick: move |_| mode.set(SkillsMode::Hub), "Skills Hub" }
                span { class: "scope-pill", "profile: {current_profile}" }
                span { style: "flex:1;" }
                button { class: "icon-button", aria_label: "Refresh skills", title: "Refresh skills", onclick: move |_| {
                    local_refresh.set(local_refresh() + 1);
                    hub_refresh.set(hub_refresh() + 1);
                }, "â†»" }
            }
            input { aria_label: "Search skills", placeholder: if mode() == SkillsMode::Hub { "Search connected Hubs" } else { "Search local skills" }, value: "{query}", oninput: move |event| query.set(event.value()) }
            if let Some(problem) = if mode() == SkillsMode::Hub { hub_error() } else { local_error() } {
                p { class: "error-text", role: "alert", "{problem}" }
            }
        }

        if mode() == SkillsMode::Local {
            section { class: "settings-card", style: "display:grid;gap:.4rem;",
                div { style: "display:flex;align-items:center;gap:.5rem;", strong { "Installed skills" } span { class: "scope-pill", "{visible_local.len()}" } }
                if local_loading() && visible_local.is_empty() { p { class: "muted", "Loading local skillsâ€¦" } }
                if !local_loading() && visible_local.is_empty() { p { class: "muted", "No local skills match this view." } }
                for skill in visible_local {
                    {
                        let name = skill.name.clone();
                        let next_enabled = !skill.enabled;
                        let service = services.skills.clone();
                        let busy = local_busy().contains(&name);
                        rsx! {
                            div { class: "settings-row", style: "align-items:flex-start;gap:.75rem;",
                                div { style: "min-width:0;flex:1;",
                                    div { style: "display:flex;align-items:center;gap:.4rem;flex-wrap:wrap;",
                                        strong { "{skill.name}" }
                                        span { class: "scope-pill", "{skill.category}" }
                                        if let Some(provenance) = skill.provenance.as_deref() { span { class: "scope-pill", "{provenance}" } }
                                        if let Some(usage) = skill.usage { span { class: "muted", "{usage} uses" } }
                                    }
                                    p { class: "muted", style: "margin:.2rem 0 0;", "{skill.description}" }
                                }
                                button { class: "button", disabled: busy, onclick: move |_| {
                                    let profile = settings_signal().profile;
                                    let epoch = action_epoch();
                                    let service = service.clone();
                                    let name = name.clone();
                                    local_busy.write().insert(name.clone());
                                    if let Some(row) = local_rows.write().iter_mut().find(|row| row.name == name) {
                                        row.enabled = next_enabled;
                                    }
                                    spawn(async move {
                                        let result = service.set_enabled(profile.as_deref(), &name, next_enabled).await;
                                        if action_epoch() != epoch { return; }
                                        local_busy.write().remove(&name);
                                        match result {
                                            Ok(_) => local_error.set(None),
                                            Err(problem) => {
                                                if let Some(row) = local_rows.write().iter_mut().find(|row| row.name == name) {
                                                    row.enabled = !next_enabled;
                                                }
                                                local_error.set(Some(problem.to_string()));
                                            }
                                        }
                                    });
                                }, if skill.enabled { "Disable" } else { "Enable" } }
                            }
                        }
                    }
                }
            }
        } else {
            section { class: "settings-card", style: "display:grid;gap:.65rem;",
                div { style: "display:flex;align-items:flex-start;gap:.5rem;",
                    div { style: "min-width:0;flex:1;", strong { "Connected Hubs" } div { class: "muted", "Searches run only against sources the Agent marks searchable; higher-trust duplicates win." } }
                    if !hub_installed().is_empty() {
                        {
                            let service = services.skills.clone();
                            let running = actions().get(UPDATE_ALL_KEY).is_some_and(|action| action.running);
                            rsx! { button { class: "button", disabled: running, onclick: move |_| {
                                let epoch = action_epoch();
                                spawn(run_hub_action(service.clone(), settings_signal().profile, HubMutation::Update, epoch, action_epoch, actions, installed_overrides, active_log, hub_error, hub_refresh, local_refresh));
                            }, if running { "Updatingâ€¦" } else { "Update installed" } } }
                        }
                    }
                }
                div { style: "display:flex;gap:.35rem;flex-wrap:wrap;",
                    for source in hub_sources().sources {
                        span { class: "scope-pill", title: if source.available == Some(false) { "Unavailable" } else if source.rate_limited == Some(true) { "Rate limited" } else { "Connected" }, "{source.label}" }
                    }
                }
                if hub_loading() && listed_hub.is_empty() { p { class: "muted", "Loading Hub catalogâ€¦" } }
                if hub_searching() { p { class: "muted", "Searching connected Hubsâ€¦" } }
                if !hub_loading() && !hub_searching() && listed_hub.is_empty() { p { class: "muted", if query().trim().is_empty() { "Search the Hub or choose a featured skill." } else { "No Hub results matched this search." } } }
                for skill in listed_hub {
                    {
                        let identifier = skill.identifier.clone();
                        let installed = installed_overrides()
                            .get(&identifier)
                            .copied()
                            .unwrap_or_else(|| hub_installed().contains_key(&identifier));
                        let running = actions().get(&identifier).is_some_and(|action| action.running);
                        let installed_name = hub_installed()
                            .get(&identifier)
                            .and_then(|entry| entry.name.clone())
                            .unwrap_or_else(|| skill.name.clone());
                        let preview_skill = skill.clone();
                        let preview_service = services.skills.clone();
                        let action_service = services.skills.clone();
                        let action_identifier = identifier.clone();
                        let action_name = installed_name.clone();
                        rsx! {
                            div { class: "settings-row", style: "align-items:flex-start;gap:.75rem;",
                                div { style: "min-width:0;flex:1;",
                                    div { style: "display:flex;align-items:center;gap:.4rem;flex-wrap:wrap;",
                                        strong { "{skill.name}" }
                                        span { class: "scope-pill", "{skill.trust_level}" }
                                        if installed { span { class: "scope-pill", "installed" } }
                                    }
                                    p { class: "muted", style: "margin:.2rem 0 0;", "{skill.description}" }
                                }
                                div { style: "display:flex;gap:.35rem;",
                                    button { class: "button", onclick: move |_| {
                                        detail.set(Some(preview_skill.clone()));
                                        preview.set(None);
                                        scan.set(None);
                                        preview_loading.set(true);
                                        let service = preview_service.clone();
                                        let identifier = preview_skill.identifier.clone();
                                        let profile = settings_signal().profile;
                                        let epoch = action_epoch();
                                        spawn(async move {
                                            let result = service.hub_preview(profile.as_deref(), &identifier).await;
                                            if action_epoch() != epoch { return; }
                                            preview_loading.set(false);
                                            match result {
                                                Ok(value) => { preview.set(Some(value)); hub_error.set(None); }
                                                Err(problem) => hub_error.set(Some(problem.to_string())),
                                            }
                                        });
                                    }, "Preview" }
                                    button { class: "button", disabled: running, onclick: move |_| {
                                        let epoch = action_epoch();
                                        let mutation = if installed {
                                            HubMutation::Uninstall { key: action_identifier.clone(), name: action_name.clone() }
                                        } else {
                                            HubMutation::Install { identifier: action_identifier.clone() }
                                        };
                                        spawn(run_hub_action(action_service.clone(), settings_signal().profile, mutation, epoch, action_epoch, actions, installed_overrides, active_log, hub_error, hub_refresh, local_refresh));
                                    }, if running { "Workingâ€¦" } else if installed { "Uninstall" } else { "Install" } }
                                }
                            }
                        }
                    }
                }
            }

            if let Some(selected) = detail() {
                section { class: "settings-card", style: "display:grid;gap:.75rem;",
                    div { style: "display:flex;align-items:flex-start;gap:.5rem;",
                        div { style: "min-width:0;flex:1;",
                            strong { "{selected.name}" }
                            div { class: "muted", "{selected.identifier}" }
                        }
                        button { class: "icon-button", aria_label: "Close skill preview", title: "Close", onclick: move |_| { detail.set(None); preview.set(None); scan.set(None); }, "Ã—" }
                    }
                    if preview_loading() { p { class: "muted", "Loading previewâ€¦" } }
                    if let Some(preview_value) = preview() {
                        div { style: "display:flex;gap:.4rem;flex-wrap:wrap;", span { class: "scope-pill", "{preview_value.trust_level}" } span { class: "scope-pill", "{preview_value.source}" } for tag in preview_value.tags { span { class: "scope-pill", "{tag}" } } }
                        p { class: "muted", "{preview_value.description}" }
                        if !preview_value.files.is_empty() { p { class: "muted", "Files: " {preview_value.files.join(", ")} } }
                        if !preview_value.skill_md.is_empty() { pre { style: "max-height:20rem;overflow:auto;white-space:pre-wrap;border:1px solid var(--stroke, #334155);padding:.75rem;border-radius:.4rem;", "{bounded_text(&preview_value.skill_md, 32000)}" } }
                    }
                    if let Some(scan_value) = scan() {
                        div { style: "display:grid;gap:.35rem;border:1px solid var(--stroke, #334155);padding:.75rem;border-radius:.4rem;",
                            strong { "Policy: {scan_value.policy} Â· Verdict: {scan_value.verdict}" }
                            if let Some(reason) = scan_value.policy_reason.as_deref() { p { class: "muted", "{reason}" } }
                            if !scan_value.summary.is_empty() { p { class: "muted", "{scan_value.summary}" } }
                            for finding in scan_value.findings.iter().take(20) {
                                p { class: "muted", style: "margin:0;", "[{finding.severity}] {finding.file}: {finding.description}" }
                            }
                        }
                    }
                    div { style: "display:flex;gap:.5rem;justify-content:flex-end;",
                        {
                            let scan_service = services.skills.clone();
                            let identifier = selected.identifier.clone();
                            rsx! { button { class: "button", disabled: scanning(), onclick: move |_| {
                                scanning.set(true);
                                let service = scan_service.clone();
                                let identifier = identifier.clone();
                                let profile = settings_signal().profile;
                                let epoch = action_epoch();
                                spawn(async move {
                                    let result = service.hub_scan(profile.as_deref(), &identifier).await;
                                    if action_epoch() != epoch { return; }
                                    scanning.set(false);
                                    match result {
                                        Ok(value) => { scan.set(Some(value)); hub_error.set(None); }
                                        Err(problem) => hub_error.set(Some(problem.to_string())),
                                    }
                                });
                            }, if scanning() { "Scanningâ€¦" } else { "Run security scan" } } }
                        }
                    }
                }
            }

            if let Some(key) = active_log() {
                {
                    let action = actions().get(&key).cloned();
          let log_text = action
              .as_ref()
              .map(|action| strip_ansi(&action.lines.join("\n")))
              .unwrap_or_default();
                    rsx! {
                        if let Some(action) = action {
                            section { class: "settings-card", style: "display:grid;gap:.5rem;",
                                div { style: "display:flex;align-items:center;gap:.5rem;", strong { "{action.kind.label()} action log" } if action.running { span { class: "scope-pill", "running" } } span { style: "flex:1;" } button { class: "icon-button", aria_label: "Close action log", title: "Close", onclick: move |_| active_log.set(None), "Ã—" } }
                                pre { style: "max-height:14rem;overflow:auto;white-space:pre-wrap;margin:0;", "{log_text}" }
                            }
                        }
                    }
                }
            }
        }
    }
          }
      }
}
