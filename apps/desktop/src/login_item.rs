use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "Hermes Local";
const AUTOSTART_ARGUMENT: &str = "--hermes-local-autostart";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginItemStatus {
    pub available: bool,
    pub enabled: bool,
    pub executable: PathBuf,
}

pub fn is_autostart_launch() -> bool {
    contains_autostart_argument(std::env::args_os())
}

fn contains_autostart_argument<I, S>(arguments: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    arguments
        .into_iter()
        .any(|argument| argument.as_ref() == OsStr::new(AUTOSTART_ARGUMENT))
}

fn registered_command(executable: &Path) -> Result<String, String> {
    if !executable.is_absolute() {
        return Err("Launch-at-login executable must be an absolute path.".to_owned());
    }
    let executable = executable
        .to_str()
        .ok_or_else(|| "Launch-at-login executable path must be valid Unicode.".to_owned())?;
    if executable.contains('"') {
        return Err("Launch-at-login executable path contains an invalid quote.".to_owned());
    }
    Ok(format!("\"{executable}\" {AUTOSTART_ARGUMENT}"))
}

fn parse_registered_command(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let (_, value) = line.split_once("REG_SZ")?;
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_owned())
    })
}

fn registration_matches(output: &str, executable: &Path) -> Result<bool, String> {
    Ok(parse_registered_command(output).as_deref()
        == Some(registered_command(executable)?.as_str()))
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
fn query_registered_command() -> Result<Option<String>, String> {
    let output = std::process::Command::new(reg_executable()?)
        .args(["query", RUN_KEY, "/v", VALUE_NAME])
        .output()
        .map_err(|error| format!("Could not query launch-at-login state: {error}"))?;

    if !output.status.success() {
        return Ok(None);
    }
    Ok(parse_registered_command(&String::from_utf8_lossy(
        &output.stdout,
    )))
}

#[cfg(windows)]
pub fn status() -> Result<LoginItemStatus, String> {
    let executable = current_executable()?;
    let expected = registered_command(&executable)?;
    let enabled = query_registered_command()?.as_deref() == Some(expected.as_str());
    Ok(LoginItemStatus {
        available: true,
        enabled,
        executable,
    })
}

#[cfg(not(windows))]
pub fn status() -> Result<LoginItemStatus, String> {
    Ok(LoginItemStatus {
        available: false,
        enabled: false,
        executable: std::env::current_exe().unwrap_or_default(),
    })
}

#[cfg(windows)]
pub fn set_enabled(enabled: bool) -> Result<LoginItemStatus, String> {
    let executable = current_executable()?;
    let expected = registered_command(&executable)?;
    let reg = reg_executable()?;

    if enabled {
        let result = std::process::Command::new(&reg)
            .args([
                "add",
                RUN_KEY,
                "/v",
                VALUE_NAME,
                "/t",
                "REG_SZ",
                "/d",
                expected.as_str(),
                "/f",
            ])
            .output()
            .map_err(|error| format!("Could not enable launch at login: {error}"))?;
        if !result.status.success() {
            return Err(format!(
                "Windows rejected launch-at-login registration: {}",
                String::from_utf8_lossy(&result.stderr).trim()
            ));
        }
    } else if query_registered_command()?.is_some() {
        let result = std::process::Command::new(&reg)
            .args(["delete", RUN_KEY, "/v", VALUE_NAME, "/f"])
            .output()
            .map_err(|error| format!("Could not disable launch at login: {error}"))?;
        if !result.status.success() {
            return Err(format!(
                "Windows rejected launch-at-login removal: {}",
                String::from_utf8_lossy(&result.stderr).trim()
            ));
        }
    }

    let state = status()?;
    if state.enabled != enabled {
        return Err("Windows launch-at-login state did not match the requested value.".to_owned());
    }
    Ok(state)
}

#[cfg(not(windows))]
pub fn set_enabled(_enabled: bool) -> Result<LoginItemStatus, String> {
    Err("Launch at login is only available on Windows.".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    #[test]
    fn command_quotes_absolute_executable_and_uses_canonical_argument() {
        let command =
            registered_command(Path::new(r"C:\Program Files\Hermes Local\hermes-local.exe"))
                .expect("valid command");
        assert_eq!(
            command,
            r#""C:\Program Files\Hermes Local\hermes-local.exe" --hermes-local-autostart"#
        );
    }

    #[test]
    fn registry_output_requires_exact_command_identity() {
        let executable = Path::new(r"C:\Hermes Local\hermes-local.exe");
        let expected = registered_command(executable).expect("valid command");
        let output = format!(
            "\nHKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Run\n    Hermes Local    REG_SZ    {expected}\n"
        );
        assert!(registration_matches(&output, executable).expect("comparison"));

        let unexpected = output.replace(&expected, &format!("{expected} --unexpected"));
        assert!(!registration_matches(&unexpected, executable).expect("comparison"));
    }

    #[test]
    fn missing_registry_value_is_not_treated_as_enabled() {
        let executable = Path::new(r"C:\Hermes Local\hermes-local.exe");
        let output = "HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Run\n";
        assert!(!registration_matches(output, executable).expect("comparison"));
    }

    #[test]
    fn autostart_detection_uses_exact_argument() {
        assert!(contains_autostart_argument([
            OsString::from("hermes-local.exe"),
            OsString::from(AUTOSTART_ARGUMENT),
        ]));
        assert!(!contains_autostart_argument([
            OsString::from("hermes-local.exe"),
            OsString::from("--hermes-local-autostart=true"),
        ]));
    }

    #[test]
    fn relative_executable_is_rejected() {
        let error = registered_command(Path::new("hermes-local.exe")).expect_err("must reject");
        assert!(error.contains("absolute"));
    }
}
