use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::OnceLock,
    time::{SystemTime, UNIX_EPOCH},
};

use sha2::{Digest, Sha256};

const CRASH_SCHEMA_VERSION: u32 = 1;
const CRASH_DIRECTORY: &str = "crashes";
const LATEST_CRASH: &str = "latest.json";
const MAX_LOCATION_BYTES: usize = 512;

static CRASH_ROOT: OnceLock<PathBuf> = OnceLock::new();

pub fn install(data_dir: &Path) -> Result<(), String> {
    let root = data_dir.join(CRASH_DIRECTORY);
    fs::create_dir_all(&root)
        .map_err(|error| format!("Could not prepare crash diagnostics directory: {error}"))?;
    let _ = CRASH_ROOT.set(root);

    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Some(root) = CRASH_ROOT.get() {
            let _ = record_panic(root, info);
        }
        previous(info);
    }));
    Ok(())
}

fn record_panic(root: &Path, info: &std::panic::PanicHookInfo<'_>) -> Result<(), String> {
    let location = info.location().map(|location| {
        bounded_location(&format!(
            "{}:{}:{}",
            location.file(),
            location.line(),
            location.column()
        ))
    });
    let message_hash = panic_payload_hash(info.payload());
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let document = serde_json::json!({
        "schemaVersion": CRASH_SCHEMA_VERSION,
        "kind": "native-panic",
        "recordedAtUnixSeconds": timestamp,
        "productVersion": env!("CARGO_PKG_VERSION"),
        "location": location,
        "messageSha256": message_hash,
    });
    atomic_write_json(&root.join(LATEST_CRASH), &document)
}

fn panic_payload_hash(payload: &(dyn std::any::Any + Send)) -> String {
    let message = payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("non-string panic payload");
    let mut digest = Sha256::new();
    digest.update(message.as_bytes());
    format!("{:x}", digest.finalize())
}

fn bounded_location(location: &str) -> String {
    if location.len() <= MAX_LOCATION_BYTES {
        return location.to_owned();
    }
    let mut end = MAX_LOCATION_BYTES;
    while !location.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &location[..end])
}

fn atomic_write_json(path: &Path, value: &serde_json::Value) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Crash diagnostic path has no parent directory.".to_owned())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Could not create crash diagnostic directory: {error}"))?;
    let temporary = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("Could not serialize crash diagnostic: {error}"))?;
    {
        let mut file = fs::File::create(&temporary)
            .map_err(|error| format!("Could not create crash diagnostic: {error}"))?;
        file.write_all(&bytes)
            .and_then(|_| file.write_all(b"\n"))
            .and_then(|_| file.sync_all())
            .map_err(|error| format!("Could not persist crash diagnostic: {error}"))?;
    }
    fs::rename(&temporary, path)
        .map_err(|error| format!("Could not promote crash diagnostic: {error}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panic_payload_is_hashed_not_persisted() {
        let secret = String::from("super-secret-token-value");
        let digest = panic_payload_hash(&secret);
        assert_eq!(digest.len(), 64);
        assert!(!digest.contains("secret"));
        assert_eq!(
            digest,
            "b9d93eac086e00bfb430493d162c2cbe11ac0facf3f3ae745f8c8773c9dc3df2"
        );
    }

    #[test]
    fn location_is_bounded_on_utf8_boundary() {
        let input = format!("{}é", "x".repeat(MAX_LOCATION_BYTES));
        let bounded = bounded_location(&input);
        assert!(bounded.ends_with('…'));
        assert!(bounded.len() <= MAX_LOCATION_BYTES + '…'.len_utf8());
    }

    #[test]
    fn atomic_writer_replaces_complete_json() {
        let root = std::env::temp_dir().join(format!(
            "hermes-crash-forensics-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let path = root.join(LATEST_CRASH);
        atomic_write_json(&path, &serde_json::json!({"value": 1})).expect("first write");
        atomic_write_json(&path, &serde_json::json!({"value": 2})).expect("second write");
        let value: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).expect("read record")).expect("valid json");
        assert_eq!(value["value"], 2);
        assert!(!path.with_extension("json.tmp").exists());
        let _ = fs::remove_dir_all(root);
    }
}
