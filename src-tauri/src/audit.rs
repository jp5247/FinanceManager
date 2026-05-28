//! Audit-log integration — wires the `fm-audit` hash-chained appender
//! into the app's mutation sites and exposes a read-only viewer command
//! for the UI.
//!
//! Storage: per-profile, plaintext JSONL at `audit/log.jsonl`. The log is
//! intentionally NOT sealed under the profile DEK so the chain can be
//! re-verified by an external tool against the on-disk file. The Tauri
//! `audit_log` read command DOES require an unlocked session today —
//! that's a UX choice (single read surface). A future recovery flow
//! could call `fm_audit::verify_chain` directly on the path. Each line
//! is hash-chained with the previous (see `fm_audit::AuditAppender`).
//!
//! ## PII redaction policy
//!
//! Because the log is plaintext at rest, `details` payloads must NOT
//! include PII: no merchant strings, lender names, asset names,
//! filenames, amounts, or rates. The opaque `entity_id` (e.g.
//! `loan:abc123`, `imp-…#42`) plus the action name is sufficient for
//! tamper-detection and "what did I do when". Audit B1 (Phase-1
//! finishing audit) enforces this at every call site.
//!
//! Call sites: every mutation that affects user-visible state should
//! emit one entry via `record(&state, action, entity_id, details)`.
//! Errors are swallowed (log to stderr) so audit-log unavailability
//! never blocks a real user action.

use crate::state::AppState;
use crate::upload::session;
use fm_audit::{AuditAppender, AuditEntry, EventInput, SystemClock};
use fm_core::UserId;
use serde::Serialize;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::State;

const LOG_REL: &str = "audit/log.jsonl";

/// Process-wide write serialization. The `AuditAppender` re-scans the chain
/// on every `open` call to recover `last_hash`; two concurrent
/// open-scan-append sequences would interleave and produce duplicate
/// `prev_hash` values, breaking the chain. Serializing every write site
/// behind this mutex makes the read-scan-then-write block atomic relative
/// to other audit writers in the same process. Reads (`audit_log`) can
/// run concurrently since they don't mutate.
static AUDIT_WRITE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEntryView {
    pub ts: String,
    pub action: String,
    pub entity_id: Option<String>,
    pub details: Value,
    pub this_hash: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditLogView {
    /// Newest first.
    pub entries: Vec<AuditEntryView>,
    /// `true` when the chain re-walked cleanly end-to-end.
    pub chain_ok: bool,
    /// Surface a one-line note when the chain is broken or unreadable
    /// (e.g. a partially written line); UI shows it in red.
    pub chain_note: Option<String>,
}

/// Best-effort write. Errors are logged to stderr and otherwise swallowed
/// so a corrupted / missing audit file never blocks the caller's primary
/// action. Designed to be called from inside other Tauri commands.
pub fn record(
    state: &State<AppState>,
    user: &UserId,
    action: &str,
    entity_id: Option<&str>,
    details: Value,
) {
    // Hold the write lock for the entire open-scan-append sequence. Poisoned
    // mutex is recovered — a panicked previous holder doesn't deserve to
    // block all future audit writes.
    let _guard = AUDIT_WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let path = match log_path(state, user) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("audit log path: {e}");
            return;
        }
    };
    let mut appender = match AuditAppender::open(&path) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("audit log open: {e}");
            return;
        }
    };
    let mut input = EventInput::new(user.as_str(), action).details(details);
    if let Some(id) = entity_id {
        input = input.entity(id);
    }
    if let Err(e) = appender.append(input, &SystemClock) {
        eprintln!("audit log append: {e}");
    }
}

#[tauri::command]
pub fn audit_log(state: State<AppState>) -> Result<AuditLogView, String> {
    let (user, _dek) = session(&state)?;
    let path = log_path(&state, &user)?;
    if !path.exists() {
        return Ok(AuditLogView {
            entries: Vec::new(),
            chain_ok: true,
            chain_note: None,
        });
    }

    // Read raw lines so we can show entries even if the chain is broken
    // at the tail (e.g. a partially-written last line). Chain integrity
    // is reported separately so the user can spot tampering.
    let text = std::fs::read_to_string(&path).map_err(|e| format!("read audit log: {e}"))?;
    let mut entries: Vec<AuditEntryView> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<AuditEntry>(trimmed) {
            Ok(e) => entries.push(AuditEntryView {
                ts: e.ts,
                action: e.action,
                entity_id: e.entity_id,
                details: e.details,
                this_hash: e.this_hash,
            }),
            Err(e) => {
                eprintln!("audit log parse: {e}");
            }
        }
    }
    entries.reverse(); // newest first

    let (chain_ok, chain_note) = match fm_audit::verify_chain(&path) {
        Ok(_) => (true, None),
        Err(e) => (false, Some(format!("Chain verify failed: {e}"))),
    };

    Ok(AuditLogView {
        entries,
        chain_ok,
        chain_note,
    })
}

fn log_path(state: &State<AppState>, user: &UserId) -> Result<PathBuf, String> {
    state
        .data_root
        .profile(user)
        .resolve(LOG_REL)
        .map_err(|e| format!("resolve audit path: {e}"))
}

#[cfg(test)]
mod tests {
    use super::AUDIT_WRITE_LOCK;
    use fm_audit::{verify_chain, AuditAppender, EventInput, SystemClock};
    use std::sync::Arc;
    use std::thread;

    /// Pins audit B4: concurrent appends to the same log file must produce
    /// a chain that `verify_chain` accepts. Without the global write lock,
    /// two threads would interleave open-scan-append sequences and write
    /// two entries with the same `prev_hash`, breaking the chain.
    #[test]
    fn concurrent_audit_writes_produce_a_valid_chain() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = Arc::new(dir.path().join("log.jsonl"));
        let n_threads = 4;
        let appends_per_thread = 25;
        let mut handles = Vec::new();
        for t in 0..n_threads {
            let p = Arc::clone(&path);
            handles.push(thread::spawn(move || {
                for i in 0..appends_per_thread {
                    // Mirror what `record()` does — hold the global lock
                    // across the open-scan-append sequence.
                    let _guard = AUDIT_WRITE_LOCK
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    let mut app = AuditAppender::open(&p).expect("open");
                    let input =
                        EventInput::new("user", "concurrent_test").entity(format!("t{t}-i{i}"));
                    app.append(input, &SystemClock).expect("append");
                }
            }));
        }
        for h in handles {
            h.join().expect("join");
        }
        let count = verify_chain(&path).expect("chain valid");
        assert_eq!(count, n_threads * appends_per_thread);
    }
}
