#![allow(dead_code)] // GT-06 service foundation; Review ship UI is a later stage.

use std::{
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

const MAX_GIT_OUTPUT_BYTES: usize = 8 * 1024 * 1024;
const MAX_GH_OUTPUT_BYTES: usize = 8 * 1024 * 1024;
const MAX_COMMIT_MESSAGE_BYTES: usize = 64 * 1024;
const GH_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PullRequestInfo {
    pub url: String,
    pub state: String,
    pub number: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShipInfo {
    pub gh_ready: bool,
    pub pull_request: Option<PullRequestInfo>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PushOutcome {
    PushedTracked,
    SetUpstream { branch: String },
    NoCurrentBranch,
}

#[derive(Clone, Debug, Default)]
pub struct GitShipService;

impl GitShipService {
    /// Commit the exact staged set. When nothing is staged, mirror the Electron
    /// Review flow by staging all tracked/untracked/deleted changes first.
    pub fn commit(
        &self,
        repository: &Path,
        message: &str,
        push_after_commit: bool,
    ) -> Result<Option<PushOutcome>, String> {
        let repository = canonical_repository(repository)?;
        let message = validate_commit_message(message)?;
        let staged = run_git(
            &repository,
            &["diff", "--cached", "--name-only", "--no-renames"],
        )?;
        if staged.trim().is_empty() {
            run_git(&repository, &["add", "-A"])?;
        }
        run_git(&repository, &["commit", "-m", message])?;

        if push_after_commit {
            self.push(&repository).map(Some)
        } else {
            Ok(None)
        }
    }

    /// Push the current branch. Existing tracking uses plain `git push`; first
    /// push binds the current branch to `origin/<branch>` like the OG Review bar.
    pub fn push(&self, repository: &Path) -> Result<PushOutcome, String> {
        let repository = canonical_repository(repository)?;
        let branch =
            match run_git_optional(&repository, &["symbolic-ref", "--quiet", "--short", "HEAD"])? {
                Some(value) if !value.trim().is_empty() => value.trim().to_owned(),
                _ => return Ok(PushOutcome::NoCurrentBranch),
            };
        run_git(&repository, &["check-ref-format", "--branch", &branch])?;

        let tracking = run_git_optional(
            &repository,
            &[
                "rev-parse",
                "--abbrev-ref",
                "--symbolic-full-name",
                "@{upstream}",
            ],
        )?
        .is_some_and(|value| !value.trim().is_empty());

        if tracking {
            run_git(&repository, &["push"])?;
            Ok(PushOutcome::PushedTracked)
        } else {
            run_git(&repository, &["push", "-u", "origin", &branch])?;
            Ok(PushOutcome::SetUpstream { branch })
        }
    }

    /// Report whether GitHub CLI is installed/authenticated and whether the
    /// current branch already has a pull request. Missing/unauthenticated `gh`
    /// intentionally degrades to `gh_ready=false`, matching the Electron bar.
    pub fn ship_info(&self, repository: &Path) -> Result<ShipInfo, String> {
        let repository = canonical_repository(repository)?;
        let Some(gh) = resolve_gh_executable(&repository) else {
            return Ok(ShipInfo {
                gh_ready: false,
                pull_request: None,
            });
        };

        let auth = run_gh(&repository, &gh, &["auth", "status"])?;
        if !auth.status.success() {
            return Ok(ShipInfo {
                gh_ready: false,
                pull_request: None,
            });
        }

        let view = run_gh(
            &repository,
            &gh,
            &["pr", "view", "--json", "url,state,number"],
        )?;
        if !view.status.success() {
            return Ok(ShipInfo {
                gh_ready: true,
                pull_request: None,
            });
        }

        Ok(ShipInfo {
            gh_ready: true,
            pull_request: parse_pull_request(&view.stdout),
        })
    }

    /// Push the current branch, then create a pull request using commit-derived
    /// title/body. Unlike the Electron helper, push failure is not swallowed:
    /// the Rust authority fails closed rather than claiming a ship succeeded.
    pub fn create_pull_request(&self, repository: &Path) -> Result<String, String> {
        let repository = canonical_repository(repository)?;
        let Some(gh) = resolve_gh_executable(&repository) else {
            return Err("GitHub CLI is not installed in a trusted executable location.".to_owned());
        };

        self.push(&repository)?;
        let output = run_gh(&repository, &gh, &["pr", "create", "--fill"])?;
        if !output.status.success() {
            return Err(gh_failure("gh pr create", &output));
        }
        let stdout = String::from_utf8(output.stdout)
            .map_err(|_| "GitHub CLI output was not valid UTF-8.".to_owned())?;
        let url = stdout
            .lines()
            .rev()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .ok_or_else(|| "GitHub CLI did not return a pull request URL.".to_owned())?;
        validate_pr_url(url)
    }
}

fn validate_commit_message(message: &str) -> Result<&str, String> {
    let message = message.trim();
    if message.is_empty() {
        return Err("Commit message is required.".to_owned());
    }
    if message.len() > MAX_COMMIT_MESSAGE_BYTES {
        return Err(format!(
            "Commit message exceeds the {} KiB safety limit.",
            MAX_COMMIT_MESSAGE_BYTES / 1024
        ));
    }
    if message.contains('\0') {
        return Err("Commit message contains a NUL character.".to_owned());
    }
    Ok(message)
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
    let root = run_git(&repository, &["rev-parse", "--show-toplevel"])?;
    let root = PathBuf::from(root.trim())
        .canonicalize()
        .map_err(|error| format!("Could not resolve Git repository root: {error}"))?;
    if repository != root {
        return Err("Git ship operations require the repository root.".to_owned());
    }
    Ok(root)
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

fn run_git_optional(repository: &Path, args: &[&str]) -> Result<Option<String>, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(args)
        .output()
        .map_err(|error| format!("Could not start Git: {error}"))?;
    if output.stdout.len() > MAX_GIT_OUTPUT_BYTES || output.stderr.len() > MAX_GIT_OUTPUT_BYTES {
        return Err("Git returned an oversized response.".to_owned());
    }
    if !output.status.success() {
        return Ok(None);
    }
    String::from_utf8(output.stdout)
        .map(Some)
        .map_err(|_| "Git output was not valid UTF-8.".to_owned())
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

fn resolve_gh_executable(repository: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::<PathBuf>::new();

    #[cfg(windows)]
    {
        for variable in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
            if let Some(root) = std::env::var_os(variable).map(PathBuf::from) {
                candidates.push(root.join("GitHub CLI").join("gh.exe"));
                if variable == "LOCALAPPDATA" {
                    candidates.push(root.join("Programs").join("GitHub CLI").join("gh.exe"));
                }
            }
        }
    }

    #[cfg(not(windows))]
    {
        candidates.extend([
            PathBuf::from("/opt/homebrew/bin/gh"),
            PathBuf::from("/usr/local/bin/gh"),
            PathBuf::from("/usr/bin/gh"),
        ]);
    }

    if let Some(path) = std::env::var_os("PATH") {
        let executable = if cfg!(windows) { "gh.exe" } else { "gh" };
        candidates.extend(
            std::env::split_paths(&path)
                .filter(|root| root.is_absolute())
                .map(|root| root.join(executable)),
        );
    }

    candidates.into_iter().find_map(|candidate| {
        if !candidate.is_absolute() || !candidate.is_file() {
            return None;
        }
        let candidate = candidate.canonicalize().ok()?;
        if candidate.starts_with(repository) {
            return None;
        }
        Some(candidate)
    })
}

fn run_gh(repository: &Path, executable: &Path, args: &[&str]) -> Result<Output, String> {
    if !executable.is_absolute() || !executable.is_file() {
        return Err("GitHub CLI executable is not a trusted absolute file.".to_owned());
    }
    let mut command = Command::new(executable);
    command
        .current_dir(repository)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    timed_output(command, GH_TIMEOUT, MAX_GH_OUTPUT_BYTES)
}

fn timed_output(
    mut command: Command,
    timeout: Duration,
    max_bytes: usize,
) -> Result<Output, String> {
    let mut child = command
        .spawn()
        .map_err(|error| format!("Could not start GitHub CLI: {error}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "GitHub CLI stdout was unavailable.".to_owned())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "GitHub CLI stderr was unavailable.".to_owned())?;
    let stdout_reader = thread::spawn(move || read_bounded(stdout, max_bytes));
    let stderr_reader = thread::spawn(move || read_bounded(stderr, max_bytes));

    let started = Instant::now();
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("Could not poll GitHub CLI: {error}"))?
        {
            break status;
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err(format!(
                "GitHub CLI exceeded the {} second timeout.",
                timeout.as_secs()
            ));
        }
        thread::sleep(Duration::from_millis(25));
    };

    let stdout = stdout_reader
        .join()
        .map_err(|_| "GitHub CLI stdout reader failed.".to_owned())??;
    let stderr = stderr_reader
        .join()
        .map_err(|_| "GitHub CLI stderr reader failed.".to_owned())??;
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

fn read_bounded<R: Read>(mut reader: R, max_bytes: usize) -> Result<Vec<u8>, String> {
    let mut stored = Vec::new();
    let mut buffer = [0_u8; 8 * 1024];
    let mut oversized = false;
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| format!("Could not read subprocess output: {error}"))?;
        if read == 0 {
            break;
        }
        if stored.len().saturating_add(read) <= max_bytes {
            stored.extend_from_slice(&buffer[..read]);
        } else {
            oversized = true;
        }
    }
    if oversized {
        Err("GitHub CLI returned an oversized response.".to_owned())
    } else {
        Ok(stored)
    }
}

fn parse_pull_request(bytes: &[u8]) -> Option<PullRequestInfo> {
    let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let url = value.get("url")?.as_str()?.trim();
    if url.is_empty() {
        return None;
    }
    let state = value.get("state")?.as_str()?.trim();
    let number = value.get("number")?.as_u64()?;
    let url = validate_pr_url(url).ok()?;
    Some(PullRequestInfo {
        url,
        state: state.to_owned(),
        number,
    })
}

fn validate_pr_url(url: &str) -> Result<String, String> {
    let url = url.trim();
    if !url.starts_with("https://github.com/")
        || url.contains('@')
        || url.chars().any(char::is_control)
    {
        return Err("GitHub CLI returned an invalid pull request URL.".to_owned());
    }
    Ok(url.to_owned())
}

fn gh_failure(label: &str, output: &Output) -> String {
    let detail = String::from_utf8_lossy(&output.stderr)
        .trim()
        .chars()
        .take(2_048)
        .collect::<String>();
    if detail.is_empty() {
        format!("{label} failed.")
    } else {
        format!("{label} failed: {detail}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn commit_stages_all_only_when_index_is_empty() {
        let repository = test_repository("commit");
        fs::write(repository.join("a.txt"), "a0\n").expect("a");
        fs::write(repository.join("b.txt"), "b0\n").expect("b");
        git_ok(&repository, &["add", "."]);
        git_ok(&repository, &["commit", "-m", "seed"]);

        fs::write(repository.join("a.txt"), "a1\n").expect("a edit");
        fs::write(repository.join("new.txt"), "new\n").expect("new");
        GitShipService
            .commit(&repository, "  commit all  ", false)
            .expect("commit all");
        assert!(git_text(&repository, &["status", "--porcelain=v1"]).is_empty());
        assert_eq!(
            git_text(&repository, &["log", "-1", "--pretty=%s"]).trim(),
            "commit all"
        );

        fs::write(repository.join("a.txt"), "a2\n").expect("a second edit");
        fs::write(repository.join("b.txt"), "b2\n").expect("b second edit");
        git_ok(&repository, &["add", "a.txt"]);
        GitShipService
            .commit(&repository, "staged only", false)
            .expect("commit staged only");
        let status = git_text(&repository, &["status", "--porcelain=v1"]);
        assert!(status.contains("b.txt"));
        assert!(!status.contains("a.txt"));
        let names = git_text(&repository, &["show", "--pretty=", "--name-only", "HEAD"]);
        assert!(names.lines().any(|line| line.trim() == "a.txt"));
        assert!(!names.lines().any(|line| line.trim() == "b.txt"));
        cleanup(repository);
    }

    #[test]
    fn push_sets_upstream_then_reuses_tracking() {
        let root = test_directory("push");
        let bare = root.join("remote.git");
        let repository = root.join("repo");
        fs::create_dir_all(&repository).expect("repo");
        git_ok_at(&root, &["init", "--bare", bare.to_string_lossy().as_ref()]);
        git_ok(&repository, &["init", "--quiet"]);
        configure_repo(&repository);
        fs::write(repository.join("seed.txt"), "seed\n").expect("seed");
        git_ok(&repository, &["add", "seed.txt"]);
        git_ok(&repository, &["commit", "-m", "seed"]);
        git_ok(&repository, &["switch", "-c", "feature"]);
        git_ok(
            &repository,
            &["remote", "add", "origin", bare.to_string_lossy().as_ref()],
        );

        assert_eq!(
            GitShipService.push(&repository).expect("first push"),
            PushOutcome::SetUpstream {
                branch: "feature".to_owned()
            }
        );
        assert_eq!(
            git_text(
                &repository,
                &[
                    "rev-parse",
                    "--abbrev-ref",
                    "--symbolic-full-name",
                    "@{upstream}"
                ]
            )
            .trim(),
            "origin/feature"
        );

        fs::write(repository.join("seed.txt"), "second\n").expect("second");
        git_ok(&repository, &["add", "seed.txt"]);
        git_ok(&repository, &["commit", "-m", "second"]);
        assert_eq!(
            GitShipService.push(&repository).expect("tracked push"),
            PushOutcome::PushedTracked
        );
        assert_eq!(
            git_text(&repository, &["rev-parse", "HEAD"]).trim(),
            git_text(&bare, &["rev-parse", "refs/heads/feature"]).trim()
        );
        cleanup(root);
    }

    #[test]
    fn pull_request_parser_requires_safe_github_url_and_shape() {
        assert_eq!(
            parse_pull_request(
                br#"{"url":"https://github.com/acme/repo/pull/42","state":"OPEN","number":42}"#
            ),
            Some(PullRequestInfo {
                url: "https://github.com/acme/repo/pull/42".to_owned(),
                state: "OPEN".to_owned(),
                number: 42,
            })
        );
        assert!(
            parse_pull_request(
                br#"{"url":"https://user@github.com/acme/repo/pull/1","state":"OPEN","number":1}"#
            )
            .is_none()
        );
        assert!(
            parse_pull_request(
                br#"{"url":"https://example.com/pull/1","state":"OPEN","number":1}"#
            )
            .is_none()
        );
        assert!(parse_pull_request(br#"{"url":"","state":"OPEN","number":1}"#).is_none());
    }

    #[test]
    fn commit_message_validation_is_trimmed_bounded_and_nul_safe() {
        assert_eq!(
            validate_commit_message("  useful message  "),
            Ok("useful message")
        );
        assert!(validate_commit_message("   ").is_err());
        assert!(validate_commit_message("bad\0message").is_err());
        let oversized = "x".repeat(MAX_COMMIT_MESSAGE_BYTES + 1);
        assert!(validate_commit_message(&oversized).is_err());
    }

    fn test_repository(label: &str) -> PathBuf {
        let repository = test_directory(label);
        git_ok(&repository, &["init", "--quiet"]);
        configure_repo(&repository);
        repository
    }

    fn configure_repo(repository: &Path) {
        git_ok(repository, &["config", "core.autocrlf", "false"]);
        git_ok(
            repository,
            &["config", "user.email", "hermes-tests@example.invalid"],
        );
        git_ok(repository, &["config", "user.name", "Hermes Tests"]);
    }

    fn test_directory(label: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "hermes-git-ship-{label}-{}-{suffix}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).expect("test directory");
        directory
    }

    fn git_ok(repository: &Path, args: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(repository)
            .args(args)
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn git_ok_at(cwd: &Path, args: &[&str]) {
        let output = Command::new("git")
            .current_dir(cwd)
            .args(args)
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn git_text(repository: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(repository)
            .args(args)
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("UTF-8 git output")
    }

    fn cleanup(path: PathBuf) {
        let _ = fs::remove_dir_all(path);
    }
}
