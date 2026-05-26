import { type FormEvent, useState } from "react";
import { unlockProfile } from "../ipc";
import type { ProfileSummary } from "../types";

interface Props {
  target: ProfileSummary;
  onUnlocked: (me: ProfileSummary) => void;
  onCancel: () => void;
}

export function ProfileUnlock({ target, onUnlocked, onCancel }: Props) {
  const [passphrase, setPassphrase] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setError(null);
    setBusy(true);
    try {
      const me = await unlockProfile(target.userId, passphrase);
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
      <p className="muted">Enter your passphrase.</p>

      <label>
        <span>Passphrase</span>
        <input
          type="password"
          value={passphrase}
          onChange={(e) => setPassphrase(e.target.value)}
          autoComplete="current-password"
          autoFocus
          disabled={busy}
        />
      </label>

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
        <button type="submit" className="btn btn-primary" disabled={busy || !passphrase}>
          {busy ? "Unlocking…" : "Unlock"}
        </button>
      </div>
    </form>
  );
}
