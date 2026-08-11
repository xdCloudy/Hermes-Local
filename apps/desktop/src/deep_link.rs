use std::{
    collections::BTreeMap,
    ffi::OsStr,
    path::{Path, PathBuf},
};

use url::Url;

const PROTOCOL: &str = "hermes";
const PROTOCOL_KEY: &str = r"HKCU\Software\Classes\hermes";
const COMMAND_KEY: &str = r"HKCU\Software\Classes\hermes\shell\open\command";

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
    if !raw.starts_with("hermes://") {
        return Err("Deep link must use the hermes:// scheme.".to_owned());
    }
    let parsed = Url::parse(raw).map_err(|error| format!("Malformed Hermes deep link: {error}"))?;
    if parsed.scheme() != PROTOCOL {
        return Err("Deep link must use the hermes:// scheme.".to_owned());
    }

    let kind = parsed.host_str().unwrap_or_default().to_owned();
    let encoded_name = parsed.path().strip_prefix('/').unwrap_or(parsed.path());
    let name = percent_decode_path(encoded_name)?;
    let mut params = BTreeMap::new();
    for (key, value) in parsed.query_pairs() {
        params.insert(key.into_owned(), value.into_owned());
    }

    Ok(DeepLink { kind, name, params })
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
    use std::ffi::OsString;

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
    fn rejects_wrong_scheme_and_malformed_percent_escape() {
        assert!(parse("https://blueprint/test").is_err());
        assert!(parse("hermes://blueprint/%ZZ").is_err());
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
