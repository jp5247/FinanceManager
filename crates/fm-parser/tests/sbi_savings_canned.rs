//! SBI savings adapter tests against canned text fixtures whose structure
//! mirrors what pdfium produces for the real SBI statements observed in the
//! Phase-0 spike. All narrations synthesized — no real customer data.

use fm_core::Amount;
use fm_parser::{
    default_adapters, detect_adapter, BankAdapter, ExtractedPdf, PageText, ParserBackend,
    SbiSavingsAdapter,
};

fn make_pdf(filename: &str, body: &str) -> ExtractedPdf {
    ExtractedPdf {
        source_file: filename.to_string(),
        source_sha256: "deadbeef".to_string(),
        backend: ParserBackend::Pdfium,
        pages: vec![PageText {
            page_number: 1,
            text: body.to_string(),
        }],
    }
}

/// Body covering: preamble, three debits across days, one credit, multi-line
/// wraps with the empty-anchor pattern (date pair on its own line, type
/// marker on next line), and a single-line anchor (type on same line as
/// the dates).
const SBI_BODY: &str = r#"State Bank of India
Statement of Account
From : 01/02/2026 To : 28/02/2026
04/02/2026 04/02/2026
WDL TFR
UPI/DR/640109915231/Ghatkopa/
PPIW/ombk.aaci1/Paid
0097692162094 AT 01131
GHATKOPAR (WEST)
- 1,500.00 - 540.20
05/02/2026 05/02/2026
WDL TFR
UPI/DR/640202121744/Famous
P/UTIB/gpay-11257/Paid
0097693162093 AT 01131
GHATKOPAR (WEST)
- 140.00 - 400.20
10/02/2026 10/02/2026 DEBIT ATMCard AMC
544670*6206
- 236.00 - 104.20
11/02/2026 11/02/2026
DEP TFR
UPI/CR/640859090908/JAI
CHET/HDFC/7738227090/Paid
0097734162099 AT 01131
GHATKOPAR (WEST)
- - 3,100.00 3,204.20
"#;

#[test]
fn adapter_id_and_version() {
    let a = SbiSavingsAdapter::new();
    assert_eq!(a.id(), "sbi-savings");
    assert_eq!(a.version(), "1.0.0");
}

#[test]
fn detect_matches_filename_hint() {
    let a = SbiSavingsAdapter::new();
    let p = make_pdf("SBI savings account.pdf", "");
    assert!(a.detect(&p));
}

#[test]
fn detect_matches_page_text_when_filename_uninformative() {
    let a = SbiSavingsAdapter::new();
    let p = make_pdf("statement.pdf", SBI_BODY);
    assert!(a.detect(&p));
}

#[test]
fn detect_rejects_hdfc_statement() {
    let a = SbiSavingsAdapter::new();
    let p = make_pdf(
        "HDFC regalia.pdf",
        "HDFC Bank Credit Cards\nDomestic Transactions",
    );
    assert!(!a.detect(&p));
}

#[test]
fn parses_four_rows_with_positional_direction() {
    let a = SbiSavingsAdapter::new();
    let p = make_pdf("SBI savings account.pdf", SBI_BODY);
    let rows = a.parse(&p, "imp-001").unwrap();
    assert_eq!(rows.len(), 4, "got {} rows: {rows:#?}", rows.len());

    // Row 1: debit, multi-line empty-anchor pattern
    let r = &rows[0];
    assert_eq!(r.txn_date, "2026-02-04");
    assert!(r.description.contains("WDL TFR"));
    assert!(r.description.contains("UPI/DR/640109915231/Ghatkopa/"));
    assert_eq!(r.debit, Some(Amount::parse_inr("1,500.00").unwrap().amount));
    assert!(r.credit.is_none());
    assert_eq!(r.balance, Some(Amount::parse_inr("540.20").unwrap().amount));

    // Row 2: debit
    let r = &rows[1];
    assert_eq!(r.txn_date, "2026-02-05");
    assert_eq!(r.debit, Some(Amount::parse_inr("140.00").unwrap().amount));

    // Row 3: debit with same-line type marker
    let r = &rows[2];
    assert_eq!(r.txn_date, "2026-02-10");
    assert!(r.description.starts_with("DEBIT ATMCard AMC"));
    assert!(r.description.contains("544670*6206"));
    assert_eq!(r.debit, Some(Amount::parse_inr("236.00").unwrap().amount));

    // Row 4: credit — `- - AMT BAL` pattern
    let r = &rows[3];
    assert_eq!(r.txn_date, "2026-02-11");
    assert!(r.description.contains("DEP TFR"));
    assert_eq!(
        r.credit,
        Some(Amount::parse_inr("3,100.00").unwrap().amount)
    );
    assert!(r.debit.is_none(), "row 4 should be credit-only");
    assert_eq!(
        r.balance,
        Some(Amount::parse_inr("3,204.20").unwrap().amount)
    );
}

#[test]
fn provenance_fields_populated() {
    let a = SbiSavingsAdapter::new();
    let p = make_pdf("SBI savings.pdf", SBI_BODY);
    let rows = a.parse(&p, "imp-xyz").unwrap();
    assert!(!rows.is_empty());
    for (idx, r) in rows.iter().enumerate() {
        assert_eq!(r.import_id, "imp-xyz");
        assert_eq!(r.source_file, "SBI savings.pdf");
        assert_eq!(r.source_sha256, "deadbeef");
        assert_eq!(r.source_page, 1);
        assert_eq!(r.parser_version, "sbi-savings@1.0.0");
        assert_eq!(r.parser_backend, ParserBackend::Pdfium);
        assert_eq!(r.row_number, (idx as u32) + 1);
        assert!(
            r.balance.is_some(),
            "SBI savings always has running balance"
        );
    }
}

#[test]
fn statement_period_line_is_not_treated_as_anchor() {
    // `From : 01/02/2026 To : 28/02/2026` has two dates but a `To :` between
    // them — anchor regex requires only whitespace between, so this line
    // must not be confused with a transaction row.
    let body = "From : 01/02/2026 To : 28/02/2026\n";
    let a = SbiSavingsAdapter::new();
    let p = make_pdf("SBI.pdf", body);
    let rows = a.parse(&p, "imp-001").unwrap();
    assert_eq!(rows.len(), 0);
}

#[test]
fn indian_lakhs_amount_and_balance_are_handled() {
    let body = "01/02/2026 01/02/2026\nBIG PURCHASE\n- 1,12,722.70 - 42,277.30\n";
    let a = SbiSavingsAdapter::new();
    let p = make_pdf("SBI savings.pdf", body);
    let rows = a.parse(&p, "imp-001").unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].debit,
        Some(Amount::parse_inr("1,12,722.70").unwrap().amount)
    );
    assert_eq!(
        rows[0].balance,
        Some(Amount::parse_inr("42,277.30").unwrap().amount)
    );
}

#[test]
fn cr_suffix_on_balance_is_tolerated() {
    let body = "01/02/2026 01/02/2026\nSOMETHING\n- - 500.00 5,000.00CR\n";
    let a = SbiSavingsAdapter::new();
    let p = make_pdf("SBI savings.pdf", body);
    let rows = a.parse(&p, "imp-001").unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].credit,
        Some(Amount::parse_inr("500.00").unwrap().amount)
    );
    assert_eq!(
        rows[0].balance,
        Some(Amount::parse_inr("5,000.00").unwrap().amount)
    );
}

#[test]
fn registry_picks_sbi_for_sbi_pdf() {
    let adapters = default_adapters();
    let p = make_pdf("SBI savings account.pdf", SBI_BODY);
    let chosen = detect_adapter(&adapters, &p).expect("an adapter should claim this");
    assert_eq!(chosen.id(), "sbi-savings");
}

#[test]
fn registry_still_picks_hdfc_for_hdfc_pdf() {
    let adapters = default_adapters();
    let p = make_pdf("HDFC regalia.pdf", "HDFC Bank Credit Cards");
    let chosen = detect_adapter(&adapters, &p).expect("an adapter should claim this");
    assert_eq!(chosen.id(), "hdfc-cc");
}
