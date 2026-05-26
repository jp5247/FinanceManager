//! End-to-end tests for the hash-chained audit log.

use fm_audit::{verify_chain, AuditAppender, AuditError, Clock, EventInput, GENESIS_HASH};
use std::cell::Cell;
use std::fs;
use tempfile::tempdir;

/// Deterministic clock for tests. Each call advances one second.
struct StepClock(Cell<u32>);

impl StepClock {
    fn new() -> Self {
        Self(Cell::new(0))
    }
}

impl Clock for StepClock {
    fn now_rfc3339(&self) -> String {
        let n = self.0.get();
        self.0.set(n + 1);
        format!("2026-05-26T00:00:{n:02}Z")
    }
}

#[test]
fn first_append_links_to_genesis() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("events.jsonl");
    let clock = StepClock::new();
    let mut appender = AuditAppender::open(&path).unwrap();
    assert_eq!(appender.last_hash(), GENESIS_HASH);

    let entry = appender
        .append(EventInput::new("user-001", "profile-created"), &clock)
        .unwrap();
    assert_eq!(entry.prev_hash, GENESIS_HASH);
    assert_eq!(entry.this_hash.len(), 64);
    assert_ne!(entry.this_hash, GENESIS_HASH);
}

#[test]
fn second_append_links_to_first() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("events.jsonl");
    let clock = StepClock::new();
    let mut appender = AuditAppender::open(&path).unwrap();

    let a = appender
        .append(EventInput::new("user-001", "first"), &clock)
        .unwrap();
    let b = appender
        .append(EventInput::new("user-001", "second"), &clock)
        .unwrap();
    assert_eq!(b.prev_hash, a.this_hash);
}

#[test]
fn verify_passes_on_clean_chain() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("events.jsonl");
    let clock = StepClock::new();
    let mut appender = AuditAppender::open(&path).unwrap();
    for i in 0..5 {
        appender
            .append(
                EventInput::new("user-001", "action").entity(format!("e-{i}")),
                &clock,
            )
            .unwrap();
    }
    drop(appender);
    let count = verify_chain(&path).unwrap();
    assert_eq!(count, 5);
}

#[test]
fn verify_detects_tampered_payload() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("events.jsonl");
    let clock = StepClock::new();
    let mut appender = AuditAppender::open(&path).unwrap();
    appender
        .append(EventInput::new("user-001", "first"), &clock)
        .unwrap();
    appender
        .append(EventInput::new("user-001", "second"), &clock)
        .unwrap();
    drop(appender);

    // Mutate the action of line 1 (the second entry) without recomputing
    // its hash.
    let original = fs::read_to_string(&path).unwrap();
    let tampered = original.replacen("\"second\"", "\"SECOND\"", 1);
    fs::write(&path, tampered).unwrap();

    let err = verify_chain(&path).unwrap_err();
    assert!(
        matches!(err, AuditError::TamperDetected(1)),
        "expected TamperDetected(1), got {err:?}"
    );
}

#[test]
fn verify_detects_chain_break_when_line_removed() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("events.jsonl");
    let clock = StepClock::new();
    let mut appender = AuditAppender::open(&path).unwrap();
    for _ in 0..3 {
        appender
            .append(EventInput::new("user-001", "x"), &clock)
            .unwrap();
    }
    drop(appender);

    // Drop the middle line.
    let lines: Vec<_> = fs::read_to_string(&path)
        .unwrap()
        .lines()
        .map(String::from)
        .collect();
    let reduced = format!("{}\n{}\n", lines[0], lines[2]);
    fs::write(&path, reduced).unwrap();

    let err = verify_chain(&path).unwrap_err();
    assert!(
        matches!(err, AuditError::ChainBreak(1)),
        "expected ChainBreak(1), got {err:?}"
    );
}

#[test]
fn reopen_resumes_existing_chain() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("events.jsonl");
    let clock = StepClock::new();

    {
        let mut appender = AuditAppender::open(&path).unwrap();
        appender
            .append(EventInput::new("user-001", "first"), &clock)
            .unwrap();
        appender
            .append(EventInput::new("user-001", "second"), &clock)
            .unwrap();
    }
    let appender2 = AuditAppender::open(&path).unwrap();
    let last_before = appender2.last_hash().to_string();
    assert_ne!(last_before, GENESIS_HASH);

    let mut appender2 = appender2;
    let third = appender2
        .append(EventInput::new("user-001", "third"), &clock)
        .unwrap();
    assert_eq!(third.prev_hash, last_before);
    assert_eq!(verify_chain(&path).unwrap(), 3);
}

#[test]
fn appending_to_corrupted_file_fails_at_open() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("events.jsonl");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"not valid json\n").unwrap();
    let err = AuditAppender::open(&path).unwrap_err();
    assert!(matches!(err, AuditError::ParseError(0, _)), "got {err:?}");
}
