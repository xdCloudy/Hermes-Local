use std::{path::Path, path::PathBuf, sync::Arc};

use hermes_core::{AppServices, PlatformService, ServiceError, ServiceFuture, ServiceResult};

#[cfg(windows)]
use std::process::{Command, Stdio};

pub const WINDOWS_APP_USER_MODEL_ID: &str = "xdCloudy.HermesLocal";
const DEFAULT_NOTIFICATION_ACTION: &str = "hermes://route/notifications";

#[cfg(windows)]
const TOAST_SCRIPT: &str = r#"[Windows.UI.Notifications.ToastNotificationManager,Windows.UI.Notifications,ContentType=WindowsRuntime] > $null; [Windows.UI.Notifications.ToastNotification,Windows.UI.Notifications,ContentType=WindowsRuntime] > $null; [Windows.Data.Xml.Dom.XmlDocument,Windows.Data.Xml.Dom.XmlDocument,ContentType=WindowsRuntime] > $null; $xml=New-Object Windows.Data.Xml.Dom.XmlDocument; $xml.LoadXml($env:HERMES_LOCAL_NOTIFICATION_XML); $toast=[Windows.UI.Notifications.ToastNotification]::new($xml); [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier($env:HERMES_LOCAL_AUMID).Show($toast)"#;

#[cfg(windows)]
const BALLOON_FALLBACK_SCRIPT: &str = r#"Add-Type -AssemblyName System.Windows.Forms; Add-Type -AssemblyName System.Drawing; $icon=New-Object System.Windows.Forms.NotifyIcon; try { $icon.Icon=[System.Drawing.SystemIcons]::Information; $icon.Visible=$true; $icon.BalloonTipIcon=[System.Windows.Forms.ToolTipIcon]::Info; $icon.BalloonTipTitle=$env:HERMES_LOCAL_NOTIFICATION_TITLE; $icon.BalloonTipText=$env:HERMES_LOCAL_NOTIFICATION_BODY; $icon.ShowBalloonTip(5000); Start-Sleep -Milliseconds 5500 } finally { $icon.Visible=$false; $icon.Dispose() }"#;

const MAX_TITLE_CHARS: usize = 96;
const MAX_BODY_CHARS: usize = 240;
const MAX_HELPER_DIAGNOSTICS: usize = 64 * 1024;

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
            Box::pin(async move {
                tokio::task::spawn_blocking(move || notify_windows(&title, &body))
                    .await
                    .map_err(|error| {
                        ServiceError::Platform(format!(
                            "Windows notification worker failed: {error}"
                        ))
                    })?
            })
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

fn xml_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

fn build_toast_xml(title: &str, body: &str) -> String {
    let action = xml_escape(DEFAULT_NOTIFICATION_ACTION);
    let mut text = String::new();
    if !title.is_empty() {
        text.push_str("<text>");
        text.push_str(&xml_escape(title));
        text.push_str("</text>");
    }
    if !body.is_empty() {
        text.push_str("<text>");
        text.push_str(&xml_escape(body));
        text.push_str("</text>");
    }
    format!(
        "<toast launch=\"{action}\" activationType=\"protocol\"><visual><binding template=\"ToastGeneric\">{text}</binding></visual><actions><action content=\"Open Hermes\" arguments=\"{action}\" activationType=\"protocol\"/></actions></toast>"
    )
}

#[cfg(windows)]
fn notify_windows(title: &str, body: &str) -> ServiceResult<bool> {
    if title.is_empty() && body.is_empty() {
        return Err(ServiceError::InvalidInput(
            "Notification title and body cannot both be empty.".into(),
        ));
    }

    let xml = build_toast_xml(title, body);
    match run_toast_helper(&xml) {
        Ok(()) => Ok(true),
        Err(toast_error) => run_balloon_fallback(title, body).map_err(|fallback_error| {
            ServiceError::Platform(format!(
                "Windows app notification failed ({toast_error}); legacy notification fallback also failed ({fallback_error})"
            ))
        }),
    }
}

#[cfg(windows)]
fn run_toast_helper(xml: &str) -> Result<(), String> {
    let output = Command::new(powershell_executable().map_err(|error| error.to_string())?)
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            TOAST_SCRIPT,
        ])
        .env("HERMES_LOCAL_AUMID", WINDOWS_APP_USER_MODEL_ID)
        .env("HERMES_LOCAL_NOTIFICATION_XML", xml)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| format!("could not start toast helper: {error}"))?;
    if output.stderr.len() > MAX_HELPER_DIAGNOSTICS {
        return Err("toast helper returned oversized diagnostics".to_owned());
    }
    if output.status.success() {
        return Ok(());
    }
    let detail = String::from_utf8_lossy(&output.stderr)
        .trim()
        .chars()
        .take(512)
        .collect::<String>();
    if detail.is_empty() {
        Err("toast helper exited unsuccessfully".to_owned())
    } else {
        Err(detail)
    }
}

#[cfg(windows)]
fn run_balloon_fallback(title: &str, body: &str) -> ServiceResult<bool> {
    Command::new(powershell_executable()?)
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            BALLOON_FALLBACK_SCRIPT,
        ])
        .env("HERMES_LOCAL_NOTIFICATION_TITLE", title)
        .env("HERMES_LOCAL_NOTIFICATION_BODY", body)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| {
            ServiceError::Platform(format!(
                "Could not start the Windows notification fallback: {error}"
            ))
        })?;
    Ok(true)
}

#[cfg(windows)]
fn powershell_executable() -> ServiceResult<PathBuf> {
    let root =
        std::env::var_os("SystemRoot").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
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
        .filter(|character| !character.is_control() || matches!(character, '\n' | '\r' | '\t'))
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

    #[test]
    fn toast_xml_escapes_content_and_routes_clicks_through_hermes_protocol() {
        let xml = build_toast_xml("A < B & C", "say \"hello\" & continue");
        assert!(xml.contains("A &lt; B &amp; C"));
        assert!(xml.contains("say &quot;hello&quot; &amp; continue"));
        assert!(xml.contains("activationType=\"protocol\""));
        assert!(xml.contains("hermes://route/notifications"));
        assert!(!xml.contains("A < B"));
    }

    #[test]
    fn app_user_model_id_is_stable_and_shortcut_safe() {
        assert_eq!(WINDOWS_APP_USER_MODEL_ID, "xdCloudy.HermesLocal");
        assert!(WINDOWS_APP_USER_MODEL_ID.len() < 128);
        assert!(!WINDOWS_APP_USER_MODEL_ID.chars().any(char::is_whitespace));
    }

    #[cfg(windows)]
    #[test]
    fn windows_notification_helper_uses_a_trusted_absolute_path() {
        let executable = powershell_executable().expect("Windows PowerShell");
        assert!(executable.is_absolute());
        assert!(executable.is_file());
    }
}
