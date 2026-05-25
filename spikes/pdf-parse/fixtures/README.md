# Fixtures

Drop real Indian bank PDF statements here for the spike to evaluate. **Nothing in this folder is committed** (see [.gitignore](../../../.gitignore)).

Recommended set for a meaningful go/no-go:

| File | Why it matters |
|---|---|
| HDFC savings (text PDF) | Cleanest case; baseline for the happy path. |
| SBI savings (text PDF) | Dense layout; often the hardest text-PDF to extract. |
| ICICI CC statement (password-protected) | Tests the password flow. |
| Any scanned/image-only PDF | Tests the OCR path. |
| Any multi-page wrap statement | Tests table continuation across pages. |

Account numbers and personal details can be redacted with a black box — column *positions* are what the parser cares about, not the digits.

After dropping files, run:

```powershell
cargo run --bin parse-text -- --input ./fixtures --output ./fixtures-output
cargo run --bin parse-scanned -- --input ./fixtures --output ./fixtures-output-ocr
```

Anonymized golden fixtures for repeatable tests will live under `fixtures/anonymized/` later (not yet created).
