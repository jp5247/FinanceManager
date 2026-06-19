# FinanceManager

Local-first personal finance application for Indian retail banking statements. See [FinanceManager.md](FinanceManager.md) for the product brief.

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

## Reference

- Five-lens blueprint: produced in conversation (Goal, system context, findings, options, phased plan, controls, tests, governance, risks, gates, open decisions).
