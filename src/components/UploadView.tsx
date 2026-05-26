import { useCallback, useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  deleteImport,
  getImport,
  listImports,
  uploadPdf,
} from "../ipc";
import type { FileMeta, RawTransaction, UploadResult } from "../types";

type Stage =
  | { kind: "idle" }
  | { kind: "needsPassword"; filePath: string }
  | { kind: "uploading" }
  | { kind: "viewing"; displayed: UploadResult; isFresh: boolean };

function fileNameOf(p: string): string {
  const i = Math.max(p.lastIndexOf("\\"), p.lastIndexOf("/"));
  return i >= 0 ? p.slice(i + 1) : p;
}

const inrFormatter = new Intl.NumberFormat("en-IN", {
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
});

function fmtINR(decimalStr: string): string {
  const n = Number.parseFloat(decimalStr);
  if (!Number.isFinite(n)) return decimalStr;
  return inrFormatter.format(n);
}

export function UploadView() {
  const [stage, setStage] = useState<Stage>({ kind: "idle" });
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [imports, setImports] = useState<FileMeta[]>([]);

  const refreshImports = useCallback(async () => {
    try {
      setImports(await listImports());
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void refreshImports();
  }, [refreshImports]);

  const pickFile = async () => {
    setError(null);
    const picked = await openDialog({
      multiple: false,
      directory: false,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (!picked || typeof picked !== "string") return;
    await runUpload(picked, null);
  };

  const runUpload = async (filePath: string, pw: string | null) => {
    setStage({ kind: "uploading" });
    setError(null);
    try {
      const result = await uploadPdf(filePath, pw);
      setStage({ kind: "viewing", displayed: result, isFresh: true });
      setPassword("");
      void refreshImports();
    } catch (e) {
      const msg = String(e);
      if (msg.toLowerCase().includes("password is incorrect") ||
          msg.toLowerCase().includes("password-protected")) {
        setStage({ kind: "needsPassword", filePath });
        setError(msg);
      } else {
        setStage({ kind: "idle" });
        setError(msg);
      }
    }
  };

  const openImport = async (importId: string) => {
    setError(null);
    try {
      const result = await getImport(importId);
      setStage({ kind: "viewing", displayed: result, isFresh: false });
    } catch (e) {
      setError(String(e));
    }
  };

  const removeImport = async (importId: string) => {
    const ok = window.confirm(
      `Delete this import? Transactions parsed from it will be removed. The original PDF on your computer is not touched.`,
    );
    if (!ok) return;
    setError(null);
    try {
      await deleteImport(importId);
      // If we were viewing the one we just deleted, drop the view.
      if (stage.kind === "viewing" && stage.displayed.importId === importId) {
        setStage({ kind: "idle" });
      }
      void refreshImports();
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <section className="upload-view">
      <div className="card upload-actions">
        <h2>Upload statement</h2>
        <p className="muted">
          PDF stays on this machine. Text is extracted, parsed by the issuer
          adapter, and persisted encrypted under your profile key.
        </p>

        {stage.kind === "needsPassword" ? (
          <form
            className="inline-form"
            onSubmit={(e) => {
              e.preventDefault();
              void runUpload(stage.filePath, password);
            }}
          >
            <label>
              <span>Passphrase for {fileNameOf(stage.filePath)}</span>
              <input
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                autoFocus
              />
            </label>
            <div className="row">
              <button
                type="button"
                className="btn btn-secondary"
                onClick={() => {
                  setStage({ kind: "idle" });
                  setPassword("");
                  setError(null);
                }}
              >
                Cancel
              </button>
              <button type="submit" className="btn btn-primary" disabled={!password}>
                Retry with passphrase
              </button>
            </div>
          </form>
        ) : (
          <button
            className="btn btn-primary"
            onClick={pickFile}
            disabled={stage.kind === "uploading"}
          >
            {stage.kind === "uploading" ? "Processing…" : "Choose PDF…"}
          </button>
        )}

        {error && <div className="error-text">{error}</div>}
      </div>

      {stage.kind === "viewing" && (
        <ResultPanel
          displayed={stage.displayed}
          isFresh={stage.isFresh}
          onClose={() => setStage({ kind: "idle" })}
        />
      )}

      <PreviousImports
        imports={imports}
        activeImportId={stage.kind === "viewing" ? stage.displayed.importId : null}
        onOpen={openImport}
        onDelete={removeImport}
      />
    </section>
  );
}

interface ResultProps {
  displayed: UploadResult;
  isFresh: boolean;
  onClose: () => void;
}

function ResultPanel({ displayed, isFresh, onClose }: ResultProps) {
  return (
    <div className="card result-panel">
      <header className="result-header">
        <div>
          <h3>{displayed.sourceFile}</h3>
          <p className="muted small">
            {isFresh ? "Just uploaded · " : "Viewing import · "}
            {displayed.transactionCount} transactions · {displayed.pageCount} pages
            · adapter <code>{displayed.adapterId}</code> · import{" "}
            <code>{displayed.importId}</code>
          </p>
        </div>
        <button className="btn btn-link inline" onClick={onClose}>
          Close
        </button>
      </header>

      <SummaryTiles
        debitCount={displayed.debitCount}
        creditCount={displayed.creditCount}
        totalDebit={displayed.totalDebit}
        totalCredit={displayed.totalCredit}
      />

      <CategoryBreakdownPanel breakdown={displayed.categoryBreakdown ?? []} />

      <TransactionTable rows={displayed.transactions} />
    </div>
  );
}

interface BreakdownProps {
  breakdown: import("../types").CategoryBreakdown[];
}

function CategoryBreakdownPanel({ breakdown }: BreakdownProps) {
  if (breakdown.length === 0) return null;

  // Show top categories by debit total. Items with zero debit (credit-only,
  // e.g. salary) are listed in a separate group below.
  const debitItems = breakdown.filter((b) => Number.parseFloat(b.totalDebit) > 0);
  const creditItems = breakdown.filter(
    (b) => Number.parseFloat(b.totalDebit) === 0 && Number.parseFloat(b.totalCredit) > 0,
  );
  const debitMax = debitItems.reduce(
    (m, b) => Math.max(m, Number.parseFloat(b.totalDebit) || 0),
    0,
  );

  return (
    <section className="breakdown-panel">
      <h4>Where the money went</h4>
      <ul className="breakdown-list">
        {debitItems.map((b) => {
          const amt = Number.parseFloat(b.totalDebit) || 0;
          const pct = debitMax > 0 ? (amt / debitMax) * 100 : 0;
          return (
            <li key={b.category} className="breakdown-row">
              <div className="bk-name">{b.category}</div>
              <div className="bk-bar" aria-hidden>
                <div className="bk-bar-fill" style={{ width: `${pct}%` }} />
              </div>
              <div className="bk-count muted">
                {b.debitCount} {b.debitCount === 1 ? "txn" : "txns"}
              </div>
              <div className="bk-amount">₹{fmtINR(b.totalDebit)}</div>
            </li>
          );
        })}
      </ul>

      {creditItems.length > 0 && (
        <>
          <h4 className="bk-heading-secondary">Money in</h4>
          <ul className="breakdown-list">
            {creditItems.map((b) => (
              <li key={b.category} className="breakdown-row credit">
                <div className="bk-name">{b.category}</div>
                <div className="bk-bar" aria-hidden>
                  <div className="bk-bar-fill credit" style={{ width: "100%" }} />
                </div>
                <div className="bk-count muted">
                  {b.creditCount} {b.creditCount === 1 ? "txn" : "txns"}
                </div>
                <div className="bk-amount credit">₹{fmtINR(b.totalCredit)}</div>
              </li>
            ))}
          </ul>
        </>
      )}
    </section>
  );
}

interface SummaryProps {
  debitCount: number;
  creditCount: number;
  totalDebit: string;
  totalCredit: string;
}

function SummaryTiles({ debitCount, creditCount, totalDebit, totalCredit }: SummaryProps) {
  const debit = Number.parseFloat(totalDebit) || 0;
  const credit = Number.parseFloat(totalCredit) || 0;
  const net = credit - debit;
  const netSign = net >= 0 ? "+" : "−";
  const netAbs = fmtINR(Math.abs(net).toFixed(2));

  return (
    <div className="summary-tiles" role="group" aria-label="Statement summary">
      <div className="summary-tile tile-debit">
        <div className="tile-label">Debits</div>
        <div className="tile-amount">
          <span className="tile-currency">₹</span>
          {fmtINR(totalDebit)}
        </div>
        <div className="tile-sub muted">
          {debitCount} {debitCount === 1 ? "transaction" : "transactions"}
        </div>
      </div>

      <div className="summary-tile tile-credit">
        <div className="tile-label">Credits</div>
        <div className="tile-amount">
          <span className="tile-currency">₹</span>
          {fmtINR(totalCredit)}
        </div>
        <div className="tile-sub muted">
          {creditCount} {creditCount === 1 ? "transaction" : "transactions"}
        </div>
      </div>

      <div className={`summary-tile tile-net ${net >= 0 ? "net-positive" : "net-negative"}`}>
        <div className="tile-label">Net flow</div>
        <div className="tile-amount">
          {netSign}
          <span className="tile-currency">₹</span>
          {netAbs}
        </div>
        <div className="tile-sub muted">credit − debit</div>
      </div>
    </div>
  );
}

function TransactionTable({ rows }: { rows: RawTransaction[] }) {
  if (rows.length === 0) {
    return <p className="muted">No transactions parsed.</p>;
  }
  return (
    <div className="txn-table-wrapper">
      <table className="txn-table">
        <thead>
          <tr>
            <th>Date</th>
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
              <td>{r.description}</td>
              <td>
                {r.category ? (
                  <span className="category-chip" title={r.categoryRuleId ?? undefined}>
                    {r.category}
                  </span>
                ) : (
                  <span className="muted">—</span>
                )}
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

interface PrevProps {
  imports: FileMeta[];
  activeImportId: string | null;
  onOpen: (importId: string) => void;
  onDelete: (importId: string) => void;
}

function PreviousImports({ imports, activeImportId, onOpen, onDelete }: PrevProps) {
  if (imports.length === 0) return null;
  return (
    <div className="card previous-imports">
      <h3>Previous imports</h3>
      <p className="muted small">Click a row to view its transactions.</p>
      <ul>
        {imports.map((m) => {
          const isActive = activeImportId === m.importId;
          return (
            <li key={m.importId} className={`previous-row ${isActive ? "active" : ""}`}>
              <button
                type="button"
                className="prev-main-button"
                onClick={() => onOpen(m.importId)}
              >
                <div className="prev-main">
                  <div className="prev-file">{m.sourceFile}</div>
                  <div className="muted small">
                    {m.transactionCount} txns · {m.adapterId}@{m.adapterVersion} ·{" "}
                    {m.uploadedAt}
                  </div>
                </div>
                <div className="prev-totals">
                  <span className="prev-debit">
                    Dr {m.debitCount} · ₹{fmtINR(m.totalDebit)}
                  </span>
                  <span className="prev-credit">
                    Cr {m.creditCount} · ₹{fmtINR(m.totalCredit)}
                  </span>
                </div>
              </button>
              <button
                type="button"
                className="prev-delete"
                aria-label={`Delete import of ${m.sourceFile}`}
                title="Delete this import"
                onClick={() => onDelete(m.importId)}
              >
                ×
              </button>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
