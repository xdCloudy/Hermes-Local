use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

const SCHEMA_VERSION: u32 = 1;
const HELPER_ARGUMENT: &str = "--hermes-update-helper";
const MAX_ATTEMPTS: u32 = 3;
const MAX_BINARY_BYTES: u64 = 512 * 1024 * 1024;
const UNLOCK_TIMEOUT: Duration = Duration::from_secs(30);
const PROBATION: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActivationStatus {
    ReadyToRestart,
    Activating,
    Probation,
    Complete,
    ActivationFailed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingUpdate {
    pub schema_version: u32,
    pub operation_id: String,
    pub status: ActivationStatus,
    pub target_version: String,
    pub expected_sha256: String,
    pub current_exe: PathBuf,
    pub staged_exe: PathBuf,
    pub backup_exe: PathBuf,
    pub helper_exe: PathBuf,
    pub activation_attempts: u32,
    pub staged_at_unix_seconds: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activation_error: Option<String>,
}

/// Handle the copied offline helper before any GUI/native service initialization.
pub fn run_helper_if_requested() -> Option<Result<(), String>> {
    let mut args = std::env::args_os();
    let _program = args.next();
    let Some(mode) = args.next() else {
        return None;
    };
    if mode != HELPER_ARGUMENT {
        return None;
    }
    let Some(plan_path) = args.next().map(PathBuf::from) else {
        return Some(Err("Update helper requires a pending plan path.".into()));
    };
    if args.next().is_some() {
        return Some(Err("Update helper received unexpected arguments.".into()));
    }
    Some(run_helper(&plan_path))
}

/// Start a verified pending activation on launch. The caller must return from
/// `main` when this returns true so the copied helper can replace the executable.
pub fn activate_pending_on_startup() -> Result<bool, String> {
    let root = default_root();
    let plan_path = root.join("pending.json");
    let Some(mut pending) = read_pending(&plan_path)? else {
        return Ok(false);
    };

    if pending.status == ActivationStatus::Complete {
        cleanup_complete(&plan_path, &pending);
        return Ok(false);
    }
    if pending.status == ActivationStatus::Probation {
        return Ok(false);
    }
    if !matches!(
        pending.status,
        ActivationStatus::ReadyToRestart | ActivationStatus::ActivationFailed
    ) || pending.activation_attempts >= MAX_ATTEMPTS
    {
        return Ok(false);
    }

    validate_plan(&root, &pending)?;
    verify_binary(&pending.staged_exe, Some(&pending.expected_sha256))?;
    let current = std::env::current_exe()
        .map_err(|error| format!("Could not resolve current executable: {error}"))?;
    if canonical(&current)? != canonical(&pending.current_exe)? {
        return Err("Pending update targets a different Hermes Local executable.".into());
    }

    copy_synced(&current, &pending.helper_exe)?;
    pending.status = ActivationStatus::Activating;
    pending.activation_attempts += 1;
    pending.activation_error = None;
    write_pending(&plan_path, &pending)?;

    match Command::new(&pending.helper_exe)
        .arg(HELPER_ARGUMENT)
        .arg(&plan_path)
        .spawn()
    {
        Ok(_) => Ok(true),
        Err(error) => {
            pending.status = ActivationStatus::ActivationFailed;
            pending.activation_error = Some(format!("Could not start update helper: {error}"));
            let _ = write_pending(&plan_path, &pending);
            Ok(false)
        }
    }
}

/// Stage a PE candidate into the private update root and bind activation to its
/// exact SHA-256. Future UpdateService/download code can call this directly.
#[allow(dead_code)]
pub fn stage_candidate(candidate: &Path, target_version: &str) -> Result<PendingUpdate, String> {
    let current = std::env::current_exe()
        .map_err(|error| format!("Could not resolve current executable: {error}"))?;
    stage_candidate_at(candidate, &current, &default_root(), target_version)
}

fn stage_candidate_at(
    candidate: &Path,
    current_exe: &Path,
    root: &Path,
    target_version: &str,
) -> Result<PendingUpdate, String> {
    let target_version = target_version.trim();
    if target_version.is_empty()
        || target_version.len() > 128
        || target_version.chars().any(char::is_control)
    {
        return Err("Update target version is invalid.".into());
    }
    require_absolute_file(candidate, "Update candidate")?;
    require_absolute_file(current_exe, "Current executable")?;
    verify_binary(candidate, None)?;

    fs::create_dir_all(root).map_err(|error| format!("Could not create update root: {error}"))?;
    let operation_id = Uuid::new_v4().simple().to_string();
    let operation_root = root.join("operations").join(&operation_id);
    fs::create_dir_all(&operation_root)
        .map_err(|error| format!("Could not create update operation: {error}"))?;

    let staged_exe = operation_root.join("hermes-local.exe");
    let backup_exe = operation_root.join("hermes-local.previous.exe");
    let helper_exe = operation_root.join("hermes-update-helper.exe");
    copy_synced(candidate, &staged_exe)?;
    let expected_sha256 = sha256_file(&staged_exe)?;
    verify_binary(&staged_exe, Some(&expected_sha256))?;

    let pending = PendingUpdate {
        schema_version: SCHEMA_VERSION,
        operation_id,
        status: ActivationStatus::ReadyToRestart,
        target_version: target_version.into(),
        expected_sha256,
        current_exe: current_exe.to_path_buf(),
        staged_exe,
        backup_exe,
        helper_exe,
        activation_attempts: 0,
        staged_at_unix_seconds: unix_seconds(),
        activation_error: None,
    };
    validate_plan(root, &pending)?;
    write_pending(&root.join("pending.json"), &pending)?;
    Ok(pending)
}

fn run_helper(plan_path: &Path) -> Result<(), String> {
    let root = default_root();
    if lexical_absolute(plan_path)? != lexical_absolute(&root.join("pending.json"))? {
        return Err("Update helper refused a plan outside the private update root.".into());
    }
    let mut pending = read_pending(plan_path)?
        .ok_or_else(|| "Pending update disappeared before activation.".to_owned())?;
    validate_plan(&root, &pending)?;
    if pending.status != ActivationStatus::Activating {
        return Err("Pending update is not activating.".into());
    }

    if let Err(error) = promote_offline(&pending) {
        pending.status = ActivationStatus::ActivationFailed;
        pending.activation_error = Some(error.clone());
        let _ = write_pending(plan_path, &pending);
        let _ = rollback_offline(&pending);
        let _ = Command::new(&pending.current_exe).spawn();
        return Err(error);
    }

    pending.status = ActivationStatus::Probation;
    write_pending(plan_path, &pending)?;
    let mut child = match Command::new(&pending.current_exe).spawn() {
        Ok(child) => child,
        Err(error) => {
            let message = format!("Activated executable could not start: {error}");
            rollback_offline(&pending)?;
            pending.status = ActivationStatus::ActivationFailed;
            pending.activation_error = Some(message.clone());
            write_pending(plan_path, &pending)?;
            let _ = Command::new(&pending.current_exe).spawn();
            return Err(message);
        }
    };

    if let Some(status) = wait_for_exit(&mut child, PROBATION)? {
        let message = format!("Activated executable exited during probation: {status}");
        rollback_offline(&pending)?;
        pending.status = ActivationStatus::ActivationFailed;
        pending.activation_error = Some(message.clone());
        write_pending(plan_path, &pending)?;
        let _ = Command::new(&pending.current_exe).spawn();
        return Err(message);
    }

    pending.status = ActivationStatus::Complete;
    pending.activation_error = None;
    write_pending(plan_path, &pending)?;
    let _ = fs::remove_file(&pending.backup_exe);
    let _ = fs::remove_file(&pending.staged_exe);
    Ok(())
}

fn promote_offline(pending: &PendingUpdate) -> Result<(), String> {
    verify_binary(&pending.staged_exe, Some(&pending.expected_sha256))?;
    copy_synced(&pending.current_exe, &pending.backup_exe)?;

    let replacement = pending.current_exe.with_extension("exe.new");
    copy_synced(&pending.staged_exe, &replacement)?;
    verify_binary(&replacement, Some(&pending.expected_sha256))?;

    let deadline = Instant::now() + UNLOCK_TIMEOUT;
    loop {
        match fs::remove_file(&pending.current_exe) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) if Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(250));
                continue;
            }
            Err(error) => {
                let _ = fs::remove_file(&replacement);
                return Err(format!("Timed out waiting for executable unlock: {error}"));
            }
        }

        match fs::rename(&replacement, &pending.current_exe) {
            Ok(()) => break,
            Err(_) if Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(250));
            }
            Err(error) => {
                let _ = rollback_offline(pending);
                return Err(format!("Could not promote staged executable: {error}"));
            }
        }
    }

    if let Err(error) = verify_binary(&pending.current_exe, Some(&pending.expected_sha256)) {
        let _ = rollback_offline(pending);
        return Err(format!("Promoted executable failed verification: {error}"));
    }
    Ok(())
}

fn rollback_offline(pending: &PendingUpdate) -> Result<(), String> {
    if !pending.backup_exe.is_file() {
        return Err("Rollback executable is unavailable.".into());
    }
    let _ = fs::remove_file(&pending.current_exe);
    copy_synced(&pending.backup_exe, &pending.current_exe)
}

fn wait_for_exit(
    child: &mut Child,
    duration: Duration,
) -> Result<Option<std::process::ExitStatus>, String> {
    let deadline = Instant::now() + duration;
    loop {
        match child
            .try_wait()
            .map_err(|error| format!("Could not inspect activated process: {error}"))?
        {
            Some(status) => return Ok(Some(status)),
            None if Instant::now() >= deadline => return Ok(None),
            None => thread::sleep(Duration::from_millis(250)),
        }
    }
}

fn validate_plan(root: &Path, pending: &PendingUpdate) -> Result<(), String> {
    if pending.schema_version != SCHEMA_VERSION {
        return Err("Unsupported pending-update schema.".into());
    }
    if pending.operation_id.len() != 32
        || !pending
            .operation_id
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("Pending update operation id is invalid.".into());
    }
    if pending.expected_sha256.len() != 64
        || !pending
            .expected_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("Pending update SHA-256 is invalid.".into());
    }
    if !pending.current_exe.is_absolute() {
        return Err("Pending update current executable is not absolute.".into());
    }

    let operation_root = lexical_absolute(&root.join("operations").join(&pending.operation_id))?;
    for (label, path) in [
        ("staged executable", &pending.staged_exe),
        ("rollback executable", &pending.backup_exe),
        ("helper executable", &pending.helper_exe),
    ] {
        if !path.is_absolute() || !lexical_absolute(path)?.starts_with(&operation_root) {
            return Err(format!(
                "Pending update {label} escaped its operation directory."
            ));
        }
    }
    Ok(())
}

fn verify_binary(path: &Path, expected_hash: Option<&str>) -> Result<(), String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("Could not inspect {}: {error}", path.display()))?;
    if !metadata.is_file() || metadata.len() < 2 || metadata.len() > MAX_BINARY_BYTES {
        return Err("Update binary size is invalid.".into());
    }
    let mut file = fs::File::open(path)
        .map_err(|error| format!("Could not open {}: {error}", path.display()))?;
    let mut magic = [0_u8; 2];
    file.read_exact(&mut magic)
        .map_err(|error| format!("Could not read PE header: {error}"))?;
    if magic != *b"MZ" {
        return Err("Update binary is not a Windows PE executable.".into());
    }
    if let Some(expected) = expected_hash
        && !sha256_file(path)?.eq_ignore_ascii_case(expected)
    {
        return Err("Update binary SHA-256 does not match the staged plan.".into());
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path)
        .map_err(|error| format!("Could not hash {}: {error}", path.display()))?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("Could not hash {}: {error}", path.display()))?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn copy_synced(source: &Path, destination: &Path) -> Result<(), String> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create update directory: {error}"))?;
    }
    let temporary = destination.with_extension("tmp");
    let _ = fs::remove_file(&temporary);
    fs::copy(source, &temporary)
        .map_err(|error| format!("Could not copy update binary: {error}"))?;
    fs::OpenOptions::new()
        .write(true)
        .open(&temporary)
        .and_then(|file| file.sync_all())
        .map_err(|error| format!("Could not flush update binary: {error}"))?;
    if destination.exists() {
        fs::remove_file(destination)
            .map_err(|error| format!("Could not replace update file: {error}"))?;
    }
    fs::rename(&temporary, destination)
        .map_err(|error| format!("Could not promote copied update file: {error}"))
}

fn read_pending(path: &Path) -> Result<Option<PendingUpdate>, String> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|error| format!("Pending update is invalid: {error}")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("Could not read pending update: {error}")),
    }
}

fn write_pending(path: &Path, pending: &PendingUpdate) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Pending update path has no parent.".to_owned())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Could not create update state directory: {error}"))?;
    let temporary = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(pending)
        .map_err(|error| format!("Could not serialize pending update: {error}"))?;
    {
        let mut file = fs::File::create(&temporary)
            .map_err(|error| format!("Could not create pending update: {error}"))?;
        file.write_all(&bytes)
            .and_then(|_| file.write_all(b"\n"))
            .and_then(|_| file.sync_all())
            .map_err(|error| format!("Could not persist pending update: {error}"))?;
    }
    if path.exists() {
        fs::remove_file(path)
            .map_err(|error| format!("Could not replace pending update: {error}"))?;
    }
    fs::rename(&temporary, path)
        .map_err(|error| format!("Could not promote pending update state: {error}"))
}

fn cleanup_complete(plan_path: &Path, pending: &PendingUpdate) {
    if let Some(operation_root) = pending.helper_exe.parent() {
        let _ = fs::remove_dir_all(operation_root);
    }
    let _ = fs::remove_file(plan_path);
}

fn require_absolute_file(path: &Path, label: &str) -> Result<(), String> {
    if path.is_absolute() && path.is_file() {
        Ok(())
    } else {
        Err(format!("{label} must be an absolute file path."))
    }
}

fn canonical(path: &Path) -> Result<PathBuf, String> {
    path.canonicalize()
        .map_err(|error| format!("Could not resolve {}: {error}", path.display()))
}

fn lexical_absolute(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err(format!("Path must be absolute: {}", path.display()));
    }
    Ok(path.components().collect())
}

fn default_root() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map_or_else(std::env::temp_dir, PathBuf::from)
        .join("Hermes Local")
        .join("updates")
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "hermes-native-update-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    fn fake_pe(path: &Path, payload: &[u8]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut bytes = b"MZ".to_vec();
        bytes.extend_from_slice(payload);
        fs::write(path, bytes).unwrap();
    }

    #[test]
    fn stage_binds_exact_hash_and_private_operation_paths() {
        let root = root("stage");
        let current = root.join("install/hermes-local.exe");
        let candidate = root.join("incoming/hermes-local.exe");
        fake_pe(&current, b"old");
        fake_pe(&candidate, b"new");
        let update_root = root.join("updates");
        let pending = stage_candidate_at(&candidate, &current, &update_root, "0.19.0").unwrap();

        assert_eq!(pending.status, ActivationStatus::ReadyToRestart);
        assert_eq!(
            pending.expected_sha256,
            sha256_file(&pending.staged_exe).unwrap()
        );
        assert!(
            pending
                .staged_exe
                .starts_with(update_root.join("operations"))
        );
        assert_eq!(
            read_pending(&update_root.join("pending.json"))
                .unwrap()
                .unwrap(),
            pending
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn tampered_candidate_is_rejected_before_current_binary_changes() {
        let root = root("tamper");
        let current = root.join("install/hermes-local.exe");
        let candidate = root.join("incoming/hermes-local.exe");
        fake_pe(&current, b"old");
        fake_pe(&candidate, b"new");
        let pending =
            stage_candidate_at(&candidate, &current, &root.join("updates"), "0.19.0").unwrap();
        fake_pe(&pending.staged_exe, b"tampered");

        assert!(promote_offline(&pending).is_err());
        assert_eq!(fs::read(&current).unwrap(), b"MZold");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn promotion_is_exact_and_rollback_restores_previous_binary() {
        let root = root("promote");
        let current = root.join("install/hermes-local.exe");
        let candidate = root.join("incoming/hermes-local.exe");
        fake_pe(&current, b"old-build");
        fake_pe(&candidate, b"new-build");
        let pending =
            stage_candidate_at(&candidate, &current, &root.join("updates"), "0.19.0").unwrap();

        promote_offline(&pending).unwrap();
        assert_eq!(sha256_file(&current).unwrap(), pending.expected_sha256);
        assert_eq!(fs::read(&pending.backup_exe).unwrap(), b"MZold-build");
        rollback_offline(&pending).unwrap();
        assert_eq!(fs::read(&current).unwrap(), b"MZold-build");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn plan_rejects_paths_outside_operation_directory() {
        let root = root("escape");
        fs::create_dir_all(&root).unwrap();
        let operation = "a".repeat(32);
        let pending = PendingUpdate {
            schema_version: SCHEMA_VERSION,
            operation_id: operation.clone(),
            status: ActivationStatus::ReadyToRestart,
            target_version: "0.19.0".into(),
            expected_sha256: "b".repeat(64),
            current_exe: root.join("install/hermes-local.exe"),
            staged_exe: root.join("outside.exe"),
            backup_exe: root.join("operations").join(&operation).join("old.exe"),
            helper_exe: root.join("operations").join(&operation).join("helper.exe"),
            activation_attempts: 0,
            staged_at_unix_seconds: 1,
            activation_error: None,
        };
        assert!(validate_plan(&root, &pending).is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn non_pe_candidate_is_rejected() {
        let root = root("not-pe");
        let current = root.join("install/hermes-local.exe");
        let candidate = root.join("incoming/hermes-local.exe");
        fake_pe(&current, b"old");
        fs::create_dir_all(candidate.parent().unwrap()).unwrap();
        fs::write(&candidate, b"not-a-pe").unwrap();
        assert!(stage_candidate_at(&candidate, &current, &root.join("updates"), "0.19.0").is_err());
        let _ = fs::remove_dir_all(root);
    }
}
