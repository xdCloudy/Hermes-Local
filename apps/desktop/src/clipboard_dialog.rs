use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_CLIPBOARD_TEXT_BYTES: usize = 4 * 1024 * 1024;
const MAX_IMAGE_BYTES: usize = 32 * 1024 * 1024;
const MAX_SAVE_BYTES: usize = 64 * 1024 * 1024;
const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

#[cfg(windows)]
const WRITE_TEXT_SCRIPT: &str =
    "$text=[Console]::In.ReadToEnd(); Set-Clipboard -Value $text -ErrorAction Stop";
#[cfg(windows)]
const READ_TEXT_SCRIPT: &str =
    "$text=Get-Clipboard -Raw -ErrorAction Stop; if($null -ne $text){[Console]::Out.Write([string]$text)}";
#[cfg(windows)]
const WRITE_PNG_SCRIPT: &str = r#"Add-Type -AssemblyName System.Windows.Forms; Add-Type -AssemblyName System.Drawing; $image=[System.Drawing.Image]::FromFile($args[0]); try { [System.Windows.Forms.Clipboard]::SetImage($image) } finally { $image.Dispose() }"#;
#[cfg(windows)]
const SAVE_BYTES_SCRIPT: &str = r#"Add-Type -AssemblyName System.Windows.Forms; $dialog=New-Object System.Windows.Forms.SaveFileDialog; try { $dialog.FileName=$args[0]; $dialog.DefaultExt=$args[1].TrimStart('.'); $dialog.Filter=($args[1].ToUpperInvariant() + ' files|*' + $args[1] + '|All files|*.*'); if($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) { [System.IO.File]::Copy($args[2], $dialog.FileName, $true); [Console]::Out.Write($dialog.FileName) } } finally { $dialog.Dispose() }"#;

pub fn available() -> bool {
    powershell_executable().is_ok()
}

pub fn write_text(text: &str) -> Result<(), String> {
    if text.len() > MAX_CLIPBOARD_TEXT_BYTES {
        return Err(format!(
            "Clipboard text exceeds the {} MiB limit.",
            MAX_CLIPBOARD_TEXT_BYTES / (1024 * 1024)
        ));
    }
    write_text_platform(text)
}

pub fn read_text() -> Result<String, String> {
    read_text_platform()
}

pub fn write_png(bytes: &[u8]) -> Result<(), String> {
    validate_png(bytes)?;
    let temporary = TemporaryPayload::new(bytes, ".png")?;
    write_png_platform(temporary.path())
}

pub fn save_bytes_with_dialog(
    bytes: &[u8],
    suggested_name: &str,
    extension: &str,
) -> Result<Option<PathBuf>, String> {
    if bytes.is_empty() {
        return Err("Save payload is empty.".to_owned());
    }
    if bytes.len() > MAX_SAVE_BYTES {
        return Err(format!(
            "Save payload exceeds the {} MiB limit.",
            MAX_SAVE_BYTES / (1024 * 1024)
        ));
    }
    validate_suggested_name(suggested_name)?;
    let extension = normalize_extension(extension)?;
    let temporary = TemporaryPayload::new(bytes, &extension)?;
    save_bytes_platform(temporary.path(), suggested_name, &extension)
}

fn validate_png(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(format!(
            "Clipboard image exceeds the {} MiB limit.",
            MAX_IMAGE_BYTES / (1024 * 1024)
        ));
    }
    if !bytes.starts_with(PNG_SIGNATURE) {
        return Err("Clipboard image must be a PNG payload.".to_owned());
    }
    Ok(())
}

fn validate_suggested_name(name: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() || name.len() > 128 {
        return Err("Suggested save name must be between 1 and 128 bytes.".to_owned());
    }
    if name == "."
        || name == ".."
        || name.chars().any(|character| {
            character.is_control()
                || matches!(
                    character,
                    '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
                )
        })
    {
        return Err("Suggested save name contains an unsafe character.".to_owned());
    }
    Ok(())
}

fn normalize_extension(extension: &str) -> Result<String, String> {
    let raw = extension.trim().trim_start_matches('.');
    if raw.is_empty()
        || raw.len() > 10
        || !raw
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
    {
        return Err("Save extension must contain 1-10 ASCII letters or digits.".to_owned());
    }
    Ok(format!(".{raw}"))
}

struct TemporaryPayload {
    path: PathBuf,
}

impl TemporaryPayload {
    fn new(bytes: &[u8], suffix: &str) -> Result<Self, String> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "hermes-local-{}-{nonce}{}",
            std::process::id(),
            suffix
        ));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| format!("Could not create temporary clipboard/save payload: {error}"))?;
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|error| {
                format!("Could not persist temporary clipboard/save payload: {error}")
            })?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TemporaryPayload {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(windows)]
fn powershell_executable() -> Result<PathBuf, String> {
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
        Err(format!(
            "Windows PowerShell is unavailable: {}",
            executable.display()
        ))
    }
}

#[cfg(not(windows))]
fn powershell_executable() -> Result<PathBuf, String> {
    Err("Native clipboard/save-dialog support is only available on Windows.".to_owned())
}

#[cfg(windows)]
fn base_powershell(script: &str) -> Result<Command, String> {
    let mut command = Command::new(powershell_executable()?);
    command.args([
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-STA",
        "-Command",
        script,
    ]);
    Ok(command)
}

#[cfg(windows)]
fn write_text_platform(text: &str) -> Result<(), String> {
    let mut child = base_powershell(WRITE_TEXT_SCRIPT)?
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("Could not start Windows clipboard writer: {error}"))?;
    child
        .stdin
        .as_mut()
        .ok_or_else(|| "Windows clipboard writer stdin was unavailable.".to_owned())?
        .write_all(text.as_bytes())
        .map_err(|error| format!("Could not send text to Windows clipboard writer: {error}"))?;
    drop(child.stdin.take());
    let output = child
        .wait_with_output()
        .map_err(|error| format!("Could not wait for Windows clipboard writer: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "Windows clipboard write failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

#[cfg(not(windows))]
fn write_text_platform(_text: &str) -> Result<(), String> {
    Err("Native clipboard support is only available on Windows.".to_owned())
}

#[cfg(windows)]
fn read_text_platform() -> Result<String, String> {
    let output = base_powershell(READ_TEXT_SCRIPT)?
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("Could not read the Windows clipboard: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "Windows clipboard read failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    if output.stdout.len() > MAX_CLIPBOARD_TEXT_BYTES {
        return Err("Windows clipboard text exceeded the read limit.".to_owned());
    }
    String::from_utf8(output.stdout)
        .map_err(|_| "Windows clipboard text was not valid UTF-8.".to_owned())
}

#[cfg(not(windows))]
fn read_text_platform() -> Result<String, String> {
    Err("Native clipboard support is only available on Windows.".to_owned())
}

#[cfg(windows)]
fn write_png_platform(path: &Path) -> Result<(), String> {
    let output = base_powershell(WRITE_PNG_SCRIPT)?
        .arg(path)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("Could not write image to the Windows clipboard: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "Windows image clipboard write failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

#[cfg(not(windows))]
fn write_png_platform(_path: &Path) -> Result<(), String> {
    Err("Native image clipboard support is only available on Windows.".to_owned())
}

#[cfg(windows)]
fn save_bytes_platform(
    temporary: &Path,
    suggested_name: &str,
    extension: &str,
) -> Result<Option<PathBuf>, String> {
    let output = base_powershell(SAVE_BYTES_SCRIPT)?
        .args([suggested_name, extension])
        .arg(temporary)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("Could not open the Windows Save As dialog: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "Windows Save As failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    if output.stdout.len() > 32 * 1024 {
        return Err("Windows Save As returned an oversized path.".to_owned());
    }
    let path = String::from_utf8(output.stdout)
        .map_err(|_| "Windows Save As returned a non-UTF-8 path.".to_owned())?;
    let path = path.trim();
    if path.is_empty() {
        Ok(None)
    } else {
        let path = PathBuf::from(path);
        if !path.is_absolute() {
            return Err("Windows Save As returned a non-absolute path.".to_owned());
        }
        Ok(Some(path))
    }
}

#[cfg(not(windows))]
fn save_bytes_platform(
    _temporary: &Path,
    _suggested_name: &str,
    _extension: &str,
) -> Result<Option<PathBuf>, String> {
    Err("Native Save As is only available on Windows.".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_validation_rejects_wrong_type_and_oversized_payloads() {
        assert!(validate_png(b"not a png").is_err());
        let mut png = PNG_SIGNATURE.to_vec();
        png.extend_from_slice(b"payload");
        assert!(validate_png(&png).is_ok());
    }

    #[test]
    fn suggested_filename_is_leaf_only() {
        for invalid in [
            "",
            ".",
            "..",
            "../secret",
            r"folder\secret",
            "a:b",
            "a|b",
        ] {
            assert!(validate_suggested_name(invalid).is_err(), "{invalid}");
        }
        assert!(validate_suggested_name("Hermes image 01.png").is_ok());
    }

    #[test]
    fn extension_is_strict_and_normalized() {
        assert_eq!(normalize_extension(".png").expect("png"), ".png");
        assert_eq!(normalize_extension("JSON").expect("json"), ".JSON");
        for invalid in ["", ".", "tar.gz", "../../exe", "verylongextension"] {
            assert!(normalize_extension(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn temporary_payload_is_removed_on_drop() {
        let path = {
            let temporary = TemporaryPayload::new(b"payload", ".bin").expect("temporary payload");
            let path = temporary.path().to_path_buf();
            assert_eq!(fs::read(&path).expect("payload read"), b"payload");
            path
        };
        assert!(!path.exists());
    }

    #[test]
    fn clipboard_text_limit_is_enforced_before_platform_access() {
        let oversized = "x".repeat(MAX_CLIPBOARD_TEXT_BYTES + 1);
        assert!(write_text(&oversized).is_err());
    }
}
