# Phase-0 PDF Parse Spike — Report

Status: **not yet run** (awaiting fixture PDFs from product owner).

## 1. Test environment

| Item | Value |
|---|---|
| Date | _TBD_ |
| Machine | _CPU / RAM / OS_ |
| Rust | `rustc --version` |
| pdfium version | _from bblanchon binaries_ |
| Tesseract version | `tesseract --version` |

## 2. Fixture set

| # | File | Issuer | Type | Pages | Password? | Notes |
|---|---|---|---|---|---|---|
| 1 |  |  | savings / CC |  | yes / no |  |
| 2 |  |  |  |  |  |  |
| 3 |  |  |  |  |  |  |
| 4 |  |  |  |  |  |  |
| 5 |  |  |  |  |  |  |

## 3. Results — text-PDF path (`parse-text`)

| File | pdfium outcome | pdfium rows~ | pdfium ms | pdf-extract outcome | pdf-extract rows~ | pdf-extract ms | Accuracy vs ground truth |
|---|---|---|---|---|---|---|---|
|  |  |  |  |  |  |  |  |

Observed structural issues:

- _e.g. "SBI: column boundaries lost; date and amount glued together"_
- _e.g. "HDFC CC: works clean with pdfium, pdf-extract drops the last column"_

## 4. Results — OCR path (`parse-scanned`)

| File | Pages | DPI | Total ms | Rows~ | Coverage vs ground truth | Notes |
|---|---|---|---|---|---|---|
|  |  | 300 |  |  |  |  |

## 5. Go / no-go assessment

| Criterion | Target | Observed | Pass? |
|---|---|---|---|
| ≥3 of 4 text PDFs hit ≥90% row coverage | ≥3/4 |  |  |
| Password fixture loads | yes |  |  |
| OCR 5-page < 60 s on reference laptop | <60 s |  |  |
| No pdfium crashes/hangs | none |  |  |

**Recommendation:** _go / no-go / conditional-go (with mitigations listed below)_.

## 6. Mitigations and fallbacks (if conditional)

- _e.g. "SBI requires custom column-position heuristic before normalization"_
- _e.g. "Switch from pdfium to pdfium-render's vendored libpdfium for scanned PDFs"_
- _e.g. "Adopt `pdftotext` (Poppler) sandboxed subprocess for SBI only"_

## 7. Next actions following this spike

- [ ] Lock storage decision (already JSON+CSV → SQLite at P2, per OD-2).
- [ ] Open Phase 1 foundations (G1 prep): repo layout, StorageRepository interface, atomic-write helper, path-resolver, encryption envelope (off by default per OD-4 but built).
- [ ] Set up CI (lint + test + clippy + fmt) before any Phase 1 code lands.
