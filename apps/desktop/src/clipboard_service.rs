#![allow(dead_code)] // DI-02 service foundation; Dioxus consumers are a later stage.

use std::{
    fs,
    path::{Path, PathBuf},
};

#[cfg(windows)]
use std::{
    io::Write,
    process::{Command, Stdio},
};

const MAX_TEXT_BYTES: usize = 4 * 1024 * 1024;
const MAX_IMAGE_BYTES: u64 = 64 * 1024 * 1024;
const PNG_MAGIC: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

#[cfg(windows)]
const WRITE_TEXT_SCRIPT: &str = r#"Add-Type -AssemblyName System.Windows.Forms; [Console]::InputEncoding=[Text.UTF8Encoding]::new($false); $text=[Console]::In.ReadToEnd(); for($i=0;$i -lt 5;$i++){ try { [System.Windows.Forms.Clipboard]::SetText($text,[System.Windows.Forms.TextDataFormat]::UnicodeText); exit 0 } catch { if($i -eq 4){ throw }; Start-Sleep -Milliseconds 50 } }"#;

#[cfg(windows)]
const READ_TEXT_SCRIPT: &str = r#"Add-Type -AssemblyName System.Windows.Forms; [Console]::OutputEncoding=[Text.UTF8Encoding]::new($false); for($i=0;$i -lt 5;$i++){ try { $text=[System.Windows.Forms.Clipboard]::GetText([System.Windows.Forms.TextDataFormat]::UnicodeText); if($text.Length -gt 1000000){ exit 43 }; [Console]::Out.Write($text); exit 0 } catch { if($i -eq 4){ throw }; Start-Sleep -Milliseconds 50 } }"#;

#[cfg(windows)]
const SAVE_IMAGE_SCRIPT: &str = r#"Add-Type -AssemblyName System.Windows.Forms; Add-Type -AssemblyName System.Drawing; $path=$env:HERMES_LOCAL_CLIPBOARD_IMAGE_DEST; for($i=0;$i -lt 5;$i++){ try { if(-not [System.Windows.Forms.Clipboard]::ContainsImage()){ exit 3 }; $image=[System.Windows.Forms.Clipboard]::GetImage(); try { $image.Save($path,[System.Drawing.Imaging.ImageFormat]::Png) } finally { $image.Dispose() }; exit 0 } catch { if($i -eq 4){ throw }; Start-Sleep -Milliseconds 50 } }"#;

#[derive(Clone, Copy, Debug, Default)]
pub struct ClipboardService;

impl ClipboardService {
    pub fn write_text(&self, text: &str) -> Result<(), String> {
        validate_text(text)?;

        #[cfg(windows)]
        {
            write_text_windows(text)
        }

        #[cfg(not(windows))]
        {
            Err("Native clipboard is only implemented for Windows Desktop.".to_owned())
        }
    }

    pub fn read_text(&self) -> Result<String, String> {
        #[cfg(windows)]
        {
            read_text_windows()
        }

        #[cfg(not(windows))]
        {
            Err("Native clipboard is only implemented for Windows Desktop.".to_owned())
        }
    }

    /// Export the current clipboard image as PNG. `Ok(false)` means the
    /// clipboard does not currently contain an image.
    pub fn save_image_png(&self, destination: &Path) -> Result<bool, String> {
        validate_image_destination(destination)?;

        #[cfg(windows)]
        {
            save_image_windows(destination)
        }

        #[cfg(not(windows))]
        {
            Err("Native clipboard images are only implemented for Windows Desktop.".to_owned())
        }
    }
}

fn validate_text(text: &str) -> Result<(), String> {
    if text.len() > MAX_TEXT_BYTES {
        return Err(format!(
            "Clipboard text exceeds the {} MiB safety limit.",
            MAX_TEXT_BYTES / (1024 * 1024)
        ));
    }
    if text.contains('\0') {
        return Err("Clipboard text contains a NUL character.".to_owned());
    }
    Ok(())
}

fn validate_image_destination(destination: &Path) -> Result<(), String> {
    if !destination.is_absolute() {
        return Err("Clipboard image destination must be absolute.".to_owned());
    }
    let extension = destination
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if !extension.eq_ignore_ascii_case("png") {
        return Err("Clipboard image destination must use a .png extension.".to_owned());
    }
    let parent = destination
        .parent()
        .ok_or_else(|| "Clipboard image destination has no parent directory.".to_owned())?;
    if !parent.is_dir() {
        return Err("Clipboard image destination parent directory does not exist.".to_owned());
    }
    Ok(())
}

#[cfg(windows)]
fn write_text_windows(text: &str) -> Result<(), String> {
    let mut child = powershell_command(WRITE_TEXT_SCRIPT)?
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("Could not start clipboard writer: {error}"))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "Clipboard writer stdin was unavailable.".to_owned())?;
    stdin
        .write_all(text.as_bytes())
        .map_err(|error| format!("Could not send text to clipboard writer: {error}"))?;
    drop(stdin);
    let output = child
        .wait_with_output()
        .map_err(|error| format!("Could not wait for clipboard writer: {error}"))?;
    check_helper_output("Clipboard writer", &output, false)?;
    Ok(())
}

#[cfg(windows)]
fn read_text_windows() -> Result<String, String> {
    let output = powershell_command(READ_TEXT_SCRIPT)?
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| format!("Could not run clipboard reader: {error}"))?;
    check_helper_output("Clipboard reader", &output, true)?;
    String::from_utf8(output.stdout).map_err(|_| "Clipboard text was not valid UTF-8.".to_owned())
}

#[cfg(windows)]
fn save_image_windows(destination: &Path) -> Result<bool, String> {
    let output = powershell_command(SAVE_IMAGE_SCRIPT)?
        .env("HERMES_LOCAL_CLIPBOARD_IMAGE_DEST", destination)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| format!("Could not run clipboard image exporter: {error}"))?;
    if output.status.code() == Some(3) {
        return Ok(false);
    }
    check_helper_output("Clipboard image exporter", &output, false)?;
    verify_png(destination)?;
    Ok(true)
}

#[cfg(windows)]
fn powershell_command(script: &str) -> Result<Command, String> {
    let executable = powershell_executable()?;
    let mut command = Command::new(executable);
    command.args([
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-STA",
        "-WindowStyle",
        "Hidden",
        "-Command",
        script,
    ]);
    Ok(command)
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
            "Windows PowerShell clipboard helper is unavailable: {}",
            executable.display()
        ))
    }
}

#[cfg(windows)]
fn check_helper_output(
    label: &str,
    output: &std::process::Output,
    check_stdout: bool,
) -> Result<(), String> {
    if output.stderr.len() > 64 * 1024 {
        return Err(format!("{label} returned oversized diagnostics."));
    }
    if check_stdout && output.stdout.len() > MAX_TEXT_BYTES {
        return Err(format!("{label} returned oversized clipboard text."));
    }
    if output.status.success() {
        return Ok(());
    }
    if output.status.code() == Some(43) {
        return Err("Clipboard text exceeds the bounded read limit.".to_owned());
    }
    let diagnostics = String::from_utf8_lossy(&output.stderr);
    let diagnostics = diagnostics.trim().chars().take(512).collect::<String>();
    if diagnostics.is_empty() {
        Err(format!("{label} failed."))
    } else {
        Err(format!("{label} failed: {diagnostics}"))
    }
}

fn verify_png(path: &Path) -> Result<(), String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("Could not inspect clipboard image: {error}"))?;
    if !metadata.is_file() || metadata.len() < PNG_MAGIC.len() as u64 {
        return Err("Clipboard image export did not produce a valid file.".to_owned());
    }
    if metadata.len() > MAX_IMAGE_BYTES {
        let _ = fs::remove_file(path);
        return Err(format!(
            "Clipboard image exceeds the {} MiB safety limit.",
            MAX_IMAGE_BYTES / (1024 * 1024)
        ));
    }
    let bytes =
        fs::read(path).map_err(|error| format!("Could not verify clipboard image: {error}"))?;
    if !bytes.starts_with(&PNG_MAGIC) {
        return Err("Clipboard image export was not a PNG file.".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn text_rejects_nul_and_oversized_payloads() {
        assert!(validate_text("normal clipboard text").is_ok());
        assert!(validate_text("bad\0text").is_err());
        let oversized = "x".repeat(MAX_TEXT_BYTES + 1);
        assert!(validate_text(&oversized).is_err());
    }

    #[test]
    fn image_destination_requires_absolute_png_in_existing_parent() {
        assert!(validate_image_destination(Path::new("relative.png")).is_err());
        let root = std::env::temp_dir();
        assert!(validate_image_destination(&root.join("clipboard.jpg")).is_err());
        assert!(validate_image_destination(&root.join("clipboard.PNG")).is_ok());
    }

    #[test]
    fn png_verifier_checks_signature_and_size_shape() {
        let root = std::env::temp_dir().join(format!(
            "hermes-clipboard-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let valid = root.join("valid.png");
        let invalid = root.join("invalid.png");
        let mut bytes = PNG_MAGIC.to_vec();
        bytes.extend_from_slice(b"fixture");
        fs::write(&valid, bytes).unwrap();
        fs::write(&invalid, b"not-png").unwrap();
        assert!(verify_png(&valid).is_ok());
        assert!(verify_png(&invalid).is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(windows)]
    #[test]
    fn clipboard_helper_uses_trusted_absolute_powershell() {
        let executable = powershell_executable().expect("Windows PowerShell");
        assert!(executable.is_absolute());
        assert!(executable.is_file());
    }
}
