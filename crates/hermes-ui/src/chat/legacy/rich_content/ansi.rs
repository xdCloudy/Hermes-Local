use dioxus::prelude::*;

const MAX_ANSI_BYTES: usize = 256_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AnsiColor {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AnsiSegment {
    text: String,
    bold: bool,
    color: Option<AnsiColor>,
}

pub(super) fn has_ansi(value: &str) -> bool {
    value.as_bytes().contains(&0x1b)
}

fn bounded(value: &str) -> &str {
    if value.len() <= MAX_ANSI_BYTES {
        return value;
    }
    let mut end = MAX_ANSI_BYTES;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}

fn color_for(code: u16) -> Option<AnsiColor> {
    Some(match code {
        30 => AnsiColor::Black,
        31 => AnsiColor::Red,
        32 => AnsiColor::Green,
        33 => AnsiColor::Yellow,
        34 => AnsiColor::Blue,
        35 => AnsiColor::Magenta,
        36 => AnsiColor::Cyan,
        37 => AnsiColor::White,
        90 => AnsiColor::BrightBlack,
        91 => AnsiColor::BrightRed,
        92 => AnsiColor::BrightGreen,
        93 => AnsiColor::BrightYellow,
        94 => AnsiColor::BrightBlue,
        95 => AnsiColor::BrightMagenta,
        96 => AnsiColor::BrightCyan,
        97 => AnsiColor::BrightWhite,
        _ => return None,
    })
}

fn apply_sgr(params: &str, bold: &mut bool, color: &mut Option<AnsiColor>) {
    let codes = if params.is_empty() {
        vec![0]
    } else {
        params
            .split(';')
            .filter_map(|part| part.parse::<u16>().ok())
            .collect::<Vec<_>>()
    };
    let mut index = 0;
    while index < codes.len() {
        let code = codes[index];
        match code {
            0 => {
                *bold = false;
                *color = None;
            }
            1 => *bold = true,
            22 => *bold = false,
            39 => *color = None,
            38 if codes.get(index + 1) == Some(&5) => index = index.saturating_add(2),
            38 if codes.get(index + 1) == Some(&2) => index = index.saturating_add(4),
            _ => {
                if let Some(next) = color_for(code) {
                    *color = Some(next);
                }
            }
        }
        index += 1;
    }
}

fn push_segment(
    segments: &mut Vec<AnsiSegment>,
    text: &str,
    bold: bool,
    color: Option<AnsiColor>,
) {
    let text = text
        .chars()
        .filter(|ch| !ch.is_control() || matches!(ch, '\n' | '\r' | '\t'))
        .collect::<String>();
    if text.is_empty() {
        return;
    }
    if let Some(previous) = segments.last_mut()
        && previous.bold == bold
        && previous.color == color
    {
        previous.text.push_str(&text);
        return;
    }
    segments.push(AnsiSegment { text, bold, color });
}

fn parse_ansi(value: &str) -> Vec<AnsiSegment> {
    let value = bounded(value);
    let bytes = value.as_bytes();
    let mut segments = Vec::new();
    let mut cursor = 0;
    let mut bold = false;
    let mut color = None;

    while cursor < bytes.len() {
        let Some(relative) = bytes[cursor..].iter().position(|byte| *byte == 0x1b) else {
            push_segment(&mut segments, &value[cursor..], bold, color);
            break;
        };
        let escape = cursor + relative;
        if escape > cursor {
            push_segment(&mut segments, &value[cursor..escape], bold, color);
        }
        if escape + 1 >= bytes.len() {
            break;
        }

        match bytes[escape + 1] {
            b'[' => {
                let mut end = escape + 2;
                while end < bytes.len() && !(0x40..=0x7e).contains(&bytes[end]) {
                    end += 1;
                }
                if end >= bytes.len() {
                    break;
                }
                if bytes[end] == b'm' {
                    let params = std::str::from_utf8(&bytes[escape + 2..end]).unwrap_or_default();
                    apply_sgr(params, &mut bold, &mut color);
                }
                cursor = end + 1;
            }
            b']' => {
                let mut end = escape + 2;
                while end < bytes.len() {
                    if bytes[end] == 0x07 {
                        end += 1;
                        break;
                    }
                    if bytes[end] == 0x1b && bytes.get(end + 1) == Some(&b'\\') {
                        end += 2;
                        break;
                    }
                    end += 1;
                }
                cursor = end;
            }
            _ => cursor = (escape + 2).min(bytes.len()),
        }
    }

    segments
}

fn color_css(color: AnsiColor) -> &'static str {
    match color {
        AnsiColor::Black => "#6e7681",
        AnsiColor::Red => "#ff7b72",
        AnsiColor::Green => "#7ee787",
        AnsiColor::Yellow => "#d29922",
        AnsiColor::Blue => "#79c0ff",
        AnsiColor::Magenta => "#d2a8ff",
        AnsiColor::Cyan => "#a5d6ff",
        AnsiColor::White => "#c9d1d9",
        AnsiColor::BrightBlack => "#8b949e",
        AnsiColor::BrightRed => "#ffa198",
        AnsiColor::BrightGreen => "#aff5b4",
        AnsiColor::BrightYellow => "#e3b341",
        AnsiColor::BrightBlue => "#a5d6ff",
        AnsiColor::BrightMagenta => "#e2c5ff",
        AnsiColor::BrightCyan => "#b3f0ff",
        AnsiColor::BrightWhite => "#f0f6fc",
    }
}

#[component]
pub(super) fn AnsiContent(text: String) -> Element {
    let segments = parse_ansi(&text);
    rsx! {
        pre { class: "rich-ansi", aria_label: "ANSI terminal output",
            for (index, segment) in segments.into_iter().enumerate() {
                {
                    let mut style = String::new();
                    if segment.bold {
                        style.push_str("font-weight:700;");
                    }
                    if let Some(color) = segment.color {
                        style.push_str("color:");
                        style.push_str(color_css(color));
                        style.push(';');
                    }
                    rsx! { span { key: "{index}", style, "{segment.text}" } }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_sgr_color_and_bold_then_resets() {
        let segments = parse_ansi("plain \u{1b}[1;31merror\u{1b}[0m done");
        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0].text, "plain ");
        assert_eq!(segments[1].text, "error");
        assert!(segments[1].bold);
        assert_eq!(segments[1].color, Some(AnsiColor::Red));
        assert_eq!(segments[2].text, " done");
        assert!(!segments[2].bold);
        assert_eq!(segments[2].color, None);
    }

    #[test]
    fn drops_cursor_and_osc_sequences() {
        let segments = parse_ansi("a\u{1b}[2Jb\u{1b}]0;title\u{7}c");
        assert_eq!(segments.iter().map(|segment| segment.text.as_str()).collect::<String>(), "abc");
    }

    #[test]
    fn unsupported_extended_colors_are_consumed_safely() {
        let segments = parse_ansi("\u{1b}[38;2;1;2;3mtrue\u{1b}[0m");
        assert_eq!(segments.iter().map(|segment| segment.text.as_str()).collect::<String>(), "true");
    }
}
