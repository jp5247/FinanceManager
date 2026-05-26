# FinanceManager

Local-first personal finance application for Indian retail banking statements. See [FinanceManager.md](FinanceManager.md) for the product brief.

## Status

Phase 1 foundations in progress. Gate G0 closed (cross-issuer PDF parse validated). Workspace, CI, and Tauri shell scaffolded; product code lands in subsequent commits.

## Running the app (Windows)

Open a fresh PowerShell window, then:

```powershell
.\scripts\dev.ps1
```

That script sets up the dev shell (adds Rust to PATH, sources MSVC env from `vcvarsall.bat`) and starts `npm run tauri dev`. Use `.\scripts\build.ps1` for a release build.

To set up the env once for an interactive shell (so plain `npm run tauri dev` / `cargo test` / etc. work for the rest of the session), dot-source the env script instead:

```powershell
. .\scripts\dev-env.ps1
```

**Why a wrapper script?** Our Visual Studio BuildTools install has a working compiler/linker on disk but a broken COM registration, so `vswhere.exe` returns empty and cargo's auto-detection can't find MSVC. The wrapper sources `vcvarsall.bat` to make the linker discoverable. Repairing the VS install (via the Visual Studio Installer → Modify → Repair) is the long-term fix; the wrapper is fine for now.

## Locked decisions (Phase 0)

| # | Decision | Choice |
|---|---|---|
| OD-1 | Tech stack | **Tauri + React + Rust core** |
| OD-2 | Storage | **JSON + CSV in Phase 1**, migration path to SQLite (single file per user) in Phase 2 behind a `StorageRepository` seam |
| OD-3 | Multi-user auth | **Passphrase + OS-keystore convenience unlock** (Argon2id KDF; DPAPI / Keychain / Secret Service for one-tap unlock) |
| OD-4 | Encryption at rest | **Default ON** (revised after Phase-0 spike — extracted statement text exposed full PII: name, address, email, card number). Argon2id KDF + OS-keystore convenience unlock. |
| OD-5 | Allowed outbound calls | Merchant canonicalization lookup (merchant string only), signed loan/tax rule-pack download, Tauri auto-update check. No others. |
| OD-6 | OCR | **In scope for Phase 1**, opt-in per upload. Confidence floor + always-flag policy mandatory. |
| OD-7 | Bank coverage | Hand-tuned adapters for **HDFC, SBI, ICICI, Axis** + generic heuristic parser + manual-mapping fallback for everything else. |
| OD-15 | Mobile in Phase 1 | **Responsive reflow only** (single Tauri window). No native mobile build. |

Open decisions deferred to product input during their respective phases: OD-8 (reimbursement timing), OD-9 (transfer neutrality), OD-10 (cash treatment), OD-11 (health-score weights), OD-12 (loan classification criterion), OD-13 (export workbook shape), OD-14 (PDF report), OD-16 (code-signing cert ownership).

## Repository layout

```text
FinanceManager/
├── FinanceManager.md              Product brief
├── docs/design/                   Architecture & UI specs
├── crates/                        Cargo workspace — domain crates
│   ├── fm-core                    Domain types (no I/O)
│   ├── fm-storage                 StorageRepository seam, atomic write, path guard
│   ├── fm-crypto                  AES-256-GCM envelope, Argon2id KDF, OS keystore
│   ├── fm-audit                   Append-only hash-chained log
│   └── fm-parser                  BankAdapter trait + normalization pipeline
├── src-tauri/                     Tauri 2 desktop shell (crate name: fm-app)
├── src/                           React 19 + TS 5 frontend
├── spikes/pdf-parse/              Phase-0 PDF stack spike (Rust)
├── scripts/                       PowerShell dev helpers
└── .github/workflows/             CI: fmt + clippy + test + tsc + PII guard
```

## Reference

- Five-lens blueprint: produced in conversation (goal, system context, findings, options, phased plan, controls, tests, governance, risks, gates, open decisions).
- Risk register and gates: blueprint §9 / §10.
