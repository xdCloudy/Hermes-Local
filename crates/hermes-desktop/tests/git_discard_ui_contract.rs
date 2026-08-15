#[test]
fn review_discard_is_typed_confirmed_and_platform_neutral() {
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
        "/../../apps/desktop/src/git_discard_service.rs"
    ));

    assert!(review.contains("services.git_discard.clone()"));
    assert!(review.contains("DiscardTarget::Path"));
    assert!(review.contains("DiscardTarget::All"));
    assert!(review.contains("Discard file"));
    assert!(review.contains("Discard all"));
    assert!(review.contains("Discard changes?"));
    assert!(review.contains("This cannot be undone. Ignored files are preserved."));
    assert!(!review.contains("Command::new"));
    assert!(!review.contains("std::process"));
    assert!(main.contains("git_discard_service::install(&mut native.services)"));
    assert!(native.contains("services.git_discard = Arc::new(GitDiscardService)"));
}
