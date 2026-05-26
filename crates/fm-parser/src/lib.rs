//! Bank-statement parsing framework.
//!
//! Defines the BankAdapter trait and the staged normalization pipeline that
//! turns extracted PDF text into the canonical transaction rows documented in
//! [`docs/design/local-data-schema.md`]. The pdfium-render extraction layer
//! lives outside this crate; this crate is the framework that consumes its
//! output.

#![forbid(unsafe_code)]
