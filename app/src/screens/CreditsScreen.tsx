import { useEffect, useRef } from "react";
import { useApp } from "../app-state";
import { Ornament } from "../components/Ornament";
import { KeyHints } from "../components/ScreenFrame";

function Corner({ position }: { position: "tl" | "tr" | "bl" | "br" }) {
  return (
    <svg className={`credits-corner ${position}`} viewBox="0 0 22 22" aria-hidden="true">
      <path d="M1 21V1h20" fill="none" stroke="currentColor" />
      <path d="M5 21V5h16" fill="none" stroke="currentColor" opacity="0.5" />
      <path d="M1 1 5 5" stroke="currentColor" />
    </svg>
  );
}

export function CreditsScreen() {
  const { back } = useApp();
  const backButton = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    backButton.current?.focus();
  }, []);

  return (
    <div className="credits-screen">
      <div className="credits-top">
        <button ref={backButton} type="button" className="back-button" onClick={back}>
          <span aria-hidden="true">‹</span> Back
        </button>
      </div>
      <main className="credits-stage">
        <div className="credits-frame">
          <Corner position="tl" />
          <Corner position="tr" />
          <Corner position="bl" />
          <Corner position="br" />
          <Ornament />
          <p className="display credits-text">designed and made by Burrichen</p>
          <Ornament />
        </div>
      </main>
      <KeyHints />
    </div>
  );
}
