import { useEffect, useRef, useState } from "react";
import { installUpdate, type InstallProgress, type UpdateOffer } from "../api";

interface Props {
  offer: UpdateOffer;
  /** Dismisses the prompt for this session; the update is not installed. */
  onLater: () => void;
}

type Phase =
  | { kind: "offer" }
  | { kind: "working"; progress: InstallProgress | null }
  | { kind: "failed"; message: string };

/** Asks whether to install an available update. Never installs on its own. */
export function UpdatePrompt({ offer, onLater }: Props) {
  const [phase, setPhase] = useState<Phase>({ kind: "offer" });
  const dialog = useRef<HTMLDivElement>(null);
  const working = phase.kind === "working";
  // Where focus was before the prompt opened; restored when it closes.
  const [returnFocus] = useState(() => document.activeElement as HTMLElement | null);
  useEffect(() => () => returnFocus?.focus(), [returnFocus]);

  useEffect(() => {
    const root = dialog.current;
    const target =
      root?.querySelector<HTMLElement>(".btn-primary") ??
      root?.querySelector<HTMLElement>("button");
    (target ?? root)?.focus();
  }, [phase.kind]);

  // Esc means Later, and focus stays inside the dialog. Runs in the capture
  // phase so the screen behind never sees these keys.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        if (!working) onLater();
      } else if (event.key === "Tab" && dialog.current) {
        const buttons = [...dialog.current.querySelectorAll<HTMLElement>("button, summary")];
        if (buttons.length === 0) return;
        const first = buttons[0];
        const last = buttons[buttons.length - 1];
        const inside = dialog.current.contains(document.activeElement);
        if (!inside || (event.shiftKey && document.activeElement === first)) {
          event.preventDefault();
          (event.shiftKey ? last : first).focus();
        } else if (!event.shiftKey && document.activeElement === last) {
          event.preventDefault();
          first.focus();
        }
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [working, onLater]);

  const update = () => {
    setPhase({ kind: "working", progress: null });
    installUpdate((progress) => setPhase({ kind: "working", progress })).then(
      // On success the installer closes the app; nothing more to show.
      () => {},
      (e: unknown) => setPhase({ kind: "failed", message: String(e) }),
    );
  };

  return (
    <div className="modal-backdrop">
      <div
        ref={dialog}
        className="modal"
        tabIndex={-1}
        role="dialog"
        aria-modal="true"
        aria-labelledby="update-title"
        aria-describedby="update-description"
      >
        <h2 id="update-title" className="display modal-title">
          Update available
        </h2>
        <p id="update-description" className="modal-text">
          A new version of Settlement Generator is available.
        </p>
        <dl className="update-versions">
          <div>
            <dt>Current version</dt>
            <dd>v{offer.current_version}</dd>
          </div>
          <div>
            <dt>Available version</dt>
            <dd>v{offer.version}</dd>
          </div>
        </dl>
        {offer.notes && (
          <details className="update-notes">
            <summary>What’s new</summary>
            <p>{offer.notes}</p>
          </details>
        )}

        {phase.kind === "offer" && (
          <>
            <p className="modal-note">
              Your settings, saved settlements and content changes are kept. A backup of your data
              is made before installing. Close any open Settlement Generator terminal (CLI) windows
              first.
            </p>
            <div className="button-row modal-actions">
              <button type="button" className="btn" onClick={onLater}>
                Later
              </button>
              <button type="button" className="btn btn-primary" onClick={update}>
                Update
              </button>
            </div>
          </>
        )}

        {phase.kind === "working" && <Progress progress={phase.progress} />}

        {phase.kind === "failed" && (
          <>
            <p role="alert" className="status-line is-error">
              {phase.message}
            </p>
            <p className="modal-note">Your data has not been changed.</p>
            <div className="button-row modal-actions">
              <button type="button" className="btn" onClick={onLater}>
                Close
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}

function Progress({ progress }: { progress: InstallProgress | null }) {
  if (progress?.stage === "backing_up") {
    return <p className="modal-note">Backing up your data…</p>;
  }
  if (progress?.stage === "installing") {
    return <p className="modal-note">Installing. Settlement Generator will close and restart.</p>;
  }
  const total = progress?.total ?? null;
  const percent =
    progress && total ? Math.min(100, Math.round((progress.downloaded / total) * 100)) : null;
  return (
    <div className="update-progress">
      <p className="modal-note">Downloading update{percent === null ? "…" : ` (${percent}%)`}</p>
      <div
        className="progress-track"
        role="progressbar"
        aria-label="Download progress"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={percent ?? undefined}
      >
        <div className="progress-fill" style={{ width: `${percent ?? 0}%` }} />
      </div>
    </div>
  );
}
