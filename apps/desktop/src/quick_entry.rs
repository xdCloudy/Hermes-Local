#![allow(dead_code)] // DI-07 foundation; live Dioxus shortcut/window wiring is the next stage.

use std::{collections::HashSet, fs, path::Path};

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const DEFAULT_SHORTCUT: &str = "CommandOrControl+Shift+Space";
pub const WINDOW_WIDTH: i64 = 640;
pub const WINDOW_HEIGHT: i64 = 168;
const WINDOW_TOP_FRACTION: f64 = 0.22;
const MAX_SETTINGS_BYTES: u64 = 64 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShortcutError {
    Empty,
    InvalidKey,
    InvalidModifier,
    NoKey,
    NoModifier,
    Reserved,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ShortcutParse {
    Invalid(ShortcutError),
    Valid(String),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct QuickEntrySettings {
    pub enabled: bool,
    pub shortcut: String,
}

impl Default for QuickEntrySettings {
    fn default() -> Self {
        Self {
            enabled: true,
            shortcut: DEFAULT_SHORTCUT.to_owned(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegistrationError {
    Invalid,
    Taken,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegistrationState {
    pub error: Option<RegistrationError>,
    pub registered: bool,
    pub shortcut: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkArea {
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WindowBounds {
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
}

pub trait ShortcutBackend {
    type Handle;

    fn register(&mut self, accelerator: &str) -> Result<Self::Handle, ()>;
    fn unregister(&mut self, handle: Self::Handle);
}

pub struct QuickEntryShortcutController<B: ShortcutBackend> {
    backend: B,
    active: Option<B::Handle>,
    state: RegistrationState,
}

impl<B: ShortcutBackend> QuickEntryShortcutController<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            active: None,
            state: RegistrationState {
                error: None,
                registered: false,
                shortcut: DEFAULT_SHORTCUT.to_owned(),
            },
        }
    }

    pub fn apply(&mut self, settings: &QuickEntrySettings) -> RegistrationState {
        self.release();
        let parsed = parse_shortcut(&settings.shortcut);
        let shortcut = match &parsed {
            ShortcutParse::Valid(accelerator) => accelerator.clone(),
            ShortcutParse::Invalid(_) => settings.shortcut.clone(),
        };

        if !settings.enabled {
            self.state = RegistrationState {
                error: None,
                registered: false,
                shortcut,
            };
            return self.state.clone();
        }

        let ShortcutParse::Valid(accelerator) = parsed else {
            self.state = RegistrationState {
                error: Some(RegistrationError::Invalid),
                registered: false,
                shortcut,
            };
            return self.state.clone();
        };

        match self.backend.register(&accelerator) {
            Ok(handle) => {
                self.active = Some(handle);
                self.state = RegistrationState {
                    error: None,
                    registered: true,
                    shortcut: accelerator,
                };
            }
            Err(()) => {
                self.state = RegistrationState {
                    error: Some(RegistrationError::Taken),
                    registered: false,
                    shortcut: accelerator,
                };
            }
        }
        self.state.clone()
    }

    pub fn current(&self) -> &RegistrationState {
        &self.state
    }

    pub fn dispose(&mut self) {
        self.release();
        self.state.error = None;
        self.state.registered = false;
    }

    fn release(&mut self) {
        if let Some(handle) = self.active.take() {
            self.backend.unregister(handle);
        }
    }
}

impl<B: ShortcutBackend> Drop for QuickEntryShortcutController<B> {
    fn drop(&mut self) {
        self.release();
    }
}

pub fn parse_shortcut(raw: &str) -> ShortcutParse {
    if raw.trim().is_empty() {
        return ShortcutParse::Invalid(ShortcutError::Empty);
    }

    let parts: Vec<_> = raw
        .split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    if parts.is_empty() {
        return ShortcutParse::Invalid(ShortcutError::Empty);
    }

    let mut modifiers = Vec::new();
    let mut key: Option<String> = None;
    for part in parts {
        let lower = part.to_ascii_lowercase();
        if is_modifier(&lower) {
            if key.is_some() {
                return ShortcutParse::Invalid(ShortcutError::InvalidModifier);
            }
            modifiers.push(lower);
            continue;
        }
        if key.is_some() {
            return ShortcutParse::Invalid(ShortcutError::InvalidKey);
        }
        if !is_key(&lower) {
            return ShortcutParse::Invalid(ShortcutError::InvalidKey);
        }
        key = Some(lower);
    }

    let Some(key) = key else {
        return ShortcutParse::Invalid(ShortcutError::NoKey);
    };
    if modifiers.is_empty() {
        return ShortcutParse::Invalid(ShortcutError::NoModifier);
    }
    if key == "escape" {
        return ShortcutParse::Invalid(ShortcutError::Reserved);
    }

    let mut seen = HashSet::new();
    let mut normalized: Vec<_> = modifiers
        .into_iter()
        .map(|modifier| canonical_modifier(&modifier))
        .filter(|modifier| seen.insert(*modifier))
        .collect();
    normalized.sort_by_key(|modifier| modifier_order(modifier));
    let mut accelerator = normalized.join("+");
    accelerator.push('+');
    accelerator.push_str(&canonical_key(&key));
    ShortcutParse::Valid(accelerator)
}

pub fn sanitize_settings(raw: &Value) -> QuickEntrySettings {
    let Some(record) = raw.as_object() else {
        return QuickEntrySettings::default();
    };
    let enabled = match record.get("enabled") {
        None => true,
        Some(Value::Bool(value)) => *value,
        Some(_) => false,
    };
    let shortcut = record
        .get("shortcut")
        .and_then(Value::as_str)
        .and_then(|value| match parse_shortcut(value) {
            ShortcutParse::Valid(accelerator) => Some(accelerator),
            ShortcutParse::Invalid(_) => None,
        })
        .unwrap_or_else(|| DEFAULT_SHORTCUT.to_owned());
    QuickEntrySettings { enabled, shortcut }
}

pub fn load_settings(path: &Path) -> Result<QuickEntrySettings, String> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(QuickEntrySettings::default());
        }
        Err(error) => return Err(format!("Could not inspect Quick Entry settings: {error}")),
    };
    if !metadata.is_file() || metadata.len() > MAX_SETTINGS_BYTES {
        return Ok(QuickEntrySettings::default());
    }
    let bytes = fs::read(path).map_err(|error| format!("Could not read Quick Entry settings: {error}"))?;
    let raw: Value = match serde_json::from_slice(&bytes) {
        Ok(raw) => raw,
        Err(_) => return Ok(QuickEntrySettings::default()),
    };
    Ok(sanitize_settings(&raw))
}

pub fn window_bounds(work_area: Option<WorkArea>) -> WindowBounds {
    let width = work_area.map_or(WINDOW_WIDTH, |area| WINDOW_WIDTH.min(area.width.max(0)));
    let height = work_area.map_or(WINDOW_HEIGHT, |area| WINDOW_HEIGHT.min(area.height.max(0)));
    let Some(area) = work_area else {
        return WindowBounds {
            x: 0,
            y: 0,
            width,
            height,
        };
    };

    let x = js_round(area.x as f64 + (area.width - width) as f64 / 2.0);
    let max_y = area.y.saturating_add(area.height).saturating_sub(height);
    let preferred_y = area.y as f64 + area.height as f64 * WINDOW_TOP_FRACTION;
    let y = js_round(preferred_y.max(area.y as f64).min(max_y as f64));
    WindowBounds {
        x,
        y,
        width,
        height,
    }
}

fn is_modifier(value: &str) -> bool {
    matches!(
        value,
        "alt"
            | "altgr"
            | "cmd"
            | "cmdorctrl"
            | "command"
            | "commandorcontrol"
            | "control"
            | "ctrl"
            | "meta"
            | "option"
            | "shift"
            | "super"
    )
}

fn canonical_modifier(value: &str) -> &'static str {
    match value {
        "alt" => "Alt",
        "altgr" => "AltGr",
        "cmd" | "command" => "Command",
        "cmdorctrl" | "commandorcontrol" => "CommandOrControl",
        "control" | "ctrl" => "Control",
        "meta" | "super" => "Super",
        "option" => "Option",
        "shift" => "Shift",
        _ => unreachable!("modifier validated before canonicalization"),
    }
}

fn modifier_order(value: &str) -> usize {
    match value {
        "CommandOrControl" => 0,
        "Command" => 1,
        "Control" => 2,
        "Super" => 3,
        "Alt" => 4,
        "Option" => 5,
        "AltGr" => 6,
        "Shift" => 7,
        _ => usize::MAX,
    }
}

fn is_key(value: &str) -> bool {
    if matches!(
        value,
        "backspace"
            | "delete"
            | "down"
            | "end"
            | "enter"
            | "escape"
            | "home"
            | "insert"
            | "left"
            | "medianexttrack"
            | "mediaplaypause"
            | "mediaprevioustrack"
            | "mediastop"
            | "pagedown"
            | "pageup"
            | "plus"
            | "printscreen"
            | "return"
            | "right"
            | "space"
            | "tab"
            | "up"
            | "volumedown"
            | "volumemute"
            | "volumeup"
    ) {
        return true;
    }
    if let Some(number) = value.strip_prefix('f').and_then(|value| value.parse::<u8>().ok()) {
        return (1..=24).contains(&number);
    }
    if let Some(number) = value.strip_prefix("num") {
        return matches!(number, "lock" | "dec" | "add" | "sub" | "mult" | "div")
            || number.parse::<u8>().is_ok_and(|number| number <= 9);
    }
    value.len() == 1
        && value
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_alphanumeric() || is_punctuation(character))
}

fn is_punctuation(character: char) -> bool {
    matches!(
        character,
        '!' | '"'
            | '#'
            | '$'
            | '%'
            | '&'
            | '\''
            | '('
            | ')'
            | '*'
            | '+'
            | ','
            | '-'
            | '.'
            | '/'
            | ':'
            | ';'
            | '<'
            | '='
            | '>'
            | '?'
            | '@'
            | '['
            | '\\'
            | ']'
            | '^'
            | '_'
            | '`'
            | '{'
            | '|'
            | '}'
            | '~'
    )
}

fn canonical_key(key: &str) -> String {
    let special = match key {
        "backspace" => Some("Backspace"),
        "delete" => Some("Delete"),
        "down" => Some("Down"),
        "end" => Some("End"),
        "enter" => Some("Enter"),
        "escape" => Some("Escape"),
        "home" => Some("Home"),
        "insert" => Some("Insert"),
        "left" => Some("Left"),
        "medianexttrack" => Some("MediaNextTrack"),
        "mediaplaypause" => Some("MediaPlayPause"),
        "mediaprevioustrack" => Some("MediaPreviousTrack"),
        "mediastop" => Some("MediaStop"),
        "pagedown" => Some("PageDown"),
        "pageup" => Some("PageUp"),
        "plus" => Some("Plus"),
        "printscreen" => Some("PrintScreen"),
        "return" => Some("Return"),
        "right" => Some("Right"),
        "space" => Some("Space"),
        "tab" => Some("Tab"),
        "up" => Some("Up"),
        "volumedown" => Some("VolumeDown"),
        "volumemute" => Some("VolumeMute"),
        "volumeup" => Some("VolumeUp"),
        _ => None,
    };
    if let Some(special) = special {
        return special.to_owned();
    }
    if value_is_function_key(key) || (key.len() == 1 && key.as_bytes()[0].is_ascii_lowercase()) {
        return key.to_ascii_uppercase();
    }
    key.to_owned()
}

fn value_is_function_key(value: &str) -> bool {
    value
        .strip_prefix('f')
        .and_then(|number| number.parse::<u8>().ok())
        .is_some_and(|number| (1..=24).contains(&number))
}

fn js_round(value: f64) -> i64 {
    (value + 0.5).floor() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn parser_matches_electron_alias_order_duplicate_and_key_rules() {
        assert_eq!(
            parse_shortcut("cmdorctrl+shift+space"),
            ShortcutParse::Valid(DEFAULT_SHORTCUT.to_owned())
        );
        assert_eq!(
            parse_shortcut("  Shift + CTRL + k "),
            ShortcutParse::Valid("Control+Shift+K".to_owned())
        );
        assert_eq!(parse_shortcut("Alt+f5"), ShortcutParse::Valid("Alt+F5".to_owned()));
        assert_eq!(parse_shortcut("Meta+/"), ShortcutParse::Valid("Super+/".to_owned()));
        assert_eq!(
            parse_shortcut("Ctrl+Control+Shift+J"),
            ShortcutParse::Valid("Control+Shift+J".to_owned())
        );
        assert_eq!(parse_shortcut("K"), ShortcutParse::Invalid(ShortcutError::NoModifier));
        assert_eq!(parse_shortcut("Shift+Control"), ShortcutParse::Invalid(ShortcutError::NoKey));
        assert_eq!(parse_shortcut("Shift+A+B"), ShortcutParse::Invalid(ShortcutError::InvalidKey));
        assert_eq!(parse_shortcut("A+Shift"), ShortcutParse::Invalid(ShortcutError::InvalidModifier));
        assert_eq!(parse_shortcut("Ctrl+Escape"), ShortcutParse::Invalid(ShortcutError::Reserved));
    }

    #[test]
    fn settings_match_default_disable_and_invalid_fallback_semantics() {
        assert_eq!(sanitize_settings(&Value::Null), QuickEntrySettings::default());
        assert_eq!(
            sanitize_settings(&serde_json::json!({"enabled": false, "shortcut": "alt+j"})),
            QuickEntrySettings {
                enabled: false,
                shortcut: "Alt+J".to_owned(),
            }
        );
        assert_eq!(
            sanitize_settings(&serde_json::json!({"enabled": true, "shortcut": "Q"})).shortcut,
            DEFAULT_SHORTCUT
        );
        assert!(!sanitize_settings(&serde_json::json!({"enabled": "yes"})).enabled);
    }

    #[test]
    fn controller_releases_old_registration_and_surfaces_invalid_or_taken() {
        let backend = FakeBackend::default();
        let mut controller = QuickEntryShortcutController::new(backend);
        let first = controller.apply(&QuickEntrySettings {
            enabled: true,
            shortcut: "alt+j".to_owned(),
        });
        assert!(first.registered);
        assert_eq!(first.shortcut, "Alt+J");
        let second = controller.apply(&QuickEntrySettings {
            enabled: true,
            shortcut: "J".to_owned(),
        });
        assert_eq!(second.error, Some(RegistrationError::Invalid));
        assert!(!second.registered);

        controller.backend.fail = true;
        let taken = controller.apply(&QuickEntrySettings {
            enabled: true,
            shortcut: "Alt+K".to_owned(),
        });
        assert_eq!(taken.error, Some(RegistrationError::Taken));
        controller.dispose();
        controller.dispose();
        assert!(!controller.current().registered);
    }

    #[test]
    fn controller_never_registers_while_disabled() {
        let backend = FakeBackend::default();
        let mut controller = QuickEntryShortcutController::new(backend);
        let state = controller.apply(&QuickEntrySettings {
            enabled: false,
            shortcut: DEFAULT_SHORTCUT.to_owned(),
        });
        assert!(!state.registered);
        assert!(controller.backend.held.is_empty());
    }

    #[test]
    fn bounds_match_electron_second_monitor_tiny_and_fallback_cases() {
        assert_eq!(
            window_bounds(None),
            WindowBounds {
                x: 0,
                y: 0,
                width: 640,
                height: 168,
            }
        );
        let normal = window_bounds(Some(WorkArea {
            x: 0,
            y: 0,
            width: 1600,
            height: 1000,
        }));
        assert_eq!(normal.x, 480);
        assert_eq!(normal.y, 220);
        assert_eq!(normal.width, 640);
        assert_eq!(normal.height, 168);

        let second = window_bounds(Some(WorkArea {
            x: 1600,
            y: -200,
            width: 1440,
            height: 900,
        }));
        assert_eq!(second.x, 2000);
        assert!(second.y >= -200);

        let tiny = window_bounds(Some(WorkArea {
            x: 0,
            y: 0,
            width: 320,
            height: 120,
        }));
        assert_eq!(tiny.width, 320);
        assert_eq!(tiny.height, 120);
        assert!(tiny.y + tiny.height <= 120);
    }

    #[derive(Default)]
    struct FakeBackend {
        fail: bool,
        held: HashSet<String>,
    }

    impl ShortcutBackend for FakeBackend {
        type Handle = String;

        fn register(&mut self, accelerator: &str) -> Result<Self::Handle, ()> {
            if self.fail || !self.held.insert(accelerator.to_owned()) {
                return Err(());
            }
            Ok(accelerator.to_owned())
        }

        fn unregister(&mut self, handle: Self::Handle) {
            self.held.remove(&handle);
        }
    }
}
