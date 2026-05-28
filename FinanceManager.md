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

### 10.5 Dashboard tab — full (delivers 4.1)
- New `dashboard_aggregate` Tauri command (`src-tauri/src/dashboard.rs`) walks every import for the unlocked profile, decrypts the transaction files, and produces a `DashboardData` containing: import count, transaction count, period bounds, total income, total expense, net savings, transfer count + total, and a category-totals list sorted by debit descending.
- Money classification implements P3, P4, P5 decisions:
  - **Income**: rows categorized `Salary`, `Dividend`, `Interest`, `Refund`.
  - **Transfer** (P3): rows categorized `Credit Card Payment` or `Bank Transfer` — excluded from income/expense totals and surfaced separately.
  - **Expense** (P4): everything else, including `ATM / Cash` (cash withdrawn is treated as spent by default).
  - **Refund offset**: a credit amount on an expense category reduces that category's net expense (e.g. an Amazon return).
- New Dashboard tab in `Home` (now the default landing tab; replaces the placeholder welcome panel). Renders financial-overview tiles (Income, Expense, Net Savings + savings rate), a transfer-excluded note, and a category breakdown panel ("Where the money went" / "Money in").
- Empty-state handling: until the user uploads a statement, the tab shows a "no statements yet" card pointing them to the Upload tab.
- **Monthly trend** (4.1.3): per-`YYYY-MM` bucketing of income vs expense. Each month renders as a card-style row with separate labelled In / Out bars, each bar showing its own ₹ amount inline (so the comparison is readable without parsing the net), and a bigger Net chip on the right. Bars are 12 px thick with subtle gradient + glow for visual presence.
- **Financial-health strip** (4.1.4): composite score 0–100 weighted per P5 (40% savings rate / 25% debt burden / 20% essential vs discretionary / 15% investment consistency). Each driver shows score + weight + one-line explainer + colored bar. Tone color (green / amber / red) on the strip's left border indicates overall health.
  - Real drivers in v1: savings rate (capped at the 50% sweet spot for full marks), essential-vs-discretionary (ratio of essential-category spend to total expense), and **investment consistency** (fraction of tracked months with at least one investment outflow).
  - Placeholder driver: debt burden defaults to 100 (no loan data yet) until the Loan Tracker tab ships.
- **Category breakdown** (4.1.1, 4.1.3): each section ("Where the money went", "Wealth-building", "Money in") computes a per-group total, then shows each category's bar width and `%` chip as that category's share of its group total (not relative to the largest peer). Amount text is kind-coloured: expense rows in red, investment + income rows in green. Section headers show the group total inline.
- **Month drill-down**: every row in the "Income vs expense by month" trend is now clickable. Opens a modal listing every transaction in that month across all uploaded statements, with a top-of-modal summary panel that shows income sources and expense sources broken down by category (so the user can see exactly which rows produced the IN and OUT figures from the trend bars). Same recategorize affordance as the other drill modals; saving a rule retroactively re-categorizes every statement. Powered by a new `list_transactions_by_month` Tauri command. Component: `MonthDrillModal`.
- **Category drill-down**: every category in the breakdown panel is now clickable. Opening one shows every transaction in that category across all uploaded statements (new `list_transactions_by_category` Tauri command), with source-file column, click-to-recategorize chip, and the same modal as Upload. Saving a rule from inside the drill view triggers a retroactive re-categorize across **every** import (new `recategorize_all_imports` Tauri command) so historical rows in the same category get the new rule applied immediately. Components: `CategoryDrillModal`, shared `RecategorizeModal` extracted from `UploadView` for reuse.
- **Custom-category synonym handling**: `classify_category` and `is_essential` match case-insensitively and recognise common user-coined labels so the user doesn't have to discover the canonical name. Examples:
  - `SIP`, `Mutual Fund`, `MF`, `ELSS`, `PPF`, `NPS`, `Equity`, `Stocks`, `Fixed Deposit`, `FD`, `Recurring Deposit`, `RD`, `Investments` → Investment kind (excluded from expense, feeds investment-consistency driver).
  - `Loans`, `EMI`, `Loan EMI` → essential.
  - `Food`, `Meals` → essential (treated like Groceries).
  - `Salary`, `Dividend`, `Interest`, `Refund`, `Bonus`, `Cashback` → income.
  - `Credit Card Payment`, `CC Payment`, `Bank Transfer` → transfer.
- **Fix-my-finance panel** (4.1.2): heuristic recommendations. Leads with the top discretionary category trim-by-20% (only if the suggestion is worth ≥ ₹500). Adds emergency-fund or savings-stepup nudge keyed off the savings rate. Adds a behavioral nudge when discretionary spend exceeds 60% of expenses. Always closes with the "add your investments for a real score" reminder until that tab lands.
- "Essential" category set: Rent, Electricity, Gas, Water, Mobile, Internet, Groceries, Maintenance, Insurance, Bills, Loan EMI, Tax, Train Travel, Fuel.
- Tested: 13 unit tests cover classification, transfer exclusion, cash-as-expense default, refund offset, period bounds, monthly bucketing, health-score high/low ends, recommendation ordering, recommendation no-op cases, empty aggregate, and sort order.

### 10.6 Upload result surface (delivers 4.2.6, 4.2.7)
- Per-statement summary tiles (Debits / Credits / Net flow).
- "Where the money went" category breakdown bar chart per import.
- Recategorize modal with optional "Save as a rule" → user rules panel below.
- Inline note when N rows were categorized via Gemini in this run; magenta warning when external lookup failed (the upload itself always succeeds).
- Previous imports list with click-to-view + delete.

### 10.7 Not yet implemented (Phase 1 backlog)
- Dashboard tab (4.1) — **shipped end-to-end (10.5)**. Investment-snapshot driver + loan-aware "fix my finance" recommendations remain blocked on the Investments and Loan Tracker tabs.
- Past Analysis tab (4.3).
- Investment Inputs tab (4.4).
- Loan Tracker tab (4.5).
- Excel export (4.6) — currently the data is queryable only through the Upload tab.
- Split / reimbursement handling (4.2.8).
- Hash-chained audit log surface in UI (audit crate exists; no viewer yet).

## 11. Open decisions register

Single home for product, UX, and engineering decisions that have been raised but not resolved. Sources: Section 7 (original brief), `/five-lens` audit outputs, ad-hoc decisions noted during implementation. Sweep this section as part of the pre-commit workflow (Section 12) — when a decision lands, move it to 11.4 with its resolving commit.

### 11.1 Product / analysis-rule decisions (originally Section 7.2)

| ID | Question | Default if undecided |
|---|---|---|
| P1 | Definitive transaction category taxonomy (current LLM list of 31 is a working approximation, not a product decision) | Keep current list; revisit when Dashboard surfaces per-category insights |
| P2 | Reimbursements (split): reduce spending immediately on mark, or only after settlement confirmation? | Immediate, with a "pending settlement" flag |
| P6 | Loan classification criteria: rate threshold vs tax-benefit-and-asset-productivity vs net effective cost | Net effective borrowing cost (most defensible) |

### 11.2 UX / surfacing decisions

| ID | Question | Default if undecided |
|---|---|---|
| U1 | Should re-categorize-import show a per-row "what changed" diff to the user? (Audit F-CRIT-3) | No diff in v1; add only if users miss silent changes |
| U2 | Per-session Gemini request counter visible in LLM settings card? (Audit D5 / F-COST-1) | No counter in v1 |
| U3 | Tauri `emit("llm:rate-limited")` during the 8s retry pause so the UI can show "retrying after rate limit…"? (Audit D6 / F-INF-1) | No emit; keep upload spinner generic |
| U4 | Multi-user login method finalization (PIN-only / passphrase / OS-account binding) — currently passphrase + recovery phrase | Status quo (passphrase + recovery) until a second user surface is needed |

### 11.3 Engineering / hardening decisions

| ID | Question | Default if undecided |
|---|---|---|
| E1 | Invalidate cached `Uncategorized` entries on *every* recategorize trigger, or only on model swap? (Audit D3 / F-COST-2) | Status quo (invalidate on all triggers) |
| E2 | `manual-cleared` sentinel: how should `recategorize_import` treat a row where the user explicitly cleared the category to defer? (Audit D4 / F-CRIT-2) | Add `category_rule_id = "manual-cleared"` and treat as manual |
| E3 | Excel export shape: single workbook with sheets per module, or one workbook per module? (Original 7.4.1) | Single workbook, multi-sheet |
| E4 | Excel export contents: formulas or values-only snapshots? (Original 7.4.2) | Values-only |
| E5 | Pre-built monthly PDF report generation in Phase 1? (Original 7.4.3) | Deferred to Phase 2 |
| E6 | Optional local encrypted backup/export package for disaster recovery? (Original 7.3.2) | Deferred to Phase 2 |
| E7 | Hash-chained audit-log UI surface in Phase 1? (Original 7.3.3 + Section 10.6 gap) | Yes — small viewer in settings |
| E8 | Internet access blocked unless user explicitly enables per-lookup? (Original 7.3.4) | Status quo: profile-level toggle (LLM enabled flag); no per-lookup prompt |
| E9 | Shared Gemini-models constants table between Rust (`llm_config.rs`) and TS (`UploadView.tsx`)? (Audit F-CRIT-5) | Keep separate; revisit if drift ever bites |
| E10 | `Loan EMI` rows: stay bucketed as expense, or split into their own `Debt` cash-flow kind? (Audit F-CRIT-2; investments resolved separately) | Stay as expense in v1; revisit when Loan Tracker (4.5) ships and we can split EMI into principal-vs-interest |
| E11 | `Personal Transfer` / `UPI Transfer` — should they be classified as Transfer (excluded from expense)? (Audit F-CRIT-1) | **Decided: no.** Both labels are too ambiguous (Personal = sending to a person; UPI = often paying a merchant); conservatively keeping as expense avoids silently zeroing real outflows. P3 transfer-exclusion stays scoped to `Credit Card Payment` and `Bank Transfer` |
| E12 | Surface a `skippedImports` count when schema-mismatched / unreadable files are dropped from the dashboard aggregate? (Audit F-INF/critique on silent fallthroughs) | Add when a schema bump actually ships |
| E13 | Trust LLM-set categories for P3 transfer exclusion, or require `category_rule_id` provenance from user/curated rules? (Audit F-SEC-5) | Status quo (trust the category label); revisit if we see Gemini hallucinations stamping real expenses as `Credit Card Payment` |
| E14 | Migrate to a single "kind registry" table colocated with `ALLOWED_CATEGORIES` so Rust + TS + dashboard share one source of truth? (Audit option-B recommendation) | Defer to next Dashboard slice |
| E15 | Move `dashboard_aggregate` decrypt-and-parse off the Tauri main command thread (spawn_blocking or memoized cache) before the working set grows past ~24 imports? (Audit F-INF-1) | Defer — current scale is fine |
| E16 | Add `last_recategorized_at: Option<String>` to `FileMeta` so the UI can show "imported … / last touched …"? (Drill audit F-CRIT-2) | Defer — add when Past Analysis tab needs it |
| E17 | Apply E15's `spawn_blocking` gate to `recategorize_all_imports` too (does N decrypt+parse+seal+write round-trips on the IPC thread). | Defer — same gate as E15 |
| E18 | Cross-import LLM batching: when `recategorize_all_imports` triggers `apply_external_lookup`, dedupe pending merchants across imports into one Gemini batch instead of one-per-import. (Drill audit F-INF-2 / F-COST-1) | Defer — LLM is opt-in; revisit if real users hit per-minute rate limits |
| E19 | Lock the Dashboard `Refresh` button while `retroBusy` is true in the drill modal (defense-in-depth against the torn-read race on Windows). (Drill audit F-SEC-2) | Defer — chip-disable inside the drill modal already covers the in-modal race; cross-pane lock is belt-and-suspenders |
| E20 | Switch `fm-storage` writes to atomic write-then-rename so concurrent readers can never see a truncated sealed file on Windows. | Defer to Phase-2 hardening |
| E21 | Emit an audit-log entry when `recategorize_all_imports` runs (hashed-chain, per E7) so the user can see what triggered a retro-rewrite. | Defer until audit-log viewer ships (E7) |
| U5 | Add a confirmation prompt before retroactive recategorize from the drill view? | No prompt; "Save as a rule" label already conveys it. Revisit if real users get surprised |

### 11.4 Resolved (recent)

| ID | Decision | Resolved in |
|---|---|---|
| ~~7.1.1~~ | UI stack — Tauri + React + TypeScript | scaffold (51c63c4) |
| ~~7.1.2~~ | Internet lookup user-opt-in (not always-offline) | LLM config + Gemini ship |
| ~~7.1.3~~ | External lookup approach — curated API (Gemini) with strict privacy filter | LLM client + OD-5 egress guard |
| ~~7.1.4~~ | Unsupported bank statement formats: rejected with adapter-not-found error in v1 | parser registry behavior |
| ~~7.3.1~~ | Encrypt local files by default — Yes (AES-256-GCM, per-profile DEK) | Phase-1 storage layer |
| ~~D1~~ | Gemini API key transport — `x-goog-api-key` header (not URL query string) | 61e245c |
| ~~D2~~ | Merchant cache key shape — composite `"{dir}\|{merchant}"` string | 61e245c |
| ~~F-CRIT-1~~ | Direction-blind cache key — fixed | 61e245c |
| ~~F-SEC-1~~ | API key in URL — fixed | 61e245c |
| ~~F-SEC-2~~ | OD-5 egress guard — added | 61e245c |
| ~~F-TDD-1~~ | `reapply_categories` invariants — covered by tests | 61e245c |
| ~~P3~~ | Own-account transfers — **neutral**, excluded from income/expense. Implemented via category-based classification (`Credit Card Payment`, `Bank Transfer`). Pairing-by-amount across statements is a Phase-2 enhancement. | (decided + implemented; this commit) |
| ~~P4~~ | Cash withdrawals — **treated as expense by default**. `ATM / Cash` category falls through the expense path in `classify_category`. User-marks-as-held remains a Phase-2 enhancement. | (decided + implemented; this commit) |
| ~~P5~~ | Financial-health score weights — **40% savings rate, 25% debt burden, 20% essential vs discretionary, 15% investment consistency**. | (decided + implemented; this commit) |
| ~~Audit F-TDD-1~~ | Income-arm `expense += debit` half-baked branch — removed; debit on income category no longer silently folded into expense (pinned by `salary_debit_is_ignored_not_silently_folded_into_expense`). | (this commit) |
| ~~Audit F-SEC-1~~ | Unbounded `raw-transactions.json` decrypt+parse — 16 MB hard cap added in `dashboard.rs` before `serde_json::from_slice`. | (this commit) |
| ~~Audit F-INF-3~~ | Category sort round-tripped through formatted strings (silently lossy on parse failure) — now sorts on in-memory `Decimal` before formatting. | (this commit) |
| ~~Audit F-INF-4~~ | Dashboard Refresh re-entry — `useRef` in-flight guard prevents concurrent `dashboard_aggregate` invokes from rapid clicks. | (this commit) |
| ~~Audit M1~~ | `decimal_to_f32` untested — added `decimal_to_f32_handles_edge_values` covering zero, fractional, and large-magnitude inputs. | (Dashboard-full commit) |
| ~~Audit M2~~ | Malformed `txn_date` would have surfaced as a synthetic `"0000-00"` monthly-trend bucket — `parse_iso_month` now validates `YYYY-MM-DD` shape; bad rows still count in headline totals but are skipped from the trend. Pinned by `parse_iso_month_validates_shape` + `monthly_trend_skips_malformed_dates`. | (Dashboard-full commit) |
| ~~Audit M3~~ | "Discretionary spend is large" behavioral nudge no longer fires when an `expense-cut` recommendation already named the specific category — prevents redundant advice. Pinned by `behavioral_nudge_skipped_when_expense_cut_already_fired`. | (Dashboard-full commit) |
| ~~Audit M4~~ | `build_recommendations` no longer round-trips Decimals through formatted strings (avoiding the F-INF-3 regression pattern); it now takes a `&[(String, Decimal, u32, CategoryKind)]` snapshot of the sorted accumulator directly. | (Dashboard-full commit) |
| ~~Audit M5~~ | `monthly_impact` is now genuinely per-month — recommendation divides the category's total debit by months in the period before the 20% trim. Pinned by `expense_cut_impact_is_monthly_not_period_total`. | (Dashboard-full commit) |
| ~~Drill F-CRIT-3~~ | `recategorize_all_imports` no longer silently swallows per-import failures — returns `{ total, touched, skipped }`; the drill modal's toast surfaces partial-success outcomes. | (this commit) |
| ~~Drill U3~~ | Category chip buttons in the drill modal are disabled while a retroactive re-categorize is in flight, preventing the user from firing parallel recategorizations during the ~200ms window. | (this commit) |
| ~~Trend negative-OUT bug + refund double-count~~ | Monthly expense could go negative when refunds on expense-categorized rows dominated debits in that month. First fix routed credits-on-expense to income, which solved the visual bug but introduced an accounting flaw — a refund of a previous purchase counted as new income, double-counting the originating salary. **Final fix**: aggregate per `(month, category)` and per category overall, then compute `expense = max(0, debit − credit)`. Refunds reduce that category's expense (never below zero) and the excess credit (if any) is silently dropped rather than become income. Same rule applies to investment-kind rows: payouts net against the same category's outflow, never become income. Pinned by `refund_nets_against_same_category_does_not_become_income` and `refunds_exceeding_debits_floor_expense_at_zero_never_become_income`. Bar widths additionally `clampPct`ed to `[0, 100]` as defense-in-depth. | (this commit) |
| ~~Headline ≠ sum of monthly~~ | After the per-category clamp landed, the overview tiles (₹3,69,888 expense) didn't equal the sum of monthly trend bars (₹4,08,110) — a ~₹38k gap caused by refunds that landed in a different month from the original purchase. Headline now derived directly from `by_month` so the trend always adds up to the overview. Rows with malformed dates are bucketed under a sentinel `__undated__` key that's included in headline sums but filtered out of the trend view, preserving the `monthly_trend_skips_malformed_dates` contract. Pinned by `headline_totals_equal_sum_of_monthly_buckets`. | (this commit) |
| ~~Savings-rate "no income" message wrong~~ | The savings-rate driver was scoring 0 and reading "NO INCOME RECORDED YET — UPLOAD A SALARY STATEMENT" even when income was clearly present (₹3,16,309). The score correctly clamps to 0 when `expense > income`, but the driver-detail couldn't tell that case from the no-income case. New `savings_rate_detail` takes the actual income / expense values and surfaces three distinct messages: "no income recorded", "spending exceeds income", or the saving-rate gradient. Pinned by `savings_rate_detail_distinguishes_no_income_from_overspending`. | (this commit) |
| ~~Custom-category gap~~ | User-created labels (`SIP`, `Loans`, `Food`) were hitting expense fallthrough — the investment-consistency driver was a hardcoded 50 placeholder and SIPs counted as leakage. Added `CategoryKind::Investment`, case-insensitive synonym table in `classify_category` / `is_essential`, and a real `investment_consistency_score` based on the fraction of tracked months with at least one investment outflow. Pinned by `classify_recognises_investment_synonyms`, `is_essential_recognises_user_synonyms`, `investments_are_excluded_from_expense_and_drive_consistency`, `investment_consistency_falls_when_user_skips_months`. | (this commit) |

## 12. Pre-commit workflow (mandatory)
For every commit that changes user-visible behavior:
1. Update Section 10 of this brief to reflect the change.
2. Sweep Section 11 — move any newly-resolved decision into 11.4 with its commit hash; add any new ones surfaced this commit.
3. Run the `/five-lens` skill on this brief to get a fresh planner pass over the five lenses (scalability, security, testing, architecture, cost). Address or capture any actionable finding before committing.
