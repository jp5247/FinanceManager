import { useCallback, useEffect, useState } from "react";
import {
  listTransactionsByCategory,
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

function fmtINR(decimalStr: string): string {
  const n = Number.parseFloat(decimalStr);
  if (!Number.isFinite(n)) return decimalStr;
  return inrFormatter.format(n);
}

interface Props {
  category: string;
  onClose: () => void;
  /** Called after any successful recategorization so the dashboard refreshes. */
  onChanged: () => void;
}

export function CategoryDrillModal({ category, onClose, onChanged }: Props) {
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
      setRows(await listTransactionsByCategory(category));
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [category]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

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
      // If a rule was saved, retroactively re-run categorization across
      // every import so the rule applies to historical statements too.
      if (saveAsRule) {
        setRetroBusy(true);
        setRetroNote(null);
        try {
          const r = await recategorizeAllImports();
          const main = `Rule saved. Re-categorized ${r.touched} of ${r.total} import${r.total === 1 ? "" : "s"} retroactively.`;
          setRetroNote(
            r.skipped > 0
              ? `${main} (${r.skipped} skipped — check dev logs for details.)`
              : main,
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
              Transactions in <span className="category-chip">{category}</span>
            </h3>
            <p className="muted small">
              Across every uploaded statement. Click a category chip to
              recategorize the row.
              {" "}
              <strong>Save as rule</strong> applies the change retroactively to
              all matching rows in your history.
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
            No transactions are currently classified as{" "}
            <strong>{category}</strong>.
          </p>
        ) : (
          <DrillTable
            rows={rows}
            onEditCategory={setEditing}
            disabled={retroBusy || saving}
          />
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

function DrillTable({
  rows,
  onEditCategory,
  disabled,
}: {
  rows: RawTransaction[];
  onEditCategory: (row: RawTransaction) => void;
  disabled: boolean;
}) {
  const debitTotal = rows
    .reduce((s, r) => s + (Number.parseFloat(r.debit ?? "0") || 0), 0)
    .toFixed(2);
  const creditTotal = rows
    .reduce((s, r) => s + (Number.parseFloat(r.credit ?? "0") || 0), 0)
    .toFixed(2);

  return (
    <>
      <div className="drill-summary muted small">
        {rows.length} transaction{rows.length === 1 ? "" : "s"} · Dr ₹
        {fmtINR(debitTotal)} · Cr ₹{fmtINR(creditTotal)}
      </div>
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
    </>
  );
}
