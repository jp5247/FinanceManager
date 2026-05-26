//! Append-only hash-chained audit log.
//!
//! Every mutation in the app emits an [`EventInput`] through an
//! [`AuditAppender`]. Each on-disk line contains the SHA-256 hash of the
//! previous line in `prevHash`, plus a `thisHash` sealing its own content.
//! Tamper detection happens by replaying the chain via [`verify_chain`].
//!
//! ## On-disk format
//!
//! One JSON object per line (JSONL), UTF-8, LF line endings. Matches the
//! shape documented in `docs/design/local-data-schema.md` §3.14, extended
//! with the two hash fields.
//!
//! ## Hash computation
//!
//! The bytes hashed for each entry are:
//! ```text
//! prev_hash || US || ts || US || user_id || US || action || US || entity_id || US || canonical(details)
//! ```
//! where `US` is the ASCII unit separator (`0x1F`) and `canonical(details)`
//! is `serde_json::to_vec` of the value. Genesis line uses 64 hex zeros as
//! `prev_hash`.

#![forbid(unsafe_code)]

mod appender;
mod entry;
mod error;

pub use appender::{verify_chain, AuditAppender, Clock, SystemClock, GENESIS_HASH};
pub use entry::{AuditEntry, EventInput};
pub use error::AuditError;
