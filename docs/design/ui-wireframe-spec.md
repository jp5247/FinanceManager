# FinanceManager UI Wireframe Specification

## 1. UX direction
- Theme: dark with neon accents.
- Typography: high contrast, readable numeric tables.
- Pointer: custom neon ring cursor with clear center dot and focus halo on interactive elements.
- Layout: desktop-first with responsive collapse for tablet/mobile.

## 2. Global app shell

## 2.1 Top bar
- Left: app logo + active user profile switch.
- Center: global month selector.
- Right: sync state (local), internet lookup toggle, settings, logout.

## 2.2 Left navigation
- Dashboard
- Upload
- Past Analysis
- Investment Inputs
- Loan Tracker
- Export

## 2.3 Notification rail
- Persistent non-blocking alerts for unresolved flags and parser warnings.

## 3. Screen wireframes

## 3.1 Dashboard screen

### A. Financial health strip (top)
- Full-width card with:
  - Health score (0-100)
  - Trend arrow vs prior month
  - Key drivers chips: Savings, Debt, Leak, Investing

### B. Snapshot cards row
- Income
- Net Expenses
- Net Savings
- Investment Growth
- Bleed Money

### C. Bleed analysis panel
- Left: top leak categories bar chart.
- Right: recurring avoidable expenses list with monthly impact.

### D. Fix my finances panel
- Prioritized recommendations table:
  - Action
  - Effort
  - Monthly savings potential
  - Risk
  - Enable tracking toggle

### E. Trend panel
- Income vs expense line chart.
- Expense by category stacked area chart.
- Time range selector: 3M, 6M, 12M, custom.

### F. Wealth plan panel
- Suggested allocation summary:
  - emergency fund
  - debt prepayment
  - investments

## 3.2 Upload screen

### A. Import drop zone
- Drag and drop or browse file picker.
- Supports multiple PDF uploads per batch.

### B. Import queue
- File name
- Statement type guess
- Institution guess
- Parse status
- Error count

### C. Parsed transactions grid
Columns:
- Date
- Description
- Amount
- Direction
- Merchant (raw)
- Canonical merchant
- Category
- Confidence
- Flag status
- Actions

Actions:
- Edit category
- Mark transfer
- Mark reimbursement (full/partial)
- Split transaction
- Ignore from analysis

### D. Flag resolution drawer
- Opens for selected flagged row.
- Shows why flagged and suggested categories.
- Save as rule checkbox to update category rules.

### E. Finalization footer
- Open flags counter.
- Button: Rebuild analysis.
- Button: Finalize month (enabled only when no blocking flags).

## 3.3 Past Analysis screen

### A. Period selector
- Month chips and year filter.

### B. Comparison header
- Selected month summary vs previous month.

### C. Historical insights
- Category drift chart.
- Income stability indicator.
- Savings consistency timeline.

### D. Recommendation archive
- Previously suggested actions and status:
  - Not started
  - In progress
  - Completed

## 3.4 Investment Inputs screen

### A. Portfolio list
Columns:
- Asset type
- Asset name
- Invested value
- Current value
- Gain/loss
- Last updated

### B. Add/edit asset form
Fields:
- Asset type
- Asset name
- Invested amount
- Current value
- As-of date
- Notes

### C. Allocation and performance widgets
- Allocation donut by asset class.
- Gain/loss trend sparkline.

## 3.5 Loan Tracker screen

### A. Loan input form
Fields:
- Loan type
- Lender
- Outstanding principal
- Interest rate
- Rate type
- Remaining tenure
- EMI
- Prepayment penalty
- Tax benefit eligibility
- Next due date

### B. Loan quality panel
- Good vs bad classification card per loan.
- Rationale bullets and effective borrowing cost.

### C. Closure strategy panel
- Suggested payoff priority list.
- Scenario simulator:
  - Extra monthly payment slider
  - Lump sum prepayment input
  - Interest saved and closure date impact

## 3.6 Export screen

### A. Export readiness block
- Checklist:
  - No unresolved flags
  - Analytics rebuilt
  - Required modules complete

### B. Export options
- Date range
- Include formulas toggle
- Include audit logs toggle

### C. Export action
- Generate workbook button.
- Download history list with checksum.

## 3.7 Multi-user and security dialogs

### A. User switch modal
- Active local users list.
- Create new user.
- Archive user profile.

### B. Privacy guardrail modal
- Explains what data can be used for internet lookup.
- Consent gate with per-session enable option.

## 4. Interaction states
- Empty state: first-time setup guidance and sample import steps.
- Loading state: skeleton rows/charts with progress text.
- Error state: human-readable parser and validation messages.
- Success state: confirmation toast and change summary.

## 5. Accessibility requirements
- Keyboard navigation for all table actions.
- WCAG contrast compliance even with neon palette.
- Color-independent status indicators (icon + text).
- Cursor customization must remain optional in settings.

## 6. Responsive behavior
- Tablet: collapse left navigation into icon rail.
- Mobile: tabbed top navigation and stacked cards.
- Transaction grid on mobile uses expandable rows.

## 7. Event contracts between screens
- Upload finalization triggers analytics rebuild.
- Analytics refresh updates Dashboard and Past Analysis.
- Investment and Loan updates trigger health score recalculation.
- Export availability depends on flag status and latest analytics timestamp.

## 8. Phase 1 minimum screens
- Dashboard
- Upload
- Past Analysis
- Investment Inputs
- Loan Tracker
- Export
- User switch modal
