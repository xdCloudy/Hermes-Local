use dioxus::prelude::*;
use hermes_core::AppServices;
use hermes_protocol::RuntimeStatus;

use super::Surface;

fn update_status(value: &serde_json::Value) -> String {
    value
        .get("status")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= 128)
        .unwrap_or("unknown")
        .to_owned()
}

#[component]
pub(super) fn About() -> Element {
    let services = use_context::<AppServices>();
    let mut version = use_signal(|| None::<String>);
    let mut runtime = use_signal(|| None::<RuntimeStatus>);
    let mut update = use_signal(|| None::<String>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);

    let load_services = services.clone();
    let _load = use_resource(move || {
        let services = load_services.clone();
        async move {
            let version_result = services.platform.version().await;
            let runtime_result = services.runtime.status().await;
            let update_result = services.updates.check().await;
            if let Ok(value) = &version_result {
                version.set(Some(value.clone()));
            }
            if let Ok(value) = &runtime_result {
                runtime.set(Some(value.clone()));
            }
            if let Ok(value) = &update_result {
                update.set(Some(update_status(value)));
            }
            let problem = version_result
                .err()
                .or_else(|| runtime_result.err())
                .or_else(|| update_result.err());
            error.set(problem.map(|problem| problem.to_string()));
            loading.set(false);
        }
    });

    let client_version = version().unwrap_or_else(|| "Unavailable".into());
    let update_state = update().unwrap_or_else(|| "Unavailable".into());

    rsx! {
        Surface { eyebrow: "Hermes Local", title: "About", subtitle: "Version, runtime identity and update provenance from typed native services.",
            if loading() {
                div { class: "loading-state", role: "status", "◌ Loading build information" }
            }
            if let Some(problem) = error() {
                p { class: "inline-error", role: "alert", "Some provenance could not be loaded: {problem}" }
            }
            section { class: "panel",
                header { class: "panel-title", "Product identity" }
                div { class: "integrity-grid",
                    div { class: "integrity-item", span { "Product" } strong { "Hermes Local" } }
                    div { class: "integrity-item", span { "Client version" } strong { "{client_version}" } }
                    div { class: "integrity-item", span { "Client runtime" } strong { "Rust + Dioxus" } }
                    div { class: "integrity-item", span { "Update state" } strong { "{update_state}" } }
                }
            }
            if let Some(runtime) = runtime() {
                {
                    let agent = runtime.agent_version.clone().unwrap_or_else(|| "Not reported".into());
                    let model = runtime.model.clone().unwrap_or_else(|| "Not reported".into());
                    let provider = runtime.provider.clone().unwrap_or_else(|| "Not reported".into());
                    rsx! {
                section { class: "panel",
                    header { class: "panel-title", "Connected runtime" }
                    div { class: "integrity-grid",
                        div { class: "integrity-item", span { "Agent version" } strong { "{agent}" } }
                        div { class: "integrity-item", span { "Model" } strong { "{model}" } }
                        div { class: "integrity-item", span { "Provider" } strong { "{provider}" } }
                        div { class: "integrity-item", span { "Runtime phase" } strong { "{runtime.phase}" } }
                    }
                }
                    }
                }
            }
            p { class: "muted", "Release builds include the generated SBOM, checksums and attestation bundles in their distribution artifacts." }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn update_projection_does_not_expose_arbitrary_state_fields() {
        let value = json!({
            "status": "idle",
            "private_path": "C:\\Users\\person\\secret",
            "token": "must-not-render"
        });
        assert_eq!(update_status(&value), "idle");
    }

    #[test]
    fn malformed_update_status_is_bounded() {
        assert_eq!(
            update_status(&json!({ "status": "x".repeat(129) })),
            "unknown"
        );
        assert_eq!(update_status(&json!({ "status": 7 })), "unknown");
    }
}
