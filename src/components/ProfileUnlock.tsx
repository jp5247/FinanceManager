import { type FormEvent, useState } from "react";
import { unlockProfile, unlockWithRecovery } from "../ipc";
import type { ProfileSummary } from "../types";

interface Props {
  target: ProfileSummary;
  onUnlocked: (me: ProfileSummary) => void;
  onCancel: () => void;
}

type Mode = "passphrase" | "recovery";

export function ProfileUnlock({ target, onUnlocked, onCancel }: Props) {
  const [mode, setMode] = useState<Mode>("passphrase");
  const [value, setValue] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const switchMode = () => {
    setMode((m) => (m === "passphrase" ? "recovery" : "passphrase"));
    setValue("");
    setError(null);
  };

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setError(null);
    setBusy(true);
    try {
      const me =
        mode === "passphrase"
          ? await unlockProfile(target.userId, value)
          : await unlockWithRecovery(target.userId, value);
      onUnlocked(me);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <form className="card form-card" onSubmit={submit}>
      <h2>Unlock {target.displayName}</h2>
      <p className="muted">
        {mode === "passphrase"
          ? "Enter your passphrase."
          : "Enter the recovery phrase shown when this profile was created."}
      </p>

      <label>
        <span>{mode === "passphrase" ? "Passphrase" : "Recovery phrase"}</span>
        {mode === "passphrase" ? (
          <input
            type="password"
            value={value}
            onChange={(e) => setValue(e.target.value)}
            autoComplete="current-password"
            autoFocus
            disabled={busy}
          />
        ) : (
          <input
            type="text"
            value={value}
            onChange={(e) => setValue(e.target.value)}
            placeholder="xxxx-xxxx-xxxx-xxxx-xxxx-xxxx"
            spellCheck={false}
            autoComplete="off"
            autoFocus
            disabled={busy}
          />
        )}
      </label>

      <div className="hint-row">
        <button type="button" className="btn btn-link inline" onClick={switchMode}>
          {mode === "passphrase"
            ? "Use recovery phrase instead"
            : "Use passphrase instead"}
        </button>
      </div>

      {error && <div className="error-text">{error}</div>}

      <div className="row">
        <button
          type="button"
          className="btn btn-secondary"
          onClick={onCancel}
          disabled={busy}
        >
          Back
        </button>
        <button
          type="submit"
          className="btn btn-primary"
          disabled={busy || !value}
        >
          {busy ? "Unlocking…" : "Unlock"}
        </button>
      </div>
    </form>
  );
}
