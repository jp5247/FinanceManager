use std::io;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuditError {
    #[error("audit log path has no parent directory")]
    InvalidPath,

    #[error("malformed JSON at line {0}: {1}")]
    ParseError(usize, String),

    #[error("chain break at line {0}: prev_hash does not match previous line's this_hash")]
    ChainBreak(usize),

    #[error("tamper detected at line {0}: recomputed hash does not match this_hash")]
    TamperDetected(usize),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
}
