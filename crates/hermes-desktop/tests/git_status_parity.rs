use std::{fs, path::Path, process::Command};

use hermes_desktop::NativeApp;
use uuid::Uuid;

fn git(repo: &Path, args: &[&str]) -> String {
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
    String::from_utf8(output.stdout).expect("utf8 git output")
}

fn test_root(label: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("hermes-git-status-{label}-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).expect("create status test root");
    root
}

#[tokio::test]
async fn status_handles_unborn_clean_nested_and_detached_repositories() {
    let data_dir = test_root("edges");
    let repo = data_dir.join("repo");
    fs::create_dir_all(&repo).expect("create repo");
    git(&repo, &["init", "--quiet"]);
    git(&repo, &["config", "user.email", "status@example.test"]);
    git(&repo, &["config", "user.name", "Status Test"]);
    let branch = git(&repo, &["symbolic-ref", "--short", "HEAD"])
        .trim()
        .to_owned();

    let app = NativeApp::new(data_dir.clone());
    let unborn = app.services.git.status(&repo).await.expect("unborn status");
    assert_eq!(unborn.branch.as_deref(), Some(branch.as_str()));
    assert!(unborn.changed.is_empty());

    let nested = repo.join("src").join("nested");
    fs::create_dir_all(&nested).expect("nested directory");
    let nested_status = app
        .services
        .git
        .status(&nested)
        .await
        .expect("nested status");
    assert_eq!(nested_status.branch.as_deref(), Some(branch.as_str()));

    fs::write(repo.join("tracked.txt"), "seed\n").expect("seed file");
    git(&repo, &["add", "--", "tracked.txt"]);
    git(&repo, &["commit", "-m", "seed"]);
    let clean = app.services.git.status(&repo).await.expect("clean status");
    assert_eq!(clean.branch.as_deref(), Some(branch.as_str()));
    assert!(clean.changed.is_empty());

    fs::write(repo.join("untracked.txt"), "new\n").expect("untracked file");
    let dirty = app.services.git.status(&repo).await.expect("dirty status");
    assert!(dirty.changed.iter().any(|path| path == "untracked.txt"));
    fs::remove_file(repo.join("untracked.txt")).expect("remove untracked");

    git(&repo, &["checkout", "--detach", "HEAD"]);
    let detached = app
        .services
        .git
        .status(&repo)
        .await
        .expect("detached status");
    assert_eq!(detached.branch, None);
    assert!(detached.changed.is_empty());

    fs::remove_dir_all(data_dir).expect("remove status test root");
}

#[test]
fn dioxus_review_consumes_status_branch_counters_and_clean_state() {
    let review = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../hermes-ui/src/review.rs"
    ));
    assert!(review.contains("service.status(Path::new(&root)).await"));
    assert!(review.contains("current.branch"));
    assert!(review.contains("current.ahead"));
    assert!(review.contains("current.behind"));
    assert!(review.contains("Working tree clean"));
    assert!(review.contains("detached / unborn"));
}
