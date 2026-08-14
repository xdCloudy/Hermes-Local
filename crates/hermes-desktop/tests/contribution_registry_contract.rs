use std::{fs, path::PathBuf};

fn workspace_file(relative: &str) -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    fs::read_to_string(root.join(relative))
        .expect("workspace source")
        .replace("\r\n", "\n")
}

#[test]
fn built_in_registry_composes_shared_and_desktop_surfaces() {
    let core = workspace_file("crates/hermes-core/src/contributions.rs");
    let ui = workspace_file("crates/hermes-ui/src/lib.rs");
    let shell = workspace_file("apps/desktop/src/shell_interaction.rs");

    assert!(core.contains("pub struct ContributionRegistry"));
    assert!(core.contains("pub fn extend_source"));
    assert!(core.contains("duplicate contribution id"));
    assert!(core.contains("references an unknown route"));
    assert!(ui.contains("ContributionArea::PrimaryNavigation"));
    assert!(ui.contains("ContributionArea::SecondaryNavigation"));
    assert!(ui.contains("ContributionArea::Launcher"));
    assert!(ui.contains("for item in primary_navigation"));
    assert!(!ui.contains("NavItem { to: Route::Overview"));
    assert!(shell.contains("commands_from_registry"));
    assert!(shell.contains("ContributionArea::Pane"));
    assert!(shell.contains("ContributionArea::Status"));
    assert!(!shell.contains("const COMMANDS"));
}
