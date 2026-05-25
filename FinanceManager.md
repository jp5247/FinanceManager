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

## 9. Implementation artifacts added
1. Detailed local file-based schema and lifecycle:
	- docs/design/local-data-schema.md
2. Wireframe-level UI screen specification:
	- docs/design/ui-wireframe-spec.md
