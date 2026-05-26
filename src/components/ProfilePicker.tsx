import type { ProfileSummary } from "../types";

interface Props {
  profiles: ProfileSummary[];
  onPick: (p: ProfileSummary) => void;
  onCreate: () => void;
}

export function ProfilePicker({ profiles, onPick, onCreate }: Props) {
  return (
    <div className="card picker-card">
      <h1 className="brand">FinanceManager</h1>
      <p className="muted">Choose a profile to unlock.</p>
      <ul className="profile-list">
        {profiles.map((p) => (
          <li key={p.userId}>
            <button className="profile-row" onClick={() => onPick(p)}>
              <span className="profile-name">{p.displayName}</span>
              <span className="profile-id muted">{p.userId}</span>
            </button>
          </li>
        ))}
      </ul>
      <button className="btn btn-secondary full-width" onClick={onCreate}>
        + New profile
      </button>
    </div>
  );
}
