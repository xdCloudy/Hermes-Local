use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::Arc,
};

use hermes_core::{
    AppServices, GitBranchInfo, GitBranchService as GitBranchServiceContract, ServiceError,
    ServiceFuture,
};

const MAX_GIT_OUTPUT_BYTES: usize = 8 * 1024 * 1024;
const TRUNK_BRANCHES: [&str; 2] = ["main", "master"];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitBranch {
    pub name: String,
    pub checked_out: bool,
    pub is_default: bool,
    pub worktree_path: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct GitBranchService;

pub fn install(services: &mut AppServices) {
    services.git_branches = Arc::new(GitBranchService);
}

impl GitBranchServiceContract for GitBranchService {
    fn list(&self, repository: &Path) -> ServiceFuture<'_, Vec<GitBranchInfo>> {
        let repository = repository.to_owned();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || GitBranchService.list(&repository))
                .await
                .map_err(join_error)?
                .map(|branches| {
                    branches
                        .into_iter()
                        .map(|branch| GitBranchInfo {
                            name: branch.name,
                            checked_out: branch.checked_out,
                            is_default: branch.is_default,
                            worktree_path: branch
                                .worktree_path
                                .map(|path| path.to_string_lossy().into_owned()),
                        })
                        .collect()
                })
                .map_err(service_error)
        })
    }

    fn switch(&self, repository: &Path, branch: &str) -> ServiceFuture<'_, String> {
        let repository = repository.to_owned();
        let branch = branch.to_owned();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || GitBranchService.switch(&repository, &branch))
                .await
                .map_err(join_error)?
                .map_err(service_error)
        })
    }
}

fn join_error(error: tokio::task::JoinError) -> ServiceError {
    ServiceError::Platform(format!("Git branch worker failed: {error}"))
}

fn service_error(error: String) -> ServiceError {
    if error.contains("required")
        || error.contains("must be")
        || error.contains("not a Git worktree")
        || error.contains("invalid")
        || error.contains("already checked out")
    {
        ServiceError::InvalidInput(error)
    } else {
        ServiceError::Platform(error)
    }
}

impl GitBranchService {
    /// List local branches in Git's most-recently-committed order, enriching
    /// each branch with real worktree checkout state and the detected trunk.
    pub fn list(&self, repository: &Path) -> Result<Vec<GitBranch>, String> {
        let repository = canonical_repository(repository)?;
        let output = run_git(
            &repository,
            &[
                "for-each-ref",
                "--format=%(refname:short)",
                "--sort=-committerdate",
                "refs/heads",
            ],
        )?;
        let worktrees =
            worktree_paths(&run_git(&repository, &["worktree", "list", "--porcelain"])?);
        let default = default_branch(&repository);

        Ok(output
            .lines()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(|name| GitBranch {
                name: name.to_owned(),
                checked_out: worktrees.contains_key(name),
                is_default: default.as_deref() == Some(name),
                worktree_path: worktrees.get(name).cloned(),
            })
            .collect())
    }

    /// Switch the selected checkout to a local branch. Ref validation is run
    /// before `git switch`, so option-like/invalid refs never reach the mutating
    /// Git operation even if a future UI bypasses its own validation.
    pub fn switch(&self, repository: &Path, branch: &str) -> Result<String, String> {
        let repository = canonical_repository(repository)?;
        let target = sanitize_branch(branch);
        if target.is_empty() {
            return Err("Branch name is required.".to_owned());
        }
        run_git(&repository, &["check-ref-format", "--branch", &target])?;
        run_git(&repository, &["switch", &target])?;
        Ok(target)
    }
}

fn canonical_repository(repository: &Path) -> Result<PathBuf, String> {
    if !repository.is_absolute() {
        return Err("Git repository path must be absolute.".to_owned());
    }
    let repository = repository
        .canonicalize()
        .map_err(|error| format!("Could not resolve Git repository: {error}"))?;
    if !repository.is_dir() {
        return Err("Git repository path must be a directory.".to_owned());
    }
    let inside = run_git(&repository, &["rev-parse", "--is-inside-work-tree"])?;
    if inside.trim() != "true" {
        return Err("Selected path is not a Git worktree.".to_owned());
    }
    Ok(repository)
}

fn run_git(repository: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(args)
        .output()
        .map_err(|error| format!("Could not start Git: {error}"))?;
    bounded_git_output(output)
}

fn bounded_git_output(output: Output) -> Result<String, String> {
    if output.stdout.len() > MAX_GIT_OUTPUT_BYTES || output.stderr.len() > MAX_GIT_OUTPUT_BYTES {
        return Err("Git returned an oversized response.".to_owned());
    }
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr)
            .trim()
            .chars()
            .take(2_048)
            .collect::<String>();
        return Err(if detail.is_empty() {
            "Git operation failed.".to_owned()
        } else {
            detail
        });
    }
    String::from_utf8(output.stdout).map_err(|_| "Git output was not valid UTF-8.".to_owned())
}

fn default_branch(repository: &Path) -> Option<String> {
    let remote = run_git(
        repository,
        &[
            "symbolic-ref",
            "--quiet",
            "--short",
            "refs/remotes/origin/HEAD",
        ],
    )
    .ok()
    .map(|value| value.trim().trim_start_matches("origin/").to_owned())
    .filter(|value| !value.is_empty());
    if remote.is_some() {
        return remote;
    }

    let configured = run_git(repository, &["config", "--get", "init.defaultBranch"])
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    if configured.is_some() {
        return configured;
    }

    TRUNK_BRANCHES.into_iter().find_map(|branch| {
        run_git(
            repository,
            &["show-ref", "--verify", &format!("refs/heads/{branch}")],
        )
        .ok()
        .map(|_| branch.to_owned())
    })
}

fn worktree_paths(output: &str) -> BTreeMap<String, PathBuf> {
    let mut result = BTreeMap::new();
    let mut current_path: Option<PathBuf> = None;
    let mut current_branch: Option<String> = None;

    let flush = |path: &mut Option<PathBuf>,
                 branch: &mut Option<String>,
                 result: &mut BTreeMap<String, PathBuf>| {
        if let (Some(path), Some(branch)) = (path.take(), branch.take()) {
            result.entry(branch).or_insert(path);
        } else {
            *path = None;
            *branch = None;
        }
    };

    for line in output.lines() {
        if line.is_empty() {
            flush(&mut current_path, &mut current_branch, &mut result);
        } else if let Some(path) = line.strip_prefix("worktree ") {
            if current_path.is_some() || current_branch.is_some() {
                flush(&mut current_path, &mut current_branch, &mut result);
            }
            current_path = Some(PathBuf::from(path.trim()));
        } else if let Some(branch) = line.strip_prefix("branch refs/heads/") {
            current_branch = Some(branch.trim().to_owned());
        }
    }
    flush(&mut current_path, &mut current_branch, &mut result);
    result
}

/// Mirrors the OG Desktop `sanitizeBranch`: whitespace becomes '-', forbidden
/// ref characters are removed, repeated separators are collapsed and unsafe
/// edge punctuation is trimmed. Git still performs authoritative validation.
fn sanitize_branch(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut pending_space = false;
    for character in value.chars() {
        if character.is_whitespace() {
            pending_space = true;
            continue;
        }
        if pending_space && !out.is_empty() {
            out.push('-');
        }
        pending_space = false;
        if character.is_ascii_alphanumeric()
            || character == '_'
            || matches!(character, '.' | '/' | '-')
        {
            out.push(character);
        }
    }

    while out.contains("--") {
        out = out.replace("--", "-");
    }
    while out.contains("//") {
        out = out.replace("//", "/");
    }
    while out.contains("..") {
        out = out.replace("..", ".");
    }
    out.trim_matches(|character| matches!(character, '-' | '.' | '/'))
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn parses_worktree_porcelain_into_branch_paths() {
        let output = "worktree C:/repo\nHEAD abc\nbranch refs/heads/main\n\nworktree C:/repo/.worktrees/feature\nHEAD def\nbranch refs/heads/feature/test\n\nworktree C:/repo/.worktrees/detached\nHEAD 123\ndetached\n";
        let paths = worktree_paths(output);
        assert_eq!(paths.get("main"), Some(&PathBuf::from("C:/repo")));
        assert_eq!(
            paths.get("feature/test"),
            Some(&PathBuf::from("C:/repo/.worktrees/feature"))
        );
        assert!(!paths.contains_key("detached"));
    }

    #[test]
    fn sanitizes_branch_like_the_og_desktop() {
        assert_eq!(sanitize_branch("  Hermes feature!! "), "Hermes-feature");
        assert_eq!(sanitize_branch("../bad//name..."), "bad/name");
        assert_eq!(sanitize_branch("feature/@{evil}"), "feature/evil");
        assert_eq!(sanitize_branch("***"), "");
    }

    #[test]
    fn real_git_list_and_switch_preserve_worktree_truth() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("hermes-git-branches-{unique}"));
        let worktree = std::env::temp_dir().join(format!("hermes-git-branches-wt-{unique}"));
        fs::create_dir_all(&root).unwrap();

        let git = |args: &[&str]| {
            let output = Command::new("git")
                .arg("-C")
                .arg(&root)
                .args(args)
                .output()
                .expect("Git must be available in CI");
            assert!(
                output.status.success(),
                "git {:?}: {}",
                args,
                String::from_utf8_lossy(&output.stderr)
            );
        };
        git(&["init", "--initial-branch=main"]);
        git(&["config", "user.email", "hermes@localhost"]);
        git(&["config", "user.name", "Hermes"]);
        git(&["config", "init.defaultBranch", "main"]);
        fs::write(root.join("README.md"), "fixture\n").unwrap();
        git(&["add", "README.md"]);
        git(&["commit", "-m", "initial"]);
        git(&["branch", "feature"]);
        let worktree_string = worktree.to_string_lossy().into_owned();
        git(&["worktree", "add", &worktree_string, "feature"]);
        git(&["branch", "other"]);

        let service = GitBranchService;
        let branches = service.list(&root).expect("branch list");
        let main = branches
            .iter()
            .find(|branch| branch.name == "main")
            .unwrap();
        let feature = branches
            .iter()
            .find(|branch| branch.name == "feature")
            .unwrap();
        assert!(main.checked_out);
        assert!(main.is_default);
        assert!(feature.checked_out);
        assert_eq!(
            feature
                .worktree_path
                .as_deref()
                .expect("feature worktree")
                .canonicalize()
                .expect("canonical feature worktree"),
            worktree
                .canonicalize()
                .expect("canonical expected worktree")
        );

        assert_eq!(service.switch(&root, "other").unwrap(), "other");
        let head = run_git(&root, &["branch", "--show-current"]).unwrap();
        assert_eq!(head.trim(), "other");

        let _ = Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(["worktree", "remove", "--force", &worktree_string])
            .output();
        let _ = fs::remove_dir_all(&worktree);
        let _ = fs::remove_dir_all(&root);
    }
}
