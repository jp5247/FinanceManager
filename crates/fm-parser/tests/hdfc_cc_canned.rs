//! HDFC CC adapter tests against canned extracted-text fixtures lifted from
//! the Phase-0 spike's real-statement output (with PII redacted by way of
//! synthesized merchant names where applicable).

use fm_core::Amount;
use fm_parser::{
    default_adapters, detect_adapter, BankAdapter, ExtractedPdf, HdfcCreditCardAdapter, PageText,
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

const HDFC_REGALIA_BODY: &str = r#"DUPLICATE Regalia Gold Credit Card Statement
HSN Code: 997113 HDFC Bank Credit Cards GSTIN: 33AAACH2702H2Z6
Domestic Transactions
DATE & TIME TRANSACTION DESCRIPTION REWARDS AMOUNT PI
21/04/2026| 00:00 IGST-VPS2711251279864-RATE 18.0 -27 (Ref#
09999999980421000325273) C 170.64 l
23/04/2026| 12:32 FORTPOINTMUMBAIMUMBAI C 1,582.00 l
04/05/2026| 19:20 BPPY CC PAYMENT DP016124192045RgnrE (Ref#
ST261250083000010053740) + C 21,447.00 l
21/05/2026| 00:00 OFFUS EMI,PRIN NB:02,00000138162352 (Ref#
09999999980521000327201) C 7,410.00 l
Page 1 of 3
"#;

#[test]
fn adapter_id_and_version() {
    let a = HdfcCreditCardAdapter::new();
    assert_eq!(a.id(), "hdfc-cc");
    assert_eq!(a.version(), "1.0.0");
}

#[test]
fn detect_matches_filename_hint() {
    let a = HdfcCreditCardAdapter::new();
    let p = make_pdf("HDFC regalia May2026_Billedstatements_2025.pdf", "");
    assert!(a.detect(&p));
}

#[test]
fn detect_matches_page_text_when_filename_uninformative() {
    let a = HdfcCreditCardAdapter::new();
    let p = make_pdf("statement.pdf", HDFC_REGALIA_BODY);
    assert!(a.detect(&p));
}

#[test]
fn detect_rejects_unrelated_pdf() {
    let a = HdfcCreditCardAdapter::new();
    let p = make_pdf(
        "sbi-savings.pdf",
        "State Bank of India\n01/05/26 some-txn 100.00",
    );
    assert!(!a.detect(&p));
}

#[test]
fn parses_four_rows_with_correct_sign_and_amount() {
    let a = HdfcCreditCardAdapter::new();
    let p = make_pdf("HDFC regalia May2026.pdf", HDFC_REGALIA_BODY);
    let rows = a.parse(&p, "imp-001").unwrap();
    assert_eq!(rows.len(), 4, "got {} rows: {rows:#?}", rows.len());

    // Row 1 — IGST debit, with wrap
    let r = &rows[0];
    assert_eq!(r.txn_date, "2026-04-21");
    assert!(r
        .description
        .starts_with("IGST-VPS2711251279864-RATE 18.0 -27"));
    assert!(r.description.contains("(Ref#"));
    assert!(r.description.contains("09999999980421000325273)"));
    assert_eq!(r.debit, Some(Amount::parse_inr("170.64").unwrap().amount));
    assert!(r.credit.is_none());

    // Row 2 — single-line debit
    let r = &rows[1];
    assert_eq!(r.txn_date, "2026-04-23");
    assert_eq!(r.description, "FORTPOINTMUMBAIMUMBAI");
    assert_eq!(r.debit, Some(Amount::parse_inr("1,582.00").unwrap().amount));

    // Row 3 — `+ C` credit with wrap
    let r = &rows[2];
    assert_eq!(r.txn_date, "2026-05-04");
    assert_eq!(
        r.credit,
        Some(Amount::parse_inr("21,447.00").unwrap().amount)
    );
    assert!(r.debit.is_none(), "row 3 should be credit-only");

    // Row 4 — EMI debit
    let r = &rows[3];
    assert_eq!(r.txn_date, "2026-05-21");
    assert_eq!(r.debit, Some(Amount::parse_inr("7,410.00").unwrap().amount));
}

#[test]
fn provenance_fields_populated() {
    let a = HdfcCreditCardAdapter::new();
    let p = make_pdf("HDFC regalia.pdf", HDFC_REGALIA_BODY);
    let rows = a.parse(&p, "imp-xyz").unwrap();
    assert!(!rows.is_empty());
    for (idx, r) in rows.iter().enumerate() {
        assert_eq!(r.import_id, "imp-xyz");
        assert_eq!(r.source_file, "HDFC regalia.pdf");
        assert_eq!(r.source_sha256, "deadbeef");
        assert_eq!(r.source_page, 1);
        assert_eq!(r.parser_version, "hdfc-cc@1.0.0");
        assert_eq!(r.parser_backend, ParserBackend::Pdfium);
        assert_eq!(r.row_number, (idx as u32) + 1);
        assert!(r.balance.is_none(), "CC statements have no running balance");
    }
}

#[test]
fn page_footer_terminates_row_wrap() {
    let body = r#"21/04/2026| 12:00 DESCRIPTION CONTINUES C 100.00 l
Page 1 of 3
Some footer content that shouldn't be absorbed.
"#;
    let a = HdfcCreditCardAdapter::new();
    let p = make_pdf("HDFC regalia.pdf", body);
    let rows = a.parse(&p, "imp-001").unwrap();
    assert_eq!(rows.len(), 1);
    assert!(!rows[0].description.contains("footer"));
}

#[test]
fn indian_lakhs_amount_is_handled() {
    let body = "01/04/2026| 09:00 BIG PURCHASE C 1,12,722.70 l\n";
    let a = HdfcCreditCardAdapter::new();
    let p = make_pdf("HDFC regalia.pdf", body);
    let rows = a.parse(&p, "imp-001").unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].debit,
        Some(Amount::parse_inr("1,12,722.70").unwrap().amount)
    );
}

#[test]
fn registry_detects_hdfc_cc() {
    let adapters = default_adapters();
    let p = make_pdf("HDFC regalia.pdf", HDFC_REGALIA_BODY);
    let chosen = detect_adapter(&adapters, &p).expect("an adapter should claim this");
    assert_eq!(chosen.id(), "hdfc-cc");
}

#[test]
fn registry_returns_none_for_unknown_format() {
    let adapters = default_adapters();
    let p = make_pdf("mystery.pdf", "no recognizable bank text");
    assert!(detect_adapter(&adapters, &p).is_none());
}
