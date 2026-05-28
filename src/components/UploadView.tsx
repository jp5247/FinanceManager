import { useCallback, useEffect, useState } from "react";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import {
  auditLog,
  deleteImport,
  deleteUserRule,
  exportToXlsx,
  getImport,
  getLlmConfig,
  listImports,
  listUserRules,
  recategorizeImport,
  recategorizeTransaction,
  resetCategorizations,
  setLlmConfig,
  uploadPdf,
} from "../ipc";
import type {
  AuditLogView,
  FileMeta,
  LlmConfigView,
  NewRuleSpec,
  RawTransaction,
  StoredRule,
  UploadResult,
} from "../types";
import { UNCATEGORIZED } from "../categories";
import { RecategorizeModal } from "./RecategorizeModal";

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
  const [userRules, setUserRules] = useState<StoredRule[]>([]);
  const [autoRecatBusy, setAutoRecatBusy] = useState(false);
  const [autoRecatNote, setAutoRecatNote] = useState<string | null>(null);

  const activeImportId = stage.kind === "viewing" ? stage.displayed.importId : null;

  const refreshImports = useCallback(async () => {
    try {
      setImports(await listImports());
    } catch (e) {
      setError(String(e));
    }
  }, []);

  const refreshUserRules = useCallback(async () => {
    try {
      setUserRules(await listUserRules());
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void refreshImports();
    void refreshUserRules();
  }, [refreshImports, refreshUserRules]);

  // Re-run the categorization pipeline on the active import after the user
  // changes something that could affect prior rows (LLM model, user rules).
  const runAutoRecategorize = useCallback(async () => {
    if (!activeImportId) return;
    setAutoRecatBusy(true);
    setAutoRecatNote(null);
    try {
      const updated = await recategorizeImport(activeImportId);
      setStage({ kind: "viewing", displayed: updated, isFresh: false });
      const llm = updated.llmCategorizedCount ?? 0;
      setAutoRecatNote(
        llm > 0
          ? `Re-categorized — ${llm} more row${llm === 1 ? "" : "s"} via Gemini.`
          : "Re-categorized.",
      );
      void refreshImports();
    } catch (e) {
      setAutoRecatNote(`Re-categorize failed: ${String(e)}`);
    } finally {
      setAutoRecatBusy(false);
    }
  }, [activeImportId, refreshImports]);

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
          onRowChanged={(updated, savedNewRule) => {
            setStage({ kind: "viewing", displayed: updated, isFresh: false });
            void refreshImports();
            void refreshUserRules();
            // A newly-saved rule might match other rows in this import —
            // re-run the pipeline so they auto-categorize too.
            if (savedNewRule) void runAutoRecategorize();
          }}
        />
      )}

      {autoRecatBusy && (
        <div className="auto-recat-toast info">Re-categorizing transactions…</div>
      )}
      {autoRecatNote && !autoRecatBusy && (
        <div className="auto-recat-toast">{autoRecatNote}</div>
      )}

      <PreviousImports
        imports={imports}
        activeImportId={activeImportId}
        onOpen={openImport}
        onDelete={removeImport}
      />

      <UserRulesPanel
        rules={userRules}
        onDelete={async (id) => {
          try {
            const updated = await deleteUserRule(id);
            setUserRules(updated);
            // Deleting a rule may flip rows back to Uncategorized — refresh
            // the open import so the table reflects it.
            void runAutoRecategorize();
          } catch (e) {
            setError(String(e));
          }
        }}
        onReset={async () => {
          const ok = window.confirm(
            "Reset categories?\n\n" +
              "This will:\n" +
              "  • delete every user rule you have saved\n" +
              "  • clear the merchant cache\n" +
              "  • re-categorize every uploaded statement from scratch (including rows you manually recategorized)\n\n" +
              "Real outflows (uploads, transactions) stay safe. This action cannot be undone.",
          );
          if (!ok) return;
          try {
            setError(null);
            const r = await resetCategorizations();
            setUserRules(await listUserRules());
            void refreshImports();
            if (stage.kind === "viewing") {
              const updated = await getImport(stage.displayed.importId);
              setStage({ kind: "viewing", displayed: updated, isFresh: false });
            }
            setError(
              `Reset done. Re-categorized ${r.touched} of ${r.total} import${
                r.total === 1 ? "" : "s"
              }.${r.skipped > 0 ? ` ${r.skipped} skipped — check dev logs.` : ""}`,
            );
          } catch (e) {
            setError(`Reset failed: ${String(e)}`);
          }
        }}
      />

      <LlmSettingsPanel onModelChanged={runAutoRecategorize} />

      <ExportPanel />

      <AuditPanel />
    </section>
  );
}

interface ResultProps {
  displayed: UploadResult;
  isFresh: boolean;
  onClose: () => void;
  onRowChanged: (updated: UploadResult, savedNewRule: boolean) => void;
}

function LlmSettingsPanel({ onModelChanged }: { onModelChanged: () => void }) {
  const [cfg, setCfg] = useState<LlmConfigView | null>(null);
  const [draftKey, setDraftKey] = useState<string>("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showKey, setShowKey] = useState(false);

  const refresh = useCallback(async () => {
    try {
      setCfg(await getLlmConfig());
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  if (!cfg) {
    return (
      <div className="card llm-settings-card">
        <h3>External categorization</h3>
        <p className="muted small">Loading…</p>
      </div>
    );
  }

  const persist = async (update: { enabled?: boolean; apiKey?: string; model?: string }) => {
    setSaving(true);
    setError(null);
    try {
      const next = await setLlmConfig(update);
      setCfg(next);
      if (update.apiKey !== undefined) setDraftKey("");
      // A model swap should re-try Gemini on previously-Uncategorized rows
      // in the open import. Enabling/disabling or changing the key doesn't
      // need to re-run (next upload picks it up).
      if (update.model !== undefined) onModelChanged();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="card llm-settings-card">
      <h3>External categorization (Gemini)</h3>
      <p className="muted small">
        For rows that didn't match a user rule or the curated table, the app
        can ask Google's Gemini API to suggest a category. <strong>Only the
        extracted merchant name and direction (debit / credit) are sent.</strong>
        {" "}No amounts, dates, account masks, or ref-IDs. Get a free key at{" "}
        <span className="mono">aistudio.google.com/apikey</span>.
      </p>

      <label className="check-row">
        <input
          type="checkbox"
          checked={cfg.enabled}
          disabled={saving}
          onChange={(e) => void persist({ enabled: e.target.checked })}
        />
        <span>Enable Gemini lookup on upload</span>
      </label>

      <div className="llm-key-row">
        <div className="llm-key-status">
          <span className="muted xsmall">API KEY</span>{" "}
          {cfg.apiKeySet ? (
            <span className="mono">{cfg.apiKeyHint}</span>
          ) : (
            <span className="muted">not configured</span>
          )}
        </div>
        <div className="llm-key-controls">
          <input
            type={showKey ? "text" : "password"}
            value={draftKey}
            onChange={(e) => setDraftKey(e.target.value)}
            placeholder="AIza..."
            disabled={saving}
            spellCheck={false}
            autoComplete="off"
          />
          <button
            type="button"
            className="btn btn-link inline"
            onClick={() => setShowKey((v) => !v)}
            disabled={saving}
          >
            {showKey ? "Hide" : "Show"}
          </button>
          <button
            type="button"
            className="btn btn-primary btn-sm"
            disabled={saving || draftKey.trim().length === 0}
            onClick={() => void persist({ apiKey: draftKey.trim() })}
          >
            Save
          </button>
          {cfg.apiKeySet && (
            <button
              type="button"
              className="btn btn-link inline danger"
              disabled={saving}
              onClick={() => void persist({ apiKey: "" })}
            >
              Clear
            </button>
          )}
        </div>
      </div>

      <label className="llm-model-row">
        <span className="muted xsmall">MODEL</span>
        <select
          value={cfg.model}
          disabled={saving}
          onChange={(e) => void persist({ model: e.target.value })}
        >
          {GEMINI_MODELS.map((m) => (
            <option key={m.id} value={m.id}>
              {m.label}
            </option>
          ))}
          {!GEMINI_MODELS.some((m) => m.id === cfg.model) && (
            <option value={cfg.model}>{cfg.model}</option>
          )}
        </select>
      </label>

      <p className="muted xsmall">
        Each upload triggers at most one batched request. If you hit a rate
        limit, switch to a <span className="mono">-lite</span> variant — they
        have higher per-minute quotas on the free tier.
      </p>

      {error && <div className="error-text">{error}</div>}
    </div>
  );
}

const GEMINI_MODELS: { id: string; label: string }[] = [
  { id: "gemini-2.0-flash", label: "gemini-2.0-flash (default, 15 RPM)" },
  { id: "gemini-2.0-flash-lite", label: "gemini-2.0-flash-lite (30 RPM)" },
  { id: "gemini-2.5-flash", label: "gemini-2.5-flash (newer, 10 RPM)" },
  { id: "gemini-2.5-flash-lite", label: "gemini-2.5-flash-lite (15 RPM)" },
];

function ResultPanel({ displayed, isFresh, onClose, onRowChanged }: ResultProps) {
  const [editing, setEditing] = useState<{ row: RawTransaction } | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async (newCategory: string, saveAsRule: NewRuleSpec | null) => {
    if (!editing) return;
    setSaving(true);
    setError(null);
    try {
      const updated = await recategorizeTransaction(
        displayed.importId,
        editing.row.rowNumber,
        newCategory,
        saveAsRule,
      );
      onRowChanged(updated, saveAsRule !== null);
      setEditing(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

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

      {(displayed.llmCategorizedCount ?? 0) > 0 && (
        <div className="llm-result-note">
          ✨ {displayed.llmCategorizedCount} row
          {displayed.llmCategorizedCount === 1 ? "" : "s"} categorized via Gemini
        </div>
      )}

      {displayed.lookupWarning && (
        <div className="llm-result-warning">{displayed.lookupWarning}</div>
      )}

      <CategoryBreakdownPanel breakdown={displayed.categoryBreakdown ?? []} />

      <TransactionTable rows={displayed.transactions} onEditCategory={(row) => setEditing({ row })} />

      {editing && (
        <RecategorizeModal
          row={editing.row}
          saving={saving}
          error={error}
          onSave={submit}
          onCancel={() => {
            setEditing(null);
            setError(null);
          }}
        />
      )}
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

interface TableProps {
  rows: RawTransaction[];
  onEditCategory: (row: RawTransaction) => void;
}

function TransactionTable({ rows, onEditCategory }: TableProps) {
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
                <button
                  type="button"
                  className={`category-chip-btn ${r.category ? "" : "uncategorized"}`}
                  title={
                    r.categoryRuleId
                      ? `rule: ${r.categoryRuleId} · click to change`
                      : "click to assign a category"
                  }
                  onClick={() => onEditCategory(r)}
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

function AuditPanel() {
  const [data, setData] = useState<AuditLogView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [expanded, setExpanded] = useState(false);

  const refresh = async () => {
    setError(null);
    try {
      setData(await auditLog());
    } catch (e) {
      setError(String(e));
    }
  };

  useEffect(() => {
    void refresh();
  }, []);

  if (!data) {
    return (
      <div className="card">
        <h3>Audit log</h3>
        <p className="muted small">{error ?? "Loading…"}</p>
      </div>
    );
  }

  const visible = expanded ? data.entries : data.entries.slice(0, 10);

  return (
    <div className="card">
      <div className="rules-header">
        <h3>Audit log ({data.entries.length})</h3>
        <button
          type="button"
          className="btn btn-link inline"
          onClick={() => void refresh()}
        >
          Refresh
        </button>
      </div>
      <p className="muted small">
        Hash-chained append-only log of every mutation (uploads,
        recategorizations, resets, investment / loan edits, deletions).
        Plaintext on disk at <span className="mono">audit/log.jsonl</span> so
        chain integrity can be re-verified at any time.
      </p>
      {!data.chainOk && (
        <div className="error-text">
          {data.chainNote ?? "Chain integrity check failed."}
        </div>
      )}
      {data.entries.length === 0 ? (
        <p className="muted">No audit entries yet.</p>
      ) : (
        <>
          <ul className="audit-list">
            {visible.map((e) => (
              <li key={e.thisHash} className="audit-row">
                <div className="audit-head">
                  <span className="mono small muted">{e.ts}</span>
                  <span className="audit-action">{e.action}</span>
                  {e.entityId && (
                    <span className="mono xsmall muted">{e.entityId}</span>
                  )}
                </div>
                {e.details !== null && (
                  <pre className="audit-details">{JSON.stringify(e.details, null, 2)}</pre>
                )}
              </li>
            ))}
          </ul>
          {data.entries.length > 10 && (
            <button
              type="button"
              className="btn btn-link inline"
              onClick={() => setExpanded((v) => !v)}
            >
              {expanded ? "Show only last 10" : `Show all ${data.entries.length}`}
            </button>
          )}
        </>
      )}
    </div>
  );
}

function ExportPanel() {
  const [imports, setImports] = useState<FileMeta[]>([]);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Load + default-select all imports on mount. Re-running this on every
  // mount keeps the list fresh after a new upload without needing a
  // cross-component event bus.
  useEffect(() => {
    void (async () => {
      try {
        const list = await listImports();
        setImports(list);
        setSelected(new Set(list.map((m) => m.importId)));
      } catch (e) {
        setError(String(e));
      }
    })();
  }, []);

  const toggle = (id: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const selectAll = () => setSelected(new Set(imports.map((m) => m.importId)));
  const selectNone = () => setSelected(new Set());

  const run = async () => {
    setError(null);
    setResult(null);
    if (selected.size === 0) {
      setError("Select at least one statement to export.");
      return;
    }
    try {
      const now = new Date();
      const stamp = `${now.getFullYear()}${String(now.getMonth() + 1).padStart(2, "0")}${String(now.getDate()).padStart(2, "0")}`;
      const target = await saveDialog({
        defaultPath: `FinanceManager-${stamp}.xlsx`,
        filters: [{ name: "Excel workbook", extensions: ["xlsx"] }],
      });
      if (!target) return;
      setBusy(true);
      const idsToSend =
        selected.size === imports.length ? null : Array.from(selected);
      const r = await exportToXlsx(target, idsToSend);
      const lines = [
        `Saved to ${r.filePath}.`,
        `${r.transactionCount} transactions · ${r.investmentCount} assets · ${r.loanCount} loans.`,
      ];
      if (r.warning) lines.push(r.warning);
      setResult(lines.join(" "));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const allSelected = imports.length > 0 && selected.size === imports.length;

  return (
    <div className="card">
      <h3>Export to Excel</h3>
      <p className="muted small">
        One workbook with sheets for Summary, Transactions, Categories,
        Investments, and Loans. Values-only — no formulas. Pick which
        uploaded statements feed the Transactions / Categories / Summary
        sheets; Investments and Loans always reflect everything.
      </p>

      {imports.length === 0 ? (
        <p className="muted small">No statements uploaded yet — nothing to export.</p>
      ) : (
        <>
          <div className="export-select-controls">
            <span className="muted xsmall">
              {selected.size} of {imports.length} selected
            </span>
            <button
              type="button"
              className="btn btn-link inline"
              onClick={allSelected ? selectNone : selectAll}
            >
              {allSelected ? "Select none" : "Select all"}
            </button>
          </div>
          <ul className="export-import-list">
            {imports.map((m) => {
              const checked = selected.has(m.importId);
              return (
                <li key={m.importId}>
                  <label className="check-row">
                    <input
                      type="checkbox"
                      checked={checked}
                      onChange={() => toggle(m.importId)}
                      disabled={busy}
                    />
                    <span className="export-import-meta">
                      <span>{m.sourceFile}</span>
                      <span className="muted xsmall">
                        {m.transactionCount} txns · {m.adapterId}@{m.adapterVersion} ·{" "}
                        {m.uploadedAt.slice(0, 10)}
                      </span>
                    </span>
                  </label>
                </li>
              );
            })}
          </ul>
        </>
      )}

      <button
        type="button"
        className="btn btn-primary btn-sm"
        onClick={() => void run()}
        disabled={busy || imports.length === 0 || selected.size === 0}
      >
        {busy ? "Exporting…" : "Export now"}
      </button>
      {result && <p className="muted small">{result}</p>}
      {error && <div className="error-text">{error}</div>}
    </div>
  );
}

interface UserRulesPanelProps {
  rules: StoredRule[];
  onDelete: (id: string) => Promise<void>;
  onReset: () => Promise<void>;
}

function UserRulesPanel({ rules, onDelete, onReset }: UserRulesPanelProps) {
  if (rules.length === 0) {
    return (
      <div className="card user-rules-card">
        <div className="rules-header">
          <h3>Your category rules</h3>
          <button
            type="button"
            className="btn btn-link inline danger"
            onClick={() => void onReset()}
            title="Wipe user rules, clear cache, re-categorize every import"
          >
            Reset categories
          </button>
        </div>
        <p className="muted small">
          No saved rules yet. When you recategorize a transaction, tick
          "Save as a rule" to make the same category apply automatically to
          future matching transactions.
        </p>
        <p className="muted xsmall">
          The <strong>Reset categories</strong> button clears any custom
          categorizations you've already made and re-runs the pipeline using
          only the curated rules + LLM. Use it when you want a clean slate.
        </p>
      </div>
    );
  }
  return (
    <div className="card user-rules-card">
      <div className="rules-header">
        <h3>Your category rules ({rules.length})</h3>
        <button
          type="button"
          className="btn btn-link inline danger"
          onClick={() => void onReset()}
          title="Wipe user rules, clear cache, re-categorize every import"
        >
          Reset categories
        </button>
      </div>
      <p className="muted small">
        Applied on every upload, before the built-in merchant table. Future
        statements with matching descriptions categorize automatically.
      </p>
      <ul className="rule-list">
        {rules.map((r) => (
          <li key={r.id} className="rule-row">
            <div className="rule-main">
              <div className="rule-pattern">
                <span className="muted xsmall">
                  {r.matchType === "regex" ? "regex" : "contains"}
                </span>{" "}
                <code>{r.matchValue}</code>{" "}
                <span className="muted">→</span>{" "}
                <span className="category-chip">{r.category}</span>
              </div>
              <div className="muted xsmall">
                created {r.createdAt}
              </div>
            </div>
            <button
              type="button"
              className="prev-delete"
              title="Delete this rule"
              aria-label={`Delete rule matching ${r.matchValue}`}
              onClick={() => void onDelete(r.id)}
            >
              ×
            </button>
          </li>
        ))}
      </ul>
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
