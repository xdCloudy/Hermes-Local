use std::{fs, path::Path, process::Command};

use hermes_desktop::NativeApp;
use uuid::Uuid;

fn git(repo: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

fn test_root() -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("hermes-review-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).expect("create review test root");
    root
}

#[tokio::test]
async fn review_status_preserves_stage_state_and_selects_the_matching_diff() {
    let data_dir = test_root();
    let repo = data_dir.join("repo");
    fs::create_dir_all(&repo).expect("create repo");
    git(&repo, &["init"]);
    git(&repo, &["config", "user.email", "review@example.test"]);
    git(&repo, &["config", "user.name", "Review Test"]);
    fs::write(repo.join("note.txt"), "one\n").expect("seed file");
    git(&repo, &["add", "--", "note.txt"]);
    git(&repo, &["commit", "-m", "seed"]);

    fs::write(repo.join("note.txt"), "one\ntwo\n").expect("modify file");
    let app = NativeApp::new(data_dir.clone());

    let working = app
        .services
        .git
        .status(&repo)
        .await
        .expect("working status");
    let change = working
        .entries
        .iter()
        .find(|entry| entry.path == "note.txt")
        .expect("changed file");
    assert!(!change.staged);
    assert!(change.unstaged);
    assert!(
        app.services
            .git
            .diff(&repo, Path::new("note.txt"))
            .await
            .expect("working diff")
            .contains("+two")
    );

    app.services
        .git
        .stage(&repo, Path::new("note.txt"))
        .await
        .expect("stage file");
    let staged = app.services.git.status(&repo).await.expect("staged status");
    let change = staged
        .entries
        .iter()
        .find(|entry| entry.path == "note.txt")
        .expect("staged file");
    assert!(change.staged);
    assert!(!change.unstaged);
    assert!(
        app.services
            .git
            .diff_staged(&repo, Path::new("note.txt"))
            .await
            .expect("staged diff")
            .contains("+two")
    );

    app.services
        .git
        .unstage(&repo, Path::new("note.txt"))
        .await
        .expect("unstage file");
    let unstaged = app
        .services
        .git
        .status(&repo)
        .await
        .expect("unstaged status");
    let change = unstaged
        .entries
        .iter()
        .find(|entry| entry.path == "note.txt")
        .expect("unstaged file");
    assert!(!change.staged);
    assert!(change.unstaged);

    fs::remove_dir_all(data_dir).expect("remove review test root");
}

#[tokio::test]
async fn review_git_paths_stay_relative_to_the_repository() {
    let data_dir = test_root();
    let repo = data_dir.join("repo");
    fs::create_dir_all(&repo).expect("create repo");
    git(&repo, &["init"]);
    let app = NativeApp::new(data_dir.clone());

    assert!(
        app.services
            .git
            .diff_staged(&repo, Path::new("../outside.txt"))
            .await
            .is_err()
    );
    assert!(
        app.services
            .git
            .stage(&repo, Path::new("../outside.txt"))
            .await
            .is_err()
    );
    assert!(
        app.services
            .git
            .unstage(&repo, Path::new("../outside.txt"))
            .await
            .is_err()
    );

    fs::remove_dir_all(data_dir).expect("remove review test root");
}
