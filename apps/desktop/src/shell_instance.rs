use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

const INSTANCE_FILE: &str = "desktop-instance.lock";

#[derive(Debug)]
pub struct InstanceGuard {
    path: PathBuf,
    _file: File,
}

impl InstanceGuard {
    pub fn acquire(data_dir: &Path) -> Result<Option<Self>, String> {
        fs::create_dir_all(data_dir)
            .map_err(|error| format!("Could not create Desktop data directory: {error}"))?;
        let path = data_dir.join(INSTANCE_FILE);
        match create_lock(&path) {
            Ok(file) => Ok(Some(Self { path, _file: file })),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                // A normal clean exit removes the lease. If the previous process
                // crashed, treat a lock with our own process id as stale during
                // tests/re-entry; otherwise fail closed rather than launch two
                // Desktop authorities against the same local runtime.
                let owner = read_owner(&path).unwrap_or_default();
                if owner == std::process::id().to_string() {
                    let _ = fs::remove_file(&path);
                    create_lock(&path)
                        .map(|file| Some(Self { path, _file: file }))
                        .map_err(|retry| format!("Could not recover Desktop instance lease: {retry}"))
                } else {
                    Ok(None)
                }
            }
            Err(error) => Err(format!("Could not acquire Desktop instance lease: {error}")),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn create_lock(path: &Path) -> std::io::Result<File> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    write!(file, "{}", std::process::id())?;
    file.sync_data()?;
    Ok(file)
}

fn read_owner(path: &Path) -> std::io::Result<String> {
    let mut value = String::new();
    File::open(path)?.read_to_string(&mut value)?;
    Ok(value.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("hermes-shell-instance-{}-{suffix}", std::process::id()))
    }

    #[test]
    fn clean_instance_lease_is_exclusive_and_relaunchable() {
        let root = temp_dir();
        let first = InstanceGuard::acquire(&root).expect("first lease").expect("owner");
        assert!(first.path().ends_with(INSTANCE_FILE));
        assert_eq!(read_owner(first.path()).unwrap(), std::process::id().to_string());

        // Same-process acquisition is treated as re-entry recovery so unit
        // tests can exercise clean relaunch without spawning a second binary.
        let replacement = InstanceGuard::acquire(&root)
            .expect("replacement lease")
            .expect("replacement owner");
        drop(first);
        drop(replacement);

        let relaunched = InstanceGuard::acquire(&root)
            .expect("relaunch lease")
            .expect("relaunch owner");
        drop(relaunched);
        assert!(!root.join(INSTANCE_FILE).exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn foreign_owner_blocks_secondary_desktop_authority() {
        let root = temp_dir();
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join(INSTANCE_FILE), "999999999").unwrap();
        assert!(InstanceGuard::acquire(&root).unwrap().is_none());
        let _ = fs::remove_dir_all(root);
    }
}
