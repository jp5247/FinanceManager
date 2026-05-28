import { type FormEvent, useCallback, useEffect, useState } from "react";
import {
  deleteLoan,
  listLoans,
  loansSummary,
  upsertLoan,
} from "../ipc";
import type { Loan, LoansSummary, UpsertLoanSpec } from "../types";

const inrFormatter = new Intl.NumberFormat("en-IN", {
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
});

function fmtINR(decimalStr: string | number): string {
  const n = typeof decimalStr === "number" ? decimalStr : Number.parseFloat(decimalStr);
  if (!Number.isFinite(n)) return String(decimalStr);
  return inrFormatter.format(n);
}

const LOAN_TYPES: readonly string[] = [
  "Home",
  "Car",
  "Personal",
  "Education",
  "Credit Card",
  "Business",
  "Gold Loan",
  "Other",
];

export function LoansView() {
  const [loans, setLoans] = useState<Loan[]>([]);
  const [summary, setSummary] = useState<LoansSummary | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [editing, setEditing] = useState<Loan | null>(null);
  const [showForm, setShowForm] = useState(false);
  const [strategy, setStrategy] = useState<"avalanche" | "snowball">("avalanche");

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [l, s] = await Promise.all([listLoans(), loansSummary()]);
      setLoans(l);
      setSummary(s);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const onSaved = async () => {
    setShowForm(false);
    setEditing(null);
    await refresh();
  };

  const onDelete = async (id: string) => {
    if (!window.confirm("Delete this loan? This can't be undone.")) return;
    try {
      await deleteLoan(id);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  if (loading) {
    return (
      <section className="loans-view">
        <div className="card"><p className="muted">Loading loans…</p></div>
      </section>
    );
  }

  return (
    <section className="loans-view">
      <header className="dash-header">
        <div>
          <h2>Loans</h2>
          <p className="muted small">
            Track outstanding loans, see good/bad classification per loan, and
            pick a prepayment strategy. Updates the Dashboard's Debt-Burden
            driver in real time.
          </p>
        </div>
        <button
          className="btn btn-primary btn-sm"
          onClick={() => {
            setEditing(null);
            setShowForm(true);
          }}
        >
          + Add loan
        </button>
      </header>

      {error && <div className="error-text">{error}</div>}

      {summary && summary.loanCount > 0 && (
        <SummaryPanel summary={summary} />
      )}

      {showForm || editing ? (
        <LoanForm
          existing={editing}
          onCancel={() => {
            setShowForm(false);
            setEditing(null);
          }}
          onSaved={onSaved}
        />
      ) : null}

      {summary && summary.loanCount === 0 && !showForm && (
        <div className="card">
          <p className="muted">
            No loans entered yet. Click <strong>+ Add loan</strong> to start.
            Once you have at least one loan, the Dashboard's Debt-Burden driver
            will compute a real score (instead of the neutral placeholder) and
            the prepayment strategies will surface here.
          </p>
        </div>
      )}

      {summary && summary.loanCount > 0 && (
        <LoansTable
          loans={loans}
          summary={summary}
          strategy={strategy}
          onStrategyChange={setStrategy}
          onEdit={(l) => {
            setEditing(l);
            setShowForm(true);
          }}
          onDelete={onDelete}
        />
      )}
    </section>
  );
}

function SummaryPanel({ summary }: { summary: LoansSummary }) {
  return (
    <div className="summary-tiles" role="group" aria-label="Loans summary">
      <div className="summary-tile tile-debit">
        <div className="tile-label">Total outstanding</div>
        <div className="tile-amount">
          <span className="tile-currency">₹</span>
          {fmtINR(summary.totalOutstanding)}
        </div>
        <div className="tile-sub muted">
          across {summary.loanCount} loan{summary.loanCount === 1 ? "" : "s"}
        </div>
      </div>
      <div className="summary-tile">
        <div className="tile-label">Monthly EMIs</div>
        <div className="tile-amount">
          <span className="tile-currency">₹</span>
          {fmtINR(summary.totalMonthlyEmi)}
        </div>
        <div className="tile-sub muted">recurring outflow</div>
      </div>
      <div className="summary-tile">
        <div className="tile-label">Weighted avg rate</div>
        <div className="tile-amount">{summary.weightedAvgRate}%</div>
        <div className="tile-sub muted">balance-weighted across loans</div>
      </div>
    </div>
  );
}

interface FormProps {
  existing: Loan | null;
  onCancel: () => void;
  onSaved: () => void | Promise<void>;
}

function LoanForm({ existing, onCancel, onSaved }: FormProps) {
  const [loanType, setLoanType] = useState(existing?.loanType ?? LOAN_TYPES[0]);
  const [customType, setCustomType] = useState(
    existing && !LOAN_TYPES.includes(existing.loanType) ? existing.loanType : "",
  );
  const [useCustom, setUseCustom] = useState(
    existing ? !LOAN_TYPES.includes(existing.loanType) : false,
  );
  const [lender, setLender] = useState(existing?.lender ?? "");
  const [principal, setPrincipal] = useState(existing?.principalOutstanding ?? "");
  const [rate, setRate] = useState(existing?.interestRate ?? "");
  const [rateType, setRateType] = useState(existing?.rateType ?? "Floating");
  const [tenure, setTenure] = useState(existing?.remainingTenureMonths?.toString() ?? "");
  const [emi, setEmi] = useState(existing?.emi ?? "");
  const [penalty, setPenalty] = useState(existing?.prepaymentPenaltyPct ?? "0");
  const [taxBenefit, setTaxBenefit] = useState(existing?.taxBenefit ?? false);
  const [startDate, setStartDate] = useState(existing?.startDate ?? "");
  const [nextDueDate, setNextDueDate] = useState(existing?.nextDueDate ?? "");
  const [notes, setNotes] = useState(existing?.notes ?? "");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setSaving(true);
    setError(null);
    try {
      const spec: UpsertLoanSpec = {
        id: existing?.id,
        loanType: useCustom ? customType.trim() : loanType,
        lender: lender.trim(),
        principalOutstanding: principal,
        interestRate: rate,
        rateType,
        remainingTenureMonths: Number.parseInt(tenure, 10) || 0,
        emi,
        prepaymentPenaltyPct: penalty,
        taxBenefit,
        startDate,
        nextDueDate,
        notes: notes.trim() || null,
      };
      await upsertLoan(spec);
      await onSaved();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <form className="card asset-form" onSubmit={submit}>
      <h3>{existing ? "Edit loan" : "Add loan"}</h3>
      <div className="asset-form-grid">
        <label>
          <span>Loan type</span>
          <select
            value={useCustom ? "__custom__" : loanType}
            onChange={(e) => {
              if (e.target.value === "__custom__") {
                setUseCustom(true);
              } else {
                setUseCustom(false);
                setLoanType(e.target.value);
              }
            }}
            disabled={saving}
          >
            {LOAN_TYPES.map((t) => (
              <option key={t} value={t}>{t}</option>
            ))}
            <option value="__custom__">Custom…</option>
          </select>
          {useCustom && (
            <input
              type="text"
              value={customType}
              onChange={(e) => setCustomType(e.target.value)}
              placeholder="e.g. Two-wheeler, Family loan"
              disabled={saving}
            />
          )}
        </label>
        <label>
          <span>Lender</span>
          <input
            type="text"
            value={lender}
            onChange={(e) => setLender(e.target.value)}
            placeholder="e.g. HDFC Bank, SBI"
            disabled={saving}
            required
          />
        </label>
        <label>
          <span>Principal outstanding (₹)</span>
          <input
            type="text"
            value={principal}
            onChange={(e) => setPrincipal(e.target.value)}
            placeholder="e.g. 25,00,000.00"
            disabled={saving}
            required
            inputMode="decimal"
          />
        </label>
        <label>
          <span>Interest rate (% / yr)</span>
          <input
            type="text"
            value={rate}
            onChange={(e) => setRate(e.target.value)}
            placeholder="e.g. 8.5"
            disabled={saving}
            required
            inputMode="decimal"
          />
        </label>
        <label>
          <span>Rate type</span>
          <select
            value={rateType}
            onChange={(e) => setRateType(e.target.value)}
            disabled={saving}
          >
            <option value="Floating">Floating</option>
            <option value="Fixed">Fixed</option>
          </select>
        </label>
        <label>
          <span>Remaining tenure (months)</span>
          <input
            type="number"
            value={tenure}
            onChange={(e) => setTenure(e.target.value)}
            placeholder="e.g. 240"
            min={0}
            disabled={saving}
            required
          />
        </label>
        <label>
          <span>EMI (₹ / month)</span>
          <input
            type="text"
            value={emi}
            onChange={(e) => setEmi(e.target.value)}
            placeholder="e.g. 22,500.00"
            disabled={saving}
            required
            inputMode="decimal"
          />
        </label>
        <label>
          <span>Prepayment penalty (%)</span>
          <input
            type="text"
            value={penalty}
            onChange={(e) => setPenalty(e.target.value)}
            placeholder="0 if none"
            disabled={saving}
            inputMode="decimal"
          />
        </label>
        <label>
          <span>Start date</span>
          <input
            type="date"
            value={startDate}
            onChange={(e) => setStartDate(e.target.value)}
            disabled={saving}
            required
          />
        </label>
        <label>
          <span>Next due date</span>
          <input
            type="date"
            value={nextDueDate}
            onChange={(e) => setNextDueDate(e.target.value)}
            disabled={saving}
            required
          />
        </label>
        <label className="check-row" style={{ gridColumn: "1 / -1" }}>
          <input
            type="checkbox"
            checked={taxBenefit}
            onChange={(e) => setTaxBenefit(e.target.checked)}
            disabled={saving}
          />
          <span>
            Tax-deductible (Home Loan §24/80EEA, Education Loan §80E, etc.) —
            shaves ~2.5pp off the effective rate for classification.
          </span>
        </label>
        <label className="asset-form-notes">
          <span>Notes (optional)</span>
          <input
            type="text"
            value={notes ?? ""}
            onChange={(e) => setNotes(e.target.value)}
            placeholder="e.g. ICICI HOMEXX1234, switched from fixed in Jan 2025"
            disabled={saving}
          />
        </label>
      </div>

      {error && <div className="error-text">{error}</div>}

      <div className="row">
        <span className="row-spacer" />
        <button
          type="button"
          className="btn btn-secondary"
          onClick={onCancel}
          disabled={saving}
        >
          Cancel
        </button>
        <button
          type="submit"
          className="btn btn-primary"
          disabled={
            saving ||
            lender.trim().length === 0 ||
            principal.trim().length === 0 ||
            rate.trim().length === 0 ||
            tenure.trim().length === 0 ||
            emi.trim().length === 0 ||
            startDate.length === 0 ||
            nextDueDate.length === 0 ||
            (useCustom && customType.trim().length === 0)
          }
        >
          {saving ? "Saving…" : existing ? "Update" : "Add"}
        </button>
      </div>
    </form>
  );
}

interface TableProps {
  loans: Loan[];
  summary: LoansSummary;
  strategy: "avalanche" | "snowball";
  onStrategyChange: (s: "avalanche" | "snowball") => void;
  onEdit: (l: Loan) => void;
  onDelete: (id: string) => void | Promise<void>;
}

function LoansTable({
  loans,
  summary,
  strategy,
  onStrategyChange,
  onEdit,
  onDelete,
}: TableProps) {
  const byId = new Map(loans.map((l) => [l.id, l]));
  const order =
    strategy === "avalanche" ? summary.avalancheOrder : summary.snowballOrder;
  const ordered = order
    .map((id) => byId.get(id))
    .filter((l): l is Loan => Boolean(l));
  const classifications = new Map(
    summary.classifications.map((c) => [c.loanId, c]),
  );

  return (
    <div className="card">
      <div className="loans-strategy-row">
        <h3>Prepayment priority</h3>
        <div className="loans-strategy-toggle">
          <button
            type="button"
            className={`btn btn-secondary btn-sm ${strategy === "avalanche" ? "active" : ""}`}
            onClick={() => onStrategyChange("avalanche")}
          >
            Avalanche (highest rate)
          </button>
          <button
            type="button"
            className={`btn btn-secondary btn-sm ${strategy === "snowball" ? "active" : ""}`}
            onClick={() => onStrategyChange("snowball")}
          >
            Snowball (smallest balance)
          </button>
        </div>
      </div>
      <p className="muted small">
        {strategy === "avalanche"
          ? "Pay the highest-rate loan down first while keeping minimum EMIs on the others. Mathematically optimal — minimises total interest paid."
          : "Pay the smallest balance down first to clear loans quickly. Psychologically motivating — feels like faster progress."}
      </p>

      <div className="txn-table-wrapper">
        <table className="txn-table">
          <thead>
            <tr>
              <th>Priority</th>
              <th>Type</th>
              <th>Lender</th>
              <th className="num">Outstanding</th>
              <th className="num">Rate (eff)</th>
              <th className="num">EMI</th>
              <th>Verdict</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {ordered.map((l, idx) => {
              const c = classifications.get(l.id);
              return (
                <tr key={l.id}>
                  <td className="mono">#{idx + 1}</td>
                  <td>{l.loanType}</td>
                  <td>
                    {l.lender}
                    {l.notes && <div className="muted xsmall">{l.notes}</div>}
                  </td>
                  <td className="num">₹{fmtINR(l.principalOutstanding)}</td>
                  <td className="num">
                    {l.interestRate}%
                    {c && c.effectiveRate !== l.interestRate && (
                      <div className="muted xsmall">eff {c.effectiveRate}%</div>
                    )}
                  </td>
                  <td className="num">₹{fmtINR(l.emi)}</td>
                  <td>
                    {c && (
                      <span
                        className={`loan-verdict loan-verdict-${c.verdict}`}
                        title={c.rationale}
                      >
                        {c.verdict}
                      </span>
                    )}
                  </td>
                  <td>
                    <button
                      type="button"
                      className="btn btn-link inline"
                      onClick={() => onEdit(l)}
                    >
                      Edit
                    </button>
                    <button
                      type="button"
                      className="btn btn-link inline danger"
                      onClick={() => void onDelete(l.id)}
                    >
                      Delete
                    </button>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>

      <div className="loans-rationale-list">
        {ordered.map((l) => {
          const c = classifications.get(l.id);
          if (!c) return null;
          return (
            <div key={l.id} className={`loans-rationale-row loan-verdict-${c.verdict}`}>
              <strong>
                {l.loanType} · {l.lender}
              </strong>
              <span className="muted small">{c.rationale}</span>
            </div>
          );
        })}
      </div>
    </div>
  );
}
