import { type FormEvent, useState } from "react";
import { createProfile } from "../ipc";
import type { ProfileSummary } from "../types";

interface Props {
  onCreated: (me: ProfileSummary) => void;
  onCancel?: () => void;
}

type Stage =
  | { kind: "form" }
  | { kind: "showRecovery"; recovery: string; summary: ProfileSummary };

export function ProfileCreate({ onCreated, onCancel }: Props) {
  const [stage, setStage] = useState<Stage>({ kind: "form" });
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
      const result = await createProfile(userId.trim(), displayName.trim(), passphrase);
      setStage({ kind: "showRecovery", recovery: result.recoveryPhrase, summary: result.summary });
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  if (stage.kind === "showRecovery") {
    return (
      <RecoveryReveal
        recovery={stage.recovery}
        onContinue={() => onCreated(stage.summary)}
      />
    );
  }

  return (
    <form className="card form-card" onSubmit={submit}>
      <h2>Create profile</h2>
      <p className="muted">
        Local, encrypted at rest. Your passphrase is the daily unlock — but a
        one-time recovery phrase shown next is the only way back in if you
        forget it.
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

interface RecoveryProps {
  recovery: string;
  onContinue: () => void;
}

function RecoveryReveal({ recovery, onContinue }: RecoveryProps) {
  const [confirmed, setConfirmed] = useState(false);
  const [copied, setCopied] = useState(false);

  const copy = async () => {
    await navigator.clipboard.writeText(recovery);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };

  return (
    <div className="card form-card">
      <h2>Save your recovery phrase</h2>
      <p className="muted">
        This is shown <strong>once</strong>. If you ever forget your passphrase,
        this is the only way to unlock your data. Write it down or store it in
        a password manager — anywhere off this machine. We cannot recover it
        for you.
      </p>

      <div className="recovery-display">
        <code>{recovery}</code>
      </div>

      <div className="row">
        <button type="button" className="btn btn-secondary" onClick={copy}>
          {copied ? "Copied!" : "Copy to clipboard"}
        </button>
      </div>

      <label className="check-row">
        <input
          type="checkbox"
          checked={confirmed}
          onChange={(e) => setConfirmed(e.target.checked)}
        />
        <span>I have saved this recovery phrase somewhere safe.</span>
      </label>

      <div className="row">
        <button
          type="button"
          className="btn btn-primary"
          onClick={onContinue}
          disabled={!confirmed}
        >
          Continue
        </button>
      </div>
    </div>
  );
}
