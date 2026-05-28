/**
 * Suggested category labels for the manual recategorize picker.
 *
 * Stored as plain strings on transactions — the UI doesn't enforce that a
 * category must come from this list. "Other..." in the picker reveals a
 * text input so the user can name a custom one. The "Reset categories"
 * button in the user-rules panel wipes any custom categorizations and
 * re-runs the pipeline using only the curated rules + LLM constrained
 * to the canonical list below.
 *
 * Order in this list also drives the picker order in the recategorize
 * modal. Income / system categories are grouped at the bottom.
 */
export const COMMON_CATEGORIES: readonly string[] = [
  // Bills
  "Credit card bill",
  "Electricity bill",
  "Gas bill",
  "Mobile/Internet bill",
  "Laundary bill",
  // EMIs
  "Home Loan EMI",
  "Car loan EMI",
  "CC EMI",
  // Lifestyle expenses
  "Food expenses",
  "Hotel/Vacation expenses",
  "Fuel expenses",
  "Vehicle repairs/maintenance",
  "Medical expenses",
  "Groceries",
  "Transportation",
  "Personal care",
  "Shopping",
  "Entertainment",
  "Gifts",
  // Income
  "Salary",
  "Side Hustle",
  "Interest",
  "Dividend",
  "Refund",
  // Wealth-building
  "SIP",
  "Stock purchase",
  "FD",
  // System / bookkeeping (kept for accounting correctness)
  "Bank Transfer",
  "EMI Conversion",
];

export const UNCATEGORIZED = "Uncategorized";
