use std::io;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("invalid path: {0}")]
    InvalidPath(&'static str),

    #[error("path traversal detected: resolved path escapes the profile root")]
    PathTraversalDetected,

    #[error("data root must be an absolute, existing directory")]
    InvalidDataRoot,

    #[error("schema version mismatch: file is v{found}, this binary expects v{expected}")]
    SchemaVersionMismatch { expected: u32, found: u32 },

    #[error("JSON encode/decode error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
}
