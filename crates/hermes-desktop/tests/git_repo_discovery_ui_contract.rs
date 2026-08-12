#[test]
fn project_centre_discovery_uses_profile_policy_typed_scan_and_cancellation() {
    let ui = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../hermes-ui/src/lib.rs"
    ));
    let main = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../apps/desktop/src/main.rs"
    ));
    let native = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../apps/desktop/src/git_repo_scan_service.rs"
    ));

    assert!(ui.contains("config_service.load(profile.as_deref()).await"));
    assert!(ui.contains("repo_scan_enabled"));
    assert!(ui.contains("repo_scan_roots"));
    assert!(ui.contains("repo_scan_exclude_paths"));
    assert!(ui.contains("services.git_repo_scan.clone()"));
    assert!(ui.contains("cancellation.cancel()"));
    assert!(ui.contains("Register project"));
    assert!(ui.contains("project_service.create(&label, std::slice::from_ref(&root)).await"));
    assert!(!ui.contains("std::fs"));
    assert!(!ui.contains("Command::new"));
    assert!(!ui.contains("std::process"));
    assert!(main.contains("git_repo_scan_service::install(&mut native.services)"));
    assert!(native.contains("services.git_repo_scan = Arc::new(GitRepoScanService)"));
    assert!(native.contains("tokio::task::spawn_blocking"));
    assert!(native.contains("cancellation.is_cancelled()"));
}
