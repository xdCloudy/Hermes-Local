use std::{fs, path::PathBuf};

use hermes_desktop::NativeApp;
use uuid::Uuid;

fn test_root() -> PathBuf {
    let root = std::env::temp_dir().join(format!("hermes-local-file-actions-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).expect("create disposable file-action root");
    root
}

#[tokio::test]
async fn rename_keeps_the_item_inside_its_parent_and_returns_relative_path() {
    let data_dir = test_root();
    let project = data_dir.join("project");
    fs::create_dir_all(project.join("src")).expect("create project tree");
    fs::write(project.join("src/old.txt"), b"hello").expect("seed source file");
    let app = NativeApp::new(data_dir.clone());

    let renamed = app
        .services
        .files
        .rename(&project, "src/old.txt".as_ref(), "new.txt")
        .await
        .expect("rename should succeed");

    assert_eq!(renamed, "src/new.txt");
    assert!(!project.join("src/old.txt").exists());
    assert_eq!(
        fs::read(project.join("src/new.txt")).expect("read renamed file"),
        b"hello"
    );

    fs::remove_dir_all(data_dir).expect("remove disposable root");
}

#[tokio::test]
async fn rename_rejects_traversal_separators_and_collisions() {
    let data_dir = test_root();
    let project = data_dir.join("project");
    fs::create_dir_all(project.join("src")).expect("create project tree");
    fs::write(project.join("src/old.txt"), b"old").expect("seed source file");
    fs::write(project.join("src/existing.txt"), b"existing").expect("seed collision file");
    let app = NativeApp::new(data_dir.clone());

    for invalid in ["", ".", "..", "../escape.txt", "nested/name.txt", "nested\\name.txt"] {
        let error = app
            .services
            .files
            .rename(&project, "src/old.txt".as_ref(), invalid)
            .await
            .expect_err("unsafe rename must be rejected");
        assert!(
            error.to_string().contains("Invalid rename"),
            "unexpected error for {invalid:?}: {error}"
        );
    }

    let collision = app
        .services
        .files
        .rename(&project, "src/old.txt".as_ref(), "existing.txt")
        .await
        .expect_err("rename collision must be rejected");
    assert!(collision.to_string().contains("already exists"));
    assert_eq!(fs::read(project.join("src/old.txt")).unwrap(), b"old");
    assert_eq!(
        fs::read(project.join("src/existing.txt")).unwrap(),
        b"existing"
    );

    fs::remove_dir_all(data_dir).expect("remove disposable root");
}

#[tokio::test]
async fn file_actions_reject_relative_paths_that_escape_the_project_root() {
    let data_dir = test_root();
    let project = data_dir.join("project");
    fs::create_dir_all(&project).expect("create project root");
    fs::write(data_dir.join("outside.txt"), b"outside").expect("seed outside file");
    let app = NativeApp::new(data_dir.clone());

    let error = app
        .services
        .files
        .rename(&project, "../outside.txt".as_ref(), "renamed.txt")
        .await
        .expect_err("source traversal must be rejected");

    assert!(!data_dir.join("renamed.txt").exists());
    assert_eq!(fs::read(data_dir.join("outside.txt")).unwrap(), b"outside");
    assert!(
        error.to_string().to_lowercase().contains("path")
            || error.to_string().to_lowercase().contains("relative")
    );

    fs::remove_dir_all(data_dir).expect("remove disposable root");
}
