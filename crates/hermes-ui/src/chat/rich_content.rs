use dioxus::prelude::*;

const MAX_CONTENT_BYTES: usize = 1_000_000;
const MAX_CODE_BYTES: usize = 256_000;
const MAX_BLOCKS: usize = 512;
const MAX_TABLE_COLUMNS: usize = 32;
const MAX_TABLE_ROWS: usize = 256;

#[derive(Clone, Debug, PartialEq)]
enum Block {
    Heading(u8, String),
    Paragraph(String),
    Quote(String),
    List { ordered: bool, items: Vec<String> },
    Code { language: String, text: String },
    Math(String),
    Table(Vec<Vec<String>>),
    Diff(String),
    Ansi(String),
    Mermaid(String),
}

#[derive(Clone, Debug, PartialEq)]
enum Inline {
    Text(String),
    Code(String),
    Math(String),
    Link { label: String, url: String },
    Image { alt: String, url: String },
    Blocked(String),
}

fn bounded(value: &str, max: usize) -> String {
    if value.len() <= max {
        return value.to_owned();
    }
    let mut end = max;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n\n[oversized content truncated]", &value[..end])
}

fn safe_target(value: &str, image: bool) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 16_384 || value.chars().any(char::is_control) {
        return None;
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
        return Some(value.to_owned());
    }
    if !(lower.starts_with("https://") || lower.starts_with("http://")) {
        return None;
    }
    let authority = value.split_once("://")?.1.split('/').next().unwrap_or_default();
    (!authority.is_empty() && !authority.contains('@')).then(|| value.to_owned())
}

fn provider_label(url: &str) -> Option<&'static str> {
    let host = url
        .split_once("://")?
        .1
        .split('/')
        .next()?
        .split(':')
        .next()?
        .trim_start_matches("www.")
        .to_ascii_lowercase();
    match host.as_str() {
        "youtube.com" | "youtu.be" => Some("YouTube"),
        "x.com" | "twitter.com" => Some("X / Twitter"),
        "reddit.com" | "old.reddit.com" => Some("Reddit"),
        "bsky.app" => Some("Bluesky"),
        "github.com" => Some("GitHub"),
        _ => None,
    }
}

fn split_row(line: &str) -> Vec<String> {
    line.trim()
        .trim_matches('|')
        .split('|')
        .take(MAX_TABLE_COLUMNS)
        .map(|value| value.trim().to_owned())
        .collect()
}

fn table_separator(line: &str) -> bool {
    let cells = split_row(line);
    !cells.is_empty()
        && cells.iter().all(|cell| {
            let cell = cell.trim().trim_matches(':');
            cell.len() >= 3 && cell.chars().all(|ch| ch == '-')
        })
}

fn list_item(line: &str) -> Option<(bool, String)> {
    let line = line.trim_start();
    if let Some(value) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
        return Some((false, value.trim().to_owned()));
    }
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    (digits > 0 && line.get(digits..digits + 2) == Some(". "))
        .then(|| (true, line[digits + 2..].trim().to_owned()))
}

fn parse_blocks(source: &str) -> Vec<Block> {
    let source = bounded(source, MAX_CONTENT_BYTES);
    let lines = source.lines().collect::<Vec<_>>();
    let mut blocks = Vec::new();
    let mut index = 0;
    while index < lines.len() && blocks.len() < MAX_BLOCKS {
        let line = lines[index];
        if line.trim().is_empty() {
            index += 1;
            continue;
        }
        if let Some(info) = line.trim_start().strip_prefix("```") {
            let language = info.trim().to_ascii_lowercase();
            let mut body = String::new();
            index += 1;
            while index < lines.len() && !lines[index].trim_start().starts_with("```") {
                if !body.is_empty() { body.push('\n'); }
                if body.len() < MAX_CODE_BYTES { body.push_str(lines[index]); }
                index += 1;
            }
            index = (index + 1).min(lines.len());
            let body = bounded(&body, MAX_CODE_BYTES);
            blocks.push(match language.as_str() {
                "math" | "tex" | "latex" => Block::Math(body),
                "diff" | "patch" => Block::Diff(body),
                "ansi" | "terminal" => Block::Ansi(body),
                "mermaid" | "mmd" => Block::Mermaid(body),
                _ => Block::Code { language, text: body },
            });
            continue;
        }
        if line.trim() == "$$" {
            let mut body = String::new();
            index += 1;
            while index < lines.len() && lines[index].trim() != "$$" {
                if !body.is_empty() { body.push('\n'); }
                body.push_str(lines[index]);
                index += 1;
            }
            index = (index + 1).min(lines.len());
            blocks.push(Block::Math(bounded(&body, MAX_CODE_BYTES)));
            continue;
        }
        let trimmed = line.trim_start();
        let level = trimmed.chars().take_while(|ch| *ch == '#').count();
        if (1..=6).contains(&level) && trimmed.chars().nth(level) == Some(' ') {
            blocks.push(Block::Heading(level as u8, trimmed[level + 1..].trim().to_owned()));
            index += 1;
            continue;
        }
        if let Some(value) = trimmed.strip_prefix("> ") {
            blocks.push(Block::Quote(value.to_owned()));
            index += 1;
            continue;
        }
        if line.contains('|') && index + 1 < lines.len() && table_separator(lines[index + 1]) {
            let mut rows = vec![split_row(line)];
            index += 2;
            while index < lines.len() && lines[index].contains('|') && rows.len() < MAX_TABLE_ROWS {
                rows.push(split_row(lines[index]));
                index += 1;
            }
            blocks.push(Block::Table(rows));
            continue;
        }
        if let Some((ordered, first)) = list_item(line) {
            let mut items = vec![first];
            index += 1;
            while index < lines.len() {
                let Some((next_ordered, next)) = list_item(lines[index]) else { break; };
                if next_ordered != ordered { break; }
                items.push(next);
                index += 1;
            }
            blocks.push(Block::List { ordered, items });
            continue;
        }
        if trimmed.starts_with("diff --git ") || trimmed.starts_with("@@ ") {
            let mut body = line.to_owned();
            index += 1;
            while index < lines.len() && !lines[index].trim().is_empty() && body.len() < MAX_CODE_BYTES {
                body.push('\n');
                body.push_str(lines[index]);
                index += 1;
            }
            blocks.push(Block::Diff(bounded(&body, MAX_CODE_BYTES)));
            continue;
        }
        if line.contains('\u{1b}') {
            blocks.push(Block::Ansi(bounded(line, MAX_CODE_BYTES)));
            index += 1;
            continue;
        }
        let mut paragraph = line.trim().to_owned();
        index += 1;
        while index < lines.len() && !lines[index].trim().is_empty() {
            let next = lines[index];
            if next.trim_start().starts_with("```") || next.trim() == "$$" || list_item(next).is_some() { break; }
            paragraph.push('\n');
            paragraph.push_str(next.trim());
            index += 1;
        }
        blocks.push(Block::Paragraph(paragraph));
    }
    blocks
}

fn bracket_target(input: &str, image: bool) -> Option<(usize, String, String)> {
    let prefix = if image { 2 } else { 1 };
    let label_end = input[prefix..].find(']')? + prefix;
    let target_start = label_end + 1;
    if input.get(target_start..target_start + 1)? != "(" { return None; }
    let target_end = input[target_start + 1..].find(')')? + target_start + 1;
    Some((target_end + 1, input[prefix..label_end].to_owned(), input[target_start + 1..target_end].trim().to_owned()))
}

fn parse_inline(text: &str) -> Vec<Inline> {
    let mut output = Vec::new();
    let mut rest = text;
    while !rest.is_empty() && output.len() < 256 {
        let start = [rest.find("!["), rest.find('['), rest.find('`'), rest.find('$')].into_iter().flatten().min();
        let Some(start) = start else { output.push(Inline::Text(rest.to_owned())); break; };
        if start > 0 {
            output.push(Inline::Text(rest[..start].to_owned()));
            rest = &rest[start..];
            continue;
        }
        if rest.starts_with("![") {
            if let Some((used, alt, target)) = bracket_target(rest, true) {
                output.push(safe_target(&target, true).map_or_else(|| Inline::Blocked(alt.clone()), |url| Inline::Image { alt, url }));
                rest = &rest[used..];
                continue;
            }
        } else if rest.starts_with('[') {
            if let Some((used, label, target)) = bracket_target(rest, false) {
                output.push(safe_target(&target, false).map_or_else(|| Inline::Blocked(label.clone()), |url| Inline::Link { label, url }));
                rest = &rest[used..];
                continue;
            }
        } else if let Some(after) = rest.strip_prefix('`') {
            if let Some(end) = after.find('`') {
                output.push(Inline::Code(after[..end].to_owned()));
                rest = &after[end + 1..];
                continue;
            }
        } else if let Some(after) = rest.strip_prefix('$') {
            if let Some(end) = after.find('$') {
                if end > 0 {
                    output.push(Inline::Math(after[..end].to_owned()));
                    rest = &after[end + 1..];
                    continue;
                }
            }
        }
        let width = rest.chars().next().map(char::len_utf8).unwrap_or(1);
        output.push(Inline::Text(rest[..width].to_owned()));
        rest = &rest[width..];
    }
    output
}

fn strip_ansi(input: &str) -> String {
    let mut output = String::new();
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\u{1b}' {
            if !ch.is_control() || matches!(ch, '\n' | '\r' | '\t') { output.push(ch); }
            continue;
        }
        if chars.peek() == Some(&'[') {
            chars.next();
            for next in chars.by_ref() { if ('@'..='~').contains(&next) { break; } }
        }
    }
    output
}

fn mermaid_allowed(source: &str) -> bool {
    let lower = source.to_ascii_lowercase();
    source.len() <= MAX_CODE_BYTES
        && !source.contains('<')
        && !source.contains('>')
        && !lower.lines().any(|line| line.trim_start().starts_with("click "))
        && !lower.contains("href")
        && !lower.contains("url(")
}

fn mermaid_edges(source: &str) -> Vec<(String, String)> {
    if !mermaid_allowed(source) { return Vec::new(); }
    source.lines().filter_map(|line| {
        let (left, right) = line.trim().split_once("-->")?;
        let clean = |value: &str| value.trim().trim_matches(|ch: char| "[](){}\"".contains(ch)).chars().take(96).collect::<String>();
        let left = clean(left);
        let right = clean(right);
        (!left.is_empty() && !right.is_empty()).then_some((left, right))
    }).take(64).collect()
}

#[component]
fn InlineContent(text: String, on_open_link: Callback<String>) -> Element {
    rsx! {
        for token in parse_inline(&text) {
            match token {
                Inline::Text(value) => rsx! { span { "{value}" } },
                Inline::Code(value) => rsx! { code { class: "rich-inline-code", "{value}" } },
                Inline::Math(value) => rsx! { span { class: "rich-math-inline", role: "math", "{value}" } },
                Inline::Link { label, url } => {
                    let destination = url.clone();
                    let embed_destination = url.clone();
                    let provider = provider_label(&url);
                    rsx! {
                        a { class: "rich-link", href: "{url}", target: "_blank", rel: "noopener noreferrer", onclick: move |event| { event.prevent_default(); on_open_link.call(destination.clone()); }, "{label}" }
                        if let Some(provider) = provider {
                            button { class: "rich-embed-card", aria_label: "Open {provider} content externally", onclick: move |_| on_open_link.call(embed_destination.clone()), strong { "{provider}" } small { "External content · open explicitly" } }
                        }
                    }
                },
                Inline::Image { alt, url } => rsx! { figure { class: "rich-image", img { src: "{url}", alt: "{alt}", loading: "lazy" } if !alt.is_empty() { figcaption { "{alt}" } } } },
                Inline::Blocked(label) => rsx! { span { class: "rich-link-blocked", title: "Blocked external target", "{label}" } },
            }
        }
    }
}

#[component]
fn TableBlock(rows: Vec<Vec<String>>, on_open_link: Callback<String>) -> Element {
    let header = rows.first().cloned().unwrap_or_default();
    let body = rows.into_iter().skip(1).collect::<Vec<_>>();
    rsx! { div { class: "rich-table-scroll", table { thead { tr { for cell in header { th { InlineContent { text: cell, on_open_link } } } } } tbody { for row in body { tr { for cell in row { td { InlineContent { text: cell, on_open_link } } } } } } } } }
}

#[component]
fn MermaidBlock(source: String) -> Element {
    let allowed = mermaid_allowed(&source);
    let edges = mermaid_edges(&source);
    rsx! { figure { class: "rich-mermaid", aria_label: "Mermaid diagram", figcaption { "Mermaid diagram" } if !allowed { p { class: "inline-error", role: "alert", "Diagram blocked by renderer policy." } } else if edges.is_empty() { pre { "{source}" } } else { div { class: "mermaid-flow", for (index, (from, to)) in edges.into_iter().enumerate() { div { class: "mermaid-edge", key: "{index}", span { class: "mermaid-node", "{from}" } span { class: "mermaid-arrow", aria_hidden: "true", "→" } span { class: "mermaid-node", "{to}" } } } } } } }
}

#[component]
pub(super) fn RichContent(text: String, on_open_link: Callback<String>) -> Element {
    rsx! {
        div { class: "rich-content",
            for (index, block) in parse_blocks(&text).into_iter().enumerate() {
                div { class: "rich-block", key: "{index}",
                    match block {
                        Block::Heading(level, value) => match level {
                            1 => rsx! { h1 { InlineContent { text: value, on_open_link } } },
                            2 => rsx! { h2 { InlineContent { text: value, on_open_link } } },
                            3 => rsx! { h3 { InlineContent { text: value, on_open_link } } },
                            _ => rsx! { h4 { InlineContent { text: value, on_open_link } } },
                        },
                        Block::Paragraph(value) => rsx! { p { InlineContent { text: value, on_open_link } } },
                        Block::Quote(value) => rsx! { blockquote { InlineContent { text: value, on_open_link } } },
                        Block::List { ordered, items } => if ordered { rsx! { ol { for item in items { li { InlineContent { text: item, on_open_link } } } } } } else { rsx! { ul { for item in items { li { InlineContent { text: item, on_open_link } } } } } },
                        Block::Code { language, text } => rsx! { figure { class: "rich-code-card", figcaption { if language.is_empty() { "text" } else { "{language}" } } pre { class: "rich-code", code { "{text}" } } } },
                        Block::Math(value) => rsx! { div { class: "rich-math-display", role: "math", code { "{value}" } } },
                        Block::Table(rows) => rsx! { TableBlock { rows, on_open_link } },
                        Block::Diff(value) => rsx! { pre { class: "rich-diff", for (line_index, line) in value.lines().enumerate() { span { key: "{line_index}", class: if line.starts_with('+') { "diff-line add" } else if line.starts_with('-') { "diff-line remove" } else if line.starts_with("@@") { "diff-line hunk" } else { "diff-line" }, "{line}\n" } } } },
                        Block::Ansi(value) => rsx! { pre { class: "rich-ansi", "{strip_ansi(&value)}" } },
                        Block::Mermaid(source) => rsx! { MermaidBlock { source } },
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
    fn bounds_content_and_wide_tables() {
        assert!(!parse_blocks(&"x".repeat(MAX_CONTENT_BYTES + 100)).is_empty());
        let row = (0..100).map(|index| index.to_string()).collect::<Vec<_>>().join("|");
        assert_eq!(split_row(&row).len(), MAX_TABLE_COLUMNS);
    }

    #[test]
    fn restricts_external_and_image_targets() {
        assert!(safe_target("https://example.com/a", false).is_some());
        assert!(safe_target("ftp://example.com/a", false).is_none());
        assert!(safe_target("data:image/png;base64,AA==", true).is_some());
        assert!(safe_target("data:text/plain;base64,AA==", true).is_none());
    }

    #[test]
    fn parses_rich_inventory() {
        let input = "# Heading\n\n```rust\nfn main() {}\n```\n\n| A | B |\n| --- | --- |\n| 1 | 2 |\n\n```mermaid\ngraph TD\nA-->B\n```";
        let blocks = parse_blocks(input);
        assert!(blocks.iter().any(|block| matches!(block, Block::Heading(..))));
        assert!(blocks.iter().any(|block| matches!(block, Block::Code { .. })));
        assert!(blocks.iter().any(|block| matches!(block, Block::Table(_))));
        assert!(blocks.iter().any(|block| matches!(block, Block::Mermaid(_))));
    }

    #[test]
    fn strips_terminal_controls() {
        assert_eq!(strip_ansi("\u{1b}[31mred\u{1b}[0m"), "red");
    }

    #[test]
    fn embed_provider_list_is_explicit() {
        assert_eq!(provider_label("https://github.com/owner/repo"), Some("GitHub"));
        assert_eq!(provider_label("https://example.com/item"), None);
    }
}
