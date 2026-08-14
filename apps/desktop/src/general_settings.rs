use std::{path::Path, sync::Arc};

use hermes_core::{AppServices, DesktopSettingsService, ServiceError, ServiceFuture};
use hermes_protocol::{DesktopGeneralStatus, DesktopLoginStatus, DesktopPowerStatus};

use crate::{login_item, power::KeepAwakeService};

pub fn install(services: &mut AppServices, data_dir: &Path) {
    let power = Arc::new(KeepAwakeService::new());
    if persisted_keep_awake(data_dir) {
        if let Err(error) = power.set(true) {
            eprintln!("Hermes Local could not restore keep-awake: {error}");
        }
    }
    services.desktop_settings = Arc::new(DesktopGeneralSettings { power });
}

fn persisted_keep_awake(data_dir: &Path) -> bool {
    const MAX_SETTINGS_BYTES: u64 = 1024 * 1024;
    let path = data_dir.join("settings.json");
    let Ok(metadata) = std::fs::metadata(&path) else {
        return false;
    };
    if metadata.len() > MAX_SETTINGS_BYTES {
        return false;
    }
    std::fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<hermes_protocol::AppSettings>(&bytes).ok())
        .is_some_and(|settings| settings.keep_awake)
}

struct DesktopGeneralSettings {
    power: Arc<KeepAwakeService>,
}

impl DesktopGeneralSettings {
    fn snapshot(power: &KeepAwakeService) -> Result<DesktopGeneralStatus, ServiceError> {
        let login = login_item::status().map_err(ServiceError::Platform)?;
        let (on_ac_power, battery_percent) = battery_status().unwrap_or((None, None));
        Ok(DesktopGeneralStatus {
            power: DesktopPowerStatus {
                available: power.available,
                keep_awake: power.is_active(),
                on_ac_power,
                battery_percent,
            },
            login: DesktopLoginStatus {
                available: login.available,
                enabled: login.enabled,
                executable: login.executable.to_string_lossy().into_owned(),
            },
        })
    }
}

impl DesktopSettingsService for DesktopGeneralSettings {
    fn status(&self) -> ServiceFuture<'_, DesktopGeneralStatus> {
        let power = self.power.clone();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || Self::snapshot(&power))
                .await
                .map_err(|error| ServiceError::Platform(error.to_string()))?
        })
    }

    fn set_keep_awake(&self, enabled: bool) -> ServiceFuture<'_, DesktopGeneralStatus> {
        let power = self.power.clone();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || {
                power.set(enabled).map_err(ServiceError::Platform)?;
                Self::snapshot(&power)
            })
            .await
            .map_err(|error| ServiceError::Platform(error.to_string()))?
        })
    }

    fn set_launch_at_login(&self, enabled: bool) -> ServiceFuture<'_, DesktopGeneralStatus> {
        let power = self.power.clone();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || {
                login_item::set_enabled(enabled).map_err(ServiceError::Platform)?;
                Self::snapshot(&power)
            })
            .await
            .map_err(|error| ServiceError::Platform(error.to_string()))?
        })
    }
}

#[cfg(windows)]
fn battery_status() -> Result<(Option<bool>, Option<u8>), String> {
    use std::process::{Command, Stdio};

    const SCRIPT: &str = r#"Add-Type -TypeDefinition 'using System.Runtime.InteropServices; public static class HermesPowerStatus { [StructLayout(LayoutKind.Sequential)] public struct S { public byte AC; public byte Flag; public byte Percent; public byte Reserved; public uint Life; public uint Full; } [DllImport("kernel32.dll")] public static extern bool GetSystemPowerStatus(out S status); }'; $s=New-Object HermesPowerStatus+S; if(-not [HermesPowerStatus]::GetSystemPowerStatus([ref]$s)){exit 42}; [Console]::Out.Write("AC={0};BAT={1}" -f $s.AC,$s.Percent)"#;
    let root = std::env::var_os("SystemRoot").map_or_else(
        || std::path::PathBuf::from(r"C:\Windows"),
        std::path::PathBuf::from,
    );
    let powershell = root
        .join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe");
    if !powershell.is_absolute() || !powershell.is_file() {
        return Err("Windows PowerShell is unavailable".into());
    }
    let output = Command::new(powershell)
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            SCRIPT,
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|error| format!("Could not query power state: {error}"))?;
    if !output.status.success() || output.stdout.len() > 64 {
        return Err("Windows power-state query failed".into());
    }
    parse_battery_status(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(not(windows))]
fn battery_status() -> Result<(Option<bool>, Option<u8>), String> {
    Ok((None, None))
}

fn parse_battery_status(value: &str) -> Result<(Option<bool>, Option<u8>), String> {
    let mut ac = None;
    let mut battery = None;
    for field in value.trim().split(';') {
        let Some((key, value)) = field.split_once('=') else {
            continue;
        };
        match key {
            "AC" => {
                ac = match value {
                    "0" => Some(false),
                    "1" => Some(true),
                    _ => None,
                }
            }
            "BAT" => {
                battery = value.parse::<u8>().ok().filter(|percent| *percent <= 100);
            }
            _ => {}
        }
    }
    if ac.is_none() && battery.is_none() {
        Err("Windows returned an invalid power-state response".into())
    } else {
        Ok((ac, battery))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bounded_windows_power_state() {
        assert_eq!(
            parse_battery_status("AC=1;BAT=73"),
            Ok((Some(true), Some(73)))
        );
        assert_eq!(
            parse_battery_status("AC=0;BAT=255"),
            Ok((Some(false), None))
        );
        assert!(parse_battery_status("garbage").is_err());
    }

    #[test]
    fn persisted_keep_awake_is_bounded_and_fail_closed() {
        let directory = std::env::temp_dir().join(format!(
            "hermes-general-settings-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&directory).expect("directory");
        std::fs::write(directory.join("settings.json"), br#"{"keep_awake":true}"#)
            .expect("settings");
        assert!(persisted_keep_awake(&directory));
        std::fs::write(directory.join("settings.json"), b"not json").expect("invalid settings");
        assert!(!persisted_keep_awake(&directory));
        std::fs::remove_dir_all(directory).expect("cleanup");
    }
}
