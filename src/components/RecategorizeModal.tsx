import { type FormEvent, useState } from "react";
import { COMMON_CATEGORIES } from "../categories";
import type { NewRuleSpec, RawTransaction } from "../types";

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
  const stop = s.search(/[/0-9]/);
  if (stop > 0 && stop < 30) s = s.slice(0, stop);
  return s.trim().slice(0, 30);
}

interface Props {
  row: RawTransaction;
  saving: boolean;
  error: string | null;
  onSave: (newCategory: string, saveAsRule: NewRuleSpec | null) => void;
  onCancel: () => void;
}

export function RecategorizeModal({ row, saving, error, onSave, onCancel }: Props) {
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
              <option key={c} value={c}>{c}</option>
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
