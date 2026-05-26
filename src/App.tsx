import { useCallback, useEffect, useState } from "react";
import { ProfilePicker } from "./components/ProfilePicker";
import { ProfileCreate } from "./components/ProfileCreate";
import { ProfileUnlock } from "./components/ProfileUnlock";
import { Home } from "./components/Home";
import { currentProfile, listProfiles } from "./ipc";
import type { ProfileSummary } from "./types";

type Screen =
  | { kind: "loading" }
  | { kind: "picker" }
  | { kind: "create" }
  | { kind: "unlock"; target: ProfileSummary }
  | { kind: "home"; me: ProfileSummary };

function App() {
  const [screen, setScreen] = useState<Screen>({ kind: "loading" });
  const [profiles, setProfiles] = useState<ProfileSummary[]>([]);
  const [bootError, setBootError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const [list, me] = await Promise.all([listProfiles(), currentProfile()]);
      setProfiles(list);
      if (me) {
        setScreen({ kind: "home", me });
      } else if (list.length === 0) {
        setScreen({ kind: "create" });
      } else {
        setScreen({ kind: "picker" });
      }
    } catch (e) {
      setBootError(String(e));
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  if (bootError) {
    return (
      <main className="app-shell">
        <div className="card error-card">
          <h2>Could not start</h2>
          <pre>{bootError}</pre>
        </div>
      </main>
    );
  }

  switch (screen.kind) {
    case "loading":
      return (
        <main className="app-shell centered">
          <div className="loading">Loading…</div>
        </main>
      );

    case "picker":
      return (
        <main className="app-shell centered">
          <ProfilePicker
            profiles={profiles}
            onPick={(p) => setScreen({ kind: "unlock", target: p })}
            onCreate={() => setScreen({ kind: "create" })}
          />
        </main>
      );

    case "create":
      return (
        <main className="app-shell centered">
          <ProfileCreate
            onCreated={(me) => setScreen({ kind: "home", me })}
            onCancel={profiles.length > 0 ? () => setScreen({ kind: "picker" }) : undefined}
          />
        </main>
      );

    case "unlock":
      return (
        <main className="app-shell centered">
          <ProfileUnlock
            target={screen.target}
            onUnlocked={(me) => setScreen({ kind: "home", me })}
            onCancel={() => setScreen({ kind: "picker" })}
          />
        </main>
      );

    case "home":
      return (
        <main className="app-shell">
          <Home me={screen.me} onLocked={() => void refresh()} />
        </main>
      );
  }
}

export default App;
