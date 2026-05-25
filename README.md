# FinanceManager

Local-first personal finance application for Indian retail banking statements. See [FinanceManager.md](FinanceManager.md) for the product brief.

## Status

Phase 0 (decisions and spikes). No application code yet.

## Locked decisions (Phase 0)

| # | Decision | Choice |
|---|---|---|
| OD-1 | Tech stack | **Tauri + React + Rust core** |
| OD-2 | Storage | **JSON + CSV in Phase 1**, migration path to SQLite (single file per user) in Phase 2 behind a `StorageRepository` seam |
| OD-3 | Multi-user auth | **Passphrase + OS-keystore convenience unlock** (Argon2id KDF; DPAPI / Keychain / Secret Service for one-tap unlock) |
| OD-4 | Encryption at rest | **Default OFF**, user-enabled in settings. Envelope code is still built in Phase 1; toggle wires it in. |
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
│   ├── local-data-schema.md
│   └── ui-wireframe-spec.md
├── spikes/
│   └── pdf-parse/                 Phase-0 PDF stack spike (Rust)
└── .claude/                       Agent + skill definitions
```

App scaffolding (Tauri shell, React UI, Rust core crates) is intentionally not yet created — it starts after the Phase 0 spike passes Gate G0.

## Phase 0 — what is in flight

1. **PDF parsing spike** — see [spikes/pdf-parse/README.md](spikes/pdf-parse/README.md). Awaiting fixture PDFs.
2. **OCR latency spike** — same crate, `parse-scanned` binary.
3. **CI bootstrap** — not yet started.

## Reference

- Five-lens blueprint: produced in conversation (Goal, system context, findings, options, phased plan, controls, tests, governance, risks, gates, open decisions).
- Risk register and gates: blueprint §9 / §10.
