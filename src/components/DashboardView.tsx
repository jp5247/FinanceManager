import { useCallback, useEffect, useRef, useState } from "react";
import { dashboardAggregate } from "../ipc";
import type { CategoryTotal, DashboardData } from "../types";

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

export function DashboardView() {
  const [data, setData] = useState<DashboardData | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  // Re-entry guard via ref — React state updates are batched, so checking
  // `loading` inside refresh sees stale values during rapid clicks.
  const inFlight = useRef(false);

  const refresh = useCallback(async () => {
    if (inFlight.current) return;
    inFlight.current = true;
    setLoading(true);
    setError(null);
    try {
      setData(await dashboardAggregate());
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

  if (!data || data.importCount === 0) {
    return (
      <section className="dashboard-view">
        <div className="card">
          <h2>No statements yet</h2>
          <p className="muted">
            Upload a bank or credit-card PDF from the Upload tab and your
            income, expenses, and category breakdown will appear here.
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
            {data.importCount} import{data.importCount === 1 ? "" : "s"} ·{" "}
            {data.transactionCount} transaction{data.transactionCount === 1 ? "" : "s"} ·{" "}
            {fmtDate(data.periodStart)} → {fmtDate(data.periodEnd)}
          </p>
        </div>
        <button className="btn btn-link inline" onClick={() => void refresh()}>
          Refresh
        </button>
      </header>

      <OverviewTiles data={data} />

      {data.transferCount > 0 && (
        <div className="dash-transfer-note muted small">
          {data.transferCount} transfer{data.transferCount === 1 ? "" : "s"} totalling ₹
          {fmtINR(data.transferTotal)} excluded from income/expense (own-account moves).
        </div>
      )}

      <CategoryBreakdown totals={data.categoryTotals} />
    </section>
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

function CategoryBreakdown({ totals }: { totals: CategoryTotal[] }) {
  const expenses = totals.filter((t) => t.kind === "expense" && Number.parseFloat(t.totalDebit) > 0);
  const income = totals.filter((t) => t.kind === "income");

  if (expenses.length === 0 && income.length === 0) {
    return null;
  }

  const expenseMax = expenses.reduce(
    (m, t) => Math.max(m, Number.parseFloat(t.totalDebit) || 0),
    0,
  );

  return (
    <div className="card dash-category-card">
      {expenses.length > 0 && (
        <>
          <h3>Where the money went</h3>
          <ul className="breakdown-list">
            {expenses.map((t) => {
              const amt = Number.parseFloat(t.totalDebit) || 0;
              const pct = expenseMax > 0 ? (amt / expenseMax) * 100 : 0;
              return (
                <li key={t.category} className="breakdown-row">
                  <div className="bk-name">{t.category}</div>
                  <div className="bk-bar" aria-hidden>
                    <div className="bk-bar-fill" style={{ width: `${pct}%` }} />
                  </div>
                  <div className="bk-count muted">
                    {t.count} {t.count === 1 ? "txn" : "txns"}
                  </div>
                  <div className="bk-amount">₹{fmtINR(t.totalDebit)}</div>
                </li>
              );
            })}
          </ul>
        </>
      )}

      {income.length > 0 && (
        <>
          <h3 className="bk-heading-secondary">Money in</h3>
          <ul className="breakdown-list">
            {income.map((t) => (
              <li key={t.category} className="breakdown-row credit">
                <div className="bk-name">{t.category}</div>
                <div className="bk-bar" aria-hidden>
                  <div className="bk-bar-fill credit" style={{ width: "100%" }} />
                </div>
                <div className="bk-count muted">
                  {t.count} {t.count === 1 ? "txn" : "txns"}
                </div>
                <div className="bk-amount credit">₹{fmtINR(t.totalCredit)}</div>
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  );
}
