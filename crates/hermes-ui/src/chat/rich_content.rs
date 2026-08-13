use dioxus::prelude::*;

const MAX_BYTES: usize = 1_000_000;
const MAX_BLOCKS: usize = 512;
const MAX_CODE_BYTES: usize = 256_000;
const MAX_TABLE_COLUMNS: usize = 32;

#[derive(Clone, Debug, PartialEq)]
enum Block {
    Heading(String),
    Quote(String),
    List(String),
    Code(String, String),
    Math(String),
    Table(Vec<String>),
    Diff(String),
    Mermaid(String),
    Ansi(String),
    Image(String, String),
    Paragraph(String),
}

fn bounded(value: &str, limit: usize) -> String {
    if value.len() <= limit {
        return value.to_owned();
    }
    let mut end = limit;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}

fn strip_ansi(value: &str) -> String {
    let mut output = String::new();
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\u{1b}' {
            if !ch.is_control() || matches!(ch, '\n' | '\r' | '\t') {
                output.push(ch);
            }
            continue;
        }
        if chars.peek() == Some(&'[') {
            chars.next();
            for next in chars.by_ref() {
                if ('@'..='~').contains(&next) {
                    break;
                }
            }
        }
    }
    output
}

fn image_target(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix("![")?;
    let alt_end = rest.find(']')?;
    let target = rest.get(alt_end + 1..)?.strip_prefix('(')?.strip_suffix(')')?;
    let lower = target.to_ascii_lowercase();
    let allowed = lower.starts_with("https://")
        || lower.starts_with("http://")
        || lower.starts_with("data:image/png;base64,")
        || lower.starts_with("data:image/jpeg;base64,")
        || lower.starts_with("data:image/gif;base64,")
        || lower.starts_with("data:image/webp;base64,");
    allowed.then(|| (rest[..alt_end].to_owned(), target.to_owned()))
}

fn mermaid_allowed(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    value.len() <= MAX_CODE_BYTES
        && !value.contains('<')
        && !value.contains('>')
        && !lower.contains("href")
        && !lower.contains("url(")
}

fn parse(value: &str) -> Vec<Block> {
    let source = bounded(value, MAX_BYTES);
    let lines = source.lines().collect::<Vec<_>>();
    let mut blocks = Vec::new();
    let mut index = 0;
    while index < lines.len() && blocks.len() < MAX_BLOCKS {
        let line = lines[index].trim();
        if line.is_empty() {
            index += 1;
            continue;
        }
        if let Some(language) = line.strip_prefix("```") {
            let language = language.trim().to_ascii_lowercase();
            let mut body = String::new();
            index += 1;
            while index < lines.len() && !lines[index].trim().starts_with("```") {
                if !body.is_empty() {
                    body.push('\n');
                }
                if body.len() < MAX_CODE_BYTES {
                    body.push_str(lines[index]);
                }
                index += 1;
            }
            index = (index + 1).min(lines.len());
            let body = bounded(&body, MAX_CODE_BYTES);
            blocks.push(match language.as_str() {
                "mermaid" | "mmd" => Block::Mermaid(body),
                "math" | "tex" | "latex" => Block::Math(body),
                "diff" | "patch" => Block::Diff(body),
                "ansi" | "terminal" => Block::Ansi(strip_ansi(&body)),
                _ => Block::Code(language, body),
            });
            continue;
        }
        if let Some((alt, target)) = image_target(line) {
            blocks.push(Block::Image(alt, target));
        } else if let Some(text) = line.strip_prefix("# ").or_else(|| line.strip_prefix("## ")) {
            blocks.push(Block::Heading(text.to_owned()));
        } else if let Some(text) = line.strip_prefix("> ") {
            blocks.push(Block::Quote(text.to_owned()));
        } else if let Some(text) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
            blocks.push(Block::List(text.to_owned()));
        } else if line.starts_with('$') && line.ends_with('$') && line.len() > 2 {
            blocks.push(Block::Math(line.trim_matches('$').to_owned()));
        } else if line.contains('|') {
            blocks.push(Block::Table(
                line.trim_matches('|')
                    .split('|')
                    .take(MAX_TABLE_COLUMNS)
                    .map(|cell| cell.trim().to_owned())
                    .collect(),
            ));
        } else if line.starts_with("@@") || line.starts_with("diff --git ") {
            blocks.push(Block::Diff(line.to_owned()));
        } else if line.contains('\u{1b}') {
            blocks.push(Block::Ansi(strip_ansi(line)));
        } else {
            blocks.push(Block::Paragraph(line.to_owned()));
        }
        index += 1;
    }
    blocks
}

#[component]
pub(super) fn RichContent(text: String, on_open_link: Callback<String>) -> Element {
    let _ = on_open_link;
    rsx! {
        div { class: "rich-content",
            for (index, block) in parse(&text).into_iter().enumerate() {
                div { class: "rich-block", key: "{index}",
                    match block {
                        Block::Heading(value) => rsx! { h2 { "{value}" } },
                        Block::Quote(value) => rsx! { blockquote { "{value}" } },
                        Block::List(value) => rsx! { ul { li { "{value}" } } },
                        Block::Code(language, value) => rsx! { figure { class: "rich-code-card", figcaption { if language.is_empty() { "text" } else { "{language}" } } pre { code { "{value}" } } } },
                        Block::Math(value) => rsx! { div { class: "rich-math-display", role: "math", code { "{value}" } } },
                        Block::Table(cells) => rsx! { div { class: "rich-table-scroll", table { tbody { tr { for cell in cells { td { "{cell}" } } } } } } },
                        Block::Diff(value) => rsx! { pre { class: "rich-diff", "{value}" } },
                        Block::Ansi(value) => rsx! { pre { class: "rich-ansi", "{value}" } },
                        Block::Image(alt, target) => rsx! { figure { class: "rich-image", img { src: "{target}", alt: "{alt}", loading: "lazy" } } },
                        Block::Mermaid(value) => if mermaid_allowed(&value) { rsx! { figure { class: "rich-mermaid", figcaption { "Mermaid" } pre { "{value}" } } } } else { rsx! { p { class: "inline-error", role: "alert", "Diagram unavailable." } } },
                        Block::Paragraph(value) => rsx! { p { "{value}" } },
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_window_is_bounded() {
        assert!(!parse(&"x".repeat(MAX_BYTES + 16)).is_empty());
    }

    #[test]
    fn ansi_sequences_are_removed() {
        assert_eq!(strip_ansi("\u{1b}[31mred\u{1b}[0m"), "red");
    }

    #[test]
    fn table_width_is_bounded() {
        let line = (0..100)
            .map(|index| index.to_string())
            .collect::<Vec<_>>()
            .join("|");
        let blocks = parse(&line);
        let Block::Table(cells) = &blocks[0] else {
            panic!("expected table");
        };
        assert_eq!(cells.len(), MAX_TABLE_COLUMNS);
    }

    #[test]
    fn supported_image_targets_are_recognized() {
        assert!(image_target("![preview](https://example.com/image.png)").is_some());
        assert!(image_target("![preview](data:image/png;base64,AA==)").is_some());
    }

    #[test]
    fn active_mermaid_markup_is_rejected() {
        assert!(mermaid_allowed("graph TD\nA--B"));
        assert!(!mermaid_allowed("graph TD\nA[<tag>]"));
    }
}
