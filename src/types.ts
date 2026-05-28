export interface ProfileSummary {
  userId: string;
  displayName: string;
  createdAt: string;
}

export interface CreateProfileResult {
  summary: ProfileSummary;
  recoveryPhrase: string;
}

export interface RawTransaction {
  importId: string;
  sourceFile: string;
  sourceSha256: string;
  sourcePage: number;
  rowNumber: number;
  parserVersion: string;
  parserBackend: "pdfium" | "pdf-extract" | "ocr-tesseract";

  txnDate: string;
  description: string;
  /** Magnitude as a stringified decimal, e.g. "1582.00". `null` when not a debit row. */
  debit: string | null;
  credit: string | null;
  balance: string | null;

  /** Set after categorization. `null` for pre-categorization data. */
  category?: string | null;
  /** ID of the rule that fired, e.g. `"food/swiggy"`. */
  categoryRuleId?: string | null;
}

export interface CategoryBreakdown {
  category: string;
  debitCount: number;
  creditCount: number;
  totalDebit: string;
  totalCredit: string;
}

export type StoredMatchType = "contains" | "regex";

export interface StoredRule {
  id: string;
  priority: number;
  matchType: StoredMatchType;
  matchValue: string;
  category: string;
  confidence: number;
  createdAt: string;
}

export interface NewRuleSpec {
  matchType: StoredMatchType;
  matchValue: string;
  category: string;
}

export interface FileMeta {
  importId: string;
  uploadedAt: string;
  sourceFile: string;
  sourceSha256: string;
  adapterId: string;
  adapterVersion: string;
  pageCount: number;
  transactionCount: number;
  debitCount: number;
  creditCount: number;
  /** Decimal string, e.g. "274712.52". */
  totalDebit: string;
  totalCredit: string;
  /** Optional — empty for pre-categorization imports. */
  categoryBreakdown?: CategoryBreakdown[];
}

export interface UploadResult {
  importId: string;
  uploadedAt: string;
  sourceFile: string;
  sourceSha256: string;
  adapterId: string;
  pageCount: number;
  transactionCount: number;
  debitCount: number;
  creditCount: number;
  totalDebit: string;
  totalCredit: string;
  categoryBreakdown: CategoryBreakdown[];
  transactions: RawTransaction[];
  /** Non-fatal note about external categorization. */
  lookupWarning?: string | null;
  /** How many rows got their category from the LLM in this run. */
  llmCategorizedCount?: number;
}

export interface LlmConfigView {
  enabled: boolean;
  model: string;
  apiKeyHint: string;
  apiKeySet: boolean;
}

export interface CategoryTotal {
  category: string;
  count: number;
  totalDebit: string;
  totalCredit: string;
  /** `"income" | "expense" | "transfer" | "investment"` */
  kind: string;
}

export interface DashboardData {
  importCount: number;
  periodStart: string | null;
  periodEnd: string | null;
  transactionCount: number;
  totalIncome: string;
  totalExpense: string;
  netSavings: string;
  transferCount: number;
  transferTotal: string;
  undatedCount: number;
  categoryTotals: CategoryTotal[];
  monthlyTrend: MonthlyBucket[];
  healthScore: HealthScore;
  recommendations: Recommendation[];
}

export interface MonthlyBucket {
  /** `YYYY-MM` */
  month: string;
  income: string;
  expense: string;
  net: string;
}

export interface HealthScore {
  composite: number;
  drivers: HealthDriver[];
}

export interface HealthDriver {
  key: string;
  label: string;
  score: number;
  weight: number;
  detail: string;
}

export interface Recommendation {
  kind: string;
  title: string;
  detail: string;
  monthlyImpact: string | null;
}

export interface InvestmentAsset {
  id: string;
  assetType: string;
  assetName: string;
  /** Decimal string, e.g. "100000.00". */
  investedAmount: string;
  currentValue: string;
  lastUpdatedAt: string;
  notes?: string | null;
}

export interface UpsertInvestmentSpec {
  /** Empty / undefined to create; existing id to update. */
  id?: string;
  assetType: string;
  assetName: string;
  investedAmount: string;
  currentValue: string;
  notes?: string | null;
}

export interface AllocationSlice {
  assetType: string;
  currentValue: string;
  sharePct: string;
  assetCount: number;
}

export interface InvestmentsSummary {
  assetCount: number;
  totalInvested: string;
  totalCurrentValue: string;
  unrealizedGainLoss: string;
  /** Signed percent as decimal string, empty when invested === 0. */
  returnPct: string;
  allocation: AllocationSlice[];
}

export interface Loan {
  id: string;
  loanType: string;
  lender: string;
  principalOutstanding: string;
  interestRate: string;
  rateType: string;
  remainingTenureMonths: number;
  emi: string;
  prepaymentPenaltyPct: string;
  taxBenefit: boolean;
  startDate: string;
  nextDueDate: string;
  notes?: string | null;
  lastUpdatedAt: string;
}

export interface UpsertLoanSpec {
  id?: string;
  loanType: string;
  lender: string;
  principalOutstanding: string;
  interestRate: string;
  rateType: string;
  remainingTenureMonths: number;
  emi: string;
  prepaymentPenaltyPct?: string;
  taxBenefit?: boolean;
  startDate: string;
  nextDueDate: string;
  notes?: string | null;
}

export interface LoanClassification {
  loanId: string;
  /** "good" | "watch" | "bad" */
  verdict: string;
  effectiveRate: string;
  rationale: string;
}

export interface LoansSummary {
  loanCount: number;
  totalOutstanding: string;
  totalMonthlyEmi: string;
  weightedAvgRate: string;
  classifications: LoanClassification[];
  /** Loan IDs ordered by avalanche (highest rate first) priority. */
  avalancheOrder: string[];
  /** Loan IDs ordered by snowball (smallest balance first) priority. */
  snowballOrder: string[];
}

export interface AuditEntryView {
  ts: string;
  action: string;
  entityId: string | null;
  details: unknown;
  thisHash: string;
}

export interface AuditLogView {
  entries: AuditEntryView[];
  chainOk: boolean;
  chainNote: string | null;
}

export interface ExportResult {
  filePath: string;
  transactionCount: number;
  uncategorizedCount: number;
  investmentCount: number;
  loanCount: number;
  warning?: string | null;
}

export interface RecategorizeAllResult {
  total: number;
  touched: number;
  skipped: number;
}

export interface LlmConfigUpdate {
  enabled?: boolean;
  model?: string;
  /** `""` clears the key, omit to leave untouched, otherwise set it. */
  apiKey?: string;
}
