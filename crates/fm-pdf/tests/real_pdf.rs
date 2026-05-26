//! End-to-end extraction test against a real PDF.
//!
//! Skipped by default. To run locally:
//!
//! ```powershell
//! $env:FM_TEST_PDF = "C:\source\repos\FinanceManager\Jai statements\HDFC regalia May2026_Billedstatements_2025_25-05-26_23-24.pdf"
//! cargo test -p fm-pdf -- --ignored
//! ```
//!
//! Requires `pdfium.dll` to be on PATH or next to the test binary. The Phase-0
//! spike's `pdfium-win-x64\bin\pdfium.dll` works fine — point at it via the
//! `PATH` environment variable if necessary.

use fm_parser::{default_adapters, detect_adapter, ParserBackend};
use fm_pdf::PdfExtractor;
use std::path::PathBuf;

#[test]
#[ignore = "requires pdfium.dll + FM_TEST_PDF env var; run with `cargo test -p fm-pdf -- --ignored`"]
fn extracts_real_pdf() {
    let path = std::env::var("FM_TEST_PDF").expect(
        "set FM_TEST_PDF to a real PDF path (e.g. one of the HDFC fixtures under Jai statements/)",
    );
    let path = PathBuf::from(path);
    assert!(path.exists(), "FM_TEST_PDF does not exist: {path:?}");

    let extractor = PdfExtractor::new().expect("pdfium should be loadable");
    let password = std::env::var("FM_TEST_PDF_PASSWORD").ok();
    let extracted = extractor
        .extract(&path, password.as_deref())
        .expect("extract should succeed for a known-good PDF");

    assert!(!extracted.pages.is_empty(), "expected at least one page");
    assert_eq!(extracted.backend, ParserBackend::Pdfium);
    assert_eq!(extracted.source_sha256.len(), 64, "sha256 hex");
    assert!(extracted.source_file.ends_with(".pdf"));

    // The fixtures we have are real bank statements — some text should be
    // present on at least one page.
    let total_chars: usize = extracted.pages.iter().map(|p| p.text.len()).sum();
    assert!(
        total_chars > 100,
        "expected at least 100 chars of extracted text, got {total_chars}"
    );
}

/// Real PDF → fm-pdf extract → fm-parser adapter → RawTransactions. This is
/// the full chain that the Upload command will run at runtime; if it works
/// here, end-to-end is proven before any UI is involved.
#[test]
#[ignore = "requires pdfium.dll + FM_TEST_PDF env var; run with `cargo test -p fm-pdf -- --ignored`"]
fn extract_then_parse_pipeline() {
    let path = std::env::var("FM_TEST_PDF").expect("set FM_TEST_PDF to a real PDF path");
    let path = PathBuf::from(path);
    let extractor = PdfExtractor::new().unwrap();
    let extracted = extractor.extract(&path, None).unwrap();

    let adapters = default_adapters();
    let adapter =
        detect_adapter(&adapters, &extracted).expect("an adapter should claim this fixture");
    let rows = adapter
        .parse(&extracted, "imp-pipeline-test")
        .expect("parse should succeed");

    assert!(
        !rows.is_empty(),
        "expected at least one transaction parsed from {path:?}"
    );

    // Sanity: every row carries the provenance we promised in schema §3.6.
    for r in &rows {
        assert_eq!(r.import_id, "imp-pipeline-test");
        assert_eq!(r.source_sha256.len(), 64);
        assert!(!r.source_file.is_empty());
        assert!(r.source_page >= 1);
        assert_eq!(r.parser_backend, ParserBackend::Pdfium);
        assert!(!r.parser_version.is_empty());
        // Each row has exactly one of debit / credit set.
        assert!(
            r.debit.is_some() ^ r.credit.is_some(),
            "row should have exactly one of debit/credit: {r:?}"
        );
    }

    eprintln!(
        "Parsed {} transactions from {} via adapter {}@{}",
        rows.len(),
        path.display(),
        adapter.id(),
        adapter.version()
    );
}
