#![allow(dead_code)] // GT-03 service foundation; Dioxus worktree UI is a later stage.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

const MAX_GIT_OUTPUT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitWorktree {
    pub path: PathBuf,
    pub branch: Option<String>,
    pub is_main: bool,
    pub detached: bool,
    pub locked: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct GitWorktreeService;

impl GitWorktreeService {
    pub fn list(&self, repository: &Path) -> Result<Vec<GitWorktree>, String> {
        let repository = canonical_repository(repository)?;
        parse_worktrees(&run_git(
            &repository,
            &["worktree", "list", "--porcelain"],
        )?)
    }

    /// Create a new local branch in a generated `.worktrees/<slug>` checkout.
    /// `base` may be a local or remote-tracking ref and is validated by Git.
    pub fn add_new(
        &self,
        repository: &Path,
        display_name: &str,
        branch: &str,
        base: Option<&str>,
    ) -> Result<GitWorktree, String> {
        let repository = canonical_repository(repository)?;
        let trees = self.list(&repository)?;
        let main = main_worktree(&trees)?;
        let branch = checked_branch_name(&repository, branch)?;
        let slug = slugify(display_name);
        let path = unique_worktree_path(&main.path.join(".worktrees").join(slug));
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Could not create worktree parent directory: {error}"))?;
        }

        let mut command = git_command(&repository);
        command.args(["worktree", "add", "-b", &branch]);
        if base.is_some_and(|value| value.starts_with("origin/")) {
            command.arg("--no-track");
        }
        command.arg(&path);
        if let Some(base) = base.map(str::trim).filter(|value| !value.is_empty()) {
            validate_refish(base)?;
            command.arg(base);
        }
        checked_output(command, "Git worktree add")?;
        self.find_registered(&repository, &path)
    }

    /// Check out an existing local branch into a generated linked worktree.
    pub fn add_existing(
        &self,
        repository: &Path,
        branch: &str,
    ) -> Result<GitWorktree, String> {
        let repository = canonical_repository(repository)?;
        let trees = self.list(&repository)?;
        let main = main_worktree(&trees)?;
        let branch = checked_branch_name(&repository, branch)?;
        if trees
            .iter()
            .any(|tree| tree.branch.as_deref() == Some(branch.as_str()))
        {
            return Err(format!("Branch '{branch}' is already checked out in a worktree."));
        }
        let path = unique_worktree_path(&main.path.join(".worktrees").join(slugify(&branch)));
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Could not create worktree parent directory: {error}"))?;
        }
        let mut command = git_command(&repository);
        command.args(["worktree", "add"]);
        command.arg(&path).arg(&branch);
        checked_output(command, "Git worktree add")?;
        self.find_registered(&repository, &path)
    }

    /// Remove a linked worktree. The main checkout and arbitrary filesystem
    /// paths are never accepted; the path must match Git's registered worktree
    /// list and remain under the main checkout's `.worktrees` directory.
    pub fn remove(
        &self,
        repository: &Path,
        worktree_path: &Path,
        force: bool,
    ) -> Result<PathBuf, String> {
        let repository = canonical_repository(repository)?;
        let trees = self.list(&repository)?;
        let main = main_worktree(&trees)?;
        let requested = lexical_absolute(worktree_path)?;
        let registered = trees
            .iter()
            .find(|tree| lexical_absolute(&tree.path).ok().as_ref() == Some(&requested))
            .ok_or_else(|| "Worktree path is not registered with this repository.".to_owned())?;
        if registered.is_main {
            return Err("The main Git worktree cannot be removed.".to_owned());
        }
        let allowed_root = lexical_absolute(&main.path.join(".worktrees"))?;
        if !requested.starts_with(&allowed_root) {
            return Err("Hermes Local only removes managed .worktrees checkouts.".to_owned());
        }

        let mut command = git_command(&repository);
        command.args(["worktree", "remove"]);
        if force {
            command.arg("--force");
        }
        command.arg(&registered.path);
        checked_output(command, "Git worktree remove")?;
        Ok(registered.path.clone())
    }

    fn find_registered(&self, repository: &Path, path: &Path) -> Result<GitWorktree, String> {
        let expected = lexical_absolute(path)?;
        self.list(repository)?
            .into_iter()
            .find(|tree| lexical_absolute(&tree.path).ok().as_ref() == Some(&expected))
            .ok_or_else(|| "Git did not register the created worktree.".to_owned())
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
    if run_git(&repository, &["rev-parse", "--is-inside-work-tree"])?
        .trim()
        != "true"
    {
        return Err("Selected path is not a Git worktree.".to_owned());
    }
    Ok(repository)
}

fn git_command(repository: &Path) -> Command {
    let mut command = Command::new("git");
    command.arg("-C").arg(repository);
    command
}

fn run_git(repository: &Path, args: &[&str]) -> Result<String, String> {
    let mut command = git_command(repository);
    command.args(args);
    let output = checked_output(command, "Git")?;
    String::from_utf8(output.stdout).map_err(|_| "Git output was not valid UTF-8.".to_owned())
}

fn checked_output(mut command: Command, label: &str) -> Result<Output, String> {
    let output = command
        .output()
        .map_err(|error| format!("Could not start {label}: {error}"))?;
    if output.stdout.len() > MAX_GIT_OUTPUT_BYTES || output.stderr.len() > MAX_GIT_OUTPUT_BYTES {
        return Err(format!("{label} returned an oversized response."));
    }
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr)
            .trim()
            .chars()
            .take(2_048)
            .collect::<String>();
        return Err(if detail.is_empty() {
            format!("{label} failed.")
        } else {
            detail
        });
    }
    Ok(output)
}

fn parse_worktrees(output: &str) -> Result<Vec<GitWorktree>, String> {
    let mut records = Vec::new();
    let mut path: Option<PathBuf> = None;
    let mut branch: Option<String> = None;
    let mut detached = false;
    let mut locked = false;

    let flush = |path: &mut Option<PathBuf>,
                 branch: &mut Option<String>,
                 detached: &mut bool,
                 locked: &mut bool,
                 records: &mut Vec<GitWorktree>| {
        if let Some(path) = path.take() {
            records.push(GitWorktree {
                path,
                branch: branch.take(),
                is_main: records.is_empty(),
                detached: *detached,
                locked: *locked,
            });
        }
        *branch = None;
        *detached = false;
        *locked = false;
    };

    for line in output.lines() {
        if line.is_empty() {
            flush(
                &mut path,
                &mut branch,
                &mut detached,
                &mut locked,
                &mut records,
            );
        } else if let Some(value) = line.strip_prefix("worktree ") {
            if path.is_some() {
                flush(
                    &mut path,
                    &mut branch,
                    &mut detached,
                    &mut locked,
                    &mut records,
                );
            }
            path = Some(PathBuf::from(value.trim()));
        } else if let Some(value) = line.strip_prefix("branch refs/heads/") {
            branch = Some(value.trim().to_owned());
        } else if line == "detached" {
            detached = true;
        } else if line.starts_with("locked") {
            locked = true;
        }
    }
    flush(
        &mut path,
        &mut branch,
        &mut detached,
        &mut locked,
        &mut records,
    );

    if records.is_empty() && !output.trim().is_empty() {
        return Err("Git returned an invalid worktree list.".to_owned());
    }
    Ok(records)
}

fn main_worktree(trees: &[GitWorktree]) -> Result<&GitWorktree, String> {
    trees
        .iter()
        .find(|tree| tree.is_main)
        .ok_or_else(|| "Git did not report a main worktree.".to_owned())
}

fn checked_branch_name(repository: &Path, branch: &str) -> Result<String, String> {
    let branch = sanitize_branch(branch);
    if branch.is_empty() {
        return Err("Branch name is required.".to_owned());
    }
    run_git(repository, &["check-ref-format", "--branch", &branch])?;
    Ok(branch)
}

fn validate_refish(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 1_024
        || value.starts_with('-')
        || value.chars().any(char::is_control)
        || value.contains('\0')
    {
        return Err("Invalid Git base reference.".to_owned());
    }
    Ok(())
}

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

fn slugify(value: &str) -> String {
    let mut slug = String::new();
    let mut separator = false;
    for character in value.trim().to_ascii_lowercase().chars() {
        if character.is_ascii_alphanumeric() {
            if separator && !slug.is_empty() {
                slug.push('-');
            }
            separator = false;
            if slug.len() < 40 {
                slug.push(character);
            }
        } else {
            separator = true;
        }
        if slug.len() >= 40 {
            break;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        "work".to_owned()
    } else {
        slug
    }
}

fn unique_worktree_path(base: &Path) -> PathBuf {
    if !base.exists() {
        return base.to_path_buf();
    }
    for index in 2..10_000 {
        let candidate = PathBuf::from(format!("{}-{index}", base.display()));
        if !candidate.exists() {
            return candidate;
        }
    }
    base.with_extension("overflow")
}

fn lexical_absolute(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err("Worktree path must be absolute.".to_owned());
    }
    Ok(path.components().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn parses_porcelain_main_linked_detached_and_locked_records() {
        let output = "worktree C:/repo\nHEAD aaa\nbranch refs/heads/main\n\nworktree C:/repo/.worktrees/feature\nHEAD bbb\nbranch refs/heads/feature/test\nlocked reason\n\nworktree C:/repo/.worktrees/detached\nHEAD ccc\ndetached\n";
        let trees = parse_worktrees(output).unwrap();
        assert_eq!(trees.len(), 3);
        assert!(trees[0].is_main);
        assert_eq!(trees[0].branch.as_deref(), Some("main"));
        assert!(!trees[1].is_main);
        assert!(trees[1].locked);
        assert_eq!(trees[1].branch.as_deref(), Some("feature/test"));
        assert!(trees[2].detached);
        assert_eq!(trees[2].branch, None);
    }

    #[test]
    fn slug_and_branch_sanitizers_match_managed_path_expectations() {
        assert_eq!(slugify(" Feature: My Thing "), "feature-my-thing");
        assert_eq!(slugify("***"), "work");
        assert_eq!(sanitize_branch(" Hermes feature!! "), "Hermes-feature");
        assert_eq!(sanitize_branch("../bad//name..."), "bad/name");
    }

    #[test]
    fn real_git_add_list_and_remove_round_trip() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("hermes-git-worktree-{unique}"));
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
        fs::write(root.join("README.md"), "fixture\n").unwrap();
        git(&["add", "README.md"]);
        git(&["commit", "-m", "initial"]);

        let service = GitWorktreeService;
        let created = service
            .add_new(&root, "Feature One", "feature/one", Some("main"))
            .expect("create worktree");
        assert!(!created.is_main);
        assert_eq!(created.branch.as_deref(), Some("feature/one"));
        assert!(created.path.starts_with(root.join(".worktrees")));
        assert!(created.path.is_dir());

        let listed = service.list(&root).expect("list worktrees");
        assert_eq!(listed.len(), 2);
        assert!(listed.iter().any(|tree| tree.branch.as_deref() == Some("feature/one")));

        let removed = service
            .remove(&root, &created.path, false)
            .expect("remove worktree");
        assert_eq!(removed, created.path);
        assert!(!removed.exists());
        assert_eq!(service.list(&root).unwrap().len(), 1);

        assert!(service.remove(&root, &root, true).is_err());
        let _ = fs::remove_dir_all(root);
    }
}
