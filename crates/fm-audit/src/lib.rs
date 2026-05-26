//! Append-only hash-chained audit log.
//!
//! Every mutation in the app emits an event here. Each line carries the SHA-256
//! of the previous line; a verifier rejects the file if the chain breaks.
//! Tamper detection happens on session start.

#![forbid(unsafe_code)]
