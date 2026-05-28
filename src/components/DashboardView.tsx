import { useCallback, useEffect, useRef, useState } from "react";
import { dashboardAggregate, investmentsSummary, loansSummary } from "../ipc";
import type {
  CategoryTotal,
  DashboardData,
  HealthScore,
  InvestmentsSummary,
  LoansSummary,
  MonthlyBucket,
  Recommendation,
} from "../types";
import { CategoryDrillModal } from "./CategoryDrillModal";
import { MonthDrillModal } from "./MonthDrillModal";

const inrFormatter = new Intl.NumberFormat("en-IN", {
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
});

function fmtINR(decimalStr: string): string {
  const n = Number.parseFloat(decimalStr);
  if (!Number.isFinite(n)) return decimalStr;
  return inrFormatter.format(n);
}

function fmtDate(iso: string | null): string {
  if (!iso) return "—";
  return iso;
}

/** Clamp a percentage into [0, 100] so an invalid (negative or NaN) value
 * never lets `width: "-32%"` fall back to `auto` and stretch the bar. */
function clampPct(pct: number): number {
  if (!Number.isFinite(pct)) return 0;
  return Math.max(0, Math.min(100, pct));
}

export function DashboardView() {
  const [data, setData] = useState<DashboardData | null>(null);
  const [invSummary, setInvSummary] = useState<InvestmentsSummary | null>(null);
  const [loanSummary, setLoanSummary] = useState<LoansSummary | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [drillCategory, setDrillCategory] = useState<string | null>(null);
  const [drillMonth, setDrillMonth] = useState<string | null>(null);
  // Re-entry guard via ref — React state updates are batched, so checking
  // `loading` inside refresh sees stale values during rapid clicks.
  const inFlight = useRef(false);

  const refresh = useCallback(async () => {
    if (inFlight.current) return;
    inFlight.current = true;
    setLoading(true);
    setError(null);
    try {
      const [d, inv, loans] = await Promise.all([
        dashboardAggregate(),
        investmentsSummary(),
        loansSummary(),
      ]);
      setData(d);
      setInvSummary(inv);
      setLoanSummary(loans);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
      inFlight.current = false;
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  if (loading) {
    return (
      <section className="dashboard-view">
        <div className="card"><p className="muted">Loading dashboard…</p></div>
      </section>
    );
  }

  if (error) {
    return (
      <section className="dashboard-view">
        <div className="card error-card">
          <h3>Could not load dashboard</h3>
          <p className="muted">{error}</p>
          <button className="btn btn-secondary" onClick={() => void refresh()}>Try again</button>
        </div>
      </section>
    );
  }

  // Render-eligibility: any one of (uploads, investments, loans) is enough
  // to show the dashboard. Previously we early-returned on `importCount==0`,
  // which silently hid the Wealth Snapshot when the user had added
  // investments but not yet uploaded a statement.
  const hasUploads = !!data && data.importCount > 0;
  const hasInvestments = !!invSummary && invSummary.assetCount > 0;
  const hasLoans = !!loanSummary && loanSummary.loanCount > 0;

  if (!data || (!hasUploads && !hasInvestments && !hasLoans)) {
    return (
      <section className="dashboard-view">
        <div className="card">
          <h2>Nothing to show yet</h2>
          <p className="muted">
            Upload a bank or credit-card PDF from the Upload tab, or add a
            position in the Investments / Loans tab — anything you record
            will surface here.
          </p>
        </div>
      </section>
    );
  }

  return (
    <section className="dashboard-view">
      <header className="dash-header">
        <div>
          <h2>Financial overview</h2>
          <p className="muted small">
            {hasUploads ? (
              <>
                {data.importCount} import{data.importCount === 1 ? "" : "s"} ·{" "}
                {data.transactionCount} transaction{data.transactionCount === 1 ? "" : "s"} ·{" "}
                {fmtDate(data.periodStart)} → {fmtDate(data.periodEnd)}
              </>
            ) : (
              <>No statements uploaded yet — upload one to see income/expense.</>
            )}
          </p>
        </div>
        <button className="btn btn-link inline" onClick={() => void refresh()}>
          Refresh
        </button>
      </header>

      {hasUploads && <HealthStrip score={data.healthScore} />}

      {hasUploads && <OverviewTiles data={data} />}

      {hasInvestments && (
        <WealthSnapshot summary={invSummary!} />
      )}

      {data.transferCount > 0 && (
        <div className="dash-transfer-note muted small">
          {data.transferCount} transfer{data.transferCount === 1 ? "" : "s"} totalling ₹
          {fmtINR(data.transferTotal)} excluded from income/expense (own-account moves).
        </div>
      )}

      <MonthlyTrendCard
        trend={data.monthlyTrend}
        onDrill={(month) => setDrillMonth(month)}
      />

      <FixMyFinance recommendations={data.recommendations} />

      <CategoryBreakdown
        totals={data.categoryTotals}
        onDrill={(category) => setDrillCategory(category)}
      />

      {drillCategory && (
        <CategoryDrillModal
          category={drillCategory}
          onClose={() => setDrillCategory(null)}
          onChanged={() => void refresh()}
        />
      )}

      {drillMonth && (
        <MonthDrillModal
          month={drillMonth}
          onClose={() => setDrillMonth(null)}
          onChanged={() => void refresh()}
        />
      )}
    </section>
  );
}

function WealthSnapshot({ summary }: { summary: InvestmentsSummary }) {
  const invested = Number.parseFloat(summary.totalInvested) || 0;
  const gain = Number.parseFloat(summary.unrealizedGainLoss) || 0;
  const positive = gain >= 0;
  return (
    <div className="card wealth-snapshot">
      <div className="wealth-headline">
        <div>
          <div className="health-title">Wealth snapshot</div>
          <p className="muted small wealth-blurb">
            From your manually-entered positions in the Investments tab.
            Update current values periodically to keep this honest.
          </p>
        </div>
        <div className="wealth-numbers">
          <div className="wealth-num">
            <div className="muted xsmall">Current value</div>
            <div className="wealth-amount">₹{fmtINR(summary.totalCurrentValue)}</div>
          </div>
          <div className="wealth-num">
            <div className="muted xsmall">Invested</div>
            <div className="wealth-amount muted">₹{fmtINR(summary.totalInvested)}</div>
          </div>
          <div className={`wealth-num ${positive ? "net-positive" : "net-negative"}`}>
            <div className="muted xsmall">Unrealized</div>
            <div className="wealth-amount">
              {positive ? "+" : "−"}₹{fmtINR(Math.abs(gain).toFixed(2))}
              {invested > 0 && summary.returnPct ? (
                <span className="wealth-pct">
                  {" "}({positive ? "+" : ""}
                  {summary.returnPct}%)
                </span>
              ) : null}
            </div>
          </div>
        </div>
      </div>
      {summary.allocation.length > 1 && (
        <div className="wealth-allocation">
          {summary.allocation.map((a) => {
            const share = Math.max(0, Math.min(100, Number.parseFloat(a.sharePct) || 0));
            return (
              <div key={a.assetType} className="wealth-alloc-pill" title={`₹${fmtINR(a.currentValue)} (${a.assetCount} asset${a.assetCount === 1 ? "" : "s"})`}>
                <span>{a.assetType}</span>
                <strong>{share.toFixed(0)}%</strong>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

function HealthStrip({ score }: { score: HealthScore }) {
  const tone =
    score.composite >= 75 ? "good" : score.composite >= 50 ? "mid" : "low";
  return (
    <div className={`card health-strip tone-${tone}`}>
      <div className="health-headline">
        <div className="health-score-num">
          {score.composite}
          <span className="health-score-suffix">/100</span>
        </div>
        <div>
          <div className="health-title">Financial health</div>
          <p className="muted small health-blurb">
            Weighted composite — savings rate 40%, debt burden 25%, essential
            vs discretionary 20%, investment consistency 15%.
          </p>
        </div>
      </div>
      <ul className="health-drivers">
        {score.drivers.map((d) => (
          <li key={d.key} className="health-driver-row">
            <div className="health-driver-head">
              <span className="health-driver-label">{d.label}</span>
              <span className="health-driver-weight muted xsmall">
                {Math.round(d.weight * 100)}%
              </span>
              <span className="health-driver-score">{d.score}</span>
            </div>
            <div className="health-driver-bar" aria-hidden>
              <div
                className={`health-driver-fill driver-${driverTone(d.score)}`}
                style={{ width: `${d.score}%` }}
              />
            </div>
            <p className="muted xsmall health-driver-detail">{d.detail}</p>
          </li>
        ))}
      </ul>
    </div>
  );
}

function driverTone(score: number): "good" | "mid" | "low" {
  if (score >= 75) return "good";
  if (score >= 50) return "mid";
  return "low";
}

function MonthlyTrendCard({
  trend,
  onDrill,
}: {
  trend: MonthlyBucket[];
  onDrill: (month: string) => void;
}) {
  if (trend.length === 0) return null;
  const max = trend.reduce((m, b) => {
    const inc = Number.parseFloat(b.income) || 0;
    const exp = Number.parseFloat(b.expense) || 0;
    return Math.max(m, inc, exp);
  }, 0);
  if (max <= 0) return null;
  return (
    <div className="card monthly-trend-card">
      <h3>Income vs expense by month</h3>
      <p className="muted xsmall trend-hint">
        Click any month to see the transactions that produced its income and
        expense.
      </p>
      <ul className="trend-list">
        {trend.map((b) => {
          const inc = Number.parseFloat(b.income) || 0;
          const exp = Number.parseFloat(b.expense) || 0;
          const net = Number.parseFloat(b.net) || 0;
          return (
            <li key={b.month} className="trend-row">
              <button
                type="button"
                className="trend-row-btn"
                onClick={() => onDrill(b.month)}
                title={`See transactions in ${b.month}`}
              >
                <div className="trend-month mono">{b.month}</div>
                <div className="trend-bars">
                  <div className="trend-bar-row">
                    <span className="trend-bar-label muted xsmall">In</span>
                    <div className="trend-bar-track" aria-label={`income ${b.income}`}>
                      <div
                        className="trend-bar-fill income"
                        style={{ width: `${clampPct((inc / max) * 100)}%` }}
                      />
                    </div>
                    <span className="trend-bar-amount credit">₹{fmtINR(b.income)}</span>
                  </div>
                  <div className="trend-bar-row">
                    <span className="trend-bar-label muted xsmall">Out</span>
                    <div className="trend-bar-track" aria-label={`expense ${b.expense}`}>
                      <div
                        className="trend-bar-fill expense"
                        style={{ width: `${clampPct((exp / max) * 100)}%` }}
                      />
                    </div>
                    <span className="trend-bar-amount debit">₹{fmtINR(b.expense)}</span>
                  </div>
                </div>
                <div className={`trend-net ${net >= 0 ? "credit" : "debit"}`}>
                  <div className="trend-net-label muted xsmall">Net</div>
                  <div>
                    {net >= 0 ? "+" : "−"}₹{fmtINR(Math.abs(net).toFixed(2))}
                  </div>
                </div>
              </button>
            </li>
          );
        })}
      </ul>
    </div>
  );
}

function FixMyFinance({ recommendations }: { recommendations: Recommendation[] }) {
  if (recommendations.length === 0) return null;
  return (
    <div className="card fix-finance-card">
      <h3>Fix my finance</h3>
      <p className="muted small">
        Heuristic suggestions based on the data above. These will sharpen up as
        the Loan Tracker and Investment Inputs tabs come online.
      </p>
      <ul className="recommendation-list">
        {recommendations.map((r, i) => (
          <li key={`${r.kind}-${i}`} className={`recommendation-row kind-${r.kind}`}>
            <div className="recommendation-head">
              <span className="recommendation-title">{r.title}</span>
              {r.monthlyImpact && (
                <span className="recommendation-impact">
                  ≈ ₹{fmtINR(r.monthlyImpact)} savings
                </span>
              )}
            </div>
            <p className="muted small">{r.detail}</p>
          </li>
        ))}
      </ul>
    </div>
  );
}

function OverviewTiles({ data }: { data: DashboardData }) {
  const income = Number.parseFloat(data.totalIncome) || 0;
  const net = Number.parseFloat(data.netSavings) || 0;
  const savingsRate = income > 0 ? (net / income) * 100 : 0;
  return (
    <div className="summary-tiles" role="group" aria-label="Financial overview">
      <div className="summary-tile tile-credit">
        <div className="tile-label">Total income</div>
        <div className="tile-amount">
          <span className="tile-currency">₹</span>
          {fmtINR(data.totalIncome)}
        </div>
        <div className="tile-sub muted">salary, dividend, interest, refunds</div>
      </div>

      <div className="summary-tile tile-debit">
        <div className="tile-label">Total expense</div>
        <div className="tile-amount">
          <span className="tile-currency">₹</span>
          {fmtINR(data.totalExpense)}
        </div>
        <div className="tile-sub muted">excludes own-account transfers</div>
      </div>

      <div className={`summary-tile tile-net ${net >= 0 ? "net-positive" : "net-negative"}`}>
        <div className="tile-label">Net savings</div>
        <div className="tile-amount">
          {net >= 0 ? "+" : "−"}
          <span className="tile-currency">₹</span>
          {fmtINR(Math.abs(net).toFixed(2))}
        </div>
        <div className="tile-sub muted">
          {income > 0 ? `${savingsRate.toFixed(1)}% savings rate` : "no income recorded"}
        </div>
      </div>
    </div>
  );
}

function CategoryBreakdown({
  totals,
  onDrill,
}: {
  totals: CategoryTotal[];
  onDrill: (category: string) => void;
}) {
  const expenses = totals.filter((t) => t.kind === "expense" && Number.parseFloat(t.totalDebit) > 0);
  const income = totals.filter((t) => t.kind === "income");
  const investments = totals.filter(
    (t) => t.kind === "investment" && Number.parseFloat(t.totalDebit) > 0,
  );

  if (expenses.length === 0 && income.length === 0 && investments.length === 0) {
    return null;
  }

  const sumDebit = (xs: CategoryTotal[]) =>
    xs.reduce((s, t) => s + (Number.parseFloat(t.totalDebit) || 0), 0);
  const sumCredit = (xs: CategoryTotal[]) =>
    xs.reduce((s, t) => s + (Number.parseFloat(t.totalCredit) || 0), 0);

  const expenseTotal = sumDebit(expenses);
  const investmentTotal = sumDebit(investments);
  const incomeTotal = sumCredit(income);

  return (
    <div className="card dash-category-card">
      <p className="muted xsmall dash-category-hint">
        Click a category to review and recategorize transactions across all
        statements. Bar width and % show the category's share of its group.
      </p>

      {expenses.length > 0 && (
        <BreakdownSection
          heading="Where the money went"
          total={expenseTotal}
          rows={expenses}
          amountKind="debit"
          accent="expense"
          onDrill={onDrill}
        />
      )}

      {investments.length > 0 && (
        <BreakdownSection
          heading="Wealth-building"
          total={investmentTotal}
          rows={investments}
          amountKind="debit"
          accent="investment"
          onDrill={onDrill}
        />
      )}

      {income.length > 0 && (
        <BreakdownSection
          heading="Money in"
          total={incomeTotal}
          rows={income}
          amountKind="credit"
          accent="credit"
          onDrill={onDrill}
        />
      )}
    </div>
  );
}

function BreakdownSection({
  heading,
  total,
  rows,
  amountKind,
  accent,
  onDrill,
}: {
  heading: string;
  total: number;
  rows: CategoryTotal[];
  amountKind: "debit" | "credit";
  /** Drives the amount-text color + bar-fill class. */
  accent: "expense" | "investment" | "credit";
  onDrill: (category: string) => void;
}) {
  return (
    <>
      <div className="breakdown-section-head">
        <h3 className="bk-heading-secondary">{heading}</h3>
        <span className={`breakdown-section-total bk-amount-${accent}`}>
          ₹{new Intl.NumberFormat("en-IN", {
            minimumFractionDigits: 2,
            maximumFractionDigits: 2,
          }).format(total)}
        </span>
      </div>
      <ul className="breakdown-list">
        {rows.map((t) => {
          const raw = amountKind === "debit" ? t.totalDebit : t.totalCredit;
          const amt = Number.parseFloat(raw) || 0;
          const pct = total > 0 ? clampPct((amt / total) * 100) : 0;
          return (
            <li key={t.category} className={`breakdown-row ${accent}`}>
              <button
                type="button"
                className="breakdown-row-btn"
                onClick={() => onDrill(t.category)}
                title={`Review ${t.count} ${t.category} transaction${t.count === 1 ? "" : "s"} — ${pct.toFixed(1)}% of ${heading.toLowerCase()}`}
              >
                <span className="bk-name">{t.category}</span>
                <span className="bk-bar" aria-hidden>
                  <span
                    className={`bk-bar-fill ${accent}`}
                    style={{ width: `${pct}%` }}
                  />
                </span>
                <span className="bk-pct">{pct.toFixed(0)}%</span>
                <span className="bk-count muted">
                  {t.count} {t.count === 1 ? "txn" : "txns"}
                </span>
                <span className={`bk-amount bk-amount-${accent}`}>
                  ₹{fmtINR(raw)}
                </span>
              </button>
            </li>
          );
        })}
      </ul>
    </>
  );
}
