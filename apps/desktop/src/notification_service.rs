use std::{path::Path, path::PathBuf, sync::Arc};

use hermes_core::{
    AppServices, PlatformService, ServiceError, ServiceFuture, ServiceResult,
};

#[cfg(windows)]
use std::process::{Command, Stdio};

#[cfg(windows)]
const NOTIFICATION_SCRIPT: &str = r#"Add-Type -AssemblyName System.Windows.Forms; Add-Type -AssemblyName System.Drawing; $icon=New-Object System.Windows.Forms.NotifyIcon; try { $icon.Icon=[System.Drawing.SystemIcons]::Information; $icon.Visible=$true; $icon.BalloonTipIcon=[System.Windows.Forms.ToolTipIcon]::Info; $icon.BalloonTipTitle=$env:HERMES_LOCAL_NOTIFICATION_TITLE; $icon.BalloonTipText=$env:HERMES_LOCAL_NOTIFICATION_BODY; $icon.ShowBalloonTip(5000); Start-Sleep -Milliseconds 5500 } finally { $icon.Visible=$false; $icon.Dispose() }"#;

const MAX_TITLE_CHARS: usize = 96;
const MAX_BODY_CHARS: usize = 240;

pub fn install(services: &mut AppServices) {
    let inner = services.platform.clone();
    services.platform = Arc::new(NotificationPlatform { inner });
}

struct NotificationPlatform {
    inner: Arc<dyn PlatformService>,
}

impl PlatformService for NotificationPlatform {
    fn pick_folder(
        &self,
        title: &str,
        starting_directory: Option<&Path>,
    ) -> ServiceFuture<'_, Option<PathBuf>> {
        self.inner.pick_folder(title, starting_directory)
    }

    fn open_external(&self, url: &str) -> ServiceFuture<'_, ()> {
        self.inner.open_external(url)
    }

    fn notify(&self, title: &str, body: &str) -> ServiceFuture<'_, bool> {
        #[cfg(windows)]
        {
            let title = sanitize_title(title);
            let body = sanitize_body(body);
            Box::pin(async move { notify_windows(&title, &body) })
        }

        #[cfg(not(windows))]
        {
            self.inner.notify(title, body)
        }
    }

    fn version(&self) -> ServiceFuture<'_, String> {
        self.inner.version()
    }
}

#[cfg(windows)]
fn notify_windows(title: &str, body: &str) -> ServiceResult<bool> {
    if title.is_empty() && body.is_empty() {
        return Err(ServiceError::InvalidInput(
            "Notification title and body cannot both be empty.".into(),
        ));
    }

    let powershell = powershell_executable()?;
    Command::new(powershell)
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            NOTIFICATION_SCRIPT,
        ])
        .env("HERMES_LOCAL_NOTIFICATION_TITLE", title)
        .env("HERMES_LOCAL_NOTIFICATION_BODY", body)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| {
            ServiceError::Platform(format!(
                "Could not start the Windows notification helper: {error}"
            ))
        })?;
    Ok(true)
}

#[cfg(windows)]
fn powershell_executable() -> ServiceResult<PathBuf> {
    let root = std::env::var_os("SystemRoot")
        .map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
    let executable = root
        .join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe");
    if executable.is_absolute() && executable.is_file() {
        Ok(executable)
    } else {
        Err(ServiceError::Unavailable(format!(
            "Windows PowerShell notification helper is unavailable: {}",
            executable.display()
        )))
    }
}

fn sanitize_title(value: &str) -> String {
    bounded_chars(
        value
            .chars()
            .map(|character| {
                if character.is_control() {
                    ' '
                } else {
                    character
                }
            })
            .collect::<String>()
            .trim(),
        MAX_TITLE_CHARS,
    )
}

fn sanitize_body(value: &str) -> String {
    let cleaned = value
        .chars()
        .filter(|character| {
            !character.is_control() || matches!(character, '\n' | '\r' | '\t')
        })
        .collect::<String>();
    bounded_chars(cleaned.trim(), MAX_BODY_CHARS)
}

fn bounded_chars(value: &str, limit: usize) -> String {
    value.chars().take(limit).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_is_bounded_and_control_characters_are_not_forwarded() {
        let value = format!("Hermes\n{}", "x".repeat(200));
        let sanitized = sanitize_title(&value);
        assert!(!sanitized.contains('\n'));
        assert!(sanitized.chars().count() <= MAX_TITLE_CHARS);
    }

    #[test]
    fn body_keeps_readable_whitespace_but_removes_other_controls() {
        let sanitized = sanitize_body("line one\nline two\tvalue\0secret");
        assert_eq!(sanitized, "line one\nline two\tvaluesecret");
        assert!(!sanitized.contains('\0'));
    }

    #[test]
    fn unicode_bounds_are_character_safe() {
        let sanitized = bounded_chars(&"é".repeat(MAX_BODY_CHARS + 10), MAX_BODY_CHARS);
        assert_eq!(sanitized.chars().count(), MAX_BODY_CHARS);
        assert!(sanitized.is_char_boundary(sanitized.len()));
    }

    #[cfg(windows)]
    #[test]
    fn windows_notification_helper_uses_a_trusted_absolute_path() {
        let executable = powershell_executable().expect("Windows PowerShell");
        assert!(executable.is_absolute());
        assert!(executable.is_file());
    }
}
