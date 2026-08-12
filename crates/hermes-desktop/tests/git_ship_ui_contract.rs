#[test]
fn review_ship_uses_typed_services_and_safe_external_opening() {
    let review = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../hermes-ui/src/review.rs"
    ));
    let main = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../apps/desktop/src/main.rs"
    ));
    let native = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../apps/desktop/src/git_ship_service.rs"
    ));

    assert!(review.contains("services.git_ship.clone()"));
    assert!(review.contains("service.commit(Path::new(&repo), &message, false)"));
    assert!(review.contains("service.commit(Path::new(&repo), &message, true)"));
    assert!(review.contains("service.push(Path::new(&repo)).await"));
    assert!(review.contains("service.create_pull_request(Path::new(&repo)).await"));
    assert!(review.contains("platform.open_external(&url).await"));
    assert!(review.contains("Commit + Push"));
    assert!(review.contains("Create PR"));
    assert!(!review.contains("Command::new"));
    assert!(!review.contains("std::process"));
    assert!(main.contains("git_ship_service::install(&mut native.services)"));
    assert!(native.contains("services.git_ship = Arc::new(GitShipService)"));
    assert!(native.contains("tokio::task::spawn_blocking"));
}
