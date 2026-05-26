import { type FormEvent, useState } from "react";
import { createProfile } from "../ipc";
import type { ProfileSummary } from "../types";

interface Props {
  onCreated: (me: ProfileSummary) => void;
  onCancel?: () => void;
}

export function ProfileCreate({ onCreated, onCancel }: Props) {
  const [userId, setUserId] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [passphrase, setPassphrase] = useState("");
  const [confirm, setConfirm] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setError(null);
    if (!userId.trim() || !displayName.trim() || !passphrase) {
      setError("All fields are required.");
      return;
    }
    if (passphrase !== confirm) {
      setError("Passphrases do not match.");
      return;
    }
    setBusy(true);
    try {
      const me = await createProfile(userId.trim(), displayName.trim(), passphrase);
      onCreated(me);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <form className="card form-card" onSubmit={submit}>
      <h2>Create profile</h2>
      <p className="muted">
        Local, encrypted at rest. Your passphrase derives the encryption key —
        we cannot recover data if you lose it.
      </p>

      <label>
        <span>User id</span>
        <input
          type="text"
          value={userId}
          onChange={(e) => setUserId(e.target.value)}
          placeholder="asha"
          autoComplete="off"
          disabled={busy}
        />
        <small className="muted">
          Lowercase letters, digits, and hyphens. Used as the local folder name.
        </small>
      </label>

      <label>
        <span>Display name</span>
        <input
          type="text"
          value={displayName}
          onChange={(e) => setDisplayName(e.target.value)}
          placeholder="Asha"
          disabled={busy}
        />
      </label>

      <label>
        <span>Passphrase</span>
        <input
          type="password"
          value={passphrase}
          onChange={(e) => setPassphrase(e.target.value)}
          autoComplete="new-password"
          disabled={busy}
        />
      </label>

      <label>
        <span>Confirm passphrase</span>
        <input
          type="password"
          value={confirm}
          onChange={(e) => setConfirm(e.target.value)}
          autoComplete="new-password"
          disabled={busy}
        />
      </label>

      {error && <div className="error-text">{error}</div>}

      <div className="row">
        {onCancel && (
          <button
            type="button"
            className="btn btn-secondary"
            onClick={onCancel}
            disabled={busy}
          >
            Cancel
          </button>
        )}
        <button type="submit" className="btn btn-primary" disabled={busy}>
          {busy ? "Creating…" : "Create profile"}
        </button>
      </div>
    </form>
  );
}
