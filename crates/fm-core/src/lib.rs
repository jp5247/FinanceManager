//! Shared domain types for FinanceManager.
//!
//! This crate is pure types and value semantics. No I/O, no filesystem, no
//! network. Other crates depend on it; it depends on nothing in this
//! workspace.

#![forbid(unsafe_code)]

mod amount;
mod ids;

pub use amount::{Amount, AmountParseError, ParsedAmount, Sign};
pub use ids::{InvalidIdError, UserId};
