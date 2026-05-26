//! Filesystem storage seam for FinanceManager.
//!
//! Owns the StorageRepository trait, the atomic write-then-rename helper, and
//! the path resolver that enforces the per-user data-root invariant.
//!
//! All disk access in the rest of the app routes through this crate.

#![forbid(unsafe_code)]
