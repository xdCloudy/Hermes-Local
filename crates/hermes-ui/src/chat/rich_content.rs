use dioxus::prelude::*;

const MAX_BYTES: usize = 1_000_000;
const MAX_BLOCKS: usize = 512;
const MAX_CODE_BYTES: usize = 256_000;
const MAX_TABLE_COLUMNS: usize = 32;
const MAX_TABLE_ROWS: usize = 256;

#[derive(Clone, Debug, PartialEq)]
enum Block {
    Heading(u8, String),
    Paragraph(String),
    Quote(String),
    List(Vec<String>),
    Code(String, String),
    Math(String),
    Table(Vec<Vec<String>>),
    Diff(String),
    Ansi(String),
    Mermaid(String),
}

fn bounded(value: &str, limit: usize) -> String {
    if value.len() <= limit {
        return value.to_owned();
    }
    let mut end = limit;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n[truncated]", &value[..end])
}

fn safe_url(value: &str, image: bool) -> bool {
    let value = value.trim();
    if value.is_empty() || value.len() > 16_384 || value.chars().any(char::is_control) {
        return false;
    }
    let lower = value.to_ascii_lowercase();
    if image
        && [
            "data:image/png;base64,",
            "data:image/jpeg;base64,",
            "data:image/gif;base64,",
            "data:image/webp;base64,",
        ]
        .iter()
        .any(|prefix| lower.starts_with(prefix))
    {
        return true;
    }
    if !(lower.starts_with("https://") || lower.starts_with("http://")) {
        return false;
    }
    value
        .split_once("://")
        .map(|(_, rest)| {
            let authority = rest.split('/').next().unwrap_or_default();
            !authority.is_empty() && !authority.contains('@')
        })
        .unwrap_or(false)
}

fn row(line: &str) -> Vec<String> {
    line.trim()
        .trim_matches('|')
        .split('|')
        .take(MAX_TABLE_COLUMNS)
        .map(|cell| cell.trim().to_owned())
        .collect()
}

fn separator(line: &str) -> bool {
    let cells = row(line);
    !cells.is_empty()
        && cells.iter().all(|cell| {
            let body = cell.trim().trim_matches(':');
            body.len() >= 3 && body.chars().all(|ch| ch == '-')
        })
}

fn parse(source: &str) -> Vec<Block> {
    let source = bounded(source, MAX_BYTES);
    let lines = source.lines().collect::<Vec<_>>();
    let mut blocks = Vec::new();
    let mut index = 0;
    while index < lines.len() && blocks.len() < MAX_BLOCKS {
        let line = lines[index];
        let trimmed = line.trim();
        if trimmed.is_empty() {
            index += 1;
            continue;
        }
        if let Some(info) = trimmed.strip_prefix("```") {
            let language = info.trim().to_ascii_lowercase();
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
                "math" | "tex" | "latex" => Block::Math(body),
                "diff" | "patch" => Block::Diff(body),
                "ansi" | "terminal" => Block::Ansi(body),
                "mermaid" | "mmd" => Block::Mermaid(body),
                _ => Block::Code(language, body),
            });
            continue;
        }
        let level = trimmed.chars().take_while(|ch| *ch == '#').count();
        if (1..=6).contains(&level) && trimmed.chars().nth(level) == Some(' ') {
            blocks.push(Block::Heading(level as u8, trimmed[level + 1..].to_owned()));
            index += 1;
            continue;
        }
        if let Some(value) = trimmed.strip_prefix("> ") {
            blocks.push(Block::Quote(value.to_owned()));
            index += 1;
            continue;
        }
        if line.contains('|') && index + 1 < lines.len() && separator(lines[index + 1]) {
            let mut rows = vec![row(line)];
            index += 2;
            while index < lines.len() && lines[index].contains('|') && rows.len() < MAX_TABLE_ROWS {
                rows.push(row(lines[index]));
                index += 1;
            }
            blocks.push(Block::Table(rows));
            continue;
        }
        if let Some(item) = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")) {
            let mut items = vec![item.to_owned()];
            index += 1;
            while index < lines.len() {
                let next = lines[index].trim();
                let Some(item) = next.strip_prefix("- ").or_else(|| next.strip_prefix("* ")) else {
                    break;
                };
                items.push(item.to_owned());
                index += 1;
            }
            blocks.push(Block::List(items));
            continue;
        }
        if trimmed.starts_with("diff --git ") || trimmed.starts_with("@@ ") {
            blocks.push(Block::Diff(bounded(line, MAX_CODE_BYTES)));
            index += 1;
            continue;
        }
        if line.contains('\u{1b}') {
            blocks.push(Block::Ansi(strip_ansi(line)));
            index += 1;
            continue;
        }
        blocks.push(Block::Paragraph(trimmed.to_owned()));
        index += 1;
    }
    blocks
}

fn strip_ansi(value: &str) -> String {
    let mut out = String::new();
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\u{1b}' {
            if !ch.is_control() || matches!(ch, '\n' | '\r' | '\t') {
                out.push(ch);
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
    out
}

fn mermaid_safe(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    value.len() <= MAX_CODE_BYTES
        && !value.contains('<')
        && !value.contains('>')
        && !lower.contains("href")
        && !lower.contains("url(")
        && !lower.lines().any(|line| line.trim_start().starts_with("click "))
}

fn markdown_link(value: &str) -> Option<(&str, &str)> {
    let start = value.find('[')?;
    let label_end = value[start + 1..].find(']')? + start + 1;
    if value.get(label_end + 1..label_end + 2)? != "(" {
        return None;
    }
    let url_end = value[label_end + 2..].find(')')? + label_end + 2;
    Some((&value[start + 1..label_end], &value[label_end + 2..url_end]))
}

fn markdown_image(value: &str) -> Option<(&str, &str)> {
    let value = value.strip_prefix("![")?;
    let alt_end = value.find(']')?;
    if value.get(alt_end + 1..alt_end + 2)? != "(" {
        return None;
    }
    let url_end = value[alt_end + 2..].find(')')? + alt_end + 2;
    Some((&value[..alt_end], &value[alt_end + 2..url_end]))
}

#[component]
fn Inline(text: String, on_open_link: Callback<String>) -> Element {
    if let Some((alt, url)) = markdown_image(&text) {
        if safe_url(url, true) {
            return rsx! { figure { class: "rich-image", img { src: "{url}", alt: "{alt}", loading: "lazy" } } };
        }
    }
    if let Some((label, url)) = markdown_link(&text) {
        if safe_url(url, false) {
            let destination = url.to_owned();
            return rsx! { a { class: "rich-link", href: "{url}", rel: "noopener noreferrer", target: "_blank", onclick: move |event| { event.prevent_default(); on_open_link.call(destination.clone()); }, "{label}" } };
        }
    }
    rsx! { span { "{text}" } }
}

#[component]
pub(super) fn RichContent(text: String, on_open_link: Callback<String>) -> Element {
    rsx! {
        div { class: "rich-content",
            for (index, block) in parse(&text).into_iter().enumerate() {
                div { class: "rich-block", key: "{index}",
                    match block {
                        Block::Heading(level, value) => if level <= 2 { rsx! { h2 { Inline { text: value, on_open_link } } } } else { rsx! { h3 { Inline { text: value, on_open_link } } } },
                        Block::Paragraph(value) => rsx! { p { Inline { text: value, on_open_link } } },
                        Block::Quote(value) => rsx! { blockquote { Inline { text: value, on_open_link } } },
                        Block::List(items) => rsx! { ul { for item in items { li { Inline { text: item, on_open_link } } } } },
                        Block::Code(language, value) => rsx! { figure { class: "rich-code-card", figcaption { if language.is_empty() { "text" } else { "{language}" } } pre { code { "{value}" } } } },
                        Block::Math(value) => rsx! { div { class: "rich-math-display", role: "math", code { "{value}" } } },
                        Block::Table(rows) => rsx! { div { class: "rich-table-scroll", table { tbody { for row in rows { tr { for cell in row { td { "{cell}" } } } } } } } },
                        Block::Diff(value) => rsx! { pre { class: "rich-diff", "{value}" } },
                        Block::Ansi(value) => rsx! { pre { class: "rich-ansi", "{value}" } },
                        Block::Mermaid(value) => if mermaid_safe(&value) { rsx! { figure { class: "rich-mermaid", figcaption { "Mermaid" } pre { "{value}" } } } } else { rsx! { p { class: "inline-error", role: "alert", "Diagram blocked by renderer policy." } } },
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
    fn bounds_large_content_and_wide_tables() {
        assert!(!parse(&"x".repeat(MAX_BYTES + 32)).is_empty());
        let wide = (0..100).map(|n| n.to_string()).collect::<Vec<_>>().join("|");
        assert_eq!(row(&wide).len(), MAX_TABLE_COLUMNS);
    }

    #[test]
    fn link_and_image_policy_is_explicit() {
        assert!(safe_url("https://example.com/a", false));
        assert!(!safe_url("file:///tmp/a", false));
        assert!(safe_url("data:image/png;base64,AA==", true));
        assert!(!safe_url("data:text/plain;base64,AA==", true));
    }

    #[test]
    fn parses_required_blocks() {
        let blocks = parse("# H\n\n```rust\nfn main() {}\n```\n|a|b|\n|---|---|\n|1|2|");
        assert!(blocks.iter().any(|block| matches!(block, Block::Heading(..))));
        assert!(blocks.iter().any(|block| matches!(block, Block::Code(..))));
        assert!(blocks.iter().any(|block| matches!(block, Block::Table(_))));
    }

    #[test]
    fn strips_ansi_control_sequences() {
        assert_eq!(strip_ansi("\u{1b}[31mred\u{1b}[0m"), "red");
    }

    #[test]
    fn blocks_active_mermaid_directives() {
        assert!(mermaid_safe("graph TD\nA--B"));
        assert!(!mermaid_safe("graph TD\nclick A link"));
    }
}
