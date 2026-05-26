//! HDFC savings adapter tests against canned text fixtures whose structure
//! mirrors what pdfium produces for the real-PDFs we observed in the
//! Phase-0 spike. Synthesized narrations only — no real customer data.

use fm_core::Amount;
use fm_parser::{
    default_adapters, detect_adapter, BankAdapter, ExtractedPdf, HdfcSavingsAdapter, PageText,
    ParserBackend,
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

/// A 5-row body covering: opening balance header, debit, debit, credit
/// (balance rises), wrap-line narration, and a section header acting as a
/// hard-break.
const SAVINGS_BODY: &str = r#"HDFC Bank Statement of Account
Account No: 50100XXXXXXXX
Opening Balance B/F 1,55,000.00
Date Narration Chq./Ref.No. Value Dt Withdrawal Amt. Deposit Amt. Closing Balance
01/05/26 UPI-MERCHANT-GPAY 0000648728252238 01/05/26 207.00 1,54,793.00
04/05/26 ACH D- CLEARING CORP-D6800438X028 0000003478615640 04/05/26 500.00 1,54,293.00
10/05/26 SALARY CR-EMPLOYER-XYZ
0000003478611111 10/05/26 1,00,000.00 2,54,293.00
12/05/26 UPI-CRED CLUB-CRED.CLUB@AXISB-UTIB000011 0000649003914140 12/05/26 21,447.00 2,32,846.00
Page 1 of 4
**End of Statement**
"#;

#[test]
fn adapter_id_and_version() {
    let a = HdfcSavingsAdapter::new();
    assert_eq!(a.id(), "hdfc-savings");
    assert_eq!(a.version(), "1.0.0");
}

#[test]
fn detect_matches_filename_hint() {
    let a = HdfcSavingsAdapter::new();
    let p = make_pdf("April HDFC savings.pdf", "");
    assert!(a.detect(&p));
}

#[test]
fn detect_matches_page_text_when_filename_uninformative() {
    let a = HdfcSavingsAdapter::new();
    let p = make_pdf("statement.pdf", SAVINGS_BODY);
    assert!(a.detect(&p));
}

#[test]
fn detect_rejects_cc_statement() {
    let a = HdfcSavingsAdapter::new();
    let p = make_pdf(
        "HDFC regalia.pdf",
        "HDFC Bank Credit Cards\n21/04/2026| 00:00 SOMEMERCHANT C 100.00 l",
    );
    // Filename says CC; text contains "credit cards" but not "savings account".
    assert!(!a.detect(&p));
}

#[test]
fn registry_picks_savings_for_savings_pdf() {
    let adapters = default_adapters();
    let p = make_pdf("April HDFC savings.pdf", SAVINGS_BODY);
    let chosen = detect_adapter(&adapters, &p).expect("an adapter should claim this");
    assert_eq!(chosen.id(), "hdfc-savings");
}

#[test]
fn registry_picks_cc_for_cc_pdf() {
    let adapters = default_adapters();
    let p = make_pdf("HDFC regalia.pdf", "HDFC Bank Credit Cards");
    let chosen = detect_adapter(&adapters, &p).expect("an adapter should claim this");
    assert_eq!(chosen.id(), "hdfc-cc");
}

#[test]
fn parses_four_rows_with_balance_delta_direction() {
    let a = HdfcSavingsAdapter::new();
    let p = make_pdf("April HDFC savings.pdf", SAVINGS_BODY);
    let rows = a.parse(&p, "imp-001").unwrap();
    assert_eq!(rows.len(), 4, "got {} rows: {rows:#?}", rows.len());

    // Row 1: debit (balance dropped from 1,55,000 -> 1,54,793)
    let r = &rows[0];
    assert_eq!(r.txn_date, "2026-05-01");
    assert!(r.description.starts_with("UPI-MERCHANT-GPAY"));
    assert_eq!(r.debit, Some(Amount::parse_inr("207.00").unwrap().amount));
    assert!(r.credit.is_none());
    assert_eq!(
        r.balance,
        Some(Amount::parse_inr("154793.00").unwrap().amount)
    );

    // Row 2: debit (balance dropped)
    let r = &rows[1];
    assert_eq!(r.txn_date, "2026-05-04");
    assert_eq!(r.debit, Some(Amount::parse_inr("500.00").unwrap().amount));

    // Row 3: credit (balance rose), wrap-line narration
    let r = &rows[2];
    assert_eq!(r.txn_date, "2026-05-10");
    assert!(r.description.contains("SALARY CR-EMPLOYER-XYZ"));
    assert_eq!(
        r.credit,
        Some(Amount::parse_inr("1,00,000.00").unwrap().amount)
    );
    assert!(r.debit.is_none());

    // Row 4: debit
    let r = &rows[3];
    assert_eq!(r.txn_date, "2026-05-12");
    assert_eq!(
        r.debit,
        Some(Amount::parse_inr("21,447.00").unwrap().amount)
    );
}

#[test]
fn provenance_fields_populated() {
    let a = HdfcSavingsAdapter::new();
    let p = make_pdf("HDFC savings.pdf", SAVINGS_BODY);
    let rows = a.parse(&p, "imp-xyz").unwrap();
    assert!(!rows.is_empty());
    for (idx, r) in rows.iter().enumerate() {
        assert_eq!(r.import_id, "imp-xyz");
        assert_eq!(r.source_file, "HDFC savings.pdf");
        assert_eq!(r.source_sha256, "deadbeef");
        assert_eq!(r.source_page, 1);
        assert_eq!(r.parser_version, "hdfc-savings@1.0.0");
        assert_eq!(r.parser_backend, ParserBackend::Pdfium);
        assert_eq!(r.row_number, (idx as u32) + 1);
        assert!(r.balance.is_some(), "savings has running balance");
    }
}

#[test]
fn missing_opening_balance_defaults_first_row_to_debit() {
    let body = r#"HDFC Bank Statement of Account
01/05/26 SOMETHING 12345 01/05/26 500.00 1,00,000.00
"#;
    let a = HdfcSavingsAdapter::new();
    let p = make_pdf("HDFC savings.pdf", body);
    let rows = a.parse(&p, "imp-001").unwrap();
    assert_eq!(rows.len(), 1);
    // No opening balance → first row falls back to debit.
    assert!(rows[0].debit.is_some());
    assert!(rows[0].credit.is_none());
}

#[test]
fn page_footer_terminates_row_wrap() {
    let body = r#"Opening Balance B/F 1,00,000.00
01/05/26 SOMETXN
Page 1 of 4
Some footer that shouldn't be absorbed.
"#;
    let a = HdfcSavingsAdapter::new();
    let p = make_pdf("HDFC savings.pdf", body);
    let rows = a.parse(&p, "imp-001").unwrap();
    // The row's joined text is only "SOMETXN" — no amount+balance → skipped.
    assert_eq!(
        rows.len(),
        0,
        "row without amount+balance should be skipped"
    );
}

/// Regression: HDFC PDFs frequently produce three-line transactions where
/// the amount+balance is on line 2 and another narration line follows on
/// line 3. The earlier adapter joined all three into one string and lost
/// the trailing amount+balance match, dropping the row entirely.
#[test]
fn three_line_wrap_with_trailing_narration_is_parsed() {
    let body = r#"Opening Balance B/F 2,34,863.98
01/04/26 UPI-MEETALI PRAVIN
PATEL-PMMEETALIPATEL0000645720987618 01/04/26 27,692.00 2,07,171.98
1@OKAXIS-HDFC0000358-645720987618-UPI
01/04/26 ACH D- NEXT 0000003283708305 01/04/26 500.00 2,06,671.98
"#;
    let a = HdfcSavingsAdapter::new();
    let p = make_pdf("HDFC savings.pdf", body);
    let rows = a.parse(&p, "imp-001").unwrap();
    assert_eq!(rows.len(), 2, "both rows should parse: {rows:#?}");

    let r0 = &rows[0];
    assert_eq!(r0.txn_date, "2026-04-01");
    assert_eq!(
        r0.debit,
        Some(Amount::parse_inr("27692.00").unwrap().amount)
    );
    assert_eq!(
        r0.balance,
        Some(Amount::parse_inr("207171.98").unwrap().amount)
    );
    // Description absorbs the line-3 continuation.
    assert!(
        r0.description.contains("UPI-MEETALI PRAVIN"),
        "description should include the merchant name: {}",
        r0.description
    );
    assert!(
        r0.description
            .contains("1@OKAXIS-HDFC0000358-645720987618-UPI"),
        "description should absorb the line-3 continuation: {}",
        r0.description
    );

    let r1 = &rows[1];
    assert_eq!(r1.txn_date, "2026-04-01");
    assert_eq!(r1.debit, Some(Amount::parse_inr("500.00").unwrap().amount));
}

#[test]
fn indian_lakhs_amount_and_balance_are_handled() {
    let body = r#"Opening Balance B/F 1,55,000.00
01/05/26 BIG PURCHASE 12345 01/05/26 1,12,722.70 42,277.30
"#;
    let a = HdfcSavingsAdapter::new();
    let p = make_pdf("HDFC savings.pdf", body);
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
