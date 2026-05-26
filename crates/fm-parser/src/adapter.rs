use crate::extracted::ExtractedPdf;
use crate::raw_txn::RawTransaction;
use fm_core::AmountParseError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("no adapter could detect this statement format")]
    NoAdapterMatched,

    #[error("amount parse failure: {0}")]
    BadAmount(#[from] AmountParseError),

    #[error("malformed date in row: {0:?}")]
    BadDate(String),

    #[error("unexpected statement structure: {0}")]
    BadStructure(&'static str),
}

/// One per-issuer parser. Implementations are pure functions over already-
/// extracted text — no I/O, no PDF library calls, no network.
pub trait BankAdapter {
    /// Adapter identifier used in `parserVersion` strings, e.g. `"hdfc-cc"`.
    fn id(&self) -> &'static str;

    /// SemVer of this adapter's parser logic. Bump when output changes.
    fn version(&self) -> &'static str;

    /// Inspect the extracted text and decide if this adapter understands it.
    /// Cheap — should NOT do a full parse pass.
    fn detect(&self, extracted: &ExtractedPdf) -> bool;

    /// Emit one [`RawTransaction`] per transaction row found. Lines that
    /// don't match the row pattern (headers, footers, totals) are skipped
    /// silently — they're not errors.
    fn parse(
        &self,
        extracted: &ExtractedPdf,
        import_id: &str,
    ) -> Result<Vec<RawTransaction>, ParseError>;
}
