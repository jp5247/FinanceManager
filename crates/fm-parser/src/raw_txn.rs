use crate::extracted::ParserBackend;
use fm_core::Amount;
use serde::{Deserialize, Serialize};

/// One row produced by a [`BankAdapter`](crate::BankAdapter).
///
/// Carries the seven source-provenance fields required by
/// `docs/design/local-data-schema.md` §3.6 plus the transaction payload.
/// Sign is encoded by which of `debit` / `credit` is `Some`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawTransaction {
    // --- Provenance fields (schema §3.6) ---
    pub import_id: String,
    pub source_file: String,
    pub source_sha256: String,
    pub source_page: u32,
    pub row_number: u32,
    pub parser_version: String,
    pub parser_backend: ParserBackend,

    // --- Transaction payload ---
    /// ISO `YYYY-MM-DD` calendar date in the statement's locale.
    pub txn_date: String,
    pub description: String,
    pub debit: Option<Amount>,
    pub credit: Option<Amount>,
    pub balance: Option<Amount>,

    // --- Categorization (added by fm-categorize after parsing) ---
    /// `None` when the parser produced the row but no rule matched yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// ID of the rule that classified this row, e.g. `"food/swiggy"`.
    /// Useful for audit / debug — which rule fired?
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category_rule_id: Option<String>,
}
