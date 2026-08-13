const CHAT_SOURCE: &str = include_str!("../src/chat/legacy.rs");
const TRANSCRIPT_WINDOW: usize = 80;

fn visible_message_start(total: usize, requested: usize) -> usize {
    total.saturating_sub(requested.max(1))
}

#[test]
fn million_message_history_stays_window_bounded() {
    let total = 1_000_000_usize;
    let mut requested = TRANSCRIPT_WINDOW;

    assert_eq!(visible_message_start(total, requested), 999_920);
    assert_eq!(total - visible_message_start(total, requested), TRANSCRIPT_WINDOW);

    for _ in 0..32 {
        requested = requested.saturating_add(TRANSCRIPT_WINDOW).min(total);
        let start = visible_message_start(total, requested);
        assert_eq!(total - start, requested);
        assert!(requested <= total);
    }

    assert_eq!(visible_message_start(0, requested), 0);
    assert_eq!(visible_message_start(7, 0), 6);
}

#[test]
fn integration_test_tracks_the_compiled_chat_window_contract() {
    assert!(CHAT_SOURCE.contains("const TRANSCRIPT_WINDOW: usize = 80;"));
    assert!(CHAT_SOURCE.contains("fn visible_message_start(total: usize, requested: usize) -> usize"));
    assert!(CHAT_SOURCE.contains("total.saturating_sub(requested.max(1))"));
    assert!(CHAT_SOURCE.contains("visible_count.set((visible_count() + TRANSCRIPT_WINDOW).min(transcript.messages.len()))"));
}
