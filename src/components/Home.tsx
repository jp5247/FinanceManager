import { useState } from "react";
import { lockProfile } from "../ipc";
import type { ProfileSummary } from "../types";
import { DashboardView } from "./DashboardView";
import { InvestmentsView } from "./InvestmentsView";
import { UploadView } from "./UploadView";

interface Props {
  me: ProfileSummary;
  onLocked: () => void;
}

type Tab = "dashboard" | "upload" | "investments";

function initial(name: string): string {
  const ch = name.trim().charAt(0);
  return ch ? ch.toUpperCase() : "?";
}

export function Home({ me, onLocked }: Props) {
  const [busy, setBusy] = useState(false);
  const [tab, setTab] = useState<Tab>("dashboard");

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
        <div className="brand-row">
          <svg className="logo-mark" viewBox="0 0 24 24" aria-hidden="true">
            <rect x="3" y="14" width="4" height="7" rx="1" />
            <rect x="10" y="10" width="4" height="11" rx="1" />
            <rect x="17" y="5" width="4" height="16" rx="1" />
            <polyline
              points="4,11 11,6 15,8 21,3"
              fill="none"
              strokeWidth="1.8"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </svg>
          <span className="brand">FinanceManager</span>
        </div>

        <nav className="tabs" aria-label="primary">
          <button
            className={`tab ${tab === "dashboard" ? "active" : ""}`}
            onClick={() => setTab("dashboard")}
          >
            Dashboard
          </button>
          <button
            className={`tab ${tab === "upload" ? "active" : ""}`}
            onClick={() => setTab("upload")}
          >
            Upload
          </button>
          <button
            className={`tab ${tab === "investments" ? "active" : ""}`}
            onClick={() => setTab("investments")}
          >
            Investments
          </button>
        </nav>

        <div className="user-chip">
          <div className="avatar" aria-hidden="true">{initial(me.displayName)}</div>
          <div className="user-text">
            <span className="muted xsmall">unlocked as</span>
            <strong>{me.displayName}</strong>
          </div>
          <button
            className="btn btn-secondary btn-sm"
            onClick={lock}
            disabled={busy}
            title="Lock and return to picker"
          >
            Lock
          </button>
        </div>
      </header>

      {tab === "dashboard" ? (
        <DashboardView />
      ) : tab === "upload" ? (
        <UploadView />
      ) : (
        <InvestmentsView />
      )}
    </>
  );
}
