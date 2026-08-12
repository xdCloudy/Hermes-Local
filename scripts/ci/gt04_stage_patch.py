from pathlib import Path


def replace_once(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one match, found {count}: {old[:140]!r}")
    path.write_text(text.replace(old, new), encoding="utf-8")


protocol = Path("crates/hermes-protocol/src/lib.rs")
replace_once(
    protocol,
    '''#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct GitStatus {
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub ahead: u32,
    #[serde(default)]
    pub behind: u32,
    #[serde(default)]
    pub changed: Vec<String>,
}
''',
    '''#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct GitChange {
    pub path: String,
    #[serde(default)]
    pub index_status: String,
    #[serde(default)]
    pub worktree_status: String,
    #[serde(default)]
    pub staged: bool,
    #[serde(default)]
    pub unstaged: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct GitStatus {
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub ahead: u32,
    #[serde(default)]
    pub behind: u32,
    #[serde(default)]
    pub changed: Vec<String>,
    #[serde(default)]
    pub entries: Vec<GitChange>,
}
''',
)

core = Path("crates/hermes-core/src/lib.rs")
replace_once(
    core,
    '''    fn diff(&self, repository: &Path, relative: &Path) -> ServiceFuture<'_, String>;
    fn stage(&self, repository: &Path, relative: &Path) -> ServiceFuture<'_, ()>;
''',
    '''    fn diff(&self, repository: &Path, relative: &Path) -> ServiceFuture<'_, String>;
    fn diff_staged(&self, _repository: &Path, _relative: &Path) -> ServiceFuture<'_, String> {
        Box::pin(async move {
            Err(ServiceError::Unavailable(
                "staged Git diff is unavailable on this platform".into(),
            ))
        })
    }
    fn stage(&self, repository: &Path, relative: &Path) -> ServiceFuture<'_, ()>;
''',
)

desktop = Path("crates/hermes-desktop/src/lib.rs")
replace_once(
    desktop,
    '''    let changed = lines
        .filter_map(|line| line.get(3..).map(str::to_owned))
        .collect();
    Ok(GitStatus {
        branch,
        ahead,
        behind,
        changed,
    })
}

fn parse_counter(header: &str, marker: &str) -> u32 {
''',
    '''    let entries: Vec<_> = lines.filter_map(parse_git_change).collect();
    let changed = entries.iter().map(|entry| entry.path.clone()).collect();
    Ok(GitStatus {
        branch,
        ahead,
        behind,
        changed,
        entries,
    })
}

fn parse_git_change(line: &str) -> Option<hermes_protocol::GitChange> {
    let status = line.get(..2)?;
    let raw_path = line.get(3..)?;
    let mut status_chars = status.chars();
    let index = status_chars.next()?;
    let worktree = status_chars.next()?;
    let path = raw_path
        .rsplit(" -> ")
        .next()
        .unwrap_or(raw_path)
        .trim_matches('"')
        .to_owned();
    if path.is_empty() {
        return None;
    }
    Some(hermes_protocol::GitChange {
        path,
        index_status: index.to_string(),
        worktree_status: worktree.to_string(),
        staged: index != ' ' && index != '?',
        unstaged: worktree != ' ' || (index == '?' && worktree == '?'),
    })
}

fn parse_counter(header: &str, marker: &str) -> u32 {
''',
)
replace_once(
    desktop,
    '''    fn stage(&self, repository: &Path, relative: &Path) -> ServiceFuture<'_, ()> {
''',
    '''    fn diff_staged(&self, repository: &Path, relative: &Path) -> ServiceFuture<'_, String> {
        let repository = repository.to_owned();
        let relative = relative.to_owned();
        Box::pin(async move {
            validate_relative_path(&relative)?;
            git(
                &repository,
                &["diff", "--cached", "--", relative.to_string_lossy().as_ref()],
            )
        })
    }

    fn stage(&self, repository: &Path, relative: &Path) -> ServiceFuture<'_, ()> {
''',
)

ui = Path("crates/hermes-ui/src/lib.rs")
replace_once(
    ui,
    '''mod files;
use files::Files;
''',
    '''mod files;
mod review;
use files::Files;
use review::Review;
''',
)
replace_once(
    ui,
    '''simple_surface!(
    Review,
    "Source control",
    "Review",
    "Inspect a change as one coherent story."
);
''',
    '',
)
