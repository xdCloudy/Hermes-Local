use std::{
    collections::BTreeSet,
    env, fs,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use tokio::process::Command;

const MAX_INCLUDE_DEPTH: usize = 8;
const MAX_CONFIG_BYTES: u64 = 1024 * 1024;
const MAX_SSH_G_BYTES: usize = 256 * 1024;
const SSH_G_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ResolvedHost {
    pub hostname: Option<String>,
    pub user: Option<String>,
    pub port: Option<u16>,
    pub identity_file: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnrichedFields {
    pub user: String,
    pub port: Option<u16>,
    pub key_path: String,
}

pub fn configured_hosts() -> Vec<String> {
    let Some(home) = home_dir() else {
        return Vec::new();
    };
    collect_hosts(&home.join(".ssh").join("config"), &home)
}

pub fn collect_hosts(root: &Path, home: &Path) -> Vec<String> {
    let ssh_dir = home.join(".ssh");
    let mut hosts = Vec::new();
    let mut seen_hosts = BTreeSet::new();
    let mut visited = BTreeSet::new();
    walk_config(
        root,
        home,
        &ssh_dir,
        0,
        &mut visited,
        &mut seen_hosts,
        &mut hosts,
    );
    hosts
}

fn walk_config(
    path: &Path,
    home: &Path,
    ssh_dir: &Path,
    depth: usize,
    visited: &mut BTreeSet<PathBuf>,
    seen_hosts: &mut BTreeSet<String>,
    hosts: &mut Vec<String>,
) {
    if depth > MAX_INCLUDE_DEPTH || !visited.insert(path.to_path_buf()) {
        return;
    }
    let Ok(metadata) = fs::metadata(path) else {
        return;
    };
    if !metadata.is_file() || metadata.len() > MAX_CONFIG_BYTES {
        return;
    }
    let Ok(text) = fs::read_to_string(path) else {
        return;
    };

    for host in parse_hosts(&text) {
        if seen_hosts.insert(host.clone()) {
            hosts.push(host);
        }
    }
    for token in parse_includes(&text) {
        let target = resolve_include(&token, home, ssh_dir);
        for expanded in expand_pattern(&target) {
            walk_config(
                &expanded,
                home,
                ssh_dir,
                depth + 1,
                visited,
                seen_hosts,
                hosts,
            );
        }
    }
}

pub fn parse_hosts(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(keyword) = parts.next() else {
            continue;
        };
        if !keyword.eq_ignore_ascii_case("host") {
            continue;
        }
        for pattern in parts {
            if pattern.is_empty()
                || pattern.contains('*')
                || pattern.contains('?')
                || pattern.starts_with('!')
            {
                continue;
            }
            if seen.insert(pattern.to_owned()) {
                out.push(pattern.to_owned());
            }
        }
    }
    out
}

pub fn parse_includes(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(keyword) = parts.next() else {
            continue;
        };
        if keyword.eq_ignore_ascii_case("include") {
            out.extend(parts.filter(|part| !part.is_empty()).map(str::to_owned));
        }
    }
    out
}

pub fn parse_ssh_g_output(text: &str) -> ResolvedHost {
    let mut out = ResolvedHost::default();
    for raw_line in text.lines() {
        let line = raw_line.trim();
        let Some((key, value)) = line.split_once(' ') else {
            continue;
        };
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        if key.eq_ignore_ascii_case("hostname") && out.hostname.is_none() {
            out.hostname = Some(value.to_owned());
        } else if key.eq_ignore_ascii_case("user") && out.user.is_none() {
            out.user = Some(value.to_owned());
        } else if key.eq_ignore_ascii_case("port") && out.port.is_none() {
            out.port = value.parse::<u16>().ok();
        } else if key.eq_ignore_ascii_case("identityfile") && out.identity_file.is_none() {
            out.identity_file = Some(value.to_owned());
        }
    }
    out
}

pub fn enrich_fields(
    user: &str,
    port: Option<u16>,
    key_path: &str,
    resolved: &ResolvedHost,
) -> EnrichedFields {
    EnrichedFields {
        user: if user.trim().is_empty() {
            resolved.user.clone().unwrap_or_default()
        } else {
            user.to_owned()
        },
        port: port.or_else(|| resolved.port.filter(|value| *value != 22)),
        key_path: if key_path.trim().is_empty() {
            resolved.identity_file.clone().unwrap_or_default()
        } else {
            key_path.to_owned()
        },
    }
}

pub async fn resolve_host(host: &str) -> Result<ResolvedHost, String> {
    validate_host(host)?;
    let ssh = resolve_ssh_executable()?;
    let mut command = Command::new(ssh);
    command
        .args(["-G", host])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let output = tokio::time::timeout(SSH_G_TIMEOUT, command.output())
        .await
        .map_err(|_| "OpenSSH config resolution timed out.".to_owned())?
        .map_err(|error| format!("Could not start OpenSSH config resolution: {error}"))?;
    if output.stdout.len() > MAX_SSH_G_BYTES || output.stderr.len() > MAX_SSH_G_BYTES {
        return Err("OpenSSH config resolution returned an oversized response.".to_owned());
    }
    if !output.status.success() {
        return Err("OpenSSH could not resolve that host configuration.".to_owned());
    }
    Ok(parse_ssh_g_output(&String::from_utf8_lossy(&output.stdout)))
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("USERPROFILE")
        .or_else(|| env::var_os("HOME"))
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

fn resolve_include(token: &str, home: &Path, ssh_dir: &Path) -> PathBuf {
    if let Some(rest) = token.strip_prefix("~/") {
        return home.join(rest);
    }
    let path = Path::new(token);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        ssh_dir.join(path)
    }
}

fn expand_pattern(pattern: &Path) -> Vec<PathBuf> {
    let text = pattern.to_string_lossy();
    if !text.contains('*') && !text.contains('?') {
        return vec![pattern.to_path_buf()];
    }
    let mut candidates = vec![PathBuf::new()];
    for component in pattern.components() {
        use std::path::Component;
        match component {
            Component::Prefix(prefix) => {
                for candidate in &mut candidates {
                    candidate.push(prefix.as_os_str());
                }
            }
            Component::RootDir => {
                for candidate in &mut candidates {
                    candidate.push(Path::new(std::path::MAIN_SEPARATOR_STR));
                }
            }
            Component::CurDir => {}
            Component::ParentDir => {
                for candidate in &mut candidates {
                    candidate.push("..");
                }
            }
            Component::Normal(name) => {
                let name = name.to_string_lossy();
                if !name.contains('*') && !name.contains('?') {
                    for candidate in &mut candidates {
                        candidate.push(name.as_ref());
                    }
                    continue;
                }
                let mut next = Vec::new();
                for candidate in &candidates {
                    let directory = if candidate.as_os_str().is_empty() {
                        Path::new(".")
                    } else {
                        candidate.as_path()
                    };
                    let Ok(entries) = fs::read_dir(directory) else {
                        continue;
                    };
                    for entry in entries.flatten() {
                        let file_name = entry.file_name();
                        if wildcard_match(&name, &file_name.to_string_lossy()) {
                            next.push(entry.path());
                        }
                    }
                }
                candidates = next;
            }
        }
    }
    candidates.sort();
    candidates
}

fn wildcard_match(pattern: &str, value: &str) -> bool {
    let pattern = pattern.as_bytes();
    let value = value.as_bytes();
    let (mut p, mut v, mut star, mut match_at) = (0, 0, None, 0);
    while v < value.len() {
        if p < pattern.len() && (pattern[p] == b'?' || pattern[p] == value[v]) {
            p += 1;
            v += 1;
        } else if p < pattern.len() && pattern[p] == b'*' {
            star = Some(p);
            p += 1;
            match_at = v;
        } else if let Some(star_index) = star {
            p = star_index + 1;
            match_at += 1;
            v = match_at;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == b'*' {
        p += 1;
    }
    p == pattern.len()
}

fn validate_host(host: &str) -> Result<(), String> {
    let host = host.trim();
    if host.is_empty()
        || host.starts_with('-')
        || host.len() > 255
        || host.chars().any(char::is_control)
    {
        return Err("Unsafe SSH host.".to_owned());
    }
    Ok(())
}

fn resolve_ssh_executable() -> Result<PathBuf, String> {
    if let Some(explicit) = env::var_os("HERMES_LOCAL_SSH") {
        let path = PathBuf::from(explicit);
        if path.is_absolute() && path.is_file() {
            return Ok(path);
        }
        return Err("HERMES_LOCAL_SSH must point to an absolute OpenSSH executable.".to_owned());
    }
    if cfg!(windows) {
        let system_root =
            env::var_os("SystemRoot").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
        let native = system_root.join("System32").join("OpenSSH").join("ssh.exe");
        if native.is_file() {
            return Ok(native);
        }
        return Err("Windows OpenSSH client was not found.".to_owned());
    }
    ["/usr/bin/ssh", "/usr/local/bin/ssh"]
        .into_iter()
        .map(PathBuf::from)
        .find(|path| path.is_file())
        .ok_or_else(|| "OpenSSH client was not found.".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn host_parser_matches_electron_literal_alias_rules() {
        let config = "Host devbox\n HostName 10.0.0.5\nHost *.internal prod !staging glob*\nHost alpha beta\n# Host ignored\nhost lower-case";
        assert_eq!(
            parse_hosts(config),
            vec!["devbox", "prod", "alpha", "beta", "lower-case"]
        );
    }

    #[test]
    fn include_parser_matches_electron_token_rules() {
        assert_eq!(
            parse_includes("Include ~/.ssh/config.d/*\nInclude work personal\n# Include ignored"),
            vec!["~/.ssh/config.d/*", "work", "personal"]
        );
    }

    #[test]
    fn collection_follows_includes_globs_and_cycles() {
        let root = std::env::temp_dir().join(format!(
            "hermes-ssh-config-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let ssh = root.join(".ssh");
        fs::create_dir_all(ssh.join("config.d")).expect("fixture dirs");
        fs::write(
            ssh.join("config"),
            "Host root\nInclude config.d/*\nInclude cycle",
        )
        .expect("root config");
        fs::write(ssh.join("config.d/10-work"), "Host work").expect("work include");
        fs::write(ssh.join("config.d/20-home"), "Host home").expect("home include");
        fs::write(ssh.join("cycle"), "Host cycle\nInclude config").expect("cycle include");
        let hosts = collect_hosts(&ssh.join("config"), &root);
        assert_eq!(hosts, vec!["root", "work", "home", "cycle"]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ssh_g_parser_takes_first_values() {
        let resolved = parse_ssh_g_output(
            "host devbox\nhostname 10.0.0.5\nuser alice\nport 2222\nidentityfile ~/.ssh/a\nidentityfile ~/.ssh/b",
        );
        assert_eq!(resolved.hostname.as_deref(), Some("10.0.0.5"));
        assert_eq!(resolved.user.as_deref(), Some("alice"));
        assert_eq!(resolved.port, Some(2222));
        assert_eq!(resolved.identity_file.as_deref(), Some("~/.ssh/a"));
    }

    #[test]
    fn enrichment_never_overwrites_manual_fields() {
        let resolved = ResolvedHost {
            hostname: Some("10.0.0.5".to_owned()),
            user: Some("config-user".to_owned()),
            port: Some(2222),
            identity_file: Some("~/.ssh/config-key".to_owned()),
        };
        assert_eq!(
            enrich_fields("manual", Some(2200), "manual-key", &resolved),
            EnrichedFields {
                user: "manual".to_owned(),
                port: Some(2200),
                key_path: "manual-key".to_owned(),
            }
        );
        assert_eq!(
            enrich_fields("", None, "", &resolved),
            EnrichedFields {
                user: "config-user".to_owned(),
                port: Some(2222),
                key_path: "~/.ssh/config-key".to_owned(),
            }
        );
    }

    #[test]
    fn default_ssh_port_remains_unspecified_like_electron_ui() {
        let resolved = ResolvedHost {
            port: Some(22),
            ..ResolvedHost::default()
        };
        assert_eq!(enrich_fields("", None, "", &resolved).port, None);
    }
}
