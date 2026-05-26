import { type FormEvent, useCallback, useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  deleteImport,
  deleteUserRule,
  getImport,
  getLlmConfig,
  listImports,
  listUserRules,
  recategorizeTransaction,
  setLlmConfig,
  uploadPdf,
} from "../ipc";
import type {
  FileMeta,
  LlmConfigView,
  NewRuleSpec,
  RawTransaction,
  StoredRule,
  UploadResult,
} from "../types";
import { COMMON_CATEGORIES, UNCATEGORIZED } from "../categories";

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
          onRowChanged={(updated) => {
            setStage({ kind: "viewing", displayed: updated, isFresh: false });
            void refreshImports();
            void refreshUserRules();
          }}
        />
      )}

      <PreviousImports
        imports={imports}
        activeImportId={stage.kind === "viewing" ? stage.displayed.importId : null}
        onOpen={openImport}
        onDelete={removeImport}
      />

      <UserRulesPanel
        rules={userRules}
        onDelete={async (id) => {
          try {
            const updated = await deleteUserRule(id);
            setUserRules(updated);
          } catch (e) {
            setError(String(e));
          }
        }}
      />

      <LlmSettingsPanel />
    </section>
  );
}

interface ResultProps {
  displayed: UploadResult;
  isFresh: boolean;
  onClose: () => void;
  onRowChanged: (updated: UploadResult) => void;
}

function LlmSettingsPanel() {
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
      onRowChanged(updated);
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

interface RecategorizeProps {
  row: RawTransaction;
  saving: boolean;
  error: string | null;
  onSave: (newCategory: string, saveAsRule: NewRuleSpec | null) => void;
  onCancel: () => void;
}

const OTHER = "Other…";

/** Pull a "good guess" pattern out of the description for the save-as-rule
 * pattern input. We just take the first 30 chars stripped of leading common
 * prefixes — the user edits before saving. */
function suggestPattern(description: string): string {
  let s = description.trim();
  for (const prefix of ["UPI-", "UPI/", "ACH D- ", "ACH CR- ", "NEFT CR-"]) {
    if (s.toUpperCase().startsWith(prefix)) {
      s = s.slice(prefix.length);
      break;
    }
  }
  // Take the first chunk up to a separator we commonly see.
  const stop = s.search(/[/0-9]/);
  if (stop > 0 && stop < 30) s = s.slice(0, stop);
  return s.trim().slice(0, 30);
}

function RecategorizeModal({
  row,
  saving,
  error,
  onSave,
  onCancel,
}: RecategorizeProps) {
  const current = row.category ?? "";
  const isCustom = current && !COMMON_CATEGORIES.includes(current);
  const [selection, setSelection] = useState<string>(
    isCustom ? OTHER : current || COMMON_CATEGORIES[0],
  );
  const [customText, setCustomText] = useState<string>(isCustom ? current : "");
  const [saveAsRule, setSaveAsRule] = useState<boolean>(false);
  const [pattern, setPattern] = useState<string>(suggestPattern(row.description));

  const finalCategory = selection === OTHER ? customText.trim() : selection;
  const submit = (e: FormEvent) => {
    e.preventDefault();
    const rule: NewRuleSpec | null =
      saveAsRule && pattern.trim().length > 0
        ? { matchType: "contains", matchValue: pattern.trim(), category: finalCategory }
        : null;
    onSave(finalCategory, rule);
  };

  return (
    <div className="modal-backdrop" onClick={onCancel}>
      <form
        className="card modal-card"
        onSubmit={submit}
        onClick={(e) => e.stopPropagation()}
      >
        <h3>Categorize transaction</h3>
        <p className="muted small">
          <strong className="mono">{row.txnDate}</strong> · {row.description}
        </p>

        <label>
          <span>Category</span>
          <select
            value={selection}
            onChange={(e) => setSelection(e.target.value)}
            disabled={saving}
          >
            {COMMON_CATEGORIES.map((c) => (
              <option key={c} value={c}>
                {c}
              </option>
            ))}
            <option value={OTHER}>{OTHER}</option>
          </select>
        </label>

        {selection === OTHER && (
          <label>
            <span>Custom category</span>
            <input
              type="text"
              value={customText}
              onChange={(e) => setCustomText(e.target.value)}
              placeholder="e.g. Pet care"
              autoFocus
              disabled={saving}
            />
          </label>
        )}

        <label className="check-row save-rule-toggle">
          <input
            type="checkbox"
            checked={saveAsRule}
            onChange={(e) => setSaveAsRule(e.target.checked)}
            disabled={saving}
          />
          <span>Save as a rule — future transactions matching this pattern will auto-categorize</span>
        </label>

        {saveAsRule && (
          <label>
            <span>Match transactions whose description contains</span>
            <input
              type="text"
              value={pattern}
              onChange={(e) => setPattern(e.target.value)}
              disabled={saving}
              placeholder="e.g. SWIGGY INSTAMART"
            />
            <small className="muted">
              Case-insensitive substring match. Edit to make it more or less specific.
            </small>
          </label>
        )}

        {error && <div className="error-text">{error}</div>}

        <div className="row">
          {row.category && (
            <button
              type="button"
              className="btn btn-link inline danger"
              onClick={() => onSave("", null)}
              disabled={saving}
              title="Mark this row as Uncategorized"
            >
              Clear
            </button>
          )}
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
              (selection === OTHER && customText.trim().length === 0) ||
              (saveAsRule && pattern.trim().length === 0)
            }
          >
            {saving ? "Saving…" : "Save"}
          </button>
        </div>
      </form>
    </div>
  );
}

interface UserRulesPanelProps {
  rules: StoredRule[];
  onDelete: (id: string) => Promise<void>;
}

function UserRulesPanel({ rules, onDelete }: UserRulesPanelProps) {
  if (rules.length === 0) {
    return (
      <div className="card user-rules-card">
        <h3>Your category rules</h3>
        <p className="muted small">
          No saved rules yet. When you recategorize a transaction, tick
          "Save as a rule" to make the same category apply automatically to
          future matching transactions.
        </p>
      </div>
    );
  }
  return (
    <div className="card user-rules-card">
      <h3>Your category rules ({rules.length})</h3>
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
