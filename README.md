# FinanceManager

Local-first personal finance application for Indian retail banking statements. See [FinanceManager.md](FinanceManager.md) for the product brief.

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
