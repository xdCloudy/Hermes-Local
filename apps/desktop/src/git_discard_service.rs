#![allow(dead_code)] // GT-05 service foundation; review confirmation/UI is a later stage.

use std::{
    fs,
    path::{Component, Path, PathBuf},
    process::{Command, Output},
};

const MAX_GIT_OUTPUT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default)]
pub struct GitDiscardService;

impl GitDiscardService {
    /// Restore one exact repository-relative path to `HEAD` and remove
    /// untracked files inside that scope. Ignored files are deliberately kept.
    pub fn discard_path(&self, repository: &Path, relative: &Path) -> Result<(), String> {
        let repository = canonical_repository_root(repository)?;
        ensure_head(&repository)?;
        let pathspec = literal_pathspec(relative)?;

        // Reset first so staged additions become ordinary untracked files and
        // staged edits/deletions return to the HEAD index state.
        run_git(
            &repository,
            &["reset", "--quiet", "HEAD", "--", pathspec.as_str()],
        )?;

        // Restore the worktree only when HEAD/index contains something in the
        // requested scope. `git restore` otherwise treats a purely untracked
        // path as an error, while discard semantics still need to clean it.
        let tracked = run_git(
            &repository,
            &["ls-files", "--with-tree=HEAD", "--", pathspec.as_str()],
        )?;
        if !tracked.trim().is_empty() {
            run_git(
                &repository,
                &[
                    "restore",
                    "--source=HEAD",
                    "--worktree",
                    "--",
                    pathspec.as_str(),
                ],
            )?;
        }

        // Match the OG `git clean -fd -- <path>` behavior. Do not use `-x` or
        // `-X`: ignored files are user data/config and must survive discard.
        run_git(
            &repository,
            &["clean", "-fd", "--", pathspec.as_str()],
        )?;

        let remaining = run_git(
            &repository,
            &[
                "status",
                "--porcelain=v1",
                "--untracked-files=all",
                "--",
                pathspec.as_str(),
            ],
        )?;
        if !remaining.trim().is_empty() {
            return Err(format!(
                "Git discard did not leave the requested path clean: {}",
                bounded_detail(&remaining)
            ));
        }
        Ok(())
    }

    /// Restore the entire tracked tree/index to `HEAD` and remove all
    /// non-ignored untracked files/directories. Ignored files are preserved.
    pub fn discard_all(&self, repository: &Path) -> Result<(), String> {
        let repository = canonical_repository_root(repository)?;
        ensure_head(&repository)?;
        run_git(&repository, &["reset", "--hard", "HEAD"])?;
        run_git(&repository, &["clean", "-fd"])?;

        let remaining = run_git(
            &repository,
            &["status", "--porcelain=v1", "--untracked-files=all"],
        )?;
        if !remaining.trim().is_empty() {
            return Err(format!(
                "Git discard-all did not leave the repository clean: {}",
                bounded_detail(&remaining)
            ));
        }
        Ok(())
    }
}

fn literal_pathspec(relative: &Path) -> Result<String, String> {
    if relative.as_os_str().is_empty() || relative.is_absolute() {
        return Err("Git discard path must be a non-empty relative path.".to_owned());
    }

    let mut segments = Vec::new();
    for component in relative.components() {
        match component {
            Component::Normal(value) => {
                let value = value
                    .to_str()
                    .ok_or_else(|| "Git discard path must be valid UTF-8.".to_owned())?;
                if value.eq_ignore_ascii_case(".git") && segments.is_empty() {
                    return Err("Git metadata cannot be discarded as a worktree path.".to_owned());
                }
                if value.contains('\0') {
                    return Err("Git discard path contains a NUL character.".to_owned());
                }
                segments.push(value.to_owned());
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err("Git discard path cannot escape the repository root.".to_owned());
            }
        }
    }
    if segments.is_empty() {
        return Err("Use discard_all for the whole repository.".to_owned());
    }

    // Git pathspec magic is still interpreted after `--`. Prefixing the value
    // with `:(literal)` prevents a filename from expanding into a broader scope.
    Ok(format!(":(literal){}", segments.join("/")))
}

fn canonical_repository_root(repository: &Path) -> Result<PathBuf, String> {
    if !repository.is_absolute() {
        return Err("Git repository path must be absolute.".to_owned());
    }
    let requested = repository
        .canonicalize()
        .map_err(|error| format!("Could not resolve Git repository: {error}"))?;
    if !requested.is_dir() {
        return Err("Git repository path must be a directory.".to_owned());
    }

    let top_level = run_git(&requested, &["rev-parse", "--show-toplevel"])?;
    let top_level = PathBuf::from(top_level.trim())
        .canonicalize()
        .map_err(|error| format!("Could not resolve Git repository root: {error}"))?;
    if requested != top_level {
        return Err("Git discard requires the repository root, not a nested directory.".to_owned());
    }
    Ok(top_level)
}

fn ensure_head(repository: &Path) -> Result<(), String> {
    run_git(repository, &["rev-parse", "--verify", "HEAD"])
        .map(|_| ())
        .map_err(|_| "Git discard requires a repository with at least one commit.".to_owned())
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
        let detail = bounded_detail(&String::from_utf8_lossy(&output.stderr));
        return Err(if detail.is_empty() {
            format!("{label} failed.")
        } else {
            detail
        });
    }
    Ok(output)
}

fn bounded_detail(detail: &str) -> String {
    detail.trim().chars().take(2_048).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn literal_pathspec_blocks_escape_metadata_and_pathspec_magic() {
        assert!(literal_pathspec(Path::new("../outside.txt")).is_err());
        assert!(literal_pathspec(Path::new(".")).is_err());
        assert!(literal_pathspec(Path::new(".git/config")).is_err());
        assert_eq!(
            literal_pathspec(Path::new("review/[abc].txt")).expect("literal pathspec"),
            ":(literal)review/[abc].txt"
        );
        assert_eq!(
            literal_pathspec(Path::new("review/:(glob)name.txt")).expect("literal pathspec"),
            ":(literal)review/:(glob)name.txt"
        );
    }

    #[test]
    fn discard_path_restores_staged_and_unstaged_changes_without_touching_other_scope() {
        let repository = test_repository("discard-path");
        fs::create_dir_all(repository.join("scope")).expect("scope directory");
        fs::write(repository.join("scope/tracked.txt"), "original\n").expect("tracked file");
        fs::write(repository.join("other.txt"), "other-original\n").expect("other file");
        fs::write(
            repository.join(".gitignore"),
            "scope/ignored.log\nignored-root.log\n",
        )
        .expect("gitignore");
        git_ok(&repository, &["add", "."]);
        git_ok(&repository, &["commit", "-m", "fixture"]);

        fs::write(repository.join("scope/tracked.txt"), "staged\n").expect("staged edit");
        git_ok(&repository, &["add", "scope/tracked.txt"]);
        fs::write(repository.join("scope/tracked.txt"), "unstaged-after-stage\n")
            .expect("unstaged edit");
        fs::write(repository.join("scope/untracked.txt"), "delete-me\n").expect("untracked");
        fs::write(repository.join("scope/ignored.log"), "preserve-me\n").expect("ignored");
        fs::write(repository.join("other.txt"), "must-remain-dirty\n").expect("other edit");

        GitDiscardService
            .discard_path(&repository, Path::new("scope"))
            .expect("discard scope");

        assert_eq!(
            fs::read_to_string(repository.join("scope/tracked.txt")).expect("tracked contents"),
            "original\n"
        );
        assert!(!repository.join("scope/untracked.txt").exists());
        assert_eq!(
            fs::read_to_string(repository.join("scope/ignored.log")).expect("ignored contents"),
            "preserve-me\n"
        );
        assert_eq!(
            fs::read_to_string(repository.join("other.txt")).expect("other contents"),
            "must-remain-dirty\n"
        );
        let status = git_text(
            &repository,
            &["status", "--porcelain=v1", "--untracked-files=all"],
        );
        assert!(status.contains("other.txt"));
        assert!(!status.contains("scope/"));
        cleanup(repository);
    }

    #[test]
    fn discard_path_removes_a_staged_addition_and_pure_untracked_file() {
        let repository = test_repository("discard-new");
        fs::write(repository.join("seed.txt"), "seed\n").expect("seed");
        git_ok(&repository, &["add", "seed.txt"]);
        git_ok(&repository, &["commit", "-m", "fixture"]);

        fs::write(repository.join("new-staged.txt"), "staged\n").expect("staged new");
        git_ok(&repository, &["add", "new-staged.txt"]);
        GitDiscardService
            .discard_path(&repository, Path::new("new-staged.txt"))
            .expect("discard staged addition");
        assert!(!repository.join("new-staged.txt").exists());

        fs::write(repository.join("untracked.txt"), "untracked\n").expect("untracked");
        GitDiscardService
            .discard_path(&repository, Path::new("untracked.txt"))
            .expect("discard untracked");
        assert!(!repository.join("untracked.txt").exists());
        assert!(git_text(&repository, &["status", "--porcelain=v1"]).is_empty());
        cleanup(repository);
    }

    #[test]
    fn discard_all_resets_index_and_worktree_but_preserves_ignored_files() {
        let repository = test_repository("discard-all");
        fs::write(repository.join("tracked.txt"), "original\n").expect("tracked");
        fs::write(repository.join(".gitignore"), "ignored.log\n").expect("gitignore");
        git_ok(&repository, &["add", "."]);
        git_ok(&repository, &["commit", "-m", "fixture"]);

        fs::write(repository.join("tracked.txt"), "changed\n").expect("changed");
        git_ok(&repository, &["add", "tracked.txt"]);
        fs::write(repository.join("tracked.txt"), "changed-again\n").expect("changed again");
        fs::write(repository.join("added.txt"), "staged new\n").expect("added");
        git_ok(&repository, &["add", "added.txt"]);
        fs::write(repository.join("untracked.txt"), "untracked\n").expect("untracked");
        fs::write(repository.join("ignored.log"), "keep\n").expect("ignored");

        GitDiscardService
            .discard_all(&repository)
            .expect("discard all");

        assert_eq!(
            fs::read_to_string(repository.join("tracked.txt")).expect("tracked contents"),
            "original\n"
        );
        assert!(!repository.join("added.txt").exists());
        assert!(!repository.join("untracked.txt").exists());
        assert_eq!(
            fs::read_to_string(repository.join("ignored.log")).expect("ignored contents"),
            "keep\n"
        );
        assert!(git_text(&repository, &["status", "--porcelain=v1"]).is_empty());
        cleanup(repository);
    }

    fn test_repository(label: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let repository = std::env::temp_dir().join(format!(
            "hermes-{label}-{}-{suffix}",
            std::process::id()
        ));
        fs::create_dir_all(&repository).expect("repository directory");
        git_ok(&repository, &["init", "--quiet"]);
        git_ok(&repository, &["config", "user.email", "hermes-tests@example.invalid"]);
        git_ok(&repository, &["config", "user.name", "Hermes Tests"]);
        repository
            .canonicalize()
            .expect("canonical test repository")
    }

    fn git_ok(repository: &Path, args: &[&str]) {
        let mut command = git_command(repository);
        command.args(args);
        let output = command.output().expect("run git fixture command");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn git_text(repository: &Path, args: &[&str]) -> String {
        let mut command = git_command(repository);
        command.args(args);
        let output = command.output().expect("run git fixture command");
        assert!(output.status.success(), "git {args:?} failed");
        String::from_utf8(output.stdout).expect("UTF-8 git output")
    }

    fn cleanup(repository: PathBuf) {
        let _ = fs::remove_dir_all(repository);
    }
}
