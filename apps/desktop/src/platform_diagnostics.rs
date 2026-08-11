use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    path::{Path, PathBuf},
};

const PROXY_KEYS: [&str; 4] = ["HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "NO_PROXY"];
const CA_KEYS: [&str; 4] = [
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
    "NODE_EXTRA_CA_CERTS",
    "REQUESTS_CA_BUNDLE",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentDiagnostics {
    pub path_entries: Vec<PathBuf>,
    pub proxy_configured: bool,
    pub custom_ca_configured: bool,
    pub wsl: bool,
    pub display_configured: bool,
    pub wayland_configured: bool,
    pub appdata_configured: bool,
    pub localappdata_configured: bool,
    pub temp_configured: bool,
}

pub fn inspect() -> EnvironmentDiagnostics {
    let environment: BTreeMap<OsString, OsString> = std::env::vars_os().collect();
    inspect_environment(&environment)
}

fn inspect_environment(environment: &BTreeMap<OsString, OsString>) -> EnvironmentDiagnostics {
    EnvironmentDiagnostics {
        path_entries: normalized_path_entries(environment.get(&OsString::from("PATH"))),
        proxy_configured: any_non_empty(environment, &PROXY_KEYS),
        custom_ca_configured: any_non_empty(environment, &CA_KEYS),
        wsl: any_non_empty(environment, &["WSL_DISTRO_NAME", "WSL_INTEROP"]),
        display_configured: non_empty(environment, "DISPLAY"),
        wayland_configured: non_empty(environment, "WAYLAND_DISPLAY"),
        appdata_configured: non_empty(environment, "APPDATA"),
        localappdata_configured: non_empty(environment, "LOCALAPPDATA"),
        temp_configured: non_empty(environment, "TEMP") || non_empty(environment, "TMP"),
    }
}

fn any_non_empty(environment: &BTreeMap<OsString, OsString>, keys: &[&str]) -> bool {
    keys.iter().any(|key| non_empty(environment, key))
}

fn non_empty(environment: &BTreeMap<OsString, OsString>, key: &str) -> bool {
    environment
        .get(&OsString::from(key))
        .and_then(|value| value.to_str())
        .is_some_and(|value| !value.trim().is_empty())
}

fn normalized_path_entries(value: Option<&OsString>) -> Vec<PathBuf> {
    let Some(value) = value else {
        return Vec::new();
    };

    let mut seen = BTreeSet::new();
    let mut entries = Vec::new();
    for entry in std::env::split_paths(value) {
        if entry.as_os_str().is_empty() {
            continue;
        }
        let normalized = lexical_normalize(&entry);
        let key = comparison_key(&normalized);
        if seen.insert(key) {
            entries.push(normalized);
        }
    }
    entries
}

fn lexical_normalize(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        use std::path::Component;
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                let _ = result.pop();
            }
            other => result.push(other.as_os_str()),
        }
    }
    if result.as_os_str().is_empty() {
        path.to_path_buf()
    } else {
        result
    }
}

fn comparison_key(path: &Path) -> String {
    let value = path.to_string_lossy();
    if cfg!(windows) {
        value.to_lowercase()
    } else {
        value.into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn environment(values: &[(&str, &str)]) -> BTreeMap<OsString, OsString> {
        values
            .iter()
            .map(|(key, value)| (OsString::from(key), OsString::from(value)))
            .collect()
    }

    #[test]
    fn diagnostics_expose_presence_not_sensitive_values() {
        let environment = environment(&[
            ("HTTPS_PROXY", "https://user:secret@example.invalid:8443"),
            ("NODE_EXTRA_CA_CERTS", r"C:\private\corp-root.pem"),
            ("WSL_DISTRO_NAME", "Ubuntu"),
            ("DISPLAY", ":0"),
        ]);
        let diagnostics = inspect_environment(&environment);
        assert!(diagnostics.proxy_configured);
        assert!(diagnostics.custom_ca_configured);
        assert!(diagnostics.wsl);
        assert!(diagnostics.display_configured);

        let rendered = format!("{diagnostics:?}");
        assert!(!rendered.contains("user:secret"));
        assert!(!rendered.contains("corp-root.pem"));
        assert!(!rendered.contains("Ubuntu"));
        assert!(!rendered.contains(":0"));
    }

    #[test]
    fn blank_sensitive_environment_does_not_count_as_configured() {
        let environment = environment(&[
            ("HTTP_PROXY", "  "),
            ("SSL_CERT_FILE", ""),
            ("WSL_INTEROP", ""),
        ]);
        let diagnostics = inspect_environment(&environment);
        assert!(!diagnostics.proxy_configured);
        assert!(!diagnostics.custom_ca_configured);
        assert!(!diagnostics.wsl);
    }

    #[test]
    fn path_entries_drop_empty_and_lexical_dot_segments() {
        let separator = if cfg!(windows) { ";" } else { ":" };
        let value = format!(
            "{}{}{}",
            Path::new("alpha").join(".").display(),
            separator,
            Path::new("beta").join("child").join("..").display()
        );
        let entries = normalized_path_entries(Some(&OsString::from(value)));
        assert_eq!(entries, vec![PathBuf::from("alpha"), PathBuf::from("beta")]);
    }
}
