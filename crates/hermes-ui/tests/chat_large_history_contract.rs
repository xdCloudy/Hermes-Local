const TRANSCRIPT_WINDOW: usize = 80;

fn visible_message_start(total: usize, requested: usize) -> usize {
    total.saturating_sub(requested.max(1))
}

#[test]
fn million_message_history_stays_window_bounded() {
    let total = 1_000_000_usize;
    let mut requested = TRANSCRIPT_WINDOW;

    assert_eq!(visible_message_start(total, requested), 999_920);
    assert_eq!(
        total - visible_message_start(total, requested),
        TRANSCRIPT_WINDOW
    );

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
fn repeated_expansion_never_overflows_or_underflows() {
    let total = usize::MAX - 1;
    let mut requested = TRANSCRIPT_WINDOW;

    for _ in 0..128 {
        let start = visible_message_start(total, requested);
        assert!(start <= total);
        assert!(total - start <= requested.max(1));
        requested = requested.saturating_add(TRANSCRIPT_WINDOW).min(total);
    }
}
