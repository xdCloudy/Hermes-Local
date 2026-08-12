use std::{fs, path::Path};

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const DEFAULT_WIDTH: i64 = 1220;
pub const DEFAULT_HEIGHT: i64 = 800;
pub const MIN_WIDTH: i64 = 400;
pub const MIN_HEIGHT: i64 = 620;
pub const MIN_VISIBLE: i64 = 48;
const MAX_STATE_BYTES: u64 = 64 * 1024;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowState {
    pub width: i64,
    pub height: i64,
    #[serde(default)]
    pub is_maximized: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<i64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkArea {
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WindowOptions {
    pub width: i64,
    pub height: i64,
    pub x: Option<i64>,
    pub y: Option<i64>,
    pub is_maximized: bool,
}

pub fn load(path: &Path) -> Result<Option<WindowState>, String> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Could not inspect window state: {error}")),
    };
    if !metadata.is_file() || metadata.len() > MAX_STATE_BYTES {
        return Ok(None);
    }
    let bytes = fs::read(path).map_err(|error| format!("Could not read window state: {error}"))?;
    let raw: Value = match serde_json::from_slice(&bytes) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };
    Ok(sanitize(&raw))
}

pub fn sanitize(raw: &Value) -> Option<WindowState> {
    let object = raw.as_object()?;
    let width = finite_number(object.get("width")?)?;
    let height = finite_number(object.get("height")?)?;
    let mut state = WindowState {
        width: js_round(width).max(MIN_WIDTH),
        height: js_round(height).max(MIN_HEIGHT),
        is_maximized: object.get("isMaximized") == Some(&Value::Bool(true)),
        x: None,
        y: None,
    };
    if let (Some(x), Some(y)) = (
        object.get("x").and_then(finite_number),
        object.get("y").and_then(finite_number),
    ) {
        state.x = Some(js_round(x));
        state.y = Some(js_round(y));
    }
    Some(state)
}

pub fn compute_options(state: Option<&WindowState>, displays: &[WorkArea]) -> WindowOptions {
    let mut width = state.map_or(DEFAULT_WIDTH, |state| state.width);
    let mut height = state.map_or(DEFAULT_HEIGHT, |state| state.height);

    let cap = displays
        .iter()
        .filter(|area| area.width > 0 && area.height > 0)
        .fold((0_i64, 0_i64), |(width, height), area| {
            (width.max(area.width), height.max(area.height))
        });
    if cap.0 > 0 && cap.1 > 0 {
        width = width.clamp(MIN_WIDTH, cap.0.max(MIN_WIDTH));
        height = height.clamp(MIN_HEIGHT, cap.1.max(MIN_HEIGHT));
    }

    let position = state.and_then(|state| match (state.x, state.y) {
        (Some(x), Some(y)) if on_screen(x, y, width, height, displays) => Some((x, y)),
        _ => None,
    });

    WindowOptions {
        width,
        height,
        x: position.map(|position| position.0),
        y: position.map(|position| position.1),
        is_maximized: state.is_some_and(|state| state.is_maximized),
    }
}

pub fn on_screen(x: i64, y: i64, width: i64, height: i64, displays: &[WorkArea]) -> bool {
    displays.iter().any(|area| {
        let overlap_x = (x.saturating_add(width))
            .min(area.x.saturating_add(area.width))
            .saturating_sub(x.max(area.x));
        let overlap_y = (y.saturating_add(height))
            .min(area.y.saturating_add(area.height))
            .saturating_sub(y.max(area.y));
        overlap_x >= MIN_VISIBLE && overlap_y >= MIN_VISIBLE
    })
}

fn finite_number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .filter(|value| value.is_finite() && value.abs() <= i64::MAX as f64)
}

fn js_round(value: f64) -> i64 {
    (value + 0.5).floor() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn sanitizes_required_size_pair_and_optional_position_pair() {
        let state = sanitize(&serde_json::json!({
            "width": 399.4,
            "height": 619.5,
            "x": -10.5,
            "y": 20.6,
            "isMaximized": true
        }))
        .expect("state");
        assert_eq!(state.width, MIN_WIDTH);
        assert_eq!(state.height, MIN_HEIGHT);
        assert_eq!(state.x, Some(-10));
        assert_eq!(state.y, Some(21));
        assert!(state.is_maximized);

        let incomplete = sanitize(&serde_json::json!({
            "width": 1220,
            "height": 800,
            "x": 40
        }))
        .expect("state");
        assert_eq!(incomplete.x, None);
        assert_eq!(incomplete.y, None);
        assert!(!incomplete.is_maximized);
        assert!(sanitize(&serde_json::json!({ "width": 1220 })).is_none());
        assert!(sanitize(&serde_json::json!(null)).is_none());
    }

    #[test]
    fn computes_defaults_caps_size_and_rejects_offscreen_positions() {
        let displays = [WorkArea {
            x: 0,
            y: 0,
            width: 1920,
            height: 1040,
        }];
        assert_eq!(
            compute_options(None, &displays),
            WindowOptions {
                width: DEFAULT_WIDTH,
                height: DEFAULT_HEIGHT,
                x: None,
                y: None,
                is_maximized: false,
            }
        );

        let state = WindowState {
            width: 4000,
            height: 3000,
            x: Some(20),
            y: Some(30),
            is_maximized: true,
        };
        assert_eq!(
            compute_options(Some(&state), &displays),
            WindowOptions {
                width: 1920,
                height: 1040,
                x: Some(20),
                y: Some(30),
                is_maximized: true,
            }
        );

        let offscreen = WindowState {
            width: 800,
            height: 700,
            x: Some(4000),
            y: Some(4000),
            is_maximized: false,
        };
        let options = compute_options(Some(&offscreen), &displays);
        assert_eq!(options.x, None);
        assert_eq!(options.y, None);
    }

    #[test]
    fn requires_at_least_48_pixels_visible_on_both_axes() {
        let display = [WorkArea {
            x: 0,
            y: 0,
            width: 1000,
            height: 800,
        }];
        assert!(on_screen(952, 752, 400, 620, &display));
        assert!(!on_screen(953, 752, 400, 620, &display));
        assert!(!on_screen(952, 753, 400, 620, &display));
    }

    #[test]
    fn load_fails_soft_for_missing_invalid_and_oversized_state() {
        let root = test_directory();
        let path = root.join("window-state.json");
        assert_eq!(load(&path).expect("missing"), None);

        fs::write(&path, b"not json").expect("invalid");
        assert_eq!(load(&path).expect("invalid"), None);

        fs::write(&path, vec![b'x'; MAX_STATE_BYTES as usize + 1]).expect("oversized");
        assert_eq!(load(&path).expect("oversized"), None);
        let _ = fs::remove_dir_all(root);
    }

    fn test_directory() -> std::path::PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "hermes-window-state-{}-{suffix}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("test directory");
        path
    }
}
