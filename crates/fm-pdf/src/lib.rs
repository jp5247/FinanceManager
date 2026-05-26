//! PDF text extraction backend for FinanceManager.
//!
//! Wraps `pdfium-render` and produces a [`fm_parser::ExtractedPdf`] ready to
//! be handed to a [`BankAdapter`](fm_parser::BankAdapter). The heavy native
//! `pdfium-render` dependency lives in this crate alone — `fm-parser` and
//! `fm-app`'s public APIs both work without touching pdfium.
//!
//! ## Runtime requirement
//!
//! `pdfium.dll` (Windows) / `libpdfium.dylib` (macOS) / `libpdfium.so` (Linux)
//! must be discoverable when [`PdfExtractor::new`] runs. The default search
//! order is:
//! 1. The directory containing the current executable
//! 2. The system library search path (PATH on Windows, `LD_LIBRARY_PATH` on
//!    Linux, etc.)
//!
//! For Tauri builds we will bundle the library next to the installed exe so
//! step 1 always succeeds.

#![forbid(unsafe_code)]

mod error;
mod extractor;
mod hash;

pub use error::PdfExtractError;
pub use extractor::PdfExtractor;
