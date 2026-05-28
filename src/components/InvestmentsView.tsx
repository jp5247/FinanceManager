import { type FormEvent, useCallback, useEffect, useState } from "react";
import {
  deleteInvestment,
  investmentsSummary,
  listInvestments,
  upsertInvestment,
} from "../ipc";
import type {
  InvestmentAsset,
  InvestmentsSummary,
  UpsertInvestmentSpec,
} from "../types";

const inrFormatter = new Intl.NumberFormat("en-IN", {
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
});

function fmtINR(decimalStr: string): string {
  const n = Number.parseFloat(decimalStr);
  if (!Number.isFinite(n)) return decimalStr;
  return inrFormatter.format(n);
}

const SUGGESTED_TYPES: readonly string[] = [
  "Mutual Fund",
  "Stock",
  "FD",
  "RD",
  "PPF",
  "NPS",
  "ELSS",
  "Bond",
  "Gold",
  "Real Estate",
  "Crypto",
  "Other",
];

export function InvestmentsView() {
  const [assets, setAssets] = useState<InvestmentAsset[]>([]);
  const [summary, setSummary] = useState<InvestmentsSummary | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [editing, setEditing] = useState<InvestmentAsset | null>(null);
  const [showForm, setShowForm] = useState(false);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [a, s] = await Promise.all([listInvestments(), investmentsSummary()]);
      setAssets(a);
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
    if (!window.confirm("Delete this asset? This can't be undone.")) return;
    try {
      await deleteInvestment(id);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  if (loading) {
    return (
      <section className="investments-view">
        <div className="card"><p className="muted">Loading investments…</p></div>
      </section>
    );
  }

  return (
    <section className="investments-view">
      <header className="dash-header">
        <div>
          <h2>Investments</h2>
          <p className="muted small">
            Manually-entered asset positions. Update the current value
            periodically to keep your wealth snapshot honest.
          </p>
        </div>
        <button
          className="btn btn-primary btn-sm"
          onClick={() => {
            setEditing(null);
            setShowForm(true);
          }}
        >
          + Add asset
        </button>
      </header>

      {error && <div className="error-text">{error}</div>}

      {summary && <SummaryPanel summary={summary} />}

      {showForm || editing ? (
        <AssetForm
          existing={editing}
          onCancel={() => {
            setShowForm(false);
            setEditing(null);
          }}
          onSaved={onSaved}
        />
      ) : null}

      <AssetsTable
        assets={assets}
        onEdit={(a) => {
          setEditing(a);
          setShowForm(true);
        }}
        onDelete={onDelete}
      />
    </section>
  );
}

function SummaryPanel({ summary }: { summary: InvestmentsSummary }) {
  const invested = Number.parseFloat(summary.totalInvested) || 0;
  const current = Number.parseFloat(summary.totalCurrentValue) || 0;
  const gain = Number.parseFloat(summary.unrealizedGainLoss) || 0;
  const positive = gain >= 0;
  if (summary.assetCount === 0) {
    return (
      <div className="card">
        <p className="muted">
          No assets yet. Click <strong>+ Add asset</strong> to enter your first
          SIP, stock, FD, or any other position. The dashboard's Investment
          driver will start using real data once you do.
        </p>
      </div>
    );
  }
  return (
    <>
      <div className="summary-tiles" role="group" aria-label="Investments summary">
        <div className="summary-tile">
          <div className="tile-label">Total invested</div>
          <div className="tile-amount">
            <span className="tile-currency">₹</span>
            {fmtINR(summary.totalInvested)}
          </div>
          <div className="tile-sub muted">across {summary.assetCount} asset{summary.assetCount === 1 ? "" : "s"}</div>
        </div>
        <div className="summary-tile">
          <div className="tile-label">Current value</div>
          <div className="tile-amount">
            <span className="tile-currency">₹</span>
            {fmtINR(summary.totalCurrentValue)}
          </div>
          <div className="tile-sub muted">last-updated wealth snapshot</div>
        </div>
        <div className={`summary-tile ${positive ? "net-positive" : "net-negative"}`}>
          <div className="tile-label">Unrealized gain / loss</div>
          <div className="tile-amount">
            {positive ? "+" : "−"}
            <span className="tile-currency">₹</span>
            {fmtINR(Math.abs(gain).toFixed(2))}
          </div>
          <div className="tile-sub muted">
            {summary.returnPct
              ? `${positive ? "+" : ""}${summary.returnPct}% return on ₹${fmtINR(summary.totalInvested)}`
              : invested > 0
              ? "—"
              : current > 0
              ? "no cost basis recorded"
              : ""}
          </div>
        </div>
      </div>

      {summary.allocation.length > 0 && (
        <div className="card dash-category-card">
          <h3>Allocation by asset type</h3>
          <ul className="breakdown-list">
            {summary.allocation.map((a) => {
              const share = Number.parseFloat(a.sharePct) || 0;
              return (
                <li key={a.assetType} className="breakdown-row investment">
                  <div className="breakdown-row-btn" style={{ cursor: "default" }}>
                    <span className="bk-name">{a.assetType}</span>
                    <span className="bk-bar" aria-hidden>
                      <span
                        className="bk-bar-fill investment"
                        style={{ width: `${Math.max(0, Math.min(100, share))}%` }}
                      />
                    </span>
                    <span className="bk-pct">{share.toFixed(0)}%</span>
                    <span className="bk-count muted">
                      {a.assetCount} {a.assetCount === 1 ? "asset" : "assets"}
                    </span>
                    <span className="bk-amount bk-amount-investment">
                      ₹{fmtINR(a.currentValue)}
                    </span>
                  </div>
                </li>
              );
            })}
          </ul>
        </div>
      )}
    </>
  );
}

interface FormProps {
  existing: InvestmentAsset | null;
  onCancel: () => void;
  onSaved: () => void | Promise<void>;
}

function AssetForm({ existing, onCancel, onSaved }: FormProps) {
  const [assetType, setAssetType] = useState(existing?.assetType ?? SUGGESTED_TYPES[0]);
  const [customType, setCustomType] = useState(
    existing && !SUGGESTED_TYPES.includes(existing.assetType) ? existing.assetType : "",
  );
  const [useCustom, setUseCustom] = useState(
    existing ? !SUGGESTED_TYPES.includes(existing.assetType) : false,
  );
  const [assetName, setAssetName] = useState(existing?.assetName ?? "");
  const [invested, setInvested] = useState(existing?.investedAmount ?? "");
  const [current, setCurrent] = useState(existing?.currentValue ?? "");
  const [notes, setNotes] = useState(existing?.notes ?? "");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setSaving(true);
    setError(null);
    try {
      const spec: UpsertInvestmentSpec = {
        id: existing?.id,
        assetType: useCustom ? customType.trim() : assetType,
        assetName: assetName.trim(),
        investedAmount: invested,
        currentValue: current,
        notes: notes.trim() || null,
      };
      await upsertInvestment(spec);
      await onSaved();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <form className="card asset-form" onSubmit={submit}>
      <h3>{existing ? "Edit asset" : "Add asset"}</h3>
      <div className="asset-form-grid">
        <label>
          <span>Type</span>
          <select
            value={useCustom ? "__custom__" : assetType}
            onChange={(e) => {
              if (e.target.value === "__custom__") {
                setUseCustom(true);
              } else {
                setUseCustom(false);
                setAssetType(e.target.value);
              }
            }}
            disabled={saving}
          >
            {SUGGESTED_TYPES.map((t) => (
              <option key={t} value={t}>{t}</option>
            ))}
            <option value="__custom__">Custom…</option>
          </select>
          {useCustom && (
            <input
              type="text"
              value={customType}
              onChange={(e) => setCustomType(e.target.value)}
              placeholder="e.g. SGB, REIT, P2P lending"
              disabled={saving}
            />
          )}
        </label>
        <label>
          <span>Name</span>
          <input
            type="text"
            value={assetName}
            onChange={(e) => setAssetName(e.target.value)}
            placeholder="e.g. PPFAS Flexicap Direct Growth"
            disabled={saving}
            required
          />
        </label>
        <label>
          <span>Invested amount (₹)</span>
          <input
            type="text"
            value={invested}
            onChange={(e) => setInvested(e.target.value)}
            placeholder="e.g. 100000.00"
            disabled={saving}
            required
            inputMode="decimal"
          />
        </label>
        <label>
          <span>Current value (₹)</span>
          <input
            type="text"
            value={current}
            onChange={(e) => setCurrent(e.target.value)}
            placeholder="e.g. 125000.00"
            disabled={saving}
            required
            inputMode="decimal"
          />
        </label>
        <label className="asset-form-notes">
          <span>Notes (optional)</span>
          <input
            type="text"
            value={notes ?? ""}
            onChange={(e) => setNotes(e.target.value)}
            placeholder="e.g. monthly SIP ₹10k, started Apr 2024"
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
            assetName.trim().length === 0 ||
            invested.trim().length === 0 ||
            current.trim().length === 0 ||
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
  assets: InvestmentAsset[];
  onEdit: (a: InvestmentAsset) => void;
  onDelete: (id: string) => void | Promise<void>;
}

function AssetsTable({ assets, onEdit, onDelete }: TableProps) {
  if (assets.length === 0) {
    return null;
  }
  return (
    <div className="card">
      <h3>Positions</h3>
      <div className="txn-table-wrapper">
        <table className="txn-table">
          <thead>
            <tr>
              <th>Type</th>
              <th>Name</th>
              <th className="num">Invested</th>
              <th className="num">Current</th>
              <th className="num">Gain / Loss</th>
              <th>Updated</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {assets.map((a) => {
              const inv = Number.parseFloat(a.investedAmount) || 0;
              const cur = Number.parseFloat(a.currentValue) || 0;
              const gain = cur - inv;
              const pct = inv > 0 ? ((gain / inv) * 100).toFixed(2) : "—";
              const positive = gain >= 0;
              return (
                <tr key={a.id}>
                  <td>{a.assetType}</td>
                  <td>
                    {a.assetName}
                    {a.notes && (
                      <div className="muted xsmall">{a.notes}</div>
                    )}
                  </td>
                  <td className="num">₹{fmtINR(a.investedAmount)}</td>
                  <td className="num">₹{fmtINR(a.currentValue)}</td>
                  <td className={`num ${positive ? "credit" : "debit"}`}>
                    {positive ? "+" : "−"}₹{fmtINR(Math.abs(gain).toFixed(2))}
                    {inv > 0 && (
                      <div className="muted xsmall">
                        {positive ? "+" : ""}
                        {pct}%
                      </div>
                    )}
                  </td>
                  <td className="mono small muted">{a.lastUpdatedAt.slice(0, 10)}</td>
                  <td>
                    <button
                      type="button"
                      className="btn btn-link inline"
                      onClick={() => onEdit(a)}
                    >
                      Edit
                    </button>
                    <button
                      type="button"
                      className="btn btn-link inline danger"
                      onClick={() => void onDelete(a.id)}
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
    </div>
  );
}
