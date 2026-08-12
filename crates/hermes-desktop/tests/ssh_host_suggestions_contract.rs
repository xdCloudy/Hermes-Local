use std::{fs, path::PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read_repo_file(relative: &str) -> String {
    let path = repo_root().join(relative);
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

#[test]
fn ssh_host_suggestions_stay_on_the_typed_native_boundary() {
    let core = read_repo_file("crates/hermes-core/src/lib.rs");
    let desktop = read_repo_file("apps/desktop/src/ssh_service.rs");
    let ui = read_repo_file("crates/hermes-ui/src/lib.rs");

    assert!(
        core.contains("fn list_ssh_hosts(&self) -> ServiceFuture<'_, Vec<String>>"),
        "ConnectionService must keep an explicit typed SSH-host discovery contract"
    );
    assert!(
        desktop.contains("fn list_ssh_hosts(&self) -> ServiceFuture<'_, Vec<String>>")
            && desktop.contains("Ok(ssh_config::configured_hosts())"),
        "the Desktop connection wrapper must own ~/.ssh/config discovery"
    );
    assert!(
        ui.contains("let hosts_vec = service.list_ssh_hosts().await.unwrap_or_default();"),
        "Gateway settings must consume the typed ConnectionService host list"
    );
    assert!(
        ui.contains("ssh_mode.set(mode == ConnectionMode::Ssh);"),
        "host discovery must remain gated to SSH mode"
    );
    assert!(
        ui.contains("if config.mode == ConnectionMode::Ssh && !config.env_override"),
        "the SSH selector must stay hidden outside editable SSH configuration"
    );
    assert!(
        ui.contains("for host in ssh_hosts()") && ui.contains("option { \"{host}\" }"),
        "discovered aliases must remain rendered into the Gateway selector"
    );
}

#[test]
fn selecting_an_ssh_alias_changes_only_the_host_field() {
    let ui = read_repo_file("crates/hermes-ui/src/lib.rs");
    let marker = "strong { \"Host alias\" }";
    let start = ui
        .find(marker)
        .unwrap_or_else(|| panic!("Gateway SSH host-alias selector is missing"));
    let tail = &ui[start..];
    let end = tail
        .find("for host in ssh_hosts()")
        .map(|offset| offset + "for host in ssh_hosts()".len())
        .unwrap_or_else(|| panic!("Gateway SSH alias option rendering is missing"));
    let selector = &tail[..end];

    assert!(
        selector.contains("next.ssh_host = val;"),
        "selecting an alias must update the SSH host"
    );
    assert!(
        !selector.contains("next.ssh_user =")
            && !selector.contains("next.ssh_port =")
            && !selector.contains("next.ssh_key_path ="),
        "selecting an alias must not overwrite explicit user, port, or key-path fields"
    );
}

#[test]
fn ssh_config_contract_keeps_parity_and_safety_regressions_covered() {
    let config = read_repo_file("apps/desktop/src/ssh_config.rs");

    for required_test in [
        "host_parser_matches_electron_literal_alias_rules",
        "include_parser_matches_electron_token_rules",
        "collection_follows_includes_globs_and_cycles",
        "ssh_g_parser_takes_first_values",
        "enrichment_never_overwrites_manual_fields",
        "default_ssh_port_remains_unspecified_like_electron_ui",
    ] {
        assert!(
            config.contains(required_test),
            "SS-01 regression coverage disappeared: {required_test}"
        );
    }

    assert!(
        config.contains("const MAX_INCLUDE_DEPTH: usize = 8;")
            && config.contains("const MAX_CONFIG_BYTES: u64 = 1024 * 1024;")
            && config.contains("const MAX_SSH_G_BYTES: usize = 256 * 1024;")
            && config.contains("const SSH_G_TIMEOUT: Duration = Duration::from_secs(5);"),
        "SSH config discovery must remain bounded"
    );
}
