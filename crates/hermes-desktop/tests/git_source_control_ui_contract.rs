#[test]
fn branch_and_worktree_surfaces_use_typed_services_only() {
    let ui = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../hermes-ui/src/source_control.rs"
    ));
    let main = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../apps/desktop/src/main.rs"
    ));
    let branches = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../apps/desktop/src/git_branch_service.rs"
    ));
    let worktrees = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../apps/desktop/src/git_worktree_service.rs"
    ));

    assert!(ui.contains("services.git_branches.clone()"));
    assert!(ui.contains("services.git_worktrees.clone()"));
    assert!(ui.contains("service.switch(Path::new(&repo), &branch).await"));
    assert!(ui.contains("service.add_existing(Path::new(&repo), &branch).await"));
    assert!(ui.contains("service.add_new(Path::new(&repo)"));
    assert!(ui.contains("service.remove(Path::new(&repo), Path::new(&target_path), force).await"));
    assert!(ui.contains("Remove worktree?"));
    assert!(ui.contains("Force removal of a dirty/locked worktree"));
    assert!(!ui.contains("Command::new"));
    assert!(!ui.contains("std::process"));
    assert!(main.contains("git_branch_service::install(&mut native.services)"));
    assert!(main.contains("git_worktree_service::install(&mut native.services)"));
    assert!(branches.contains("services.git_branches = Arc::new(GitBranchService)"));
    assert!(worktrees.contains("services.git_worktrees = Arc::new(GitWorktreeService)"));
    assert!(branches.contains("tokio::task::spawn_blocking"));
    assert!(worktrees.contains("tokio::task::spawn_blocking"));
}
