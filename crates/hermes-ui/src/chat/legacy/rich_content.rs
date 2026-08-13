use dioxus::prelude::*;

mod ansi;
mod base;

#[component]
pub(super) fn RichContent(text: String, on_open_link: Callback<String>) -> Element {
    if ansi::has_ansi(&text) && !text.contains("```") {
        return rsx! { ansi::AnsiContent { text } };
    }
    rsx! { base::RichContent { text, on_open_link } }
}

#[cfg(test)]
mod contract_tests {
    include!("rich_content/base.rs");

    #[test]
    fn markdown_structure_and_link_policy_match_the_chat_contract() {
        let blocks = parse("# Heading\n\n- one\n- two\n\n> quote\n\nplain");
        assert!(
            blocks
                .iter()
                .any(|block| matches!(block, Block::Heading(1, value) if value == "Heading"))
        );
        assert!(blocks.iter().any(|block| {
            matches!(block, Block::List { ordered: false, items } if items == &vec!["one".to_owned(), "two".to_owned()])
        }));
        assert!(
            blocks
                .iter()
                .any(|block| matches!(block, Block::Quote(value) if value == "quote"))
        );

        let link = parse_inline("[Open](https://example.com/path)");
        assert!(matches!(
            link.as_slice(),
            [Inline::Link { label, url, .. }] if label == "Open" && url == "https://example.com/path"
        ));
        assert!(safe_target("https://example.com/path", false).is_some());
        assert!(safe_target("https://user@example.com/path", false).is_none());
        assert!(safe_target("data:text/plain;base64,AA==", false).is_none());
    }

    #[test]
    fn raw_html_remains_text_and_disallowed_links_become_blocked_tokens() {
        let blocks = parse("<b>literal markup</b>");
        assert!(matches!(
            blocks.as_slice(),
            [Block::Paragraph(value)] if value == "<b>literal markup</b>"
        ));
        let inline = parse_inline("[Blocked](data:text/plain;base64,AA==)");
        assert!(matches!(
            inline.as_slice(),
            [Inline::Blocked(label)] if label == "Blocked"
        ));
    }

    #[test]
    fn wide_tables_and_large_diffs_remain_bounded() {
        let header = (0..100)
            .map(|index| format!("h{index}"))
            .collect::<Vec<_>>()
            .join("|");
        let separator = (0..100).map(|_| "---").collect::<Vec<_>>().join("|");
        let rows = (0..400)
            .map(|index| format!("{index}|value"))
            .collect::<Vec<_>>()
            .join("\n");
        let table = format!("{header}\n{separator}\n{rows}");
        let blocks = parse(&table);
        let table = blocks
            .iter()
            .find_map(|block| match block {
                Block::Table(rows) => Some(rows),
                _ => None,
            })
            .expect("bounded table");
        assert_eq!(table[0].len(), MAX_TABLE_COLUMNS);
        assert_eq!(table.len(), MAX_TABLE_ROWS);

        let diff = format!("diff --git a/file b/file\n{}", "+value\n".repeat(80_000));
        let blocks = parse(&diff);
        let diff = blocks
            .iter()
            .find_map(|block| match block {
                Block::Diff(value) => Some(value),
                _ => None,
            })
            .expect("bounded diff");
        assert!(diff.len() <= MAX_CODE_BYTES + 64);
    }
}
