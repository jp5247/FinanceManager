import { useCallback, useEffect, useMemo, useState } from "react";
import {
  listTransactionsByMonth,
  recategorizeAllImports,
  recategorizeTransaction,
} from "../ipc";
import type { NewRuleSpec, RawTransaction } from "../types";
import { UNCATEGORIZED } from "../categories";
import { RecategorizeModal } from "./RecategorizeModal";

const inrFormatter = new Intl.NumberFormat("en-IN", {
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
});

function fmtINR(decimalStr: string | number): string {
  const n = typeof decimalStr === "number" ? decimalStr : Number.parseFloat(decimalStr);
  if (!Number.isFinite(n)) return String(decimalStr);
  return inrFormatter.format(n);
}

/** Mirror of `classify_category` in dashboard.rs. Keep in sync with E14. */
function classify(category: string): "income" | "expense" | "transfer" | "investment" {
  const n = category.trim().toLowerCase();
  if (["salary", "dividend", "interest", "refund", "bonus", "cashback"].includes(n)) {
    return "income";
  }
  if (["credit card payment", "cc payment", "bank transfer"].includes(n)) {
    return "transfer";
  }
  if (
    [
      "investments",
      "investment",
      "sip",
      "sips",
      "mutual fund",
      "mutual funds",
      "mf",
      "elss",
      "ppf",
      "nps",
      "equity",
      "stocks",
      "fixed deposit",
      "fd",
      "recurring deposit",
      "rd",
    ].includes(n)
  ) {
    return "investment";
  }
  return "expense";
}

interface Summary {
  income: number;
  expense: number;
  net: number;
  incomeByCategory: { category: string; total: number }[];
  expenseByCategory: { category: string; total: number; debit: number; credit: number }[];
  transferTotal: number;
}

function summarise(rows: RawTransaction[]): Summary {
  // Per-category, per-direction aggregation — matches the backend's
  // accounting rule so the modal totals equal the trend tile.
  const cat: Map<string, { debit: number; credit: number }> = new Map();
  for (const r of rows) {
    const c = r.category ?? "Uncategorized";
    const acc = cat.get(c) ?? { debit: 0, credit: 0 };
    acc.debit += Number.parseFloat(r.debit ?? "0") || 0;
    acc.credit += Number.parseFloat(r.credit ?? "0") || 0;
    cat.set(c, acc);
  }

  let income = 0;
  let expense = 0;
  let transferTotal = 0;
  const incomeByCategory: { category: string; total: number }[] = [];
  const expenseByCategory: {
    category: string;
    total: number;
    debit: number;
    credit: number;
  }[] = [];

  for (const [category, acc] of cat) {
    const kind = classify(category);
    if (kind === "income") {
      income += acc.credit;
      if (acc.credit > 0) incomeByCategory.push({ category, total: acc.credit });
    } else if (kind === "expense") {
      const net = Math.max(0, acc.debit - acc.credit);
      expense += net;
      if (net > 0 || acc.debit > 0 || acc.credit > 0) {
        expenseByCategory.push({
          category,
          total: net,
          debit: acc.debit,
          credit: acc.credit,
        });
      }
    } else if (kind === "transfer") {
      transferTotal += acc.debit + acc.credit;
    }
    // investment kind is shown in its own group on the dashboard; for the
    // monthly drill modal we fold investment-kind rows into the expense
    // table so the user sees every outflow in one place — but it does
    // NOT count toward `expense`.
  }

  incomeByCategory.sort((a, b) => b.total - a.total);
  expenseByCategory.sort((a, b) => b.total - a.total);

  return {
    income,
    expense,
    net: income - expense,
    incomeByCategory,
    expenseByCategory,
    transferTotal,
  };
}

interface Props {
  month: string;
  onClose: () => void;
  onChanged: () => void;
}

export function MonthDrillModal({ month, onClose, onChanged }: Props) {
  const [rows, setRows] = useState<RawTransaction[] | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [editing, setEditing] = useState<RawTransaction | null>(null);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [retroBusy, setRetroBusy] = useState(false);
  const [retroNote, setRetroNote] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setRows(await listTransactionsByMonth(month));
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [month]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const summary = useMemo(() => (rows ? summarise(rows) : null), [rows]);

  const submit = async (newCategory: string, saveAsRule: NewRuleSpec | null) => {
    if (!editing) return;
    setSaving(true);
    setSaveError(null);
    try {
      await recategorizeTransaction(
        editing.importId,
        editing.rowNumber,
        newCategory,
        saveAsRule,
      );
      setEditing(null);
      if (saveAsRule) {
        setRetroBusy(true);
        setRetroNote(null);
        try {
          const r = await recategorizeAllImports();
          setRetroNote(
            `Rule saved. Re-categorized ${r.touched} of ${r.total} import${r.total === 1 ? "" : "s"} retroactively.`,
          );
        } catch (e) {
          setRetroNote(`Rule saved, but retroactive re-categorize failed: ${String(e)}`);
        } finally {
          setRetroBusy(false);
        }
      }
      await refresh();
      onChanged();
    } catch (e) {
      setSaveError(String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="card category-drill-modal"
        onClick={(e) => e.stopPropagation()}
      >
        <header className="drill-header">
          <div>
            <h3>
              Sources for <span className="category-chip">{month}</span>
            </h3>
            <p className="muted small">
              Every transaction in <strong>{month}</strong> across all uploaded
              statements. The category totals below sum to the IN / OUT figures
              shown on the dashboard trend. Click a category chip to
              recategorize a row.
            </p>
          </div>
          <button className="btn btn-link inline" onClick={onClose}>
            Close
          </button>
        </header>

        {retroBusy && (
          <div className="auto-recat-toast info">
            Re-categorizing transactions across every statement…
          </div>
        )}
        {retroNote && !retroBusy && (
          <div className="auto-recat-toast">{retroNote}</div>
        )}

        {loading ? (
          <p className="muted">Loading…</p>
        ) : error ? (
          <div className="error-text">{error}</div>
        ) : !rows || rows.length === 0 ? (
          <p className="muted">
            No transactions in <strong>{month}</strong>.
          </p>
        ) : (
          <>
            {summary && <MonthSummaryPanel summary={summary} />}
            <DrillTable
              rows={rows}
              onEditCategory={setEditing}
              disabled={retroBusy || saving}
            />
          </>
        )}

        {editing && (
          <RecategorizeModal
            row={editing}
            saving={saving}
            error={saveError}
            onSave={submit}
            onCancel={() => {
              setEditing(null);
              setSaveError(null);
            }}
          />
        )}
      </div>
    </div>
  );
}

function MonthSummaryPanel({ summary }: { summary: Summary }) {
  return (
    <div className="month-summary">
      <div className="month-summary-row">
        <div className="month-summary-side income">
          <div className="month-summary-head">
            <span className="month-summary-label">Income sources</span>
            <span className="month-summary-total credit">
              ₹{fmtINR(summary.income)}
            </span>
          </div>
          {summary.incomeByCategory.length === 0 ? (
            <p className="muted xsmall">No income rows in this month.</p>
          ) : (
            <ul className="month-summary-list">
              {summary.incomeByCategory.map((c) => (
                <li key={c.category}>
                  <span className="month-summary-cat">{c.category}</span>
                  <span className="month-summary-amt credit">
                    +₹{fmtINR(c.total)}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </div>
        <div className="month-summary-side expense">
          <div className="month-summary-head">
            <span className="month-summary-label">Expense sources</span>
            <span className="month-summary-total debit">
              ₹{fmtINR(summary.expense)}
            </span>
          </div>
          {summary.expenseByCategory.length === 0 ? (
            <p className="muted xsmall">No expense rows in this month.</p>
          ) : (
            <ul className="month-summary-list">
              {summary.expenseByCategory.map((c) => (
                <li key={c.category}>
                  <span className="month-summary-cat">{c.category}</span>
                  {c.credit > 0 && c.debit > 0 ? (
                    <span className="muted xsmall month-summary-note">
                      ₹{fmtINR(c.debit)} spent − ₹{fmtINR(c.credit)} refund
                    </span>
                  ) : null}
                  <span className="month-summary-amt debit">
                    −₹{fmtINR(c.total)}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </div>
      </div>
      <div className={`month-summary-net ${summary.net >= 0 ? "credit" : "debit"}`}>
        Net this month: {summary.net >= 0 ? "+" : "−"}₹
        {fmtINR(Math.abs(summary.net))}
        {summary.transferTotal > 0 && (
          <span className="muted small">
            {" "}
            · ₹{fmtINR(summary.transferTotal)} in own-account transfers
            excluded
          </span>
        )}
      </div>
    </div>
  );
}

function DrillTable({
  rows,
  onEditCategory,
  disabled,
}: {
  rows: RawTransaction[];
  onEditCategory: (row: RawTransaction) => void;
  disabled: boolean;
}) {
  return (
    <div className="txn-table-wrapper drill-table-wrapper">
      <table className="txn-table">
        <thead>
          <tr>
            <th>Date</th>
            <th>Source</th>
            <th>Description</th>
            <th>Category</th>
            <th className="num">Debit</th>
            <th className="num">Credit</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <tr key={`${r.importId}-${r.rowNumber}`}>
              <td className="mono">{r.txnDate}</td>
              <td className="mono small muted">{r.sourceFile}</td>
              <td>{r.description}</td>
              <td>
                <button
                  type="button"
                  className={`category-chip-btn ${r.category ? "" : "uncategorized"}`}
                  title={
                    r.categoryRuleId
                      ? `rule: ${r.categoryRuleId} · click to change`
                      : "click to assign a category"
                  }
                  onClick={() => onEditCategory(r)}
                  disabled={disabled}
                >
                  {r.category ?? UNCATEGORIZED}
                </button>
              </td>
              <td className="num">{r.debit ? fmtINR(r.debit) : ""}</td>
              <td className="num credit">{r.credit ? fmtINR(r.credit) : ""}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
