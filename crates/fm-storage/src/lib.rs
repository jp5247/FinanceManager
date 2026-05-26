//! Filesystem storage seam for FinanceManager.
//!
//! Owns the [`StorageRepository`] trait, the atomic write-then-rename helper,
//! and the path resolver that enforces the per-user data-root invariant.
//!
//! All disk access in the rest of the app routes through this crate.
//!
//! ## Path-traversal guard (SEC-04)
//!
//! Every relative path supplied by callers passes through [`ProfileRoot::resolve`],
//! which rejects empty paths, absolute paths, drive prefixes, and any `..`
//! segment. The check is **syntactic** — it does not stat the filesystem — so
//! it is fast and immune to TOCTOU races on the parent dir, but it cannot
//! detect symlinks whose target escapes the data root. A compromised host OS
//! defeats this control by design (acknowledged residual risk).

#![forbid(unsafe_code)]

mod atomic;
mod error;
mod json;
mod path;
mod repository;

pub use atomic::atomic_write;
pub use error::StorageError;
pub use json::{read_json, write_json, VersionedJson};
pub use path::{DataRoot, ProfileRoot};
pub use repository::{FilesystemStorage, StorageRepository};
