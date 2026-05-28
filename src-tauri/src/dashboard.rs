//! Dashboard aggregation — reads every import for the unlocked profile and
//! computes the cross-statement totals the Dashboard tab renders.
//!
//! ## Money classification (matches FinanceManager.md decisions)
//!
//! - **Income** — rows categorized `Salary`, `Dividend`, `Interest`, or
//!   `Refund`.
//! - **Transfer** (P3) — rows categorized `Credit Card Payment` or
//!   `Bank Transfer`. Excluded from both income and expense; surfaced
//!   separately so the headline numbers don't double-count.
//! - **Expense** — everything else, including `ATM / Cash` (P4: cash
//!   withdrawn is treated as spent by default).
//!
//! This is intentionally simple — heuristic pairing of debit/credit rows
//! across statements is a Phase-2 problem. For v1 we trust the existing
//! categorization to label cross-account movements correctly.

use crate::state::AppState;
use crate::upload::{list_imports_internal, session, FileMeta};
use fm_core::UserId;
use fm_crypto::{open, KeyBytes};
use fm_parser::RawTransaction;
use fm_storage::{StorageRepository, VersionedJson};
use rust_decimal::Decimal;
use serde::Serialize;
use std::collections::HashMap;
use tauri::State;

const RAW_TXN_SCHEMA: u32 = 1;

/// Hard cap on a single sealed `raw-transactions.json` we'll decrypt + parse
/// into memory. Real statements top out around 100 KB; 16 MB is plenty of
/// headroom while still bounding a maliciously-crafted (or corrupted)
/// file's ability to wedge the dashboard.
const MAX_SEALED_TXN_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardData {
    /// Number of imports the aggregate includes. 0 means "no statements
    /// uploaded yet" → UI shows an empty-state.
    pub import_count: u32,

    /// Earliest and latest transaction dates seen across all imports
    /// (ISO `YYYY-MM-DD`). `None` if there are no transactions.
    pub period_start: Option<String>,
    pub period_end: Option<String>,

    pub transaction_count: u32,

    /// Decimal strings, e.g. `"42500.00"`.
    pub total_income: String,
    pub total_expense: String,
    pub net_savings: String,

    /// Transfers between own accounts — credit-card payments, account-to-
    /// account moves. Counted separately so income/expense aren't inflated.
    pub transfer_count: u32,
    pub transfer_total: String,

    /// Per-category breakdown, sorted by total descending. Combined across
    /// every import in the profile.
    pub category_totals: Vec<CategoryTotal>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryTotal {
    pub category: String,
    pub count: u32,
    pub total_debit: String,
    pub total_credit: String,
    /// `"income"` | `"expense"` | `"transfer"` — derived classification
    /// the UI uses for coloring + grouping.
    pub kind: &'static str,
}

#[tauri::command]
pub fn dashboard_aggregate(state: State<AppState>) -> Result<DashboardData, String> {
    let (user, dek) = session(&state)?;
    let imports = list_imports_internal(&state, &user, &dek)?;
    aggregate_imports(&state, &user, &dek, &imports)
}

fn aggregate_imports(
    state: &State<AppState>,
    user: &UserId,
    dek: &KeyBytes,
    imports: &[FileMeta],
) -> Result<DashboardData, String> {
    let mut all_rows: Vec<RawTransaction> = Vec::new();

    for m in imports {
        let txn_rel = txn_path(&m.import_id);
        if !state
            .storage
            .exists(user, &txn_rel)
            .map_err(|e| e.to_string())?
        {
            continue;
        }
        let sealed = state
            .storage
            .read(user, &txn_rel)
            .map_err(|e| e.to_string())?;
        if sealed.len() > MAX_SEALED_TXN_BYTES {
            return Err(format!(
                "import {} has an unexpectedly large transactions file ({} bytes); refusing to load",
                m.import_id,
                sealed.len()
            ));
        }
        let plaintext = open(dek, &sealed).map_err(|e| e.to_string())?;
        let doc: VersionedJson<Vec<RawTransaction>> =
            serde_json::from_slice(&plaintext).map_err(|e| e.to_string())?;
        if doc.schema_version != RAW_TXN_SCHEMA {
            continue;
        }
        all_rows.extend(doc.data);
    }

    Ok(summarise_rows(imports.len() as u32, &all_rows))
}

fn summarise_rows(import_count: u32, rows: &[RawTransaction]) -> DashboardData {
    let mut income = Decimal::ZERO;
    let mut expense = Decimal::ZERO;
    let mut transfer = Decimal::ZERO;
    let mut transfer_count: u32 = 0;
    let mut earliest: Option<String> = None;
    let mut latest: Option<String> = None;

    #[derive(Default)]
    struct CatAcc {
        count: u32,
        debit: Decimal,
        credit: Decimal,
    }
    let mut by_cat: HashMap<String, CatAcc> = HashMap::new();

    for r in rows {
        // Track period bounds.
        if earliest.as_deref().is_none_or(|e| r.txn_date.as_str() < e) {
            earliest = Some(r.txn_date.clone());
        }
        if latest.as_deref().is_none_or(|e| r.txn_date.as_str() > e) {
            latest = Some(r.txn_date.clone());
        }

        let cat = r.category.as_deref().unwrap_or("Uncategorized");
        let entry = by_cat.entry(cat.to_string()).or_default();
        entry.count += 1;
        let debit = r
            .debit
            .as_ref()
            .map(|a| a.as_decimal())
            .unwrap_or(Decimal::ZERO);
        let credit = r
            .credit
            .as_ref()
            .map(|a| a.as_decimal())
            .unwrap_or(Decimal::ZERO);
        entry.debit += debit;
        entry.credit += credit;

        match classify_category(cat) {
            CategoryKind::Income => {
                income += credit;
                // A debit on an income category (e.g. a salary recovery) is
                // an exotic-enough case that we'd rather not silently fold
                // it into expense — the user can recategorize.
            }
            CategoryKind::Expense => {
                expense += debit;
                // A categorized expense with a credit amount (refund, return
                // recorded against the same merchant) reduces expense.
                expense -= credit;
            }
            CategoryKind::Transfer => {
                transfer += debit + credit;
                if debit > Decimal::ZERO || credit > Decimal::ZERO {
                    transfer_count += 1;
                }
            }
        }
    }

    let net = income - expense;
    // Sort while we still have Decimals — round-tripping through formatted
    // strings is both slower and silently lossy on parse failure.
    let mut sorted: Vec<(String, CatAcc, CategoryKind)> = by_cat
        .into_iter()
        .map(|(category, acc)| {
            let kind = classify_category(&category);
            (category, acc, kind)
        })
        .collect();
    sorted.sort_by(|a, b| {
        b.1.debit
            .cmp(&a.1.debit)
            .then_with(|| b.1.credit.cmp(&a.1.credit))
    });
    let category_totals: Vec<CategoryTotal> = sorted
        .into_iter()
        .map(|(category, acc, kind)| CategoryTotal {
            category,
            count: acc.count,
            total_debit: format!("{:.2}", acc.debit),
            total_credit: format!("{:.2}", acc.credit),
            kind: kind.as_str(),
        })
        .collect();

    DashboardData {
        import_count,
        period_start: earliest,
        period_end: latest,
        transaction_count: rows.len() as u32,
        total_income: format!("{:.2}", income),
        total_expense: format!("{:.2}", expense),
        net_savings: format!("{:.2}", net),
        transfer_count,
        transfer_total: format!("{:.2}", transfer),
        category_totals,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CategoryKind {
    Income,
    Expense,
    Transfer,
}

impl CategoryKind {
    fn as_str(self) -> &'static str {
        match self {
            CategoryKind::Income => "income",
            CategoryKind::Expense => "expense",
            CategoryKind::Transfer => "transfer",
        }
    }
}

/// Classify a category label into income / expense / transfer for headline
/// aggregation. Anything not explicitly income or transfer is treated as
/// expense (the conservative direction for "money leakage" surfacing).
fn classify_category(category: &str) -> CategoryKind {
    match category {
        "Salary" | "Dividend" | "Interest" | "Refund" => CategoryKind::Income,
        // P3: own-account moves don't change net worth, so they're neither.
        "Credit Card Payment" | "Bank Transfer" => CategoryKind::Transfer,
        _ => CategoryKind::Expense,
    }
}

fn txn_path(import_id: &str) -> String {
    let (year, month) = parse_year_month(import_id).unwrap_or(("0000", "00"));
    format!("source/uploads/{year}/{month}/{import_id}/raw-transactions.json")
}

fn parse_year_month(import_id: &str) -> Option<(&str, &str)> {
    let after = import_id.strip_prefix("imp-")?;
    let year = after.get(0..4)?;
    let month = after.get(5..7)?;
    Some((year, month))
}

#[cfg(test)]
mod tests {
    use super::*;
    use fm_core::Amount;
    use fm_parser::ParserBackend;

    fn row(
        date: &str,
        description: &str,
        category: &str,
        debit: Option<&str>,
        credit: Option<&str>,
    ) -> RawTransaction {
        RawTransaction {
            import_id: "imp-001".into(),
            source_file: "fixture.pdf".into(),
            source_sha256: "deadbeef".into(),
            source_page: 1,
            row_number: 1,
            parser_version: "test@0.0.0".into(),
            parser_backend: ParserBackend::Pdfium,
            txn_date: date.into(),
            description: description.into(),
            debit: debit.map(|s| Amount::parse_inr(s).unwrap().amount),
            credit: credit.map(|s| Amount::parse_inr(s).unwrap().amount),
            balance: None,
            category: Some(category.into()),
            category_rule_id: Some("test".into()),
        }
    }

    #[test]
    fn classify_categories_correctly() {
        assert_eq!(classify_category("Salary"), CategoryKind::Income);
        assert_eq!(classify_category("Dividend"), CategoryKind::Income);
        assert_eq!(
            classify_category("Credit Card Payment"),
            CategoryKind::Transfer
        );
        assert_eq!(classify_category("Bank Transfer"), CategoryKind::Transfer);
        // P4: ATM / Cash counts as expense.
        assert_eq!(classify_category("ATM / Cash"), CategoryKind::Expense);
        assert_eq!(classify_category("Groceries"), CategoryKind::Expense);
        assert_eq!(classify_category("Uncategorized"), CategoryKind::Expense);
    }

    #[test]
    fn transfers_are_excluded_from_income_and_expense() {
        // ₹50,000 salary in, ₹20,000 CC payment out, ₹3,000 groceries out.
        // Income should be 50,000. Expense should be 3,000. Transfer 20,000.
        // If transfers leaked into expense, net would be 27,000 (wrong).
        let rows = vec![
            row("2026-04-01", "SALARY CR", "Salary", None, Some("50000.00")),
            row(
                "2026-04-15",
                "BPPY CC PAYMENT",
                "Credit Card Payment",
                Some("20000.00"),
                None,
            ),
            row(
                "2026-04-20",
                "BIGBASKET",
                "Groceries",
                Some("3000.00"),
                None,
            ),
        ];
        let d = summarise_rows(1, &rows);
        assert_eq!(d.total_income, "50000.00");
        assert_eq!(d.total_expense, "3000.00");
        assert_eq!(d.net_savings, "47000.00");
        assert_eq!(d.transfer_total, "20000.00");
        assert_eq!(d.transfer_count, 1);
    }

    #[test]
    fn cash_withdrawal_counted_as_expense() {
        let rows = vec![row(
            "2026-04-10",
            "ATM WDL 1234",
            "ATM / Cash",
            Some("5000.00"),
            None,
        )];
        let d = summarise_rows(1, &rows);
        assert_eq!(d.total_expense, "5000.00");
        assert_eq!(d.transfer_total, "0.00");
    }

    #[test]
    fn refund_credit_offsets_an_expense_category() {
        // Spending ₹1,000 at Amazon, getting ₹400 back on a return, both
        // ending up under "Online Shopping" — net should be ₹600 expense.
        let rows = vec![
            row(
                "2026-04-05",
                "AMAZON",
                "Online Shopping",
                Some("1000.00"),
                None,
            ),
            row(
                "2026-04-08",
                "AMAZON REFUND",
                "Online Shopping",
                None,
                Some("400.00"),
            ),
        ];
        let d = summarise_rows(1, &rows);
        assert_eq!(d.total_expense, "600.00");
    }

    #[test]
    fn period_covers_earliest_to_latest_date() {
        let rows = vec![
            row("2026-04-15", "X", "Groceries", Some("100.00"), None),
            row("2026-03-02", "X", "Groceries", Some("100.00"), None),
            row("2026-05-20", "X", "Groceries", Some("100.00"), None),
        ];
        let d = summarise_rows(1, &rows);
        assert_eq!(d.period_start.as_deref(), Some("2026-03-02"));
        assert_eq!(d.period_end.as_deref(), Some("2026-05-20"));
    }

    #[test]
    fn salary_debit_is_ignored_not_silently_folded_into_expense() {
        // A debit on a Salary row (e.g. salary clawback) is unusual enough
        // that we'd rather not silently inflate expense. The row's amounts
        // still show up in its category total, but headline expense stays 0.
        let rows = vec![row(
            "2026-04-01",
            "SALARY REVERSAL",
            "Salary",
            Some("5000.00"),
            None,
        )];
        let d = summarise_rows(1, &rows);
        assert_eq!(d.total_income, "0.00");
        assert_eq!(d.total_expense, "0.00");
        // Category breakdown still surfaces the row so it's not invisible.
        assert_eq!(d.category_totals[0].category, "Salary");
        assert_eq!(d.category_totals[0].total_debit, "5000.00");
    }

    #[test]
    fn empty_imports_return_zeroed_aggregate() {
        let d = summarise_rows(0, &[]);
        assert_eq!(d.import_count, 0);
        assert_eq!(d.total_income, "0.00");
        assert_eq!(d.total_expense, "0.00");
        assert_eq!(d.net_savings, "0.00");
        assert!(d.period_start.is_none());
    }

    #[test]
    fn category_totals_sorted_by_debit_descending() {
        let rows = vec![
            row("2026-04-01", "X", "Groceries", Some("3000.00"), None),
            row("2026-04-02", "X", "Restaurants", Some("8000.00"), None),
            row("2026-04-03", "X", "Fuel", Some("2000.00"), None),
        ];
        let d = summarise_rows(1, &rows);
        assert_eq!(d.category_totals[0].category, "Restaurants");
        assert_eq!(d.category_totals[1].category, "Groceries");
        assert_eq!(d.category_totals[2].category, "Fuel");
    }
}
