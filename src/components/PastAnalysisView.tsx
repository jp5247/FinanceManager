import { useCallback, useEffect, useState } from "react";
import { dashboardAggregate } from "../ipc";
import type { DashboardData } from "../types";

const inrFormatter = new Intl.NumberFormat("en-IN", {
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
});

function fmtINR(decimalStr: string): string {
  const n = Number.parseFloat(decimalStr);
  if (!Number.isFinite(n)) return decimalStr;
  return inrFormatter.format(n);
}

function clampPct(pct: number): number {
  if (!Number.isFinite(pct)) return 0;
  return Math.max(0, Math.min(100, pct));
}

/** Pull available `YYYY-MM` months from a default-range aggregate so the
 * picker options reflect the user's actual data, not an arbitrary list. */
function useAvailableMonths(): string[] {
  const [months, setMonths] = useState<string[]>([]);
  useEffect(() => {
    void (async () => {
      try {
        const d = await dashboardAggregate();
        setMonths(d.monthlyTrend.map((b) => b.month).sort());
      } catch {
        // ignore — picker will just be empty
      }
    })();
  }, []);
  return months;
}

export function PastAnalysisView() {
  const months = useAvailableMonths();
  const [fromMonth, setFromMonth] = useState<string>("");
  const [toMonth, setToMonth] = useState<string>("");
  const [compareMode, setCompareMode] = useState(false);

  // Initialize the range to the oldest data → second-most-recent month once
  // months are known. The "current" dashboard owns the last 1–2 months.
  useEffect(() => {
    if (months.length > 0 && !fromMonth && !toMonth) {
      setFromMonth(months[0]);
      // Default upper bound: a couple months back if we have enough data.
      const end = months.length > 2 ? months[months.length - 3] : months[months.length - 1];
      setToMonth(end);
    }
  }, [months, fromMonth, toMonth]);

  if (months.length === 0) {
    return (
      <section className="dashboard-view">
        <div className="card">
          <h2>Past Analysis</h2>
          <p className="muted">
            No statements uploaded yet. Once you have at least one upload,
            this tab will let you focus on a specific month-range and compare
            two months side by side.
          </p>
        </div>
      </section>
    );
  }

  return (
    <section className="dashboard-view">
      <header className="dash-header">
        <div>
          <h2>Past Analysis</h2>
          <p className="muted small">
            Pick a month-range to focus on, or compare any two months. The
            same headline numbers and category breakdown as the Dashboard,
            but filtered to the period you choose.
          </p>
        </div>
      </header>

      <div className="card range-picker-card">
        <div className="range-picker-row">
          <label>
            <span className="muted xsmall">FROM</span>
            <select value={fromMonth} onChange={(e) => setFromMonth(e.target.value)}>
              {months.map((m) => (
                <option key={m} value={m}>{m}</option>
              ))}
            </select>
          </label>
          <label>
            <span className="muted xsmall">TO</span>
            <select value={toMonth} onChange={(e) => setToMonth(e.target.value)}>
              {months.map((m) => (
                <option key={m} value={m}>{m}</option>
              ))}
            </select>
          </label>
          <label className="check-row">
            <input
              type="checkbox"
              checked={compareMode}
              onChange={(e) => setCompareMode(e.target.checked)}
            />
            <span>Compare two months side by side</span>
          </label>
        </div>
      </div>

      {compareMode ? (
        <CompareView months={months} />
      ) : (
        <PeriodView
          fromMonth={fromMonth || months[0]}
          toMonth={toMonth || months[months.length - 1]}
        />
      )}
    </section>
  );
}

function PeriodView({ fromMonth, toMonth }: { fromMonth: string; toMonth: string }) {
  const [data, setData] = useState<DashboardData | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setData(await dashboardAggregate(fromMonth, toMonth));
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [fromMonth, toMonth]);

  useEffect(() => {
    void load();
  }, [load]);

  if (loading) return <div className="card"><p className="muted">Loading…</p></div>;
  if (error) return <div className="error-text">{error}</div>;
  if (!data) return null;
  if (data.transactionCount === 0) {
    return (
      <div className="card">
        <p className="muted">
          No transactions in <strong>{fromMonth} → {toMonth}</strong>. Try a
          wider range.
        </p>
      </div>
    );
  }
  return <DashboardLite data={data} />;
}

function CompareView({ months }: { months: string[] }) {
  const initial = months.length >= 2
    ? [months[months.length - 2], months[months.length - 1]] as const
    : [months[0], months[0]] as const;
  const [monthA, setMonthA] = useState<string>(initial[0]);
  const [monthB, setMonthB] = useState<string>(initial[1]);
  const [a, setA] = useState<DashboardData | null>(null);
  const [b, setB] = useState<DashboardData | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    void (async () => {
      setLoading(true);
      try {
        const [da, db] = await Promise.all([
          dashboardAggregate(monthA, monthA),
          dashboardAggregate(monthB, monthB),
        ]);
        setA(da);
        setB(db);
      } finally {
        setLoading(false);
      }
    })();
  }, [monthA, monthB]);

  if (loading) return <div className="card"><p className="muted">Loading comparison…</p></div>;
  if (!a || !b) return null;

  return (
    <div className="compare-grid">
      <div className="compare-side">
        <header className="dash-header">
          <h3>
            <select value={monthA} onChange={(e) => setMonthA(e.target.value)}>
              {months.map((m) => (
                <option key={m} value={m}>{m}</option>
              ))}
            </select>
          </h3>
        </header>
        <DashboardLite data={a} />
      </div>
      <div className="compare-side">
        <header className="dash-header">
          <h3>
            <select value={monthB} onChange={(e) => setMonthB(e.target.value)}>
              {months.map((m) => (
                <option key={m} value={m}>{m}</option>
              ))}
            </select>
          </h3>
        </header>
        <DashboardLite data={b} />
      </div>
      <CompareDeltas a={a} b={b} monthA={monthA} monthB={monthB} />
    </div>
  );
}

function CompareDeltas({
  a,
  b,
  monthA,
  monthB,
}: {
  a: DashboardData;
  b: DashboardData;
  monthA: string;
  monthB: string;
}) {
  const iA = Number.parseFloat(a.totalIncome) || 0;
  const iB = Number.parseFloat(b.totalIncome) || 0;
  const eA = Number.parseFloat(a.totalExpense) || 0;
  const eB = Number.parseFloat(b.totalExpense) || 0;
  const nA = Number.parseFloat(a.netSavings) || 0;
  const nB = Number.parseFloat(b.netSavings) || 0;

  const deltaRow = (label: string, va: number, vb: number, positiveIsGood: boolean) => {
    const diff = vb - va;
    const direction = diff >= 0 ? "+" : "−";
    const good = positiveIsGood ? diff >= 0 : diff <= 0;
    const cls = diff === 0 ? "muted" : good ? "credit" : "debit";
    return (
      <tr>
        <td>{label}</td>
        <td className="num">₹{fmtINR(va.toFixed(2))}</td>
        <td className="num">₹{fmtINR(vb.toFixed(2))}</td>
        <td className={`num ${cls}`}>
          {direction}₹{fmtINR(Math.abs(diff).toFixed(2))}
        </td>
      </tr>
    );
  };

  return (
    <div className="card compare-deltas">
      <h3>Change</h3>
      <table className="txn-table">
        <thead>
          <tr>
            <th></th>
            <th className="num">{monthA}</th>
            <th className="num">{monthB}</th>
            <th className="num">Δ</th>
          </tr>
        </thead>
        <tbody>
          {deltaRow("Income", iA, iB, true)}
          {deltaRow("Expense", eA, eB, false)}
          {deltaRow("Net savings", nA, nB, true)}
        </tbody>
      </table>
    </div>
  );
}

/** Stripped-down dashboard rendering reused by Past Analysis (no
 *  click-to-drill, no health strip — those belong on the live Dashboard). */
function DashboardLite({ data }: { data: DashboardData }) {
  const income = Number.parseFloat(data.totalIncome) || 0;
  const net = Number.parseFloat(data.netSavings) || 0;
  const savingsRate = income > 0 ? (net / income) * 100 : 0;
  return (
    <>
      <div className="summary-tiles" role="group">
        <div className="summary-tile tile-credit">
          <div className="tile-label">Income</div>
          <div className="tile-amount">
            <span className="tile-currency">₹</span>
            {fmtINR(data.totalIncome)}
          </div>
        </div>
        <div className="summary-tile tile-debit">
          <div className="tile-label">Expense</div>
          <div className="tile-amount">
            <span className="tile-currency">₹</span>
            {fmtINR(data.totalExpense)}
          </div>
        </div>
        <div className={`summary-tile tile-net ${net >= 0 ? "net-positive" : "net-negative"}`}>
          <div className="tile-label">Net</div>
          <div className="tile-amount">
            {net >= 0 ? "+" : "−"}
            <span className="tile-currency">₹</span>
            {fmtINR(Math.abs(net).toFixed(2))}
          </div>
          <div className="tile-sub muted">
            {income > 0 ? `${savingsRate.toFixed(1)}% saved` : "no income"}
          </div>
        </div>
      </div>

      {data.monthlyTrend.length > 1 && (
        <MiniTrend trend={data.monthlyTrend} />
      )}

      <BreakdownLite data={data} />
    </>
  );
}

function MiniTrend({ trend }: { trend: DashboardData["monthlyTrend"] }) {
  const max = trend.reduce(
    (m, b) => Math.max(m, Number.parseFloat(b.income) || 0, Number.parseFloat(b.expense) || 0),
    0,
  );
  if (max <= 0) return null;
  return (
    <div className="card monthly-trend-card">
      <h3>Per-month within selection</h3>
      <ul className="trend-list">
        {trend.map((b) => {
          const inc = Number.parseFloat(b.income) || 0;
          const exp = Number.parseFloat(b.expense) || 0;
          const net = Number.parseFloat(b.net) || 0;
          return (
            <li key={b.month} className="trend-row">
              <div className="trend-row-btn" style={{ cursor: "default" }}>
                <div className="trend-month mono">{b.month}</div>
                <div className="trend-bars">
                  <div className="trend-bar-row">
                    <span className="trend-bar-label muted xsmall">In</span>
                    <div className="trend-bar-track">
                      <div className="trend-bar-fill income" style={{ width: `${clampPct((inc / max) * 100)}%` }} />
                    </div>
                    <span className="trend-bar-amount credit">₹{fmtINR(b.income)}</span>
                  </div>
                  <div className="trend-bar-row">
                    <span className="trend-bar-label muted xsmall">Out</span>
                    <div className="trend-bar-track">
                      <div className="trend-bar-fill expense" style={{ width: `${clampPct((exp / max) * 100)}%` }} />
                    </div>
                    <span className="trend-bar-amount debit">₹{fmtINR(b.expense)}</span>
                  </div>
                </div>
                <div className={`trend-net ${net >= 0 ? "credit" : "debit"}`}>
                  <div className="trend-net-label muted xsmall">Net</div>
                  <div>{net >= 0 ? "+" : "−"}₹{fmtINR(Math.abs(net).toFixed(2))}</div>
                </div>
              </div>
            </li>
          );
        })}
      </ul>
    </div>
  );
}

function BreakdownLite({ data }: { data: DashboardData }) {
  const expenses = data.categoryTotals.filter(
    (t) => t.kind === "expense" && Number.parseFloat(t.totalDebit) > 0,
  );
  if (expenses.length === 0) return null;
  const total = expenses.reduce(
    (s, t) => s + (Number.parseFloat(t.totalDebit) || 0),
    0,
  );
  return (
    <div className="card dash-category-card">
      <h3>Where the money went</h3>
      <ul className="breakdown-list">
        {expenses.map((t) => {
          const amt = Number.parseFloat(t.totalDebit) || 0;
          const pct = total > 0 ? clampPct((amt / total) * 100) : 0;
          return (
            <li key={t.category} className="breakdown-row">
              <div className="breakdown-row-btn" style={{ cursor: "default" }}>
                <span className="bk-name">{t.category}</span>
                <span className="bk-bar" aria-hidden>
                  <span className="bk-bar-fill expense" style={{ width: `${pct}%` }} />
                </span>
                <span className="bk-pct">{pct.toFixed(0)}%</span>
                <span className="bk-count muted">
                  {t.count} {t.count === 1 ? "txn" : "txns"}
                </span>
                <span className="bk-amount bk-amount-expense">₹{fmtINR(t.totalDebit)}</span>
              </div>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
