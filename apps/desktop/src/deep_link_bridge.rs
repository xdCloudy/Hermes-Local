use dioxus::prelude::*;
use hermes_ui::{ExternalActivation, Route};

use crate::{DesktopDataDir, deep_link};

const ACTIVATION_POLL_MS: u64 = 250;
const MAX_SESSION_ID_CHARS: usize = 512;

fn route_activation(name: &str) -> Option<Route> {
    Some(match name {
        "overview" => Route::Overview {},
        "chat" => Route::Chat {},
        "projects" => Route::Projects {},
        "files" => Route::Files {},
        "git" => Route::Git {},
        "worktrees" => Route::Worktrees {},
        "review" => Route::Review {},
        "terminal" => Route::Terminal {},
        "tasks" => Route::Tasks {},
        "services" => Route::Services {},
        "models" => Route::Models {},
        "profiles" => Route::Profiles {},
        "tools" => Route::Tools {},
        "memory" => Route::Memory {},
        "sessions" => Route::Sessions {},
        "integrations" => Route::Integrations {},
        "benchmarks" => Route::Benchmarks {},
        "security" => Route::Security {},
        "logs" => Route::Logs {},
        "artifacts" => Route::Artifacts {},
        "starmap" => Route::Starmap {},
        "model" => Route::Model {},
        "runtime" => Route::Runtime {},
        "trust" => Route::Trust {},
        "skills" => Route::Skills {},
        "mcp" => Route::Mcp {},
        "delegations" => Route::Delegations {},
        "cloud" => Route::Cloud {},
        "usage" => Route::Usage {},
        "automations" => Route::Automations {},
        "notifications" => Route::Notifications {},
        "quick-entry" => Route::QuickEntry {},
        "settings" => Route::Settings {},
        "settings/appearance" => Route::Appearance {},
        "settings/general" => Route::GeneralSettings {},
        "settings/provider" => Route::ProviderSettings {},
        "settings/updates" => Route::UpdateSettings {},
        "about" => Route::About {},
        _ => return None,
    })
}

fn valid_session_id(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= MAX_SESSION_ID_CHARS
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.' | ':')
        })
}

fn activation_for_uri(raw: &str) -> Result<Option<ExternalActivation>, String> {
    let link = deep_link::parse(raw)?;
    Ok(match link.kind.as_str() {
        "blueprint" if !link.name.trim().is_empty() => Some(ExternalActivation::Blueprint {
            name: link.name,
            params: link.params,
        }),
        "route" => route_activation(&link.name).map(ExternalActivation::Navigate),
        "session" if valid_session_id(&link.name) => Some(ExternalActivation::Navigate(
            Route::Session { id: link.name },
        )),
        _ => None,
    })
}

#[component]
pub fn DeepLinkBridge(children: Element) -> Element {
    let data_dir = use_context::<DesktopDataDir>().0.clone();
    let mut activation_queue = hermes_ui::use_external_activation_queue();
    let desktop = dioxus::desktop::window();

    let _activations = use_resource(move || {
        let data_dir = data_dir.clone();
        let desktop = desktop.clone();
        async move {
            loop {
                let drain_dir = data_dir.clone();
                match tokio::task::spawn_blocking(move || deep_link::drain_pending(&drain_dir)).await {
                    Ok(Ok(pending)) => {
                        let mut accepted = false;
                        for raw in pending {
                            match activation_for_uri(&raw) {
                                Ok(Some(activation)) => {
                                    activation_queue.write().push_back(activation);
                                    accepted = true;
                                }
                                Ok(None) => {
                                    eprintln!("Hermes Local ignored an unsupported deep-link activation.");
                                }
                                Err(error) => {
                                    eprintln!("Hermes Local rejected a deep-link activation: {error}");
                                }
                            }
                        }
                        if accepted {
                            desktop.set_focus();
                        }
                    }
                    Ok(Err(error)) => {
                        eprintln!("Hermes Local activation queue is unavailable: {error}");
                    }
                    Err(error) => {
                        eprintln!("Hermes Local activation worker failed: {error}");
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(ACTIVATION_POLL_MS)).await;
            }
        }
    });

    rsx! { {children} }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_route_activation_is_allowlisted() {
        assert_eq!(
            activation_for_uri("hermes://route/notifications").unwrap(),
            Some(ExternalActivation::Navigate(Route::Notifications {}))
        );
        assert_eq!(activation_for_uri("hermes://route/not-real").unwrap(), None);
    }

    #[test]
    fn blueprint_activation_preserves_reviewable_command_data() {
        let activation = activation_for_uri(
            "hermes://blueprint/morning-brief?mode=fast&note=hello%20world",
        )
        .unwrap();
        let Some(ExternalActivation::Blueprint { name, params }) = activation else {
            panic!("expected blueprint activation");
        };
        assert_eq!(name, "morning-brief");
        assert_eq!(params.get("mode").map(String::as_str), Some("fast"));
        assert_eq!(params.get("note").map(String::as_str), Some("hello world"));
    }

    #[test]
    fn session_activation_is_bounded_to_safe_route_ids() {
        assert_eq!(
            activation_for_uri("hermes://session/abc-123_def").unwrap(),
            Some(ExternalActivation::Navigate(Route::Session {
                id: "abc-123_def".into(),
            }))
        );
        assert_eq!(activation_for_uri("hermes://session/folder%2Fescape").unwrap(), None);
    }
}
