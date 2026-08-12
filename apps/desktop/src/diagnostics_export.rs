#![allow(dead_code)] // AG-13 service foundation; Diagnostics UI/export picker is a later stage.

use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use sha2::{Digest, Sha256};
use uuid::Uuid;

const DIAGNOSTICS_SCHEMA_VERSION: u32 = 1;
const MAX_LOG_TAIL_BYTES: u64 = 2 * 1024 * 1024;
const MAX_LOG_LINES: usize = 200;
const MAX_CRASH_BYTES: u64 = 1024 * 1024;
const LONG_OPAQUE_MIN: usize = 48;
const REDACTED: &str = "[REDACTED]";
const REDACTED_PATH: &str = "[PRIVATE-PATH]";
const REDACTED_TARGET: &str = "[PRIVATE-TARGET]";
const REDACTED_LONG: &str = "[REDACTED-LONG-VALUE]";
const SENSITIVE_KEYS: [&str; 9] = [
    "authorization",
    "bearer",
    "token",
    "api_key",
    "api-key",
    "apikey",
    "password",
    "passphrase",
    "secret",
];
const SAFE_LOGS: [(&str, &str); 4] = [
    ("supervisor", "logs/supervisor/supervisor.log"),
    ("setup", "logs/setup/setup.log"),
    ("launcher", "logs/launcher/launcher.log"),
    ("security", "logs/security/security.log"),
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiagnosticExport {
    pub report_path: PathBuf,
    pub checksum_path: PathBuf,
    pub sha256: String,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DiagnosticsExportService;

impl DiagnosticsExportService {
    /// Write a privacy-bounded support report and SHA-256 sidecar.
    ///
    /// Only allowlisted log tails and the already-sanitized native crash record
    /// are read. Environment values, conversations, arbitrary project files and
    /// credentials are never collected. `forbidden_secrets` adds a final exact
    /// leak check for live high-entropy values known by the caller.
    pub fn export(
        &self,
        data_dir: &Path,
        destination_dir: &Path,
        forbidden_secrets: &[&str],
    ) -> Result<DiagnosticExport, String> {
        let data_dir = canonical_directory(data_dir, "Hermes Local data")?;
        let destination_dir = prepare_destination(destination_dir)?;
        let private_roots = private_roots(&data_dir);

        let mut safe_logs = BTreeMap::<String, Vec<String>>::new();
        for (name, relative) in SAFE_LOGS {
            safe_logs.insert(
                name.to_owned(),
                read_safe_log_tail(&data_dir, Path::new(relative), &private_roots)?,
            );
        }
        let crash = read_safe_crash_record(&data_dir, &private_roots)?;
        let environment = crate::platform_diagnostics::inspect();
        let generated_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let report = serde_json::json!({
            "schemaVersion": DIAGNOSTICS_SCHEMA_VERSION,
            "generatedAtUnixSeconds": generated_at.as_secs(),
            "privacy": {
                "tokens": "redacted",
                "passwords": "redacted",
                "credentialedUrls": "redacted",
                "privateTargets": "redacted",
                "privatePaths": "redacted",
                "environmentValues": "omitted",
                "conversations": "omitted",
                "privateFiles": "omitted"
            },
            "productVersion": env!("CARGO_PKG_VERSION"),
            "environment": {
                "pathEntryCount": environment.path_entries.len(),
                "proxyConfigured": environment.proxy_configured,
                "customCaConfigured": environment.custom_ca_configured,
                "wsl": environment.wsl,
                "displayConfigured": environment.display_configured,
                "waylandConfigured": environment.wayland_configured,
                "appdataConfigured": environment.appdata_configured,
                "localappdataConfigured": environment.localappdata_configured,
                "tempConfigured": environment.temp_configured
            },
            "safeLogs": safe_logs,
            "crash": crash
        });
        let mut bytes = serde_json::to_vec_pretty(&report)
            .map_err(|error| format!("Could not serialize diagnostics report: {error}"))?;
        bytes.push(b'\n');
        ensure_forbidden_values_absent(&bytes, forbidden_secrets)?;

        let identifier = Uuid::new_v4().simple().to_string();
        let report_path = destination_dir.join(format!(
            "Hermes-Local-Diagnostics-{}-{identifier}.json",
            generated_at.as_secs()
        ));
        let checksum_path = report_path.with_extension("json.sha256");
        let sha256 = sha256_hex(&bytes);
        write_atomic(&report_path, &bytes)?;
        let file_name = report_path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| "Diagnostic report filename was not valid UTF-8.".to_owned())?;
        write_atomic(
            &checksum_path,
            format!("{sha256}  {file_name}\n").as_bytes(),
        )?;

        Ok(DiagnosticExport {
            report_path,
            checksum_path,
            sha256,
        })
    }
}

fn canonical_directory(path: &Path, label: &str) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err(format!("{label} directory must be absolute."));
    }
    let path = path
        .canonicalize()
        .map_err(|error| format!("Could not resolve {label} directory: {error}"))?;
    if !path.is_dir() {
        return Err(format!("{label} path must be a directory."));
    }
    Ok(path)
}

fn prepare_destination(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err("Diagnostic export destination must be absolute.".to_owned());
    }
    fs::create_dir_all(path)
        .map_err(|error| format!("Could not create diagnostic export directory: {error}"))?;
    canonical_directory(path, "Diagnostic export")
}

fn private_roots(data_dir: &Path) -> Vec<String> {
    let mut roots = vec![data_dir.to_string_lossy().into_owned()];
    for variable in [
        "USERPROFILE",
        "HOME",
        "HERMES_LOCAL_ROOT",
        "APPDATA",
        "LOCALAPPDATA",
    ] {
        if let Some(value) = std::env::var_os(variable) {
            let value = PathBuf::from(value);
            if value.is_absolute() {
                roots.push(value.to_string_lossy().into_owned());
            }
        }
    }
    roots.sort_by_key(|root| std::cmp::Reverse(root.len()));
    roots.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    roots
}

fn read_safe_log_tail(
    root: &Path,
    relative: &Path,
    private_roots: &[String],
) -> Result<Vec<String>, String> {
    let path = root.join(relative);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("Could not inspect diagnostic log: {error}")),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!(
            "Diagnostic log is not a regular non-symlink file: {}",
            relative.display()
        ));
    }

    let mut file =
        fs::File::open(&path).map_err(|error| format!("Could not open diagnostic log: {error}"))?;
    let start = metadata.len().saturating_sub(MAX_LOG_TAIL_BYTES);
    file.seek(SeekFrom::Start(start))
        .map_err(|error| format!("Could not seek diagnostic log: {error}"))?;
    let mut bytes = Vec::with_capacity((metadata.len() - start).min(MAX_LOG_TAIL_BYTES) as usize);
    file.take(MAX_LOG_TAIL_BYTES)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Could not read diagnostic log: {error}"))?;
    let text = String::from_utf8_lossy(&bytes);
    let mut lines = text
        .lines()
        .rev()
        .take(MAX_LOG_LINES)
        .map(|line| sanitize_log_line(&redact_sensitive_text(line, private_roots)))
        .collect::<Vec<_>>();
    lines.reverse();
    Ok(lines)
}

fn read_safe_crash_record(
    root: &Path,
    private_roots: &[String],
) -> Result<Option<serde_json::Value>, String> {
    let relative = Path::new("crashes/latest.json");
    let path = root.join(relative);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Could not inspect crash diagnostic: {error}")),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > MAX_CRASH_BYTES
    {
        return Err("Crash diagnostic is not a bounded regular file.".to_owned());
    }
    let bytes =
        fs::read(&path).map_err(|error| format!("Could not read crash diagnostic: {error}"))?;
    let mut value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Crash diagnostic was not valid JSON: {error}"))?;
    redact_json_strings(&mut value, private_roots);
    Ok(Some(value))
}

fn redact_json_strings(value: &mut serde_json::Value, private_roots: &[String]) {
    match value {
        serde_json::Value::String(text) => {
            *text = redact_sensitive_text(text, private_roots);
        }
        serde_json::Value::Array(values) => {
            for value in values {
                redact_json_strings(value, private_roots);
            }
        }
        serde_json::Value::Object(values) => {
            for value in values.values_mut() {
                redact_json_strings(value, private_roots);
            }
        }
        _ => {}
    }
}

fn redact_sensitive_text(input: &str, private_roots: &[String]) -> String {
    let prefixed = redact_prefixed_tokens(input);
    let assignments = SENSITIVE_KEYS
        .iter()
        .fold(prefixed, |text, key| redact_key_value(&text, key));
    let urls = redact_credentialed_urls(&assignments);
    let paths = redact_private_paths(&urls, private_roots);
    let targets = redact_private_ipv4(&paths);
    redact_long_opaque_values(&targets)
}

fn redact_prefixed_tokens(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0;
    let mut copied = 0;
    while cursor + 3 <= bytes.len() {
        let prefix = &bytes[cursor..cursor + 3];
        let matches = prefix[2] == b'-'
            && matches!(prefix[0].to_ascii_lowercase(), b's' | b'p' | b'r')
            && prefix[1].to_ascii_lowercase() == b'k';
        if !matches {
            cursor += 1;
            continue;
        }
        let mut end = cursor + 3;
        while end < bytes.len() && is_token_byte(bytes[end]) {
            end += 1;
        }
        if end - (cursor + 3) < 8 {
            cursor += 1;
            continue;
        }
        output.push_str(&input[copied..cursor]);
        output.push_str(&input[cursor..cursor + 2]);
        output.push('=');
        output.push_str(REDACTED);
        copied = end;
        cursor = end;
    }
    output.push_str(&input[copied..]);
    output
}

fn redact_key_value(input: &str, key: &str) -> String {
    let bytes = input.as_bytes();
    let key_bytes = key.as_bytes();
    let mut output = String::with_capacity(input.len());
    let mut search = 0;
    let mut copied = 0;

    while search + key_bytes.len() <= bytes.len() {
        if !bytes[search..search + key_bytes.len()].eq_ignore_ascii_case(key_bytes)
            || (search > 0 && is_identifier_byte(bytes[search - 1]))
        {
            search += 1;
            continue;
        }
        let after_key = search + key_bytes.len();
        if after_key < bytes.len() && is_identifier_byte(bytes[after_key]) {
            search += 1;
            continue;
        }

        let mut cursor = after_key;
        if cursor < bytes.len() && matches!(bytes[cursor], b'\'' | b'"') {
            cursor += 1;
        }
        let whitespace_start = cursor;
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        let had_whitespace = cursor > whitespace_start;
        if cursor < bytes.len() && matches!(bytes[cursor], b':' | b'=') {
            cursor += 1;
            while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
                cursor += 1;
            }
        } else if !had_whitespace {
            search += 1;
            continue;
        }
        if cursor < bytes.len() && matches!(bytes[cursor], b'\'' | b'"') {
            cursor += 1;
        }
        let value_start = cursor;
        while cursor < bytes.len() && !is_value_delimiter(bytes[cursor]) {
            cursor += 1;
        }
        if cursor == value_start {
            search += 1;
            continue;
        }

        output.push_str(&input[copied..value_start]);
        output.push_str(REDACTED);
        copied = cursor;
        search = cursor;
    }
    output.push_str(&input[copied..]);
    output
}

fn redact_credentialed_urls(input: &str) -> String {
    let bytes = input.as_bytes();
    let schemes: [&[u8]; 4] = [b"https://", b"http://", b"wss://", b"ws://"];
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0;
    let mut copied = 0;

    while cursor < bytes.len() {
        let Some((scheme_start, scheme_len)) = next_scheme(bytes, cursor, &schemes) else {
            break;
        };
        let authority_start = scheme_start + scheme_len;
        let mut authority_end = authority_start;
        while authority_end < bytes.len()
            && !bytes[authority_end].is_ascii_whitespace()
            && !matches!(bytes[authority_end], b'/' | b'?' | b'#')
        {
            authority_end += 1;
        }
        let authority = &bytes[authority_start..authority_end];
        let at = authority.iter().position(|byte| *byte == b'@');
        let userinfo_colon =
            at.and_then(|at| authority[..at].iter().position(|byte| *byte == b':'));
        if let (Some(at), Some(_)) = (at, userinfo_colon) {
            output.push_str(&input[copied..authority_start]);
            output.push_str(REDACTED);
            output.push('@');
            copied = authority_start + at + 1;
            cursor = copied;
        } else {
            cursor = authority_end.max(authority_start + 1);
        }
    }
    output.push_str(&input[copied..]);
    output
}

fn next_scheme(bytes: &[u8], start: usize, schemes: &[&[u8]]) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize)> = None;
    for index in start..bytes.len() {
        for scheme in schemes {
            if index + scheme.len() <= bytes.len()
                && bytes[index..index + scheme.len()].eq_ignore_ascii_case(scheme)
                && best.is_none_or(|(current, _)| index < current)
            {
                best = Some((index, scheme.len()));
            }
        }
        if best.is_some_and(|(current, _)| current == index) {
            break;
        }
    }
    best
}

fn redact_private_paths(input: &str, private_roots: &[String]) -> String {
    let mut redacted = input.to_owned();
    for root in private_roots.iter().filter(|root| root.len() >= 3) {
        redacted = replace_ascii_case_insensitive(&redacted, root, REDACTED_PATH);
        let forward = root.replace('\\', "/");
        if forward != *root {
            redacted = replace_ascii_case_insensitive(&redacted, &forward, REDACTED_PATH);
        }
        let backward = root.replace('/', "\\");
        if backward != *root {
            redacted = replace_ascii_case_insensitive(&redacted, &backward, REDACTED_PATH);
        }
    }
    redacted
}

fn replace_ascii_case_insensitive(input: &str, needle: &str, replacement: &str) -> String {
    if needle.is_empty() || needle.len() > input.len() {
        return input.to_owned();
    }
    let bytes = input.as_bytes();
    let needle = needle.as_bytes();
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0;
    let mut copied = 0;
    while cursor + needle.len() <= bytes.len() {
        if bytes[cursor..cursor + needle.len()].eq_ignore_ascii_case(needle) {
            output.push_str(&input[copied..cursor]);
            output.push_str(replacement);
            cursor += needle.len();
            copied = cursor;
        } else {
            cursor += 1;
        }
    }
    output.push_str(&input[copied..]);
    output
}

fn redact_private_ipv4(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0;
    let mut copied = 0;
    while cursor < bytes.len() {
        if !bytes[cursor].is_ascii_digit()
            || (cursor > 0 && matches!(bytes[cursor - 1], b'0'..=b'9' | b'.'))
        {
            cursor += 1;
            continue;
        }
        let mut end = cursor;
        while end < bytes.len() && matches!(bytes[end], b'0'..=b'9' | b'.') {
            end += 1;
        }
        let bounded = end == bytes.len() || !matches!(bytes[end], b'0'..=b'9' | b'.');
        if bounded && is_private_ipv4(&input[cursor..end]) {
            output.push_str(&input[copied..cursor]);
            output.push_str(REDACTED_TARGET);
            copied = end;
        }
        cursor = end.max(cursor + 1);
    }
    output.push_str(&input[copied..]);
    output
}

fn is_private_ipv4(candidate: &str) -> bool {
    let octets = candidate
        .split('.')
        .map(str::parse::<u8>)
        .collect::<Result<Vec<_>, _>>();
    let Ok(octets) = octets else {
        return false;
    };
    if octets.len() != 4 {
        return false;
    }
    match octets.as_slice() {
        [127, ..] | [10, ..] | [192, 168, ..] => true,
        [172, second, ..] => (16..=31).contains(second),
        _ => false,
    }
}

fn redact_long_opaque_values(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0;
    let mut copied = 0;
    while cursor < bytes.len() {
        if !is_opaque_byte(bytes[cursor]) {
            cursor += 1;
            continue;
        }
        let start = cursor;
        while cursor < bytes.len() && is_opaque_byte(bytes[cursor]) {
            cursor += 1;
        }
        if cursor - start >= LONG_OPAQUE_MIN {
            output.push_str(&input[copied..start]);
            output.push_str(REDACTED_LONG);
            copied = cursor;
        }
    }
    output.push_str(&input[copied..]);
    output
}

fn is_token_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')
}

fn is_opaque_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'=' | b'_' | b'-')
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')
}

fn is_value_delimiter(byte: u8) -> bool {
    byte.is_ascii_whitespace() || matches!(byte, b',' | b';' | b'\'' | b'"' | b'}' | b']')
}

fn sanitize_log_line(line: &str) -> String {
    line.chars()
        .map(|character| {
            if character == '\t' || !character.is_control() {
                character
            } else {
                '�'
            }
        })
        .collect()
}

fn ensure_forbidden_values_absent(bytes: &[u8], forbidden: &[&str]) -> Result<(), String> {
    for secret in forbidden.iter().copied().filter(|secret| secret.len() >= 8) {
        if bytes
            .windows(secret.len())
            .any(|window| window == secret.as_bytes())
        {
            return Err(
                "Diagnostic privacy validation found a forbidden secret in the export.".to_owned(),
            );
        }
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Diagnostic export path has no parent directory.".to_owned())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Could not create diagnostic export directory: {error}"))?;
    let temporary = path.with_extension(format!(
        "{}.tmp",
        path.extension()
            .and_then(|value| value.to_str())
            .unwrap_or("tmp")
    ));
    {
        let mut file = fs::File::create(&temporary)
            .map_err(|error| format!("Could not create diagnostic export: {error}"))?;
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|error| format!("Could not persist diagnostic export: {error}"))?;
    }
    fs::rename(&temporary, path)
        .map_err(|error| format!("Could not promote diagnostic export: {error}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redaction_covers_credentials_urls_private_targets_paths_and_long_values() {
        let private_root = if cfg!(windows) {
            r"C:\Users\Cloudy\Hermes"
        } else {
            "/home/cloudy/Hermes"
        };
        let opaque = "A".repeat(LONG_OPAQUE_MIN);
        let input = format!(
            "sk-abcdefghijk Bearer bearer-secret token=token-secret api_key='api-secret' \
             https://user:pass@example.com/path http://visible.example/path \
             127.0.0.1 10.1.2.3 192.168.4.5 172.31.8.9 8.8.8.8 \
             {private_root}/private.txt {opaque}"
        );
        let redacted = redact_sensitive_text(&input, &[private_root.to_owned()]);
        for secret in [
            "abcdefghijk",
            "bearer-secret",
            "token-secret",
            "api-secret",
            "user:pass",
            "127.0.0.1",
            "10.1.2.3",
            "192.168.4.5",
            "172.31.8.9",
            private_root,
            opaque.as_str(),
        ] {
            assert!(
                !redacted.contains(secret),
                "private value remained: {secret}"
            );
        }
        assert!(redacted.contains("http://visible.example/path"));
        assert!(redacted.contains("8.8.8.8"));
        assert!(redacted.contains(REDACTED_PATH));
        assert!(redacted.contains(REDACTED_TARGET));
        assert!(redacted.contains(REDACTED_LONG));
    }

    #[test]
    fn safe_log_tail_is_bounded_ordered_and_redacted() {
        let root = test_directory("tail");
        let path = root.join("logs/supervisor/supervisor.log");
        fs::create_dir_all(path.parent().expect("log parent")).expect("log directory");
        let mut content = String::new();
        for index in 0..250 {
            if index == 249 {
                content.push_str("token=top-secret-token\n");
            } else {
                content.push_str(&format!("line-{index}\n"));
            }
        }
        fs::write(&path, content).expect("log fixture");
        let lines = read_safe_log_tail(
            &root,
            Path::new("logs/supervisor/supervisor.log"),
            &[root.to_string_lossy().into_owned()],
        )
        .expect("read safe tail");
        assert_eq!(lines.len(), MAX_LOG_LINES);
        assert_eq!(lines.first().map(String::as_str), Some("line-50"));
        assert!(lines.last().is_some_and(|line| line.contains(REDACTED)));
        assert!(!lines.iter().any(|line| line.contains("top-secret-token")));
        cleanup(root);
    }

    #[test]
    fn export_uses_allowlist_redaction_and_sha256_sidecar() {
        let data = test_directory("export-data");
        let destination = test_directory("export-destination");
        let log = data.join("logs/security/security.log");
        fs::create_dir_all(log.parent().expect("log parent")).expect("log directory");
        let opaque = "B".repeat(LONG_OPAQUE_MIN + 8);
        fs::write(
            &log,
            format!(
                "api-key=live-secret-123456 path={} target=192.168.10.2 \
                 url=https://alice:password@example.com/x opaque={opaque}\n",
                data.display()
            ),
        )
        .expect("safe log fixture");
        fs::write(data.join("private-conversation.txt"), "do-not-export-this")
            .expect("private file");
        fs::create_dir_all(data.join("crashes")).expect("crash directory");
        let crash_fixture = serde_json::json!({
            "schemaVersion": 1,
            "location": data.to_string_lossy(),
            "message": "token=crash-secret-123456 10.0.0.7"
        });
        fs::write(
            data.join("crashes/latest.json"),
            serde_json::to_vec(&crash_fixture).expect("serialize crash fixture"),
        )
        .expect("crash fixture");

        let export = DiagnosticsExportService
            .export(&data, &destination, &["live-secret-123456"])
            .expect("diagnostic export");
        let bytes = fs::read(&export.report_path).expect("report");
        let text = String::from_utf8(bytes.clone()).expect("UTF-8 report");
        let data_text = data.to_string_lossy().into_owned();
        for private in [
            "live-secret-123456",
            "crash-secret-123456",
            "do-not-export-this",
            "192.168.10.2",
            "10.0.0.7",
            "alice:password",
            opaque.as_str(),
            data_text.as_str(),
        ] {
            assert!(
                !text.contains(private),
                "private export value remained: {private}"
            );
        }
        assert!(!text.contains("pathEntries\""));
        assert!(text.contains("pathEntryCount"));
        assert!(text.contains(REDACTED_PATH));
        assert!(text.contains(REDACTED_TARGET));
        assert!(text.contains(REDACTED_LONG));
        assert_eq!(export.sha256, sha256_hex(&bytes));
        let sidecar = fs::read_to_string(&export.checksum_path).expect("sidecar");
        assert!(sidecar.starts_with(&export.sha256));
        assert!(
            sidecar.contains(
                export
                    .report_path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .expect("report filename")
            )
        );
        cleanup(data);
        cleanup(destination);
    }

    #[test]
    fn exact_forbidden_secret_check_blocks_unlabelled_leaks() {
        let data = test_directory("forbidden-data");
        let destination = test_directory("forbidden-destination");
        let log = data.join("logs/launcher/launcher.log");
        fs::create_dir_all(log.parent().expect("log parent")).expect("log directory");
        fs::write(&log, "opaque-live-secret-987654\n").expect("unsafe log fixture");

        let result =
            DiagnosticsExportService.export(&data, &destination, &["opaque-live-secret-987654"]);
        assert!(result.is_err());
        assert!(
            fs::read_dir(&destination)
                .expect("destination listing")
                .next()
                .is_none()
        );
        cleanup(data);
        cleanup(destination);
    }

    fn test_directory(label: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "hermes-diagnostics-{label}-{}-{suffix}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).expect("test directory");
        directory.canonicalize().expect("canonical test directory")
    }

    fn cleanup(path: PathBuf) {
        let _ = fs::remove_dir_all(path);
    }
}
