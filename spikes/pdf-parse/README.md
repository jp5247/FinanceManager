# Phase-0 Spike: PDF Parse Stack

Purpose: validate that the Rust PDF stack can extract tabular transaction rows from real Indian bank/CC statements with acceptable accuracy and speed. This is a **go/no-go check** for the Tauri + Rust architecture decision (R-2 in [docs/design/local-data-schema.md](../../docs/design/local-data-schema.md) sibling brief, blueprint risk register).

## What this crate evaluates

| Binary | Backend | What it answers |
|---|---|---|
| `parse-text` | `pdfium-render` + `pdf-extract` | Can we extract usable rows from text-based bank PDFs? Which library is more accurate? |
| `parse-scanned` | `pdfium-render` (raster) → Tesseract CLI | Can OCR handle scanned/image-only statements at reasonable speed? |

## Prerequisites

1. **Rust toolchain** (`rustup` → stable).
2. **pdfium dynamic library.** `pdfium-render` does not bundle it. Download a prebuilt binary from <https://github.com/bblanchon/pdfium-binaries/releases>:
   - Windows: `pdfium-windows-x64.tgz` → place `pdfium.dll` next to the spike executable (or pass `--pdfium-dir <path>`).
3. **Tesseract CLI** (for `parse-scanned` only). Install from <https://github.com/UB-Mannheim/tesseract/wiki> and ensure `tesseract.exe` is on PATH (or pass `--tesseract <path>`).

## Run

```powershell
cd spikes/pdf-parse
cargo build --release

# Text PDFs
./target/release/parse-text.exe --input ./fixtures --output ./out

# Scanned PDFs (Tesseract required)
./target/release/parse-scanned.exe --input ./fixtures --output ./out-ocr --dpi 300 --lang eng
```

Each run writes:
- One `.txt` per input file per backend (extracted text).
- `summary.json` with per-file outcomes, page count, row estimate, and elapsed ms.

## Success criteria (go/no-go)

The spike **passes** if, across the fixture set:
- At least 3 of 4 text-PDF bank statements yield ≥ 90% of visible transaction rows on the page using **either** pdfium or pdf-extract.
- The password-protected fixture loads with the correct password.
- OCR completes a 5-page scanned statement in under 60 s on a 2020-class laptop and produces ≥ 80% row coverage with confidence visible per-line (Tesseract `--psm 6` + reading the `.tsv` output — TODO if needed).

The spike **fails** (forcing a stack reconsideration) if any of the following hold:
- Multiple banks consistently lose table structure (columns merged into one stream with no recoverable order).
- pdfium crashes or hangs on any input.
- OCR latency is >5 min/page making the feature unusable.

## Report

Findings, numbers, and the go/no-go recommendation get written to [report.md](./report.md). That document is what feeds the Phase 0 decision gate (G0) in the blueprint.
