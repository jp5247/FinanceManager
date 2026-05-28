//! Excel export — single-workbook, values-only snapshot of every cleaned
//! input the user has captured. Decision E3 + E4 picked this shape; see
//! `FinanceManager.md` §11.4.
//!
//! Sheets, in order:
//! 1. **Summary** — headline cash-flow + wealth + debt numbers.
//! 2. **Transactions** — every row across every import, with provenance.
//! 3. **Categories** — per-category aggregates.
//! 4. **Investments** — manual asset positions.
//! 5. **Loans** — loan positions with verdict + effective rate.
//!
//! Uncategorized rows are not a hard gate — we warn (via the returned
//! `ExportResult.warning`) but the export still runs so the user always
//! has an escape hatch.

use crate::dashboard::{dashboard_aggregate, DashboardData};
use crate::investments;
use crate::loans;
use crate::state::AppState;
use crate::upload::{list_imports_internal, session};
use fm_core::UserId;
use fm_crypto::{open, KeyBytes};
use fm_parser::RawTransaction;
use fm_storage::{StorageRepository, VersionedJson};
use rust_xlsxwriter::{Color, Format, FormatBorder, Workbook};
use serde::Serialize;
use std::path::Path;
use tauri::State;

const RAW_TXN_SCHEMA: u32 = 1;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub file_path: String,
    pub transaction_count: u32,
    pub uncategorized_count: u32,
    pub investment_count: u32,
    pub loan_count: u32,
    /// Non-blocking warning surfaced when the data isn't fully clean (e.g.
    /// uncategorized rows). The export still runs to completion.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

#[tauri::command]
pub fn export_to_xlsx(file_path: String, state: State<AppState>) -> Result<ExportResult, String> {
    let (user, dek) = session(&state)?;

    let dashboard = dashboard_aggregate(state.clone(), None, None)?;
    let txns = load_all_transactions(&state, &user, &dek)?;
    let assets = investments::list_investments(state.clone())?;
    let loans_summary = loans::loans_summary(state.clone())?;
    let loans_list = loans::list_loans(state.clone())?;

    let uncategorized = txns
        .iter()
        .filter(|r| {
            r.category
                .as_deref()
                .map(|c| c.eq_ignore_ascii_case("Uncategorized"))
                .unwrap_or(true)
        })
        .count() as u32;

    let mut book = Workbook::new();
    write_summary_sheet(&mut book, &dashboard, &assets, &loans_summary)?;
    write_transactions_sheet(&mut book, &txns)?;
    write_categories_sheet(&mut book, &dashboard)?;
    write_investments_sheet(&mut book, &assets)?;
    write_loans_sheet(&mut book, &loans_list, &loans_summary)?;

    book.save(Path::new(&file_path))
        .map_err(|e| format!("save xlsx: {e}"))?;

    let warning = if uncategorized > 0 {
        Some(format!(
            "{uncategorized} transaction{} still Uncategorized — exported anyway, but the per-category breakdown will under-attribute spend.",
            if uncategorized == 1 { "" } else { "s" }
        ))
    } else {
        None
    };

    Ok(ExportResult {
        file_path,
        transaction_count: txns.len() as u32,
        uncategorized_count: uncategorized,
        investment_count: assets.len() as u32,
        loan_count: loans_list.len() as u32,
        warning,
    })
}

fn load_all_transactions(
    state: &State<AppState>,
    user: &UserId,
    dek: &KeyBytes,
) -> Result<Vec<RawTransaction>, String> {
    let imports = list_imports_internal(state, user, dek)?;
    let mut out = Vec::new();
    for m in imports {
        let rel = format!(
            "source/uploads/{year}/{month}/{id}/raw-transactions.json",
            year = &m
                .import_id
                .strip_prefix("imp-")
                .and_then(|s| s.get(0..4))
                .unwrap_or("0000"),
            month = &m
                .import_id
                .strip_prefix("imp-")
                .and_then(|s| s.get(5..7))
                .unwrap_or("00"),
            id = m.import_id,
        );
        if !state
            .storage
            .exists(user, &rel)
            .map_err(|e| e.to_string())?
        {
            continue;
        }
        let sealed = state.storage.read(user, &rel).map_err(|e| e.to_string())?;
        let plaintext = open(dek, &sealed).map_err(|e| e.to_string())?;
        let doc: VersionedJson<Vec<RawTransaction>> =
            serde_json::from_slice(&plaintext).map_err(|e| e.to_string())?;
        if doc.schema_version != RAW_TXN_SCHEMA {
            continue;
        }
        out.extend(doc.data);
    }
    out.sort_by(|a, b| b.txn_date.cmp(&a.txn_date));
    Ok(out)
}

fn header_format() -> Format {
    Format::new()
        .set_bold()
        .set_background_color(Color::RGB(0x1F2937))
        .set_font_color(Color::White)
        .set_border(FormatBorder::Thin)
}

fn label_format() -> Format {
    Format::new().set_bold()
}

fn money_format() -> Format {
    Format::new().set_num_format("#,##0.00")
}

fn percent_format() -> Format {
    Format::new().set_num_format("0.00%")
}

fn write_summary_sheet(
    book: &mut Workbook,
    d: &DashboardData,
    assets: &[crate::investments::InvestmentAsset],
    loans: &crate::loans::LoansSummary,
) -> Result<(), String> {
    let ws = book
        .add_worksheet()
        .set_name("Summary")
        .map_err(|e| e.to_string())?;
    let label = label_format();
    let money = money_format();

    let mut row: u32 = 0;
    ws.write_with_format(
        row,
        0,
        "FinanceManager export",
        &label_format().clone().set_font_size(14),
    )
    .map_err(|e| e.to_string())?;
    row += 2;

    ws.write_with_format(row, 0, "Cash flow (all uploaded periods)", &label)
        .map_err(|e| e.to_string())?;
    row += 1;
    ws.write(row, 0, "Total income")
        .map_err(|e| e.to_string())?;
    ws.write_with_format(row, 1, decimal_to_f64(&d.total_income), &money)
        .map_err(|e| e.to_string())?;
    row += 1;
    ws.write(row, 0, "Total expense")
        .map_err(|e| e.to_string())?;
    ws.write_with_format(row, 1, decimal_to_f64(&d.total_expense), &money)
        .map_err(|e| e.to_string())?;
    row += 1;
    ws.write(row, 0, "Net savings").map_err(|e| e.to_string())?;
    ws.write_with_format(row, 1, decimal_to_f64(&d.net_savings), &money)
        .map_err(|e| e.to_string())?;
    row += 1;
    ws.write(row, 0, "Transfers (excluded)")
        .map_err(|e| e.to_string())?;
    ws.write_with_format(row, 1, decimal_to_f64(&d.transfer_total), &money)
        .map_err(|e| e.to_string())?;
    row += 2;

    ws.write_with_format(row, 0, "Financial health", &label)
        .map_err(|e| e.to_string())?;
    row += 1;
    ws.write(row, 0, "Composite score")
        .map_err(|e| e.to_string())?;
    ws.write(row, 1, d.health_score.composite as f64)
        .map_err(|e| e.to_string())?;
    row += 1;
    for driver in &d.health_score.drivers {
        ws.write(row, 0, format!("  {}", driver.label))
            .map_err(|e| e.to_string())?;
        ws.write(row, 1, driver.score as f64)
            .map_err(|e| e.to_string())?;
        ws.write_with_format(row, 2, driver.weight as f64, &percent_format())
            .map_err(|e| e.to_string())?;
        row += 1;
    }
    row += 1;

    ws.write_with_format(row, 0, "Wealth (manual positions)", &label)
        .map_err(|e| e.to_string())?;
    row += 1;
    let invested: f64 = assets
        .iter()
        .map(|a| decimal_to_f64(&a.invested_amount))
        .sum();
    let current: f64 = assets
        .iter()
        .map(|a| decimal_to_f64(&a.current_value))
        .sum();
    ws.write(row, 0, "Total invested")
        .map_err(|e| e.to_string())?;
    ws.write_with_format(row, 1, invested, &money)
        .map_err(|e| e.to_string())?;
    row += 1;
    ws.write(row, 0, "Current value")
        .map_err(|e| e.to_string())?;
    ws.write_with_format(row, 1, current, &money)
        .map_err(|e| e.to_string())?;
    row += 1;
    ws.write(row, 0, "Unrealized gain/loss")
        .map_err(|e| e.to_string())?;
    ws.write_with_format(row, 1, current - invested, &money)
        .map_err(|e| e.to_string())?;
    row += 2;

    ws.write_with_format(row, 0, "Debt", &label)
        .map_err(|e| e.to_string())?;
    row += 1;
    ws.write(row, 0, "Total outstanding")
        .map_err(|e| e.to_string())?;
    ws.write_with_format(row, 1, decimal_to_f64(&loans.total_outstanding), &money)
        .map_err(|e| e.to_string())?;
    row += 1;
    ws.write(row, 0, "Monthly EMI").map_err(|e| e.to_string())?;
    ws.write_with_format(row, 1, decimal_to_f64(&loans.total_monthly_emi), &money)
        .map_err(|e| e.to_string())?;
    row += 1;
    ws.write(row, 0, "Weighted avg rate (%)")
        .map_err(|e| e.to_string())?;
    ws.write(row, 1, decimal_to_f64(&loans.weighted_avg_rate))
        .map_err(|e| e.to_string())?;

    ws.set_column_width(0, 32.0).map_err(|e| e.to_string())?;
    ws.set_column_width(1, 18.0).map_err(|e| e.to_string())?;
    ws.set_column_width(2, 12.0).map_err(|e| e.to_string())?;
    Ok(())
}

fn write_transactions_sheet(book: &mut Workbook, rows: &[RawTransaction]) -> Result<(), String> {
    let ws = book
        .add_worksheet()
        .set_name("Transactions")
        .map_err(|e| e.to_string())?;
    let head = header_format();
    let money = money_format();

    let headers = [
        "Date",
        "Source file",
        "Adapter",
        "Description",
        "Category",
        "Rule",
        "Debit",
        "Credit",
        "Balance",
        "Import ID",
        "Row",
    ];
    for (col, h) in headers.iter().enumerate() {
        ws.write_with_format(0, col as u16, *h, &head)
            .map_err(|e| e.to_string())?;
    }

    for (i, r) in rows.iter().enumerate() {
        let row = (i + 1) as u32;
        ws.write(row, 0, &r.txn_date).map_err(|e| e.to_string())?;
        ws.write(row, 1, &r.source_file)
            .map_err(|e| e.to_string())?;
        ws.write(row, 2, &r.parser_version)
            .map_err(|e| e.to_string())?;
        ws.write(row, 3, &r.description)
            .map_err(|e| e.to_string())?;
        ws.write(row, 4, r.category.as_deref().unwrap_or("Uncategorized"))
            .map_err(|e| e.to_string())?;
        ws.write(row, 5, r.category_rule_id.as_deref().unwrap_or(""))
            .map_err(|e| e.to_string())?;
        if let Some(d) = &r.debit {
            ws.write_with_format(row, 6, decimal_to_f64(&d.to_string()), &money)
                .map_err(|e| e.to_string())?;
        }
        if let Some(c) = &r.credit {
            ws.write_with_format(row, 7, decimal_to_f64(&c.to_string()), &money)
                .map_err(|e| e.to_string())?;
        }
        if let Some(b) = &r.balance {
            ws.write_with_format(row, 8, decimal_to_f64(&b.to_string()), &money)
                .map_err(|e| e.to_string())?;
        }
        ws.write(row, 9, &r.import_id).map_err(|e| e.to_string())?;
        ws.write(row, 10, r.row_number as f64)
            .map_err(|e| e.to_string())?;
    }

    let widths = [
        12.0, 28.0, 18.0, 48.0, 22.0, 16.0, 12.0, 12.0, 12.0, 22.0, 6.0,
    ];
    for (col, w) in widths.iter().enumerate() {
        ws.set_column_width(col as u16, *w)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn write_categories_sheet(book: &mut Workbook, d: &DashboardData) -> Result<(), String> {
    let ws = book
        .add_worksheet()
        .set_name("Categories")
        .map_err(|e| e.to_string())?;
    let head = header_format();
    let money = money_format();

    let headers = ["Category", "Kind", "Count", "Total debit", "Total credit"];
    for (col, h) in headers.iter().enumerate() {
        ws.write_with_format(0, col as u16, *h, &head)
            .map_err(|e| e.to_string())?;
    }
    for (i, c) in d.category_totals.iter().enumerate() {
        let row = (i + 1) as u32;
        ws.write(row, 0, &c.category).map_err(|e| e.to_string())?;
        ws.write(row, 1, c.kind).map_err(|e| e.to_string())?;
        ws.write(row, 2, c.count as f64)
            .map_err(|e| e.to_string())?;
        ws.write_with_format(row, 3, decimal_to_f64(&c.total_debit), &money)
            .map_err(|e| e.to_string())?;
        ws.write_with_format(row, 4, decimal_to_f64(&c.total_credit), &money)
            .map_err(|e| e.to_string())?;
    }
    ws.set_column_width(0, 28.0).map_err(|e| e.to_string())?;
    ws.set_column_width(1, 12.0).map_err(|e| e.to_string())?;
    ws.set_column_width(3, 14.0).map_err(|e| e.to_string())?;
    ws.set_column_width(4, 14.0).map_err(|e| e.to_string())?;
    Ok(())
}

fn write_investments_sheet(
    book: &mut Workbook,
    assets: &[crate::investments::InvestmentAsset],
) -> Result<(), String> {
    let ws = book
        .add_worksheet()
        .set_name("Investments")
        .map_err(|e| e.to_string())?;
    let head = header_format();
    let money = money_format();

    let headers = [
        "Type",
        "Name",
        "Invested",
        "Current",
        "Gain/Loss",
        "Updated",
        "Notes",
    ];
    for (col, h) in headers.iter().enumerate() {
        ws.write_with_format(0, col as u16, *h, &head)
            .map_err(|e| e.to_string())?;
    }
    for (i, a) in assets.iter().enumerate() {
        let row = (i + 1) as u32;
        let inv = decimal_to_f64(&a.invested_amount);
        let cur = decimal_to_f64(&a.current_value);
        ws.write(row, 0, &a.asset_type).map_err(|e| e.to_string())?;
        ws.write(row, 1, &a.asset_name).map_err(|e| e.to_string())?;
        ws.write_with_format(row, 2, inv, &money)
            .map_err(|e| e.to_string())?;
        ws.write_with_format(row, 3, cur, &money)
            .map_err(|e| e.to_string())?;
        ws.write_with_format(row, 4, cur - inv, &money)
            .map_err(|e| e.to_string())?;
        ws.write(
            row,
            5,
            a.last_updated_at.get(..10).unwrap_or(&a.last_updated_at),
        )
        .map_err(|e| e.to_string())?;
        ws.write(row, 6, a.notes.as_deref().unwrap_or(""))
            .map_err(|e| e.to_string())?;
    }
    ws.set_column_width(0, 16.0).map_err(|e| e.to_string())?;
    ws.set_column_width(1, 32.0).map_err(|e| e.to_string())?;
    Ok(())
}

fn write_loans_sheet(
    book: &mut Workbook,
    loans: &[crate::loans::Loan],
    summary: &crate::loans::LoansSummary,
) -> Result<(), String> {
    let ws = book
        .add_worksheet()
        .set_name("Loans")
        .map_err(|e| e.to_string())?;
    let head = header_format();
    let money = money_format();

    let class_by_id: std::collections::HashMap<&str, &crate::loans::LoanClassification> = summary
        .classifications
        .iter()
        .map(|c| (c.loan_id.as_str(), c))
        .collect();

    let headers = [
        "Type",
        "Lender",
        "Outstanding",
        "Rate (%)",
        "Eff rate (%)",
        "Rate type",
        "EMI",
        "Tenure (months)",
        "Tax benefit",
        "Verdict",
        "Rationale",
        "Start",
        "Next due",
    ];
    for (col, h) in headers.iter().enumerate() {
        ws.write_with_format(0, col as u16, *h, &head)
            .map_err(|e| e.to_string())?;
    }
    for (i, l) in loans.iter().enumerate() {
        let row = (i + 1) as u32;
        let c = class_by_id.get(l.id.as_str());
        ws.write(row, 0, &l.loan_type).map_err(|e| e.to_string())?;
        ws.write(row, 1, &l.lender).map_err(|e| e.to_string())?;
        ws.write_with_format(row, 2, decimal_to_f64(&l.principal_outstanding), &money)
            .map_err(|e| e.to_string())?;
        ws.write(row, 3, decimal_to_f64(&l.interest_rate))
            .map_err(|e| e.to_string())?;
        ws.write(
            row,
            4,
            c.map(|c| decimal_to_f64(&c.effective_rate)).unwrap_or(0.0),
        )
        .map_err(|e| e.to_string())?;
        ws.write(row, 5, &l.rate_type).map_err(|e| e.to_string())?;
        ws.write_with_format(row, 6, decimal_to_f64(&l.emi), &money)
            .map_err(|e| e.to_string())?;
        ws.write(row, 7, l.remaining_tenure_months as f64)
            .map_err(|e| e.to_string())?;
        ws.write(row, 8, if l.tax_benefit { "Yes" } else { "No" })
            .map_err(|e| e.to_string())?;
        ws.write(row, 9, c.map(|c| c.verdict).unwrap_or(""))
            .map_err(|e| e.to_string())?;
        ws.write(row, 10, c.map(|c| c.rationale.as_str()).unwrap_or(""))
            .map_err(|e| e.to_string())?;
        ws.write(row, 11, &l.start_date)
            .map_err(|e| e.to_string())?;
        ws.write(row, 12, &l.next_due_date)
            .map_err(|e| e.to_string())?;
    }
    ws.set_column_width(0, 14.0).map_err(|e| e.to_string())?;
    ws.set_column_width(1, 20.0).map_err(|e| e.to_string())?;
    ws.set_column_width(10, 64.0).map_err(|e| e.to_string())?;
    Ok(())
}

fn decimal_to_f64(s: &str) -> f64 {
    s.trim().replace(',', "").parse::<f64>().unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `decimal_to_f64` is the silent-failure path for every numeric cell
    /// in the export — flagged in the Phase-1 finishing audit as a risk
    /// because a malformed Decimal string silently writes 0.00. Test that:
    /// 1) it round-trips canonical decimal strings,
    /// 2) it strips Indian-format commas,
    /// 3) garbage explicitly hits the 0.0 fallback (so a future change
    ///    that decides to bubble the error instead is detected).
    #[test]
    fn decimal_to_f64_round_trips_canonical_strings() {
        assert!((decimal_to_f64("1234.56") - 1234.56).abs() < 0.0001);
        assert!((decimal_to_f64("0.00") - 0.0).abs() < 0.0001);
        assert!((decimal_to_f64("-99.99") + 99.99).abs() < 0.0001);
    }

    #[test]
    fn decimal_to_f64_strips_indian_format_commas() {
        assert!((decimal_to_f64("1,25,000.50") - 125000.50).abs() < 0.001);
        assert!((decimal_to_f64("47,500.00") - 47500.0).abs() < 0.001);
    }

    #[test]
    fn decimal_to_f64_silent_zero_on_garbage() {
        // Documented silent-failure behaviour. If this assertion ever
        // flips, downstream cells in the Excel export start writing 0.00
        // instead of failing loudly — make sure we know we're changing
        // contract.
        assert_eq!(decimal_to_f64("not a number"), 0.0);
        assert_eq!(decimal_to_f64(""), 0.0);
    }
}
