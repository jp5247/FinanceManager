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

export interface LlmConfigUpdate {
  enabled?: boolean;
  model?: string;
  /** `""` clears the key, omit to leave untouched, otherwise set it. */
  apiKey?: string;
}
