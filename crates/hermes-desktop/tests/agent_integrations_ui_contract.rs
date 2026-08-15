use std::{fs, path::PathBuf};

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read_source(path: PathBuf) -> String {
    fs::read_to_string(path)
        .expect("source file")
        .replace("\r\n", "\n")
}

#[test]
fn cron_messaging_and_webhooks_cross_typed_bounded_services() {
    let root = repository_root();
    let core = read_source(root.join("crates/hermes-core/src/lib.rs"));
    let desktop = read_source(root.join("crates/hermes-desktop/src/lib.rs"));
    let shell = read_source(root.join("crates/hermes-ui/src/lib.rs"));
    let automations = read_source(root.join("crates/hermes-ui/src/automations.rs"));
    let integrations = read_source(root.join("crates/hermes-ui/src/integrations.rs"));

    assert!(core.contains("pub trait CronService"));
    assert!(core.contains("pub trait IntegrationService"));
    assert!(core.contains("pub cron: Arc<dyn CronService>"));
    assert!(core.contains("pub integrations: Arc<dyn IntegrationService>"));
    assert!(desktop.contains("impl CronService for GatewayServices"));
    assert!(desktop.contains("impl IntegrationService for GatewayServices"));
    assert!(desktop.contains("MAX_AGENT_FEATURE_RESPONSE_BYTES"));
    assert!(desktop.contains("agent_feature_segment"));

    assert!(shell.contains("mod automations;"));
    assert!(shell.contains("mod integrations;"));
    assert!(!shell.contains("simple_surface!(\n    Automations,"));
    assert!(!shell.contains("simple_surface!(\n    Integrations,"));
    assert!(automations.contains(".cron\n                    .create"));
    assert!(automations.contains(".cron\n                    .update"));
    assert!(automations.contains("services.cron.trigger"));
    assert!(automations.contains("services.cron.delete"));
    assert!(integrations.contains("Secret values are write-only"));
    assert!(integrations.contains("one-time secret"));
    assert!(integrations.contains("approve_pairing"));
    assert!(integrations.contains("revoke_pairing"));
    assert!(integrations.contains("set_webhook_enabled"));
    assert!(integrations.contains("delete_webhook"));
}
