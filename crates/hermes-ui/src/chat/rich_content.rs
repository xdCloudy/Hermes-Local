use dioxus::prelude::*;

const MAX_BYTES: usize = 1_000_000;
const MAX_BLOCKS: usize = 512;
const MAX_CODE_BYTES: usize = 256_000;
const MAX_INLINE_TOKENS: usize = 256;
const MAX_MERMAID_EDGES: usize = 64;
const MAX_TABLE_COLUMNS: usize = 32;
const MAX_TABLE_ROWS: usize = 256;

#[derive(Clone, Debug, PartialEq)]
enum Block {
    Heading(u8, String),
    Quote(String),
    List { ordered: bool, items: Vec<String> },
    Code { language: String, text: String },
    Math(String),
    Table(Vec<Vec<String>>),
    Diff(String),
    Mermaid(String),
    Ansi(String),
    Paragraph(String),
}

#[derive(Clone, Debug, PartialEq)]
enum Inline {
    Text(String),
    Code(String),
    Math(String),
    Link {
        label: String,
        url: String,
        provider: Option<String>,
    },
    Image { alt: String, url: String },
    Blocked(String),
}

#[derive(Clone, Debug, PartialEq)]
enum MathSegment {
    Text(String),
    Sup(String),
    Sub(String),
    Fraction(String, String),
    Sqrt(String),
}

#[derive(Clone, Debug, PartialEq)]
struct CodeToken {
    kind: &'static str,
    text: String,
}

fn bounded(value: &str, limit: usize) -> String {
    if value.len() <= limit {
        return value.to_owned();
    }
    let mut end = limit;
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
    let authority = value
        .split_once("://")?
        .1
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    (!authority.is_empty() && !authority.contains('@')).then(|| value.to_owned())
}

fn provider_label(url: &str) -> Option<&'static str> {
    let host = url
        .split_once("://")?
        .1
        .split(['/', '?', '#'])
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
    if let Some(value) = line
        .strip_prefix("- ")
        .or_else(|| line.strip_prefix("* "))
    {
        return Some((false, value.trim().to_owned()));
    }
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    (digits > 0 && line.get(digits..digits + 2) == Some(". "))
        .then(|| (true, line[digits + 2..].trim().to_owned()))
}

fn parse(value: &str) -> Vec<Block> {
    let source = bounded(value, MAX_BYTES);
    let lines = source.lines().collect::<Vec<_>>();
    let mut blocks = Vec::new();
    let mut index = 0;
    while index < lines.len() && blocks.len() < MAX_BLOCKS {
        let line = lines[index];
        let trimmed = line.trim_start();
        if trimmed.trim().is_empty() {
            index += 1;
            continue;
        }
        if let Some(language) = trimmed.strip_prefix("```") {
            let language = language.trim().to_ascii_lowercase();
            let mut body = String::new();
            index += 1;
            while index < lines.len() && !lines[index].trim_start().starts_with("```") {
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
                "ansi" | "terminal" => Block::Ansi(body),
                _ => Block::Code {
                    language,
                    text: body,
                },
            });
            continue;
        }
        if trimmed.trim() == "$$" {
            let mut body = String::new();
            index += 1;
            while index < lines.len() && lines[index].trim() != "$$" {
                if !body.is_empty() {
                    body.push('\n');
                }
                body.push_str(lines[index]);
                index += 1;
            }
            index = (index + 1).min(lines.len());
            blocks.push(Block::Math(bounded(&body, MAX_CODE_BYTES)));
            continue;
        }
        let level = trimmed.chars().take_while(|ch| *ch == '#').count();
        if (1..=6).contains(&level) && trimmed.chars().nth(level) == Some(' ') {
            blocks.push(Block::Heading(
                level as u8,
                trimmed[level + 1..].trim().to_owned(),
            ));
            index += 1;
            continue;
        }
        if let Some(text) = trimmed.strip_prefix("> ") {
            blocks.push(Block::Quote(text.to_owned()));
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
                let Some((next_ordered, next)) = list_item(lines[index]) else {
                    break;
                };
                if next_ordered != ordered {
                    break;
                }
                items.push(next);
                index += 1;
            }
            blocks.push(Block::List { ordered, items });
            continue;
        }
        if trimmed.starts_with("diff --git ") || trimmed.starts_with("@@ ") {
            let mut body = line.to_owned();
            index += 1;
            while index < lines.len()
                && !lines[index].trim().is_empty()
                && body.len() < MAX_CODE_BYTES
            {
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
            let next_trimmed = next.trim_start();
            if next_trimmed.starts_with("```")
                || next.trim() == "$$"
                || list_item(next).is_some()
                || next_trimmed.starts_with("# ")
                || next_trimmed.starts_with("> ")
            {
                break;
            }
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
    if input.get(target_start..target_start + 1)? != "(" {
        return None;
    }
    let target_end = input[target_start + 1..].find(')')? + target_start + 1;
    Some((
        target_end + 1,
        input[prefix..label_end].to_owned(),
        input[target_start + 1..target_end].trim().to_owned(),
    ))
}

fn parse_inline(text: &str) -> Vec<Inline> {
    let mut output = Vec::new();
    let mut rest = text;
    while !rest.is_empty() && output.len() < MAX_INLINE_TOKENS {
        let start = [
            rest.find("!["),
            rest.find('['),
            rest.find('`'),
            rest.find('$'),
        ]
        .into_iter()
        .flatten()
        .min();
        let Some(start) = start else {
            output.push(Inline::Text(rest.to_owned()));
            break;
        };
        if start > 0 {
            output.push(Inline::Text(rest[..start].to_owned()));
            rest = &rest[start..];
            continue;
        }
        if rest.starts_with("![") {
            if let Some((used, alt, target)) = bracket_target(rest, true) {
                let token = match safe_target(&target, true) {
                    Some(url) => Inline::Image { alt, url },
                    None => Inline::Blocked(alt),
                };
                output.push(token);
                rest = &rest[used..];
                continue;
            }
        } else if rest.starts_with('[') {
            if let Some((used, label, target)) = bracket_target(rest, false) {
                let token = match safe_target(&target, false) {
                    Some(url) => Inline::Link {
                        provider: provider_label(&url).map(str::to_owned),
                        label,
                        url,
                    },
                    None => Inline::Blocked(label),
                };
                output.push(token);
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
            if let Some(end) = after.find('$')
                && end > 0
            {
                output.push(Inline::Math(after[..end].to_owned()));
                rest = &after[end + 1..];
                continue;
            }
        }
        let width = rest.chars().next().map(char::len_utf8).unwrap_or(1);
        output.push(Inline::Text(rest[..width].to_owned()));
        rest = &rest[width..];
    }
    if !rest.is_empty() {
        output.push(Inline::Text(bounded(rest, 8_192)));
    }
    output
}

fn extract_group(input: &str) -> Option<(String, usize)> {
    if !input.starts_with('{') {
        return None;
    }
    let mut depth = 0_u16;
    for (index, ch) in input.char_indices() {
        match ch {
            '{' => depth = depth.saturating_add(1),
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some((input[1..index].to_owned(), index + 1));
                }
            }
            _ => {}
        }
    }
    None
}

fn math_symbol(command: &str) -> Option<&'static str> {
    match command {
        "alpha" => Some("α"),
        "beta" => Some("β"),
        "gamma" => Some("γ"),
        "delta" => Some("δ"),
        "epsilon" => Some("ε"),
        "theta" => Some("θ"),
        "lambda" => Some("λ"),
        "mu" => Some("μ"),
        "pi" => Some("π"),
        "rho" => Some("ρ"),
        "sigma" => Some("σ"),
        "phi" => Some("φ"),
        "omega" => Some("ω"),
        "Gamma" => Some("Γ"),
        "Delta" => Some("Δ"),
        "Theta" => Some("Θ"),
        "Lambda" => Some("Λ"),
        "Pi" => Some("Π"),
        "Sigma" => Some("Σ"),
        "Phi" => Some("Φ"),
        "Omega" => Some("Ω"),
        "times" => Some("×"),
        "cdot" => Some("·"),
        "pm" => Some("±"),
        "le" | "leq" => Some("≤"),
        "ge" | "geq" => Some("≥"),
        "neq" => Some("≠"),
        "infty" => Some("∞"),
        "rightarrow" | "to" => Some("→"),
        "leftarrow" => Some("←"),
        "sum" => Some("∑"),
        "prod" => Some("∏"),
        "int" => Some("∫"),
        _ => None,
    }
}

fn parse_math(source: &str) -> Vec<MathSegment> {
    let source = bounded(source, 16_384);
    let mut output = Vec::new();
    let mut rest = source.as_str();
    let mut plain = String::new();
    let flush_plain = |plain: &mut String, output: &mut Vec<MathSegment>| {
        if !plain.is_empty() {
            output.push(MathSegment::Text(std::mem::take(plain)));
        }
    };
    while !rest.is_empty() && output.len() < 256 {
        if let Some(after) = rest.strip_prefix("\\frac")
            && let Some((top, top_used)) = extract_group(after)
            && let Some((bottom, bottom_used)) = extract_group(&after[top_used..])
        {
            flush_plain(&mut plain, &mut output);
            output.push(MathSegment::Fraction(top, bottom));
            rest = &after[top_used + bottom_used..];
            continue;
        }
        if let Some(after) = rest.strip_prefix("\\sqrt")
            && let Some((value, used)) = extract_group(after)
        {
            flush_plain(&mut plain, &mut output);
            output.push(MathSegment::Sqrt(value));
            rest = &after[used..];
            continue;
        }
        if let Some(after) = rest.strip_prefix("^{")
            && let Some((value, used)) = extract_group(&rest[1..])
        {
            flush_plain(&mut plain, &mut output);
            output.push(MathSegment::Sup(value));
            rest = &after[used - 1..];
            continue;
        }
        if let Some(after) = rest.strip_prefix("_{")
            && let Some((value, used)) = extract_group(&rest[1..])
        {
            flush_plain(&mut plain, &mut output);
            output.push(MathSegment::Sub(value));
            rest = &after[used - 1..];
            continue;
        }
        if let Some(after) = rest.strip_prefix('\\') {
            let command_len = after
                .chars()
                .take_while(|ch| ch.is_ascii_alphabetic())
                .map(char::len_utf8)
                .sum::<usize>();
            if command_len > 0 {
                let command = &after[..command_len];
                if let Some(symbol) = math_symbol(command) {
                    plain.push_str(symbol);
                } else {
                    plain.push_str(command);
                }
                rest = &after[command_len..];
                continue;
            }
        }
        let Some(ch) = rest.chars().next() else {
            break;
        };
        plain.push(ch);
        rest = &rest[ch.len_utf8()..];
    }
    flush_plain(&mut plain, &mut output);
    output
}

fn keyword(language: &str, word: &str) -> bool {
    match language {
        "rust" | "rs" => matches!(
            word,
            "as" | "async" | "await" | "break" | "const" | "continue" | "crate" | "else"
                | "enum" | "fn" | "for" | "if" | "impl" | "in" | "let" | "loop" | "match"
                | "mod" | "move" | "mut" | "pub" | "ref" | "return" | "self" | "Self"
                | "static" | "struct" | "super" | "trait" | "type" | "use" | "where" | "while"
        ),
        "python" | "py" => matches!(
            word,
            "and" | "as" | "async" | "await" | "break" | "class" | "continue" | "def"
                | "del" | "elif" | "else" | "except" | "False" | "finally" | "for" | "from"
                | "global" | "if" | "import" | "in" | "is" | "lambda" | "None" | "nonlocal"
                | "not" | "or" | "pass" | "raise" | "return" | "True" | "try" | "while" | "with"
                | "yield"
        ),
        "javascript" | "js" | "typescript" | "ts" | "tsx" | "jsx" => matches!(
            word,
            "async" | "await" | "break" | "case" | "catch" | "class" | "const" | "continue"
                | "default" | "delete" | "do" | "else" | "export" | "extends" | "false" | "finally"
                | "for" | "from" | "function" | "if" | "import" | "in" | "instanceof" | "let"
                | "new" | "null" | "of" | "return" | "static" | "super" | "switch" | "this"
                | "throw" | "true" | "try" | "typeof" | "undefined" | "var" | "while"
        ),
        "bash" | "sh" | "shell" | "powershell" | "ps1" => matches!(
            word,
            "case" | "do" | "done" | "elif" | "else" | "esac" | "fi" | "for" | "function"
                | "if" | "in" | "select" | "then" | "until" | "while"
        ),
        _ => false,
    }
}

fn syntax_tokens(language: &str, line: &str) -> Vec<CodeToken> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//")
        || trimmed.starts_with('#')
        || (matches!(language, "sql") && trimmed.starts_with("--"))
    {
        return vec![CodeToken {
            kind: "comment",
            text: line.to_owned(),
        }];
    }
    line.split_inclusive(char::is_whitespace)
        .map(|chunk| {
            let word = chunk.trim_matches(|ch: char| !ch.is_alphanumeric() && ch != '_');
            let kind = if keyword(language, word) {
                "keyword"
            } else if word.parse::<f64>().is_ok() {
                "number"
            } else if chunk.trim_start().starts_with(['\'', '"', '`']) {
                "string"
            } else {
                "plain"
            };
            CodeToken {
                kind,
                text: chunk.to_owned(),
            }
        })
        .collect()
}

fn mermaid_allowed(source: &str) -> bool {
    let lower = source.to_ascii_lowercase();
    source.len() <= MAX_CODE_BYTES
        && !source.contains('<')
        && !source.contains('>')
        && !lower.contains("href")
        && !lower.contains("url(")
        && !lower
            .lines()
            .any(|line| line.trim_start().starts_with("click "))
}

fn clean_mermaid_node(value: &str) -> String {
    value
        .trim()
        .trim_matches(|ch: char| "[](){}\"'".contains(ch))
        .chars()
        .filter(|ch| !ch.is_control())
        .take(96)
        .collect()
}

fn mermaid_edges(source: &str) -> Vec<(String, String)> {
    if !mermaid_allowed(source) {
        return Vec::new();
    }
    source
        .lines()
        .filter_map(|line| {
            let (left, right) = line.trim().split_once("-->")?;
            let left = clean_mermaid_node(left);
            let right = clean_mermaid_node(right);
            (!left.is_empty() && !right.is_empty()).then_some((left, right))
        })
        .take(MAX_MERMAID_EDGES)
        .collect()
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

#[component]
fn MathContent(source: String, display: bool) -> Element {
    let class = if display {
        "rich-math-display"
    } else {
        "rich-math-inline"
    };
    rsx! {
        span { class, role: "math", aria_label: "{source}",
            for (index, segment) in parse_math(&source).into_iter().enumerate() {
                span { key: "{index}",
                    match segment {
                        MathSegment::Text(value) => rsx! { span { "{value}" } },
                        MathSegment::Sup(value) => rsx! { sup { "{value}" } },
                        MathSegment::Sub(value) => rsx! { sub { "{value}" } },
                        MathSegment::Sqrt(value) => rsx! { span { "√(" span { "{value}" } ")" } },
                        MathSegment::Fraction(top, bottom) => rsx! {
                            span {
                                style: "display:inline-flex;flex-direction:column;align-items:center;vertical-align:middle;line-height:1.1;margin:0 .15em;",
                                span { style: "border-bottom:1px solid currentColor;padding:0 .15em;", "{top}" }
                                span { style: "padding:0 .15em;", "{bottom}" }
                            }
                        },
                    }
                }
            }
        }
    }
}

#[component]
fn InlineContent(text: String, on_open_link: Callback<String>) -> Element {
    rsx! {
        for (index, token) in parse_inline(&text).into_iter().enumerate() {
            span { key: "{index}",
                match token {
                    Inline::Text(value) => rsx! { span { "{value}" } },
                    Inline::Code(value) => rsx! { code { class: "rich-inline-code", "{value}" } },
                    Inline::Math(value) => rsx! { MathContent { source: value, display: false } },
                    Inline::Link { label, url, provider } => {
                        let destination = url.clone();
                        let card_destination = url.clone();
                        rsx! {
                            button {
                                class: "rich-link",
                                title: "Open external link",
                                onclick: move |_| on_open_link.call(destination.clone()),
                                "{label}"
                            }
                            if let Some(provider) = provider {
                                button {
                                    class: "rich-embed-card",
                                    aria_label: "Open {provider} content externally",
                                    onclick: move |_| on_open_link.call(card_destination.clone()),
                                    strong { "{provider}" }
                                    small { "External content · open explicitly" }
                                }
                            }
                        }
                    },
                    Inline::Image { alt, url } => rsx! {
                        figure { class: "rich-image",
                            img { src: "{url}", alt: "{alt}", loading: "lazy" }
                            if !alt.is_empty() { figcaption { "{alt}" } }
                        }
                    },
                    Inline::Blocked(label) => rsx! {
                        span { class: "rich-link-blocked", title: "Blocked external target", "{label}" }
                    },
                }
            }
        }
    }
}

#[component]
fn CodeBlock(language: String, text: String) -> Element {
    let language_label = if language.is_empty() {
        "text".to_owned()
    } else {
        language.clone()
    };
    rsx! {
        figure { class: "rich-code-card",
            figcaption { "{language_label}" }
            pre { class: "rich-code",
                code {
                    for (line_index, line) in text.lines().enumerate() {
                        span { key: "{line_index}", class: "code-line",
                            for (token_index, token) in syntax_tokens(&language, line).into_iter().enumerate() {
                                span {
                                    key: "{token_index}",
                                    class: "syntax-{token.kind}",
                                    style: match token.kind {
                                        "keyword" => "color:var(--ui-accent,#8ab4ff);font-weight:600;",
                                        "number" => "color:var(--ui-warning,#d29922);",
                                        "string" => "color:var(--ui-success,#3fb950);",
                                        "comment" => "opacity:.65;font-style:italic;",
                                        _ => "",
                                    },
                                    "{token.text}"
                                }
                            }
                            "\n"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn TableBlock(rows: Vec<Vec<String>>, on_open_link: Callback<String>) -> Element {
    let header = rows.first().cloned().unwrap_or_default();
    let body = rows.into_iter().skip(1).collect::<Vec<_>>();
    rsx! {
        div { class: "rich-table-scroll",
            table {
                thead { tr { for cell in header { th { InlineContent { text: cell, on_open_link } } } } }
                tbody { for row in body { tr { for cell in row { td { InlineContent { text: cell, on_open_link } } } } } }
            }
        }
    }
}

#[component]
fn MermaidBlock(source: String) -> Element {
    let allowed = mermaid_allowed(&source);
    let edges = mermaid_edges(&source);
    rsx! {
        figure { class: "rich-mermaid", aria_label: "Mermaid diagram",
            figcaption { "Mermaid diagram" }
            if !allowed {
                p { class: "inline-error", role: "alert", "Diagram blocked by renderer policy." }
            } else if edges.is_empty() {
                pre { "{source}" }
            } else {
                div { class: "mermaid-flow",
                    for (index, (from, to)) in edges.into_iter().enumerate() {
                        div { class: "mermaid-edge", key: "{index}",
                            span { class: "mermaid-node", "{from}" }
                            span { class: "mermaid-arrow", aria_hidden: "true", "→" }
                            span { class: "mermaid-node", "{to}" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn RichContent(text: String, on_open_link: Callback<String>) -> Element {
    rsx! {
        div { class: "rich-content",
            for (index, block) in parse(&text).into_iter().enumerate() {
                div { class: "rich-block", key: "{index}",
                    match block {
                        Block::Heading(level, value) => match level {
                            1 => rsx! { h1 { InlineContent { text: value, on_open_link } } },
                            2 => rsx! { h2 { InlineContent { text: value, on_open_link } } },
                            3 => rsx! { h3 { InlineContent { text: value, on_open_link } } },
                            _ => rsx! { h4 { InlineContent { text: value, on_open_link } } },
                        },
                        Block::Quote(value) => rsx! { blockquote { InlineContent { text: value, on_open_link } } },
                        Block::List { ordered, items } => if ordered {
                            rsx! { ol { for item in items { li { InlineContent { text: item, on_open_link } } } } }
                        } else {
                            rsx! { ul { for item in items { li { InlineContent { text: item, on_open_link } } } } }
                        },
                        Block::Code { language, text } => rsx! { CodeBlock { language, text } },
                        Block::Math(value) => rsx! { MathContent { source: value, display: true } },
                        Block::Table(rows) => rsx! { TableBlock { rows, on_open_link } },
                        Block::Diff(value) => rsx! {
                            pre { class: "rich-diff",
                                for (line_index, line) in value.lines().enumerate() {
                                    span {
                                        key: "{line_index}",
                                        class: if line.starts_with('+') { "diff-line add" } else if line.starts_with('-') { "diff-line remove" } else if line.starts_with("@@") { "diff-line hunk" } else { "diff-line" },
                                        style: if line.starts_with('+') { "color:var(--ui-success,#3fb950);" } else if line.starts_with('-') { "color:var(--ui-danger,#f85149);" } else { "" },
                                        "{line}\n"
                                    }
                                }
                            }
                        },
                        Block::Ansi(value) => rsx! { pre { class: "rich-ansi", "{strip_ansi(&value)}" } },
                        Block::Mermaid(source) => rsx! { MermaidBlock { source } },
                        Block::Paragraph(value) => rsx! { p { InlineContent { text: value, on_open_link } } },
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
    fn content_and_tables_are_bounded() {
        assert!(!parse(&"x".repeat(MAX_BYTES + 16)).is_empty());
        let line = (0..100)
            .map(|index| index.to_string())
            .collect::<Vec<_>>()
            .join("|");
        assert_eq!(split_row(&line).len(), MAX_TABLE_COLUMNS);
    }

    #[test]
    fn external_targets_and_provider_cards_are_explicit() {
        assert!(safe_target("https://example.com/a", false).is_some());
        assert!(safe_target("ftp://example.com/a", false).is_none());
        assert_eq!(
            provider_label("https://github.com/owner/repo"),
            Some("GitHub")
        );
        assert_eq!(provider_label("https://example.com/item"), None);
    }

    #[test]
    fn image_targets_are_bounded_to_supported_forms() {
        assert!(safe_target("https://example.com/image.png", true).is_some());
        assert!(safe_target("data:image/png;base64,AA==", true).is_some());
        assert!(safe_target("data:text/plain;base64,AA==", true).is_none());
    }

    #[test]
    fn ansi_sequences_are_removed() {
        assert_eq!(strip_ansi("\u{1b}[31mred\u{1b}[0m"), "red");
    }

    #[test]
    fn math_parser_handles_structure_and_symbols() {
        let parts = parse_math("\\frac{a}{b} + \\sqrt{x} = \\pi");
        assert!(parts.contains(&MathSegment::Fraction("a".into(), "b".into())));
        assert!(parts.contains(&MathSegment::Sqrt("x".into())));
        assert!(parts.iter().any(|part| matches!(part, MathSegment::Text(value) if value.contains('π'))));
    }

    #[test]
    fn syntax_inventory_recognizes_keywords() {
        let tokens = syntax_tokens("rust", "pub fn run() {}");
        assert!(tokens.iter().any(|token| token.kind == "keyword"));
    }

    #[test]
    fn mermaid_is_non_privileged_and_bounded() {
        assert_eq!(mermaid_edges("graph TD\nA-->B").len(), 1);
        assert!(!mermaid_allowed("graph TD\nclick A callback"));
    }

    #[test]
    fn parses_required_rich_blocks() {
        let input = "# Heading\n\n```rust\nfn main() {}\n```\n\n| A | B |\n| --- | --- |\n| 1 | 2 |\n\n```mermaid\ngraph TD\nA-->B\n```";
        let blocks = parse(input);
        assert!(
            blocks
                .iter()
                .any(|block| matches!(block, Block::Heading(..)))
        );
        assert!(
            blocks
                .iter()
                .any(|block| matches!(block, Block::Code { .. }))
        );
        assert!(blocks.iter().any(|block| matches!(block, Block::Table(_))));
        assert!(
            blocks
                .iter()
                .any(|block| matches!(block, Block::Mermaid(_)))
        );
    }
}
