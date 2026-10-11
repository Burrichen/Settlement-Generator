import { useEffect, useState } from "react";
import { displayVersion, getRoadmap, type RoadmapEntry } from "../api";
import { useApp } from "../app-state";
import { ScreenFrame } from "../components/ScreenFrame";

export function RoadmapScreen() {
  const { info, back } = useApp();
  const [entries, setEntries] = useState<RoadmapEntry[] | null>(null);

  useEffect(() => {
    getRoadmap().then(setEntries, () => setEntries([]));
  }, []);

  return (
    <ScreenFrame title="Roadmap" backLabel="Main Menu" onBack={back}>
      {info && <p className="roadmap-current">Installed version {displayVersion(info)}</p>}
      {entries && (
        <ol className="roadmap">
          {entries.map((entry) => (
            <li key={entry.horizon}>
              <span className="label">{entry.label}</span>
              <p className="display roadmap-summary">{entry.summary}</p>
            </li>
          ))}
        </ol>
      )}
    </ScreenFrame>
  );
}
