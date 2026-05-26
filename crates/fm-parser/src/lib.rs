//! Bank-statement parsing framework.
//!
//! Pure-Rust pipeline: given the text already extracted from a PDF
//! (typically by `pdfium-render`), per-issuer [`BankAdapter`]s emit
//! [`RawTransaction`] records that match the on-disk schema documented in
//! `docs/design/local-data-schema.md` §3.6.
//!
//! The PDF extraction itself lives outside this crate — this layer only sees
//! the already-extracted text, which makes the whole pipeline unit-testable
//! against canned text fixtures without touching real statements.

#![forbid(unsafe_code)]

mod adapter;
mod adapters;
mod extracted;
mod raw_txn;
mod registry;

pub use adapter::{BankAdapter, ParseError};
pub use adapters::hdfc_cc::HdfcCreditCardAdapter;
pub use extracted::{ExtractedPdf, PageText, ParserBackend};
pub use raw_txn::RawTransaction;
pub use registry::{default_adapters, detect_adapter};
