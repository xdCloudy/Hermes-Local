use std::{
    collections::BTreeMap,
    ffi::OsStr,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use url::Url;

const PROTOCOL: &str = "hermes";
const PROTOCOL_KEY: &str = r"HKCU\Software\Classes\hermes";
const COMMAND_KEY: &str = r"HKCU\Software\Classes\hermes\shell\open\command";
const ACTIVATION_DIRECTORY: &str = "desktop-activations";
const ACTIVATION_SCHEMA_VERSION: u32 = 1;
const MAX_DEEP_LINK_BYTES: usize = 8 * 1024;
const MAX_KIND_CHARS: usize = 48;
const MAX_NAME_CHARS: usize = 1_024;
const MAX_PARAM_COUNT: usize = 32;
const MAX_PARAM_KEY_CHARS: usize = 96;
const MAX_PARAM_VALUE_CHARS: usize = 1_024;
const MAX_ACTIVATION_FILES: usize = 32;
const MAX_ACTIVATION_FILE_BYTES: u64 = 12 * 1024;
static ACTIVATION_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeepLink {
    pub kind: String,
    pub name: String,
    pub params: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolStatus {
    pub available: bool,
    pub registered: bool,
    pub executable: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ActivationRecord {
    schema_version: u32,
    uri: String,
}

pub fn extract_from_args<I, S>(arguments: I) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    arguments.into_iter().find_map(|argument| {
        let value = argument.as_ref().to_str()?;
        value.starts_with("hermes://").then(|| value.to_owned())
    })
}

pub fn parse(raw: &str) -> Result<DeepLink, String> {
    if raw.len() > MAX_DEEP_LINK_BYTES {
        return Err("Hermes deep link exceeds the bounded input limit.".to_owned());
    }
    if raw.chars().any(char::is_control) {
        return Err("Hermes deep link contains a control character.".to_owned());
    }
    if !raw.starts_with("hermes://") {
        return Err("Deep link must use the hermes:// scheme.".to_owned());
    }
    let parsed = Url::parse(raw).map_err(|error| format!("Malformed Hermes deep link: {error}"))?;
    if parsed.scheme() != PROTOCOL {
        return Err("Deep link must use the hermes:// scheme.".to_owned());
    }

    let kind = parsed.host_str().unwrap_or_default().to_owned();
    validate_component("kind", &kind, MAX_KIND_CHARS, false)?;
    let encoded_name = parsed.path().strip_prefix('/').unwrap_or(parsed.path());
    let name = percent_decode_path(encoded_name)?;
    validate_component("name", &name, MAX_NAME_CHARS, true)?;
    let mut params = BTreeMap::new();
    for (key, value) in parsed.query_pairs() {
        let key = key.into_owned();
        let value = value.into_owned();
        validate_component("query key", &key, MAX_PARAM_KEY_CHARS, false)?;
        validate_component("query value", &value, MAX_PARAM_VALUE_CHARS, true)?;
        params.insert(key, value);
        if params.len() > MAX_PARAM_COUNT {
            return Err("Hermes deep link has too many query parameters.".to_owned());
        }
    }

    Ok(DeepLink { kind, name, params })
}

fn validate_component(label: &str, value: &str, limit: usize, empty_allowed: bool) -> Result<(), String> {
    if !empty_allowed && value.is_empty() {
        return Err(format!("Hermes deep-link {label} cannot be empty."));
    }
    if value.chars().count() > limit {
        return Err(format!("Hermes deep-link {label} exceeds its safety limit."));
    }
    if value.chars().any(char::is_control) {
        return Err(format!("Hermes deep-link {label} contains a control character."));
    }
    Ok(())
}

fn percent_decode_path(value: &str) -> Result<String, String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            decoded.push(bytes[index]);
            index += 1;
            continue;
        }
        if index + 2 >= bytes.len() {
            return Err("Deep link path contains an incomplete percent escape.".to_owned());
        }
        let high = hex_value(bytes[index + 1])
            .ok_or_else(|| "Deep link path contains an invalid percent escape.".to_owned())?;
        let low = hex_value(bytes[index + 2])
            .ok_or_else(|| "Deep link path contains an invalid percent escape.".to_owned())?;
        decoded.push((high << 4) | low);
        index += 3;
    }
    String::from_utf8(decoded).map_err(|_| "Deep link path is not valid UTF-8.".to_owned())
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn activation_directory(data_dir: &Path, create: bool) -> Result<PathBuf, String> {
    if !data_dir.is_absolute() {
        return Err("Desktop activation data directory must be absolute.".to_owned());
    }
    if create {
        fs::create_dir_all(data_dir)
            .map_err(|error| format!("Could not create Desktop data directory: {error}"))?;
    }
    let directory = data_dir.join(ACTIVATION_DIRECTORY);
    match fs::symlink_metadata(&directory) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err("Desktop activation queue is not a regular directory.".to_owned());
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && create => {
            fs::create_dir(&directory)
                .map_err(|error| format!("Could not create Desktop activation queue: {error}"))?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(directory),
        Err(error) => return Err(format!("Could not inspect Desktop activation queue: {error}")),
    }
    Ok(directory)
}

pub fn enqueue(data_dir: &Path, raw: &str) -> Result<(), String> {
    parse(raw)?;
    let directory = activation_directory(data_dir, true)?;
    let pending = fs::read_dir(&directory)
        .map_err(|error| format!("Could not enumerate Desktop activation queue: {error}"))?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().and_then(|value| value.to_str()) == Some("json"))
        .count();
    if pending >= MAX_ACTIVATION_FILES {
        return Err("Desktop activation queue is full.".to_owned());
    }

    let sequence = ACTIVATION_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let stem = format!("{nanos:032x}-{:010}-{sequence:016x}", std::process::id());
    let temporary = directory.join(format!("{stem}.tmp"));
    let destination = directory.join(format!("{stem}.json"));
    let payload = serde_json::to_vec(&ActivationRecord {
        schema_version: ACTIVATION_SCHEMA_VERSION,
        uri: raw.to_owned(),
    })
    .map_err(|error| format!("Could not encode Desktop activation: {error}"))?;
    if payload.len() as u64 > MAX_ACTIVATION_FILE_BYTES {
        return Err("Desktop activation payload exceeds its safety limit.".to_owned());
    }

    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| format!("Could not create Desktop activation: {error}"))?;
        file.write_all(&payload)
            .map_err(|error| format!("Could not write Desktop activation: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("Could not flush Desktop activation: {error}"))?;
        drop(file);
        fs::rename(&temporary, &destination)
            .map_err(|error| format!("Could not publish Desktop activation: {error}"))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub fn drain_pending(data_dir: &Path) -> Result<Vec<String>, String> {
    let directory = activation_directory(data_dir, false)?;
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let mut paths = fs::read_dir(&directory)
        .map_err(|error| format!("Could not enumerate Desktop activation queue: {error}"))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
        .collect::<Vec<_>>();
    paths.sort();
    paths.truncate(MAX_ACTIVATION_FILES);

    let mut activations = Vec::new();
    for path in paths {
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        if metadata.file_type().is_symlink()
            || !metadata.is_file()
            || metadata.len() > MAX_ACTIVATION_FILE_BYTES
        {
            let _ = fs::remove_file(&path);
            continue;
        }
        let decoded = fs::read(&path)
            .ok()
            .filter(|bytes| bytes.len() as u64 <= MAX_ACTIVATION_FILE_BYTES)
            .and_then(|bytes| serde_json::from_slice::<ActivationRecord>(&bytes).ok())
            .filter(|record| record.schema_version == ACTIVATION_SCHEMA_VERSION)
            .and_then(|record| parse(&record.uri).ok().map(|_| record.uri));
        let _ = fs::remove_file(&path);
        if let Some(uri) = decoded {
            activations.push(uri);
        }
    }
    Ok(activations)
}

fn registered_command(executable: &Path) -> Result<String, String> {
    if !executable.is_absolute() {
        return Err("Deep-link executable must be an absolute path.".to_owned());
    }
    let executable = executable
        .to_str()
        .ok_or_else(|| "Deep-link executable path must be valid Unicode.".to_owned())?;
    if executable.contains('"') {
        return Err("Deep-link executable path contains an invalid quote.".to_owned());
    }
    Ok(format!("\"{executable}\" \"%1\""))
}

fn parse_default_registry_value(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let (_, value) = line.split_once("REG_SZ")?;
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_owned())
    })
}

#[cfg(windows)]
fn reg_executable() -> Result<PathBuf, String> {
    let root =
        std::env::var_os("SystemRoot").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
    let reg = root.join("System32").join("reg.exe");
    if reg.is_absolute() && reg.is_file() {
        Ok(reg)
    } else {
        Err(format!(
            "Windows registry helper is unavailable: {}",
            reg.display()
        ))
    }
}

#[cfg(windows)]
fn current_executable() -> Result<PathBuf, String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("Could not resolve the Hermes Local executable: {error}"))?;
    if executable.is_absolute() && executable.is_file() {
        Ok(executable)
    } else {
        Err("Hermes Local executable path is not an absolute file path.".to_owned())
    }
}

#[cfg(windows)]
fn query_command() -> Result<Option<String>, String> {
    let output = std::process::Command::new(reg_executable()?)
        .args(["query", COMMAND_KEY, "/ve"])
        .output()
        .map_err(|error| format!("Could not query Hermes protocol registration: {error}"))?;
    if !output.status.success() {
        return Ok(None);
    }
    Ok(parse_default_registry_value(&String::from_utf8_lossy(
        &output.stdout,
    )))
}

#[cfg(windows)]
pub fn status() -> Result<ProtocolStatus, String> {
    let executable = current_executable()?;
    let expected = registered_command(&executable)?;
    let registered = query_command()?.as_deref() == Some(expected.as_str());
    Ok(ProtocolStatus {
        available: true,
        registered,
        executable,
    })
}

#[cfg(not(windows))]
pub fn status() -> Result<ProtocolStatus, String> {
    Ok(ProtocolStatus {
        available: false,
        registered: false,
        executable: std::env::current_exe().unwrap_or_default(),
    })
}

#[cfg(windows)]
pub fn register() -> Result<ProtocolStatus, String> {
    let executable = current_executable()?;
    let command = registered_command(&executable)?;
    let reg = reg_executable()?;
    let writes: [(&str, Vec<&str>); 3] = [
        (
            PROTOCOL_KEY,
            vec!["/ve", "/t", "REG_SZ", "/d", "URL:Hermes Protocol", "/f"],
        ),
        (
            PROTOCOL_KEY,
            vec!["/v", "URL Protocol", "/t", "REG_SZ", "/d", "", "/f"],
        ),
        (
            COMMAND_KEY,
            vec!["/ve", "/t", "REG_SZ", "/d", command.as_str(), "/f"],
        ),
    ];
    for (key, args) in writes {
        let output = std::process::Command::new(&reg)
            .arg("add")
            .arg(key)
            .args(args)
            .output()
            .map_err(|error| format!("Could not register Hermes protocol: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "Windows rejected Hermes protocol registration: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
    }

    let state = status()?;
    if !state.registered {
        return Err(
            "Hermes protocol registration did not match the current executable.".to_owned(),
        );
    }
    Ok(state)
}

#[cfg(not(windows))]
pub fn register() -> Result<ProtocolStatus, String> {
    status()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{ffi::OsString, time::Duration};

    fn temp_dir(label: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_nanos();
        std::env::temp_dir().join(format!(
            "hermes-deep-link-{label}-{}-{suffix}",
            std::process::id()
        ))
    }

    #[test]
    fn parses_blueprint_deep_link_like_electron_oracle() {
        let link = parse("hermes://blueprint/morning-brief?time=08%3A00&mode=fast")
            .expect("valid deep link");
        assert_eq!(link.kind, "blueprint");
        assert_eq!(link.name, "morning-brief");
        assert_eq!(link.params.get("time").map(String::as_str), Some("08:00"));
        assert_eq!(link.params.get("mode").map(String::as_str), Some("fast"));
    }

    #[test]
    fn decodes_path_and_last_duplicate_query_value_wins() {
        let link = parse("hermes://blueprint/folder%2Fdaily%20brief?slot=old&slot=new")
            .expect("valid deep link");
        assert_eq!(link.name, "folder/daily brief");
        assert_eq!(link.params.get("slot").map(String::as_str), Some("new"));
    }

    #[test]
    fn extracts_only_canonical_scheme_from_process_args() {
        assert_eq!(
            extract_from_args([
                OsString::from("hermes-local.exe"),
                OsString::from("https://example.invalid"),
                OsString::from("hermes://blueprint/test"),
            ]),
            Some("hermes://blueprint/test".to_owned())
        );
        assert_eq!(
            extract_from_args([OsString::from("HERMES://blueprint/test")]),
            None
        );
    }

    #[test]
    fn rejects_wrong_scheme_malformed_escape_controls_and_oversized_input() {
        assert!(parse("https://blueprint/test").is_err());
        assert!(parse("hermes://blueprint/%ZZ").is_err());
        assert!(parse("hermes://blueprint/test\nignored").is_err());
        let oversized = format!("hermes://blueprint/{}", "x".repeat(MAX_DEEP_LINK_BYTES));
        assert!(parse(&oversized).is_err());
    }

    #[test]
    fn activation_queue_round_trips_validated_links_without_shared_file_writes() {
        let root = temp_dir("queue");
        enqueue(&root, "hermes://blueprint/morning?mode=fast").expect("enqueue blueprint");
        enqueue(&root, "hermes://route/notifications").expect("enqueue route");
        let drained = drain_pending(&root).expect("drain");
        assert_eq!(drained.len(), 2);
        assert!(drained.contains(&"hermes://blueprint/morning?mode=fast".to_owned()));
        assert!(drained.contains(&"hermes://route/notifications".to_owned()));
        assert!(drain_pending(&root).expect("empty drain").is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn activation_queue_rejects_invalid_input_before_publishing() {
        let root = temp_dir("invalid");
        assert!(enqueue(&root, "https://example.invalid/").is_err());
        assert!(drain_pending(&root).expect("drain").is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn registry_command_is_exact_and_quotes_url_placeholder() {
        let command =
            registered_command(Path::new(r"C:\Program Files\Hermes Local\hermes-local.exe"))
                .expect("valid command");
        assert_eq!(
            command,
            r#""C:\Program Files\Hermes Local\hermes-local.exe" "%1""#
        );
    }
}
