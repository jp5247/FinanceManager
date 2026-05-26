import { useState } from "react";
import { lockProfile } from "../ipc";
import type { ProfileSummary } from "../types";

interface Props {
  me: ProfileSummary;
  onLocked: () => void;
}

export function Home({ me, onLocked }: Props) {
  const [busy, setBusy] = useState(false);

  const lock = async () => {
    setBusy(true);
    try {
      await lockProfile();
      onLocked();
    } catch {
      setBusy(false);
    }
  };

  return (
    <>
      <header className="topbar">
        <div className="brand">FinanceManager</div>
        <div className="user-chip">
          <span className="muted">unlocked as</span> <strong>{me.displayName}</strong>
          <button className="btn btn-link" onClick={lock} disabled={busy}>
            Lock
          </button>
        </div>
      </header>

      <section className="placeholder-panel">
        <h2>Welcome, {me.displayName}.</h2>
        <p className="muted">
          Phase 1 backend is wired up — your profile is unlocked and the session
          is held in memory. Upload, Dashboard, Loans and Investments land in
          the next phases.
        </p>
      </section>
    </>
  );
}
