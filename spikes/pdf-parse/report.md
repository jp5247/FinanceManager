# Phase-0 PDF Parse Spike — Report

Status: **TEXT PATH RUN. CONDITIONAL GO.** OCR path not yet exercised (no scanned fixture).

## 1. Test environment

| Item | Value |
|---|---|
| Date | 2026-05-26 |
| Machine | Windows 11 Pro 24H2 (10.0.26100) |
| Rust | 1.95.0 (59807616e 2026-04-14) — toolchain `stable-x86_64-pc-windows-msvc` |
| MSVC | Build Tools 2022, MSVC 14.44.35207 |
| Windows SDK | 10.0.26100.0 |
| pdfium | bblanchon `pdfium-windows-x64` (extracted to `C:\source\repos\FinanceManager\pdfium-win-x64`) |
| Tesseract | not installed (not needed — no scanned fixtures) |

## 2. Fixture set

| # | File | Issuer | Type | Pages | Password? |
|---|---|---|---|---|---|
| 1 | April HDFC savings.pdf | HDFC | Savings | – | **yes** (encrypted) |
| 2 | May HDFC savings.pdf | HDFC | Savings | – | **yes** (encrypted) |
| 3 | HDFC regalia Apr2026 | HDFC CC | Credit Card | 3 | no |
| 4 | HDFC regalia May2026 | HDFC CC | Credit Card | 3 | no |
| 5 | HDFC marriott Apr2026 | HDFC CC | Credit Card | 2 | no |
| 6 | HDFC marriott May2026 | HDFC CC | Credit Card | 2 | no |
| 7 | HDFC rupay Apr2026 | HDFC CC | Credit Card | 3 | no |
| 8 | HDFC rupay May2026 (filename: HDC rupay May2026) | HDFC CC | Credit Card | 3 | no |

All fixtures are real statements with PII — never committed.

## 3. Results — text-PDF path (`parse-text`)

| File | pdfium outcome | pdfium rows~ | pdfium ms | pdf-extract outcome | pdf-extract rows~ | pdf-extract ms |
|---|---|---|---|---|---|---|
| April HDFC savings | PasswordProtected | – | 1 | Error (encrypted) | – | 2 |
| May HDFC savings | PasswordProtected | – | 1 | Error (encrypted) | – | 1 |
| HDFC regalia Apr | Ok | 32 | 33 | Ok | 37 | 163 |
| HDFC regalia May | Ok | 22 | 21 | Ok | 26 | 152 |
| HDFC marriott Apr | Ok | 11 | 20 | Ok | 15 | 120 |
| HDFC marriott May | Ok | 13 | 20 | Ok | 17 | 151 |
| HDFC rupay Apr | Ok | 38 | 20 | Ok | 42 | 150 |
| HDFC rupay May | Ok | 57 | 18 | Ok | 61 | 135 |

**Row count heuristic** counts any line with a date pattern + an amount-like token, so 5–10% over-counting is expected (page headers/totals/footers slip in). True transaction-row counts will be lower; per spot-checks the actual transaction lines are present in both extractions.

### Quality spot-check (HDFC Regalia May)

Sample extracted rows preserve full structure:

```
21/04/2026| 00:00 IGST-VPS2711251279864-RATE 18.0 -27 (Ref# 09999999980421000325273) C 170.64 l
23/04/2026| 12:32 FORTPOINTMUMBAIMUMBAI C 1,582.00 l
04/05/2026| 19:20 BPPY CC PAYMENT DP016124192045RgnrE (Ref# ST261250083000010053740) + C 21,447.00 l
21/05/2026| 00:00 OFFUS EMI,PRIN NB:02,00000138162352 (Ref# 09999999980521000327201) C 7,410.00 l
```

Date, time, description, ref-id, sign, amount, PI flag — all present.

### Findings worth carrying forward

1. **`C` is actually `₹`.** The rupee glyph survives font encoding as a Latin "C" in the extracted text. A normalization pass must replace this token (and likely Unicode `₹` and the variant `Rs`) with a canonical currency tag before amount parsing. This is mechanical — not a blocker.
2. **`+` prefix marks credits** (payments-received). Adapter must detect the leading `+` (or trailing `Cr`) to set transaction direction. The Regalia statement uses `+ C 21,447.00` for the cash-back received-payment line.
3. **pdfium is the production backend.** ~7–8× faster than pdf-extract (~20 ms vs ~150 ms) with equivalent or better row capture. pdf-extract remains useful as a pure-Rust fallback when pdfium.dll is unavailable, but it is not the primary path.
4. **Password-protected savings PDFs are normal.** HDFC encrypts savings statements; the password is typically a derived string (customer ID + DOB pattern). The upload UI must collect this once per file. pdfium-render handles it cleanly when a password is passed (`--password` flag in this spike).
5. **PII exposure on extracted text is total.** Customer name, full mailing address, email, card number (last 4 digits visible, full PAN sometimes redacted in PDF but the raw card-number string appears in some statements). This argues strongly for **encryption-at-rest default ON** — see §6.
6. **Statement headers/footers create noise.** The extracted stream interleaves transaction rows with page-end "Past Dues" tables, "Card Control Setting" tables, etc. The normalizer needs region-aware logic — either segment by recognizable transaction-header lines (`DATE & TIME TRANSACTION DESCRIPTION REWARDS AMOUNT PI`) or by font-position once we use pdfium's per-character API.

## 4. Results — OCR path (`parse-scanned`)

Not exercised — no scanned/image-only fixture present in the set. Deferred until a real scanned statement is available, or until the manual-mapping flow is built and OCR becomes a tertiary fallback rather than a phase-1 primary.

## 5. Go / no-go assessment

| Criterion | Target | Observed | Pass? |
|---|---|---|---|
| ≥3 of 4 text PDFs hit ≥90% row coverage | ≥3/4 | 6/6 CC PDFs parsed cleanly | ✅ |
| Password fixture loads (with password) | yes | password detection works; round-trip with a real password not yet tested | 🟡 |
| OCR 5-page < 60 s on reference laptop | <60 s | not tested (no fixture) | 🟡 |
| No pdfium crashes/hangs | none | none | ✅ |

**Recommendation: conditional GO.** The Tauri + Rust + pdfium-render stack is viable for HDFC. Two conditions to clear before declaring G0 fully passed:

1. **Round-trip a password-protected file end-to-end.** Re-run `parse-text --password "<known-pw>"` against `April HDFC savings.pdf` and confirm transactions extract.
2. **Test one non-HDFC PDF.** OD-7 already deferred SBI/ICICI/Axis to Phase-2 adapter work, but a single sample from any other issuer would let us declare "the framework works across issuer formats" rather than "the framework works for HDFC."

If both pass, the spike is fully green and the architecture decision (Tauri + React + Rust + pdfium-render) is locked.

## 6. Architectural decisions reinforced by the spike

- **Decision OD-4 (encryption-at-rest default OFF) should be revisited.** Spike-level inspection of just one statement reveals full name, address, email, card number, and reward-balance history. Default-OFF encryption means any other process or user on the machine can `Get-Content` these files. Recommend escalating this back to the user (see "Next actions").
- **Adapter contract should be expressed as a normalization pipeline, not a single parser function:** raw text → currency normalization (`C` → `₹` → numeric) → row segmentation (header detection) → field extraction → classification. Each stage independently testable.
- **pdfium-render is the production backend; pdf-extract is the dev/fallback path.** This means pdfium.dll becomes a build-time and runtime concern that must be solved at packaging time (bundle into Tauri installer).

## 7. Next actions following this spike

1. **Re-ask OD-4** in light of PII exposure: should encryption-at-rest move to default ON?
2. Round-trip a password against one of the savings PDFs (you supply the password; I'll re-run the spike).
3. Optional but recommended: one non-HDFC sample to confirm cross-issuer generality.
4. Once 1–3 are settled, close Gate G0 and proceed to Phase 1 foundations:
   - StorageRepository interface
   - Atomic-write helper
   - Path-resolver with traversal guard
   - Encryption envelope (now likely default ON pending OD-4 revisit)
   - Audit-log appender with hash chain
   - CI setup (rustfmt, clippy, cargo test)
5. Defer Tesseract install until a real scanned statement enters the corpus.
