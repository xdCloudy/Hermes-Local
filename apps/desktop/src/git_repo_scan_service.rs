use std::{
    collections::{BTreeMap, HashSet, VecDeque},
    fs,
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use hermes_core::{
    AppServices, DiscoveredGitRepository, GitRepoScanService as GitRepoScanServiceContract,
    RepoScanCancellation, ServiceError, ServiceFuture,
};

const DEFAULT_MAX_DEPTH: usize = 3;
const MAX_SCAN_DEPTH: usize = 32;
const MAX_VISITED_DIRECTORIES: usize = 100_000;
const JUNK_DIRECTORIES: [&str; 6] = [
    "Applications",
    "Library",
    "node_modules",
    "site-packages",
    "vendor",
    "venv",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveredRepository {
    pub root: PathBuf,
    pub label: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepoScanOptions {
    pub enabled: bool,
    pub max_depth: usize,
    pub exclude_paths: Vec<PathBuf>,
}

impl Default for RepoScanOptions {
    fn default() -> Self {
        Self {
            enabled: true,
            max_depth: DEFAULT_MAX_DEPTH,
            exclude_paths: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct GitRepoScanService;

pub fn install(services: &mut AppServices) {
    services.git_repo_scan = Arc::new(GitRepoScanService);
}

impl GitRepoScanServiceContract for GitRepoScanService {
    fn scan(
        &self,
        roots: &[PathBuf],
        exclude_paths: &[PathBuf],
        enabled: bool,
        cancellation: RepoScanCancellation,
    ) -> ServiceFuture<'_, Vec<DiscoveredGitRepository>> {
        let roots = roots.to_vec();
        let exclude_paths = exclude_paths.to_vec();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || {
                let options = RepoScanOptions {
                    enabled,
                    exclude_paths,
                    ..RepoScanOptions::default()
                };
                GitRepoScanService
                    .scan_with_cancel(&roots, &options, || cancellation.is_cancelled())
            })
            .await
            .map_err(|error| {
                ServiceError::Platform(format!("repository scan worker failed: {error}"))
            })?
            .map(|repositories| {
                repositories
                    .into_iter()
                    .map(|repository| DiscoveredGitRepository {
                        root: repository.root,
                        label: repository.label,
                    })
                    .collect()
            })
            .map_err(|error| {
                if error.contains("cancelled") {
                    ServiceError::Unavailable(error)
                } else {
                    ServiceError::Platform(error)
                }
            })
        })
    }
}

impl GitRepoScanService {
    /// Scan configured roots for normal Git repositories. An empty root list
    /// preserves the Electron behavior of scanning the current user's home.
    /// Filesystem permission/read errors are skipped per-directory rather than
    /// aborting the whole discovery pass.
    pub fn scan(
        &self,
        roots: &[PathBuf],
        options: &RepoScanOptions,
    ) -> Result<Vec<DiscoveredRepository>, String> {
        self.scan_with_cancel(roots, options, || false)
    }

    fn scan_with_cancel<F>(
        &self,
        roots: &[PathBuf],
        options: &RepoScanOptions,
        cancelled: F,
    ) -> Result<Vec<DiscoveredRepository>, String>
    where
        F: Fn() -> bool,
    {
        if cancelled() {
            return Err("Repository discovery was cancelled.".to_owned());
        }
        if !options.enabled {
            return Ok(Vec::new());
        }
        if options.max_depth > MAX_SCAN_DEPTH {
            return Err(format!(
                "Git repository scan depth exceeds the safety limit of {MAX_SCAN_DEPTH}."
            ));
        }

        let home = home_directory()?;
        let requested_roots = if roots.is_empty() {
            vec![home.clone()]
        } else {
            roots.to_vec()
        };
        let search_roots = deduplicated_paths(&requested_roots, &home)?;
        let exclusions = deduplicated_paths(&options.exclude_paths, &home)?;
        let exclusion_keys = exclusions
            .iter()
            .map(|path| path_key(path))
            .collect::<Vec<_>>();

        let mut found = BTreeMap::<String, DiscoveredRepository>::new();
        let mut visited = HashSet::<String>::new();
        let mut queue = VecDeque::<(PathBuf, usize)>::new();
        queue.extend(search_roots.into_iter().map(|root| (root, 0)));

        while let Some((directory, depth)) = queue.pop_front() {
            if cancelled() {
                return Err("Repository discovery was cancelled.".to_owned());
            }
            if depth > options.max_depth {
                continue;
            }
            let key = path_key(&directory);
            if exclusion_keys
                .iter()
                .any(|excluded| key_is_within(&key, excluded))
                || !visited.insert(key.clone())
            {
                continue;
            }
            if visited.len() > MAX_VISITED_DIRECTORIES {
                return Err(format!(
                    "Git repository discovery exceeded {MAX_VISITED_DIRECTORIES} directories."
                ));
            }

            let entries = match fs::read_dir(&directory) {
                Ok(entries) => entries.filter_map(Result::ok).collect::<Vec<_>>(),
                Err(_) => continue,
            };

            let git_directory = entries.iter().any(|entry| {
                entry.file_name() == ".git"
                    && entry.file_type().is_ok_and(|file_type| file_type.is_dir())
            });
            if git_directory && fs::File::open(directory.join(".git").join("HEAD")).is_ok() {
                found.entry(key).or_insert_with(|| DiscoveredRepository {
                    label: directory
                        .file_name()
                        .and_then(|name| name.to_str())
                        .filter(|name| !name.is_empty())
                        .map_or_else(|| directory.display().to_string(), str::to_owned),
                    root: directory,
                });
                continue;
            }

            if depth == options.max_depth {
                continue;
            }
            for entry in entries {
                if cancelled() {
                    return Err("Repository discovery was cancelled.".to_owned());
                }
                let Ok(file_type) = entry.file_type() else {
                    continue;
                };
                if !file_type.is_dir() {
                    continue;
                }
                let name = entry.file_name();
                let Some(name) = name.to_str() else {
                    continue;
                };
                if name.starts_with('.') || JUNK_DIRECTORIES.contains(&name) {
                    continue;
                }
                queue.push_back((entry.path(), depth + 1));
            }
        }

        Ok(found.into_values().collect())
    }
}

fn deduplicated_paths(paths: &[PathBuf], home: &Path) -> Result<Vec<PathBuf>, String> {
    let mut normalized = BTreeMap::<String, PathBuf>::new();
    for path in paths {
        if let Some(path) = normalize_scan_path(path, home)? {
            normalized.entry(path_key(&path)).or_insert(path);
        }
    }
    Ok(normalized.into_values().collect())
}

fn normalize_scan_path(path: &Path, home: &Path) -> Result<Option<PathBuf>, String> {
    let raw = path.to_string_lossy();
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(None);
    }

    let expanded = if raw == "~" {
        home.to_owned()
    } else if let Some(rest) = raw.strip_prefix("~/").or_else(|| raw.strip_prefix("~\\")) {
        home.join(rest)
    } else if path.is_absolute() {
        path.to_owned()
    } else {
        home.join(path)
    };
    Ok(Some(lexically_normalized_absolute(&expanded)?))
}

fn lexically_normalized_absolute(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err("Repository scan path could not be resolved as absolute.".to_owned());
    }
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    return Err("Repository scan path escaped its filesystem root.".to_owned());
                }
            }
        }
    }
    Ok(normalized)
}

fn path_key(path: &Path) -> String {
    let value = path.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        value.to_lowercase()
    } else {
        value
    }
}

fn key_is_within(candidate: &str, parent: &str) -> bool {
    if candidate == parent {
        return true;
    }
    let parent = parent.trim_end_matches('/');
    candidate
        .strip_prefix(parent)
        .is_some_and(|suffix| suffix.starts_with('/'))
}

fn home_directory() -> Result<PathBuf, String> {
    #[cfg(windows)]
    let value = std::env::var_os("USERPROFILE")
        .or_else(|| {
            let drive = std::env::var_os("HOMEDRIVE")?;
            let path = std::env::var_os("HOMEPATH")?;
            let mut home = PathBuf::from(drive);
            home.push(path);
            Some(home.into_os_string())
        })
        .map(PathBuf::from);

    #[cfg(not(windows))]
    let value = std::env::var_os("HOME").map(PathBuf::from);

    value
        .filter(|path| path.is_absolute())
        .ok_or_else(|| "Could not determine the current user's home directory.".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn disabled_scan_returns_before_resolving_roots() {
        let result = GitRepoScanService.scan(
            &[PathBuf::from("definitely-relative-and-unused")],
            &RepoScanOptions {
                enabled: false,
                ..RepoScanOptions::default()
            },
        );
        assert_eq!(result, Ok(Vec::new()));
    }

    #[test]
    fn cooperative_cancellation_stops_before_touching_roots() {
        let error = GitRepoScanService
            .scan_with_cancel(
                &[PathBuf::from("definitely-relative-and-unused")],
                &RepoScanOptions::default(),
                || true,
            )
            .expect_err("cancelled scan");
        assert!(error.contains("cancelled"));
    }

    #[test]
    fn scans_configured_root_and_excludes_complete_subtree() {
        let root = test_directory("root-exclusion");
        let included = root.join("included");
        let excluded = root.join("excluded");
        let invalid = root.join("invalid");
        make_repo(&included, true);
        make_repo(&excluded, true);
        make_repo(&invalid, false);

        let result = GitRepoScanService
            .scan(
                std::slice::from_ref(&root),
                &RepoScanOptions {
                    max_depth: 2,
                    exclude_paths: vec![excluded],
                    ..RepoScanOptions::default()
                },
            )
            .expect("scan repositories");
        assert_eq!(
            result,
            vec![DiscoveredRepository {
                label: "included".to_owned(),
                root: included,
            }]
        );
        cleanup(root);
    }

    #[test]
    fn overlapping_roots_are_deduplicated() {
        let root = test_directory("dedupe");
        let repository = root.join("repo");
        make_repo(&repository, true);

        let result = GitRepoScanService
            .scan(
                &[root.clone(), repository.clone(), repository.clone()],
                &RepoScanOptions::default(),
            )
            .expect("scan repositories");
        assert_eq!(
            result,
            vec![DiscoveredRepository {
                label: "repo".to_owned(),
                root: repository,
            }]
        );
        cleanup(root);
    }

    #[test]
    fn hidden_and_junk_directories_are_not_descended() {
        let root = test_directory("junk");
        for name in [".hidden", "node_modules", "vendor", "venv"] {
            make_repo(&root.join(name).join("repo"), true);
        }
        make_repo(&root.join("src").join("repo"), true);

        let result = GitRepoScanService
            .scan(std::slice::from_ref(&root), &RepoScanOptions::default())
            .expect("scan repositories");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].label, "repo");
        assert!(result[0].root.ends_with(Path::new("src").join("repo")));
        cleanup(root);
    }

    #[test]
    fn path_normalization_expands_tilde_and_relative_paths_from_home() {
        let home = test_directory("home");
        assert_eq!(
            normalize_scan_path(Path::new("~/src"), &home).expect("tilde"),
            Some(home.join("src"))
        );
        assert_eq!(
            normalize_scan_path(Path::new("src"), &home).expect("relative"),
            Some(home.join("src"))
        );
        cleanup(home);
    }

    #[test]
    fn containment_is_segment_aware() {
        let separator = if cfg!(windows) { "C:/src" } else { "/src" };
        let parent = format!("{separator}/fever");
        assert!(key_is_within(&format!("{parent}/repo"), &parent));
        assert!(key_is_within(&parent, &parent));
        assert!(!key_is_within(&format!("{separator}/feverish"), &parent));
    }

    fn make_repo(root: &Path, valid: bool) {
        fs::create_dir_all(root.join(".git")).expect("repository metadata");
        if valid {
            fs::write(root.join(".git").join("HEAD"), "ref: refs/heads/main\n")
                .expect("HEAD fixture");
        }
    }

    fn test_directory(label: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "hermes-repo-scan-{label}-{}-{suffix}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).expect("test directory");
        directory.canonicalize().expect("canonical test directory")
    }

    fn cleanup(path: PathBuf) {
        let _ = fs::remove_dir_all(path);
    }
}
