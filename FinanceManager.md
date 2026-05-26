# FinanceManager App Requirement Brief

## 1. Product goal
Build a local-first personal finance application that helps users identify:
- Income and expense patterns
- Money leakage (avoidable recurring spending)
- Actionable wealth-building improvements
- Loan payoff optimization opportunities

The app should behave like a practical finance manager and present recommendations in clear, non-technical language.

## 2. Core implementation constraints (mandatory)
1. User financial data must never be sent to external AI services or third-party analytics.
2. All user input, parsed statements, and analysis outputs must remain local on the user machine.
3. No database engine should be used (no MySQL, PostgreSQL, SQLite, MongoDB, etc.).
4. If external web lookup is required for generic merchant intelligence or policy/rule updates, only non-sensitive metadata may be used (for example merchant name only, never transaction amount, account number, or statement file).
5. System must support multiple users with isolated local data storage per user.

## 3. UX and design requirements
1. Dark theme with neon visual language.
2. Custom pointer/cursor theme should blend with UI but remain clearly visible and accessible.
3. Desktop-first and mobile-friendly responsive layout.
4. Top status strip on Dashboard to display overall financial health.

## 4. Functional modules

### 4.1 Dashboard tab
Display current-period insights using Upload, Investment, and Loan data.

Required sections:
1. Financial overview:
	- Total income
	- Total expenses
	- Net savings
	- Investment snapshot
	- Bleeding money summary
2. Fix-my-finance section:
	- Top expense cut recommendations
	- Expected monthly savings if applied
	- Wealth-building suggestions (for example emergency fund, debt reduction, SIP increase)
3. Trends and analytics:
	- Income trend over time
	- Expense trend over time
	- Expense by category trend
4. Financial health strip:
	- Composite score (0-100)
	- Score drivers (spending ratio, debt ratio, savings consistency)

### 4.2 Upload tab
Upload and process PDF statements for Indian banks and credit cards.

Required behavior:
1. Accept multiple PDF files.
2. Parse statement rows into normalized transactions.
3. Classify each transaction as income, expense, transfer, refund, investment, or loan-related.
4. Categorize merchant/transaction purpose.
5. For unknown merchants/categories:
	- Attempt lookup using non-sensitive merchant text only.
	- Save inferred canonical merchant name for future reuse.
6. Flag uncertain rows for manual review.
7. Provide in-place correction UI for flagged records.
8. Handle split/peer reimbursements:
	- Allow user to mark partial or full reimbursement.
	- Adjust effective spending used in analysis.
9. Block downstream analysis finalization until flags are resolved or explicitly deferred.

### 4.3 Past Analysis tab
Show historical analysis for months prior to the active dashboard range.

Example rule:
- If current month is June, dashboard shows May and June.
- Past Analysis shows April and older periods.

Features:
1. Month selector and comparison view.
2. Historical trends and category shifts.
3. Archived recommendations and outcome tracking.

### 4.4 Investment Inputs tab
Allow manual entry and updates of investment assets.

Fields per asset:
1. Asset type
2. Asset name
3. Invested amount
4. Current value
5. Date of entry/update
6. Optional notes

Analysis usage:
1. Include investment growth and allocation in dashboard health metrics.
2. Reflect unrealized gain/loss summaries.

### 4.5 Loan Tracker tab
Capture complete loan details and provide payoff strategy guidance.

Input fields (minimum):
1. Loan type
2. Lender
3. Principal outstanding
4. Interest rate (fixed/floating)
5. Remaining tenure
6. EMI
7. Prepayment penalty rules
8. Tax benefit eligibility
9. Start date and next due date

Analysis output:
1. Classify loans as good loan or bad loan with rationale.
2. Show prioritized closure strategy (for example avalanche/snowball variants).
3. Estimate interest savings for early closure scenarios.
4. Use latest rule references (fetched from web) without sending private user data.

### 4.6 Export capability
Enable exporting all cleaned and validated inputs to Excel only after upload flags are cleared (or user-approved overrides are completed).

Export scope:
1. Transactions
2. Category mappings
3. Adjustments/reimbursements
4. Investments
5. Loans
6. Summary metrics

### 4.7 Multi-user support
1. Local user profiles with separate file storage.
2. Authentication mode to be finalized (PIN/password/local OS user binding).
3. Strict isolation so one user cannot read another user's data.

## 5. Non-functional requirements
1. Local performance should support large monthly statement volumes with smooth UI interaction.
2. Reliable parser with deterministic re-run behavior.
3. Full local audit trail for manual edits and category overrides.
4. Clear error messaging for parse failures and unsupported statement formats.
5. Extensible bank parser architecture for future bank format additions.

## 6. Suggested local storage approach (no DB)
1. File-based storage using structured JSON/CSV per user.
2. Versioned folders by month and module.
3. Separate config for merchant canonicalization and custom category rules.
4. Local encrypted-at-rest option for sensitive files.

## 7. Clarification questions before design and implementation

### 7.1 Critical decisions
1. Preferred tech stack for app UI and backend:
	- Electron + React
	- Tauri + React
	- .NET desktop
	- Other
2. Must the app run offline-only always, or can optional internet lookup be user-enabled?
3. For external lookups, should we use:
	- Direct scraping only
	- Curated APIs
	- Hybrid with strict privacy filter
4. Should unsupported bank statement formats be:
	- Rejected with template guidance
	- Semi-manual mapping flow
	- OCR fallback flow
5. Multi-user login method preference:
	- Local username/password
	- PIN only
	- OS-account based profile switching

### 7.2 Data and analysis rules
1. What is your definitive transaction category taxonomy?
2. Should reimbursements reduce spending immediately or after settlement confirmation?
3. Do transfers between own accounts count as neutral (excluded from income/expense)?
4. How should cash withdrawals be treated by default?
5. How is financial health score weighted:
	- Savings rate weight
	- Debt burden weight
	- Essential vs discretionary spend weight
	- Investment consistency weight
6. Loan classification criteria preference:
	- By interest rate threshold
	- By tax benefit and asset productivity
	- By net effective borrowing cost

### 7.3 Compliance, security, and operations
1. Should local files be encrypted by default?
2. Do you want optional local backup/export package for disaster recovery?
3. Do you need an immutable local audit log for all user edits?
4. Should internet access be blocked unless user explicitly enables it per lookup?

### 7.4 Reporting and exports
1. Required Excel format:
	- Single workbook with multiple sheets
	- Separate workbook per module
2. Should exports include formulas or values-only snapshots?
3. Should we add pre-built monthly PDF report generation in phase 1?

## 8. Definition of done for phase 1
1. User can upload statement PDFs and resolve all flagged rows.
2. Dashboard shows accurate income, expenses, leakage points, and recommendations.
3. Past Analysis works for older months.
4. Investment and Loan modules feed unified analysis.
5. Export works only after data quality flags are resolved.
6. Multi-user local profile separation is functional.
7. No sensitive data leaves machine.

## 9. Implementation artifacts
(Previous Phase-0 design notes — local data schema and UI wireframes — have been retired now that Section 10 captures the shipped behavior directly.)

## 10. Implementation status (Phase 1, in flight)

Captures behavior actually shipped on `master`. Update this section as part of every commit that adds or changes user-visible behavior.

### 10.1 Tech stack chosen
- Tauri 2 + React 19 + Vite 6 + TypeScript 5 (decision against 7.1.1).
- Cargo workspace under `crates/` for parser, storage, crypto, audit, profile, pdf, categorize; `src-tauri/` hosts the app shell + Tauri commands.

### 10.2 Multi-user + at-rest encryption (delivers 2.5, 4.7, 6.4, 7.3.1)
- Per-profile data root with passphrase-derived KEK (Argon2id) wrapping a per-profile DEK.
- Recovery-phrase wrap of the same DEK for the forgot-passphrase path.
- All JSON files (file metadata, raw transactions, rules, merchant cache, LLM config) sealed with AES-256-GCM via `fm-crypto` before hitting disk.

### 10.3 PDF ingest + adapter framework (delivers 4.2.1–4.2.4, 5.5)
- `fm-pdf` wraps pdfium-render for text extraction; password-protected statements supported via a per-upload prompt.
- `BankAdapter` trait with currently three adapters: HDFC credit card, HDFC savings (balance-delta direction inference + two-phase wrap absorption), SBI savings (positional columns). Adapter is auto-selected by filename hint + page-text detection.
- Per-import audit-ready provenance fields on every row (source file, sha256, page, parser version + backend).
- Duplicate-upload guard: re-importing the same PDF by content hash is refused.
- **HDFC CC wrap-line termination**: absorption stops once the row has its currency-marked amount terminator (`(+)? (C|₹) <amount> [l|I]?$`), preventing inline "Rewards Program Points Summary" / "Cash Back Summary" tables from being glued onto the preceding transaction's description.

### 10.4 Categorization pipeline (delivers 2.4, 4.2.4, 4.2.5)
Order is fixed and matches OD-5:
1. **User-saved rules** (priority 1000) — created via the recategorize modal's "Save as a rule" toggle, persisted encrypted at `mappings/category-rules.json`.
2. **Curated merchant table** (priority 500) — contains-only entries shipped with the app (no regex keyword fishing after the IRFC-dividend / "Indian Railway" false-positive incident).
3. **External LLM lookup (Gemini, opt-in)** — for rows still uncategorized, the extracted merchant name + direction (debit/credit) are sent to Google's Generative Language API. **Only those two fields per row leave the device** — never amounts, dates, account masks, ref-IDs, or balances.
4. Anything still unmatched stays `Uncategorized` and is surfaced for manual recategorization.

Supporting pieces:
- **Per-profile merchant cache** (`mappings/merchant-cache.json`, encrypted): every LLM result — including `Uncategorized` outcomes — is cached so repeat uploads don't re-pay. Cache key is lowercased merchant string.
- **Privacy-preserving merchant extractor** in `fm-categorize::extract_merchant`: strips UPI/ACH/NACH/NEFT/CC EMI/BillPay prefixes and ref-IDs before anything is considered for an external call.
- **Gemini client** (`src-tauri/src/llm.rs`): batched single-request-per-upload, structured JSON response schema constrained to the app's allowed category list (off-list values are dropped to `Uncategorized` so the picker stays consistent). API key travels via the `x-goog-api-key` header (never the URL query string).
- **OD-5 egress guard**: last-line defense in `categorize_via_gemini` rejects items whose merchant string still carries long digit runs (≥ 6), `XXXX` masks, `@` UPI handles, or amount-like tokens. Dropped items come back as `Uncategorized` without a network call.
- **Direction-keyed merchant cache**: `cache_key(merchant, direction)` composes a `d|`/`c|` prefix so the same merchant in opposite directions cannot poison each other (regression prevention for the IRFC dividend / Indian Railway false-positive class — see `merchant_cache::tests::key_separates_directions_for_same_merchant`).
- **429 handling**: one retry with an 8s backoff for per-minute quota; per-day quota is detected via the `PerDay` quotaId hint and surfaces a friendly "switch model or try tomorrow" error without burning a retry.
- **Model selector in UI**: free-tier options (`gemini-2.0-flash`, `-flash-lite`, `gemini-2.5-flash`, `-flash-lite`) selectable from the LLM settings card. Switching the model auto-triggers re-categorization on the currently open import.
- **Auto-recategorize triggers**: a new `recategorize_import` Tauri command re-runs the full pipeline on an existing import while preserving manual edits (rows whose `category_rule_id == "manual"` are untouched). It fires automatically when the user changes the LLM model, saves a new rule via the modal, or deletes a user rule. Cached `Uncategorized` entries are invalidated before the re-run so a model swap gets a fresh Gemini attempt.

### 10.5 Upload result surface (delivers 4.2.6, 4.2.7)
- Per-statement summary tiles (Debits / Credits / Net flow).
- "Where the money went" category breakdown bar chart per import.
- Recategorize modal with optional "Save as a rule" → user rules panel below.
- Inline note when N rows were categorized via Gemini in this run; magenta warning when external lookup failed (the upload itself always succeeds).
- Previous imports list with click-to-view + delete.

### 10.6 Not yet implemented (Phase 1 backlog)
- Dashboard tab (4.1) — financial overview, fix-my-finance, trends, health strip.
- Past Analysis tab (4.3).
- Investment Inputs tab (4.4).
- Loan Tracker tab (4.5).
- Excel export (4.6) — currently the data is queryable only through the Upload tab.
- Split / reimbursement handling (4.2.8).
- Hash-chained audit log surface in UI (audit crate exists; no viewer yet).

## 11. Pre-commit workflow (mandatory)
For every commit that changes user-visible behavior:
1. Update Section 10 of this brief to reflect the change.
2. Run the `/five-lens` skill on this brief to get a fresh planner pass over the five lenses (scalability, security, testing, architecture, cost). Address or capture any actionable finding before committing.
