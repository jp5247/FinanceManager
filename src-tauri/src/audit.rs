//! Audit-log integration — wires the `fm-audit` hash-chained appender
//! into the app's mutation sites and exposes a read-only viewer command
//! for the UI.
//!
//! Storage: per-profile, plaintext JSONL at `audit/log.jsonl`. The log is
//! intentionally NOT sealed under the profile DEK so a future
//! disaster-recovery flow can verify the chain without unlocking the
//! profile. Each line is hash-chained with the previous (see
//! `fm_audit::AuditAppender`).
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
use tauri::State;

const LOG_REL: &str = "audit/log.jsonl";

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
