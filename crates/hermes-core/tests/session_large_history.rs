use std::time::{Duration, Instant};

use hermes_core::SessionTranscript;
use hermes_protocol::{ChatMessage, MessageRole, SessionResumeResponse};

const LARGE_HISTORY: usize = 100_000;
const TRANSCRIPT_WINDOW: usize = 80;

fn message(index: usize) -> ChatMessage {
    ChatMessage {
        id: index.to_string(),
        role: if index.is_multiple_of(2) {
            MessageRole::User
        } else {
            MessageRole::Assistant
        },
        text: format!("message {index}"),
        ..ChatMessage::default()
    }
}

#[test]
fn hydrates_a_large_transcript_without_losing_identity() {
    let messages = (0..LARGE_HISTORY).map(message).collect::<Vec<_>>();
    let started = Instant::now();
    let transcript = SessionTranscript::load(
        "stored-large".into(),
        SessionResumeResponse {
            session_id: "runtime-large".into(),
            messages,
            message_count: LARGE_HISTORY,
            ..SessionResumeResponse::default()
        },
    );
    let elapsed = started.elapsed();

    assert_eq!(transcript.messages.len(), LARGE_HISTORY);
    assert_eq!(transcript.message_count, LARGE_HISTORY);
    assert!(!transcript.messages_omitted);
    assert_eq!(
        transcript
            .messages
            .first()
            .map(|message| message.id.as_str()),
        Some("0")
    );
    assert_eq!(
        transcript
            .messages
            .last()
            .map(|message| message.id.as_str()),
        Some("99999")
    );
    assert_eq!(
        transcript
            .messages
            .iter()
            .rev()
            .take(TRANSCRIPT_WINDOW)
            .count(),
        TRANSCRIPT_WINDOW
    );

    // This is deliberately a generous regression ceiling rather than a
    // microbenchmark: CI runners vary, but hydration should remain linear and
    // comfortably bounded for a six-figure transcript.
    assert!(
        elapsed < Duration::from_secs(15),
        "large transcript hydration took {elapsed:?}"
    );
}

#[test]
fn million_message_server_history_stays_explicitly_paginated() {
    let tail = (0..TRANSCRIPT_WINDOW).map(message).collect::<Vec<_>>();
    let transcript = SessionTranscript::load(
        "stored-million".into(),
        SessionResumeResponse {
            session_id: "runtime-million".into(),
            messages: tail,
            message_count: 1_000_000,
            messages_omitted: true,
            ..SessionResumeResponse::default()
        },
    );

    assert_eq!(transcript.messages.len(), TRANSCRIPT_WINDOW);
    assert_eq!(transcript.message_count, 1_000_000);
    assert!(transcript.messages_omitted);
}
