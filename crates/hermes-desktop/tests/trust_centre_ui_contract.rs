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
fn trust_centre_stays_on_the_typed_service_boundary() {
    let core = read_repo_file("crates/hermes-core/src/lib.rs");
    let ui = read_repo_file("crates/hermes-ui/src/lib.rs");

    assert!(
        core.contains("pub trait TrustService: Send + Sync")
            && core.contains("fn snapshot(&self) -> ServiceFuture<'_, TrustSnapshot>")
            && core
                .contains("fn set_policy(&self, policy: &str) -> ServiceFuture<'_, TrustSnapshot>"),
        "Trust Centre must keep an explicit typed service contract"
    );
    assert!(
        ui.contains("let trust_services = services.trust.clone();")
            && ui.contains("let service = services.trust.clone();"),
        "Trust Centre must consume AppServices.trust rather than a generic native command"
    );
    assert!(
        ui.contains("match service.set_policy(&policy).await"),
        "Trust policy mutations must remain routed through TrustService"
    );
}

#[test]
fn trust_centre_keeps_user_visible_async_states() {
    let ui = read_repo_file("crates/hermes-ui/src/lib.rs");

    for required in [
        "Loading Trust Centre",
        "Trust policy updated successfully.",
        "saving.set(true);",
        "error.set(None);",
        "refresh.set(refresh() + 1);",
    ] {
        assert!(
            ui.contains(required),
            "Trust Centre async-state regression: {required}"
        );
    }
}
