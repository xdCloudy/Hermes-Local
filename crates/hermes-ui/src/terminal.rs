use std::{cell::RefCell, collections::VecDeque, path::Path};

use dioxus::prelude::*;
use hermes_core::{AppServices, ServiceError};
use hermes_protocol::ProjectsSnapshot;

use super::{ProjectUiState, Surface};

const MAX_SCROLLBACK_LINES: usize = 4_000;
const MAX_SCROLLBACK_CELLS: usize = 262_144;
const MAX_PERSISTED_PROJECTS: usize = 8;
const MAX_ESCAPE_BYTES: usize = 128;
const MAX_CURSOR_COLUMN: usize = 4_096;
const READ_INTERVAL_MS: u64 = 50;

thread_local! {
    static SCROLLBACK_CACHE: RefCell<VecDeque<(String, TerminalBuffer)>> =
        RefCell::new(VecDeque::new());
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct TextStyle {
    bold: bool,
    dim: bool,
    italic: bool,
    underline: bool,
    inverse: bool,
    foreground: Option<AnsiColor>,
    background: Option<AnsiColor>,
}

impl TextStyle {
    fn css(self) -> String {
        let mut rules = Vec::with_capacity(6);
        let (foreground, background) = if self.inverse {
            (
                self.background
                    .map(AnsiColor::css)
                    .unwrap_or_else(|| "var(--terminal-background, #0d1117)".to_owned()),
                self.foreground
                    .map(AnsiColor::css)
                    .unwrap_or_else(|| "var(--terminal-foreground, #d7dae0)".to_owned()),
            )
        } else {
            (
                self.foreground.map(AnsiColor::css).unwrap_or_default(),
                self.background.map(AnsiColor::css).unwrap_or_default(),
            )
        };

        if !foreground.is_empty() {
            rules.push(format!("color:{foreground}"));
        }
        if !background.is_empty() {
            rules.push(format!("background-color:{background}"));
        }
        if self.bold {
            rules.push("font-weight:700".to_owned());
        }
        if self.dim {
            rules.push("opacity:.68".to_owned());
        }
        if self.italic {
            rules.push("font-style:italic".to_owned());
        }
        if self.underline {
            rules.push("text-decoration:underline".to_owned());
        }
        rules.join(";")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AnsiColor {
    Indexed(u8),
    Rgb(u8, u8, u8),
}

impl AnsiColor {
    fn css(self) -> String {
        match self {
            Self::Rgb(red, green, blue) => format!("rgb({red} {green} {blue})"),
            Self::Indexed(index) => {
                const ANSI_16: [&str; 16] = [
                    "#000000", "#cd3131", "#0dbc79", "#e5e510", "#2472c8", "#bc3fbc", "#11a8cd",
                    "#e5e5e5", "#666666", "#f14c4c", "#23d18b", "#f5f543", "#3b8eea", "#d670d6",
                    "#29b8db", "#ffffff",
                ];
                match index {
                    0..=15 => ANSI_16[usize::from(index)].to_owned(),
                    16..=231 => {
                        let cube = index - 16;
                        let red = cube / 36;
                        let green = (cube % 36) / 6;
                        let blue = cube % 6;
                        let component = |value: u8| if value == 0 { 0 } else { 55 + (value * 40) };
                        format!(
                            "rgb({} {} {})",
                            component(red),
                            component(green),
                            component(blue)
                        )
                    }
                    232..=255 => {
                        let level = 8 + ((index - 232) * 10);
                        format!("rgb({level} {level} {level})")
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TerminalCell {
    value: char,
    style: TextStyle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ParserState {
    Ground,
    Escape,
    Csi(Vec<u8>),
    Osc { escaped: bool },
}

impl Default for ParserState {
    fn default() -> Self {
        Self::Ground
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TerminalBuffer {
    lines: VecDeque<Vec<TerminalCell>>,
    cursor_column: usize,
    style: TextStyle,
    parser: ParserState,
    utf8_pending: Vec<u8>,
    cell_count: usize,
}

impl Default for TerminalBuffer {
    fn default() -> Self {
        Self {
            lines: VecDeque::from([Vec::new()]),
            cursor_column: 0,
            style: TextStyle::default(),
            parser: ParserState::Ground,
            utf8_pending: Vec::new(),
            cell_count: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TerminalRun {
    text: String,
    css: String,
}

impl TerminalBuffer {
    fn push_bytes(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            let state = std::mem::take(&mut self.parser);
            self.parser = match state {
                ParserState::Ground => self.handle_ground(byte),
                ParserState::Escape => self.handle_escape(byte),
                ParserState::Csi(mut sequence) => {
                    if (0x40..=0x7e).contains(&byte) {
                        self.handle_csi(&sequence, byte);
                        ParserState::Ground
                    } else if sequence.len() < MAX_ESCAPE_BYTES {
                        sequence.push(byte);
                        ParserState::Csi(sequence)
                    } else {
                        ParserState::Ground
                    }
                }
                ParserState::Osc { escaped } => {
                    if byte == 0x07 || (escaped && byte == b'\\') {
                        ParserState::Ground
                    } else {
                        ParserState::Osc {
                            escaped: byte == 0x1b,
                        }
                    }
                }
            };
        }
        self.trim_scrollback();
    }

    fn handle_ground(&mut self, byte: u8) -> ParserState {
        match byte {
            0x1b => {
                self.finish_incomplete_utf8();
                ParserState::Escape
            }
            b'\n' => {
                self.finish_incomplete_utf8();
                self.new_line();
                ParserState::Ground
            }
            b'\r' => {
                self.finish_incomplete_utf8();
                self.cursor_column = 0;
                ParserState::Ground
            }
            0x08 => {
                self.finish_incomplete_utf8();
                self.cursor_column = self.cursor_column.saturating_sub(1);
                ParserState::Ground
            }
            b'\t' => {
                self.finish_incomplete_utf8();
                let next_stop = ((self.cursor_column / 8) + 1) * 8;
                while self.cursor_column < next_stop.min(MAX_CURSOR_COLUMN) {
                    self.write_char(' ');
                }
                ParserState::Ground
            }
            0x00..=0x1f | 0x7f => {
                self.finish_incomplete_utf8();
                ParserState::Ground
            }
            _ => {
                self.push_utf8_byte(byte);
                ParserState::Ground
            }
        }
    }

    fn handle_escape(&mut self, byte: u8) -> ParserState {
        match byte {
            b'[' => ParserState::Csi(Vec::new()),
            b']' => ParserState::Osc { escaped: false },
            b'c' => {
                self.reset_terminal();
                ParserState::Ground
            }
            0x1b => ParserState::Escape,
            _ => ParserState::Ground,
        }
    }

    fn handle_csi(&mut self, sequence: &[u8], final_byte: u8) {
        let params = csi_params(sequence);
        match final_byte {
            b'm' => self.apply_sgr(&params),
            b'K' => self.erase_line(params.first().copied().unwrap_or(0)),
            b'J' => self.erase_display(params.first().copied().unwrap_or(0)),
            b'C' => {
                let amount = usize::from(params.first().copied().unwrap_or(1).max(1));
                self.cursor_column = self
                    .cursor_column
                    .saturating_add(amount)
                    .min(MAX_CURSOR_COLUMN);
            }
            b'D' => {
                let amount = usize::from(params.first().copied().unwrap_or(1).max(1));
                self.cursor_column = self.cursor_column.saturating_sub(amount);
            }
            b'G' => {
                let column = usize::from(params.first().copied().unwrap_or(1).max(1));
                self.cursor_column = column.saturating_sub(1).min(MAX_CURSOR_COLUMN);
            }
            b'H' | b'f' => {
                let column = usize::from(params.get(1).copied().unwrap_or(1).max(1));
                self.cursor_column = column.saturating_sub(1).min(MAX_CURSOR_COLUMN);
            }
            _ => {}
        }
    }

    fn apply_sgr(&mut self, params: &[u16]) {
        let params = if params.is_empty() { &[0][..] } else { params };
        let mut index = 0;
        while index < params.len() {
            match params[index] {
                0 => self.style = TextStyle::default(),
                1 => self.style.bold = true,
                2 => self.style.dim = true,
                3 => self.style.italic = true,
                4 => self.style.underline = true,
                7 => self.style.inverse = true,
                22 => {
                    self.style.bold = false;
                    self.style.dim = false;
                }
                23 => self.style.italic = false,
                24 => self.style.underline = false,
                27 => self.style.inverse = false,
                30..=37 => {
                    self.style.foreground = Some(AnsiColor::Indexed((params[index] - 30) as u8));
                }
                39 => self.style.foreground = None,
                40..=47 => {
                    self.style.background = Some(AnsiColor::Indexed((params[index] - 40) as u8));
                }
                49 => self.style.background = None,
                90..=97 => {
                    self.style.foreground =
                        Some(AnsiColor::Indexed((params[index] - 90 + 8) as u8));
                }
                100..=107 => {
                    self.style.background =
                        Some(AnsiColor::Indexed((params[index] - 100 + 8) as u8));
                }
                38 | 48 => {
                    let foreground = params[index] == 38;
                    if params.get(index + 1) == Some(&5) {
                        if let Some(value) = params
                            .get(index + 2)
                            .and_then(|value| u8::try_from(*value).ok())
                        {
                            self.set_color(foreground, AnsiColor::Indexed(value));
                            index += 2;
                        }
                    } else if params.get(index + 1) == Some(&2) && index + 4 < params.len() {
                        let red = u8::try_from(params[index + 2]).ok();
                        let green = u8::try_from(params[index + 3]).ok();
                        let blue = u8::try_from(params[index + 4]).ok();
                        if let (Some(red), Some(green), Some(blue)) = (red, green, blue) {
                            self.set_color(foreground, AnsiColor::Rgb(red, green, blue));
                            index += 4;
                        }
                    }
                }
                _ => {}
            }
            index += 1;
        }
    }

    fn set_color(&mut self, foreground: bool, color: AnsiColor) {
        if foreground {
            self.style.foreground = Some(color);
        } else {
            self.style.background = Some(color);
        }
    }

    fn erase_line(&mut self, mode: u16) {
        let Some(line) = self.lines.back_mut() else {
            return;
        };
        match mode {
            0 => {
                if self.cursor_column < line.len() {
                    let removed = line.len() - self.cursor_column;
                    line.truncate(self.cursor_column);
                    self.cell_count = self.cell_count.saturating_sub(removed);
                }
            }
            1 => {
                if !line.is_empty() {
                    let end = self.cursor_column.min(line.len().saturating_sub(1));
                    for cell in &mut line[..=end] {
                        *cell = TerminalCell {
                            value: ' ',
                            style: TextStyle::default(),
                        };
                    }
                }
            }
            2 => {
                self.cell_count = self.cell_count.saturating_sub(line.len());
                line.clear();
            }
            _ => {}
        }
    }

    fn erase_display(&mut self, mode: u16) {
        match mode {
            0 => self.erase_line(0),
            1 => {
                while self.lines.len() > 1 {
                    if let Some(line) = self.lines.pop_front() {
                        self.cell_count = self.cell_count.saturating_sub(line.len());
                    }
                }
                self.erase_line(1);
            }
            2 | 3 => self.clear_scrollback(),
            _ => {}
        }
    }

    fn push_utf8_byte(&mut self, byte: u8) {
        self.utf8_pending.push(byte);
        loop {
            match std::str::from_utf8(&self.utf8_pending) {
                Ok(text) => {
                    let decoded = text.to_owned();
                    self.utf8_pending.clear();
                    for character in decoded.chars() {
                        self.write_char(character);
                    }
                    break;
                }
                Err(problem) => {
                    let valid_up_to = problem.valid_up_to();
                    if valid_up_to > 0 {
                        if let Ok(text) = std::str::from_utf8(&self.utf8_pending[..valid_up_to]) {
                            let decoded = text.to_owned();
                            self.utf8_pending.drain(..valid_up_to);
                            for character in decoded.chars() {
                                self.write_char(character);
                            }
                            continue;
                        }
                    }
                    if let Some(error_len) = problem.error_len() {
                        let error_len = error_len.min(self.utf8_pending.len());
                        self.utf8_pending.drain(..error_len);
                        self.write_char('\u{fffd}');
                        continue;
                    }
                    break;
                }
            }
        }
    }

    fn finish_incomplete_utf8(&mut self) {
        if !self.utf8_pending.is_empty() {
            self.utf8_pending.clear();
            self.write_char('\u{fffd}');
        }
    }

    fn write_char(&mut self, value: char) {
        let Some(line) = self.lines.back_mut() else {
            self.lines.push_back(Vec::new());
            self.write_char(value);
            return;
        };

        if self.cursor_column > line.len() {
            let gap = self.cursor_column - line.len();
            line.extend(std::iter::repeat_n(
                TerminalCell {
                    value: ' ',
                    style: self.style,
                },
                gap,
            ));
            self.cell_count = self.cell_count.saturating_add(gap);
        }

        let cell = TerminalCell {
            value,
            style: self.style,
        };
        if self.cursor_column < line.len() {
            line[self.cursor_column] = cell;
        } else {
            line.push(cell);
            self.cell_count = self.cell_count.saturating_add(1);
        }
        self.cursor_column = self.cursor_column.saturating_add(1).min(MAX_CURSOR_COLUMN);
    }

    fn new_line(&mut self) {
        self.lines.push_back(Vec::new());
        self.cursor_column = 0;
        self.trim_scrollback();
    }

    fn begin_session(&mut self) {
        self.finish_incomplete_utf8();
        self.parser = ParserState::Ground;
        self.style = TextStyle::default();
        if self.lines.back().is_some_and(|line| !line.is_empty()) {
            self.new_line();
        }
    }

    fn reset_terminal(&mut self) {
        self.clear_scrollback();
        self.style = TextStyle::default();
        self.parser = ParserState::Ground;
        self.utf8_pending.clear();
    }

    fn clear_scrollback(&mut self) {
        self.lines.clear();
        self.lines.push_back(Vec::new());
        self.cursor_column = 0;
        self.cell_count = 0;
    }

    fn trim_scrollback(&mut self) {
        while self.lines.len() > MAX_SCROLLBACK_LINES || self.cell_count > MAX_SCROLLBACK_CELLS {
            if self.lines.len() <= 1 {
                break;
            }
            if let Some(line) = self.lines.pop_front() {
                self.cell_count = self.cell_count.saturating_sub(line.len());
            }
        }

        if self.cell_count > MAX_SCROLLBACK_CELLS {
            if let Some(line) = self.lines.front_mut() {
                let excess = (self.cell_count - MAX_SCROLLBACK_CELLS).min(line.len());
                line.drain(..excess);
                self.cell_count = self.cell_count.saturating_sub(excess);
                self.cursor_column = self.cursor_column.saturating_sub(excess);
            }
        }
    }

    fn rendered_lines(&self) -> Vec<Vec<TerminalRun>> {
        self.lines
            .iter()
            .map(|line| {
                let mut runs = Vec::<TerminalRun>::new();
                let mut active_style = None;
                for cell in line {
                    if active_style == Some(cell.style) {
                        if let Some(last) = runs.last_mut() {
                            last.text.push(cell.value);
                        }
                        continue;
                    }
                    active_style = Some(cell.style);
                    runs.push(TerminalRun {
                        text: cell.value.to_string(),
                        css: cell.style.css(),
                    });
                }
                runs
            })
            .collect()
    }

    #[cfg(test)]
    fn plain_text(&self) -> String {
        self.lines
            .iter()
            .map(|line| line.iter().map(|cell| cell.value).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn persistence_snapshot(&self) -> Self {
        let mut snapshot = self.clone();
        snapshot.finish_incomplete_utf8();
        snapshot.parser = ParserState::Ground;
        snapshot.style = TextStyle::default();
        snapshot.cursor_column = snapshot.lines.back().map_or(0, Vec::len);
        snapshot.trim_scrollback();
        snapshot
    }
}

fn csi_params(sequence: &[u8]) -> Vec<u16> {
    if sequence.is_empty() {
        return Vec::new();
    }
    String::from_utf8_lossy(sequence)
        .trim_start_matches(|character| matches!(character, '?' | '>' | '!'))
        .replace(':', ";")
        .split(';')
        .map(|value| value.parse::<u16>().unwrap_or(0))
        .collect()
}

fn load_scrollback(key: &str) -> TerminalBuffer {
    SCROLLBACK_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let Some(index) = cache.iter().position(|(entry_key, _)| entry_key == key) else {
            return TerminalBuffer::default();
        };
        let (_, buffer) = cache
            .remove(index)
            .expect("scrollback cache index is valid");
        let restored = buffer.clone();
        cache.push_back((key.to_owned(), buffer));
        restored
    })
}

fn store_scrollback(key: &str, buffer: &TerminalBuffer) {
    let snapshot = buffer.persistence_snapshot();
    SCROLLBACK_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(index) = cache.iter().position(|(entry_key, _)| entry_key == key) {
            cache.remove(index);
        }
        cache.push_back((key.to_owned(), snapshot));
        while cache.len() > MAX_PERSISTED_PROJECTS {
            cache.pop_front();
        }
    });
}

fn active_project_root(snapshot: &ProjectsSnapshot) -> Option<(String, String, String)> {
    let active_id = snapshot.active_id.as_deref()?;
    let project = snapshot
        .projects
        .iter()
        .find(|project| project.id == active_id)?;
    let folder = project
        .folders
        .iter()
        .find(|folder| folder.is_primary)
        .or_else(|| project.folders.first())?;
    Some((
        project.id.clone(),
        project.name.clone(),
        folder.path.clone(),
    ))
}

fn dimensions(cols: &str, rows: &str) -> Result<(u16, u16), String> {
    let cols = cols
        .trim()
        .parse::<u16>()
        .map_err(|_| "Terminal columns must be a positive integer.".to_owned())?;
    let rows = rows
        .trim()
        .parse::<u16>()
        .map_err(|_| "Terminal rows must be a positive integer.".to_owned())?;
    if cols == 0 || rows == 0 {
        return Err("Terminal dimensions must be greater than zero.".to_owned());
    }
    Ok((cols, rows))
}

fn append_output(mut output: Signal<TerminalBuffer>, bytes: &[u8]) {
    if bytes.is_empty() {
        return;
    }
    output.write().push_bytes(bytes);
}

#[component]
pub(super) fn Terminal() -> Element {
    let services = use_context::<AppServices>();
    let projects = use_context::<ProjectUiState>();
    let snapshot = (projects.snapshot)();
    let active = active_project_root(&snapshot);
    let initial_history_key = active.as_ref().map(|(project_id, _, _)| project_id.clone());
    let initial_output = initial_history_key
        .as_deref()
        .map(load_scrollback)
        .unwrap_or_default();

    let mut history_key = use_signal(move || initial_history_key);
    let mut terminal_id = use_signal(|| None::<String>);
    let mut output = use_signal(move || initial_output);
    let mut input = use_signal(String::new);
    let mut cols = use_signal(|| "120".to_owned());
    let mut rows = use_signal(|| "30".to_owned());
    let mut starting = use_signal(|| false);
    let mut mutating = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    let project_snapshot = projects.snapshot;
    let switch_service = services.terminal.clone();
    use_effect(move || {
        let next_key =
            active_project_root(&project_snapshot()).map(|(project_id, _, _)| project_id);
        let previous_key = history_key();
        if previous_key == next_key {
            return;
        }

        if let Some(key) = previous_key.as_deref() {
            store_scrollback(key, &output());
        }
        if let Some(id) = terminal_id() {
            let _ = switch_service.dispose_now(&id);
            terminal_id.set(None);
        }

        let restored = next_key.as_deref().map(load_scrollback).unwrap_or_default();
        output.set(restored);
        input.set(String::new());
        error.set(None);
        history_key.set(next_key);
    });

    let cleanup_service = services.terminal.clone();
    let cleanup_id = terminal_id;
    let cleanup_output = output;
    let cleanup_history_key = history_key;
    use_drop(move || {
        if let Some(key) = cleanup_history_key() {
            store_scrollback(&key, &cleanup_output());
        }
        if let Some(id) = cleanup_id() {
            let _ = cleanup_service.dispose_now(&id);
        }
    });

    let (has_scrollback, rendered_lines) = {
        let buffer = output.read();
        (buffer.cell_count > 0, buffer.rendered_lines())
    };
    let scrollback_line_count = if has_scrollback {
        rendered_lines.len()
    } else {
        0
    };
    let running = terminal_id().is_some();

    rsx! {
        Surface { eyebrow: "Developer tools", title: "Terminal", subtitle: "A native PTY session scoped to the active project, with bounded ANSI-aware scrollback restored when the project terminal is reopened.",
            if let Some((project_id, project_name, root)) = active {
                div { style: "display:grid;gap:1rem;min-height:0;",
                    section { class: "settings-card", style: "display:grid;gap:.75rem;",
                        header { style: "display:flex;align-items:flex-start;gap:.75rem;",
                            div { style: "min-width:0;flex:1;",
                                strong { "{project_name}" }
                                div { class: "muted", title: "{root}", "{root}" }
                            }
                            if let Some(id) = terminal_id() {
                                span { class: "scope-pill", title: "{id}", "PTY active" }
                            } else if has_scrollback {
                                span { class: "scope-pill", title: "Scrollback is cached in memory for this project.", "History restored" }
                            }
                        }
                        div { style: "display:flex;gap:.5rem;align-items:end;flex-wrap:wrap;",
                            label { class: "field-stack", span { "Columns" }
                                input { r#type: "number", min: "1", max: "1000", value: "{cols}", disabled: mutating(), oninput: move |event| cols.set(event.value()) }
                            }
                            label { class: "field-stack", span { "Rows" }
                                input { r#type: "number", min: "1", max: "1000", value: "{rows}", disabled: mutating(), oninput: move |event| rows.set(event.value()) }
                            }
                            if !running {
                                button { class: "button", disabled: starting(), onclick: {
                                    let start_service = services.terminal.clone();
                                    let read_service = services.terminal.clone();
                                    let cwd = root.clone();
                                    let history_project_id = project_id.clone();
                                    move |_| {
                                        let Ok((next_cols, next_rows)) = dimensions(&cols(), &rows()) else {
                                            error.set(Some("Enter valid non-zero terminal dimensions.".to_owned()));
                                            return;
                                        };
                                        let start_service = start_service.clone();
                                        let read_service = read_service.clone();
                                        let cwd = cwd.clone();
                                        let history_project_id = history_project_id.clone();
                                        starting.set(true);
                                        error.set(None);
                                        output.write().begin_session();
                                        store_scrollback(&history_project_id, &output());
                                        spawn(async move {
                                            match start_service.start(Path::new(&cwd), next_cols, next_rows).await {
                                                Ok(id) => {
                                                    terminal_id.set(Some(id.clone()));
                                                    starting.set(false);
                                                    loop {
                                                        if terminal_id().as_deref() != Some(id.as_str()) {
                                                            break;
                                                        }
                                                        match read_service.read(&id).await {
                                                            Ok(bytes) => append_output(output, &bytes),
                                                            Err(ServiceError::NotFound(_)) => {
                                                                terminal_id.set(None);
                                                                break;
                                                            }
                                                            Err(problem) => {
                                                                error.set(Some(problem.to_string()));
                                                                break;
                                                            }
                                                        }
                                                        tokio::time::sleep(std::time::Duration::from_millis(READ_INTERVAL_MS)).await;
                                                    }
                                                }
                                                Err(problem) => {
                                                    error.set(Some(problem.to_string()));
                                                    starting.set(false);
                                                }
                                            }
                                        });
                                    }
                                }, if starting() { "Starting…" } else { "Start terminal" } }
                            } else {
                                button { class: "button", disabled: mutating(), onclick: {
                                    let service = services.terminal.clone();
                                    move |_| {
                                        let Some(id) = terminal_id() else { return; };
                                        let Ok((next_cols, next_rows)) = dimensions(&cols(), &rows()) else {
                                            error.set(Some("Enter valid non-zero terminal dimensions.".to_owned()));
                                            return;
                                        };
                                        let service = service.clone();
                                        mutating.set(true);
                                        error.set(None);
                                        spawn(async move {
                                            if let Err(problem) = service.resize(&id, next_cols, next_rows).await {
                                                error.set(Some(problem.to_string()));
                                            }
                                            mutating.set(false);
                                        });
                                    }
                                }, "Resize" }
                                button { class: "button", disabled: mutating(), onclick: {
                                    let service = services.terminal.clone();
                                    move |_| {
                                        let Some(id) = terminal_id() else { return; };
                                        let service = service.clone();
                                        mutating.set(true);
                                        error.set(None);
                                        spawn(async move {
                                            match service.dispose(&id).await {
                                                Ok(()) => terminal_id.set(None),
                                                Err(problem) => error.set(Some(problem.to_string())),
                                            }
                                            mutating.set(false);
                                        });
                                    }
                                }, "Dispose" }
                            }
                            button { class: "button", disabled: !has_scrollback, onclick: move |_| {
                                output.write().clear_scrollback();
                                store_scrollback(&project_id, &output());
                            }, "Clear scrollback" }
                        }
                    }

                    section { class: "settings-card", style: "display:grid;gap:.75rem;min-height:24rem;",
                        div { style: "display:flex;align-items:center;gap:.5rem;",
                            strong { "Terminal output" }
                            span { class: "muted", "{scrollback_line_count} lines · max {MAX_SCROLLBACK_LINES}" }
                        }
                        div {
                            role: "log",
                            aria_label: "Terminal output",
                            aria_live: "off",
                            style: "margin:0;min-height:16rem;max-height:34rem;overflow:auto;white-space:pre;font-family:var(--font-mono,monospace);background:var(--terminal-background,rgba(0,0,0,.18));padding:.75rem;border-radius:.45rem;",
                            if !has_scrollback {
                                span { class: "muted", "Terminal output will appear here." }
                            } else {
                                for line in rendered_lines {
                                    div { style: "min-height:1.2em;",
                                        for run in line {
                                            span { style: "{run.css}", "{run.text}" }
                                        }
                                    }
                                }
                            }
                        }
                        textarea { aria_label: "Terminal input", placeholder: if running { "Type raw terminal input…" } else { "Start the terminal first" }, rows: "3", value: "{input}", disabled: !running || mutating(), oninput: move |event| input.set(event.value()) }
                        div { style: "display:flex;gap:.5rem;flex-wrap:wrap;",
                            button { class: "button", disabled: !running || mutating() || input().is_empty(), onclick: {
                                let service = services.terminal.clone();
                                move |_| {
                                    let Some(id) = terminal_id() else { return; };
                                    let bytes = input().into_bytes();
                                    let service = service.clone();
                                    mutating.set(true);
                                    error.set(None);
                                    spawn(async move {
                                        match service.write(&id, &bytes).await {
                                            Ok(()) => input.set(String::new()),
                                            Err(problem) => error.set(Some(problem.to_string())),
                                        }
                                        mutating.set(false);
                                    });
                                }
                            }, "Send" }
                            button { class: "button", disabled: !running || mutating(), onclick: {
                                let service = services.terminal.clone();
                                move |_| {
                                    let Some(id) = terminal_id() else { return; };
                                    let mut bytes = input().into_bytes();
                                    bytes.extend_from_slice(b"\r\n");
                                    let service = service.clone();
                                    mutating.set(true);
                                    error.set(None);
                                    spawn(async move {
                                        match service.write(&id, &bytes).await {
                                            Ok(()) => input.set(String::new()),
                                            Err(problem) => error.set(Some(problem.to_string())),
                                        }
                                        mutating.set(false);
                                    });
                                }
                            }, "Send + Enter" }
                            button { class: "button", disabled: !running || mutating(), onclick: {
                                let service = services.terminal.clone();
                                move |_| {
                                    let Some(id) = terminal_id() else { return; };
                                    let service = service.clone();
                                    mutating.set(true);
                                    error.set(None);
                                    spawn(async move {
                                        if let Err(problem) = service.write(&id, b"\x03").await {
                                            error.set(Some(problem.to_string()));
                                        }
                                        mutating.set(false);
                                    });
                                }
                            }, "Ctrl+C" }
                        }
                        if let Some(problem) = error() { p { class: "inline-error", role: "alert", "{problem}" } }
                    }
                }
            } else {
                div { class: "settings-card", p { "Select an active project before starting a terminal so the PTY has an explicit workspace cwd." } }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AnsiColor, MAX_SCROLLBACK_LINES, TerminalBuffer, TextStyle, csi_params, dimensions,
    };

    #[test]
    fn dimensions_reject_zero_and_non_numeric_values() {
        assert_eq!(dimensions("120", "30"), Ok((120, 30)));
        assert!(dimensions("0", "30").is_err());
        assert!(dimensions("120", "nope").is_err());
    }

    #[test]
    fn ansi_sgr_is_rendered_without_control_bytes() {
        let mut buffer = TerminalBuffer::default();
        buffer.push_bytes(b"plain \x1b[1;31mred\x1b[0m normal");

        assert_eq!(buffer.plain_text(), "plain red normal");
        let runs = buffer.rendered_lines();
        assert_eq!(runs.len(), 1);
        assert!(runs[0].iter().any(|run| {
            run.text == "red" && run.css.contains("font-weight:700") && run.css.contains("#cd3131")
        }));
    }

    #[test]
    fn split_truecolor_escape_sequences_survive_read_boundaries() {
        let mut buffer = TerminalBuffer::default();
        buffer.push_bytes(b"\x1b[38;2;12");
        buffer.push_bytes(b";34;56mX\x1b[0m");

        assert_eq!(buffer.plain_text(), "X");
        let runs = buffer.rendered_lines();
        assert!(runs[0][0].css.contains("rgb(12 34 56)"));
    }

    #[test]
    fn carriage_return_and_erase_line_support_progress_updates() {
        let mut buffer = TerminalBuffer::default();
        buffer.push_bytes(b"progress 10%\rprogress 90%\r\x1b[Kdone");

        assert_eq!(buffer.plain_text(), "done");
    }

    #[test]
    fn osc_metadata_is_suppressed_instead_of_rendered() {
        let mut buffer = TerminalBuffer::default();
        buffer.push_bytes(b"before\x1b]0;window title\x07after");
        buffer.push_bytes(b"\x1b]8;;https://example.invalid\x1b\\link\x1b]8;;\x1b\\");

        assert_eq!(buffer.plain_text(), "beforeafterlink");
    }

    #[test]
    fn utf8_split_across_reads_is_reassembled() {
        let mut buffer = TerminalBuffer::default();
        let bytes = "λ🙂".as_bytes();
        buffer.push_bytes(&bytes[..2]);
        buffer.push_bytes(&bytes[2..5]);
        buffer.push_bytes(&bytes[5..]);

        assert_eq!(buffer.plain_text(), "λ🙂");
    }

    #[test]
    fn scrollback_is_bounded_by_line_count() {
        let mut buffer = TerminalBuffer::default();
        for _ in 0..(MAX_SCROLLBACK_LINES + 100) {
            buffer.push_bytes(b"x\n");
        }

        assert!(buffer.lines.len() <= MAX_SCROLLBACK_LINES);
        assert!(buffer.plain_text().ends_with('\n') || buffer.plain_text().ends_with('x'));
    }

    #[test]
    fn sgr_256_colour_and_parser_normalization_are_supported() {
        let mut buffer = TerminalBuffer::default();
        buffer.push_bytes(b"\x1b[38;5;196mX");

        assert_eq!(buffer.style.foreground, Some(AnsiColor::Indexed(196)));
        assert_eq!(csi_params(b"?25"), vec![25]);
        assert_eq!(
            TextStyle {
                foreground: Some(AnsiColor::Indexed(196)),
                ..TextStyle::default()
            }
            .css(),
            "color:rgb(255 0 0)"
        );
    }

    #[test]
    fn persistence_snapshot_drops_partial_parser_state_but_keeps_scrollback() {
        let mut buffer = TerminalBuffer::default();
        buffer.push_bytes(b"kept\x1b[31");
        let snapshot = buffer.persistence_snapshot();

        assert_eq!(snapshot.plain_text(), "kept");
        assert_eq!(snapshot.style, TextStyle::default());
        assert!(snapshot.utf8_pending.is_empty());
    }
}
