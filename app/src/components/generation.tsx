// Presentational pieces of the generation flows. All decisions (legal
// results, rolls, dependencies) come from the core via `SessionView`.
import { useEffect, useRef, useState, type FormEvent } from "react";
import { formatRange, type Named, type StageOutcome, type StageView } from "../api";

/** Tags in plain words, kept visually secondary. Only rendered with Tag View. */
export function TagList({ tags, label = "Tags" }: { tags?: Named[]; label?: string }) {
  if (!tags || tags.length === 0) return null;
  return (
    <p className="tag-list">
      <span className="tag-list-label">{label}</span>{" "}
      {tags.map((t, i) => (
        <span key={t.id} title={t.id}>
          {t.name}
          {i < tags.length - 1 ? ", " : ""}
        </span>
      ))}
    </p>
  );
}

export function ActiveTags({ tags }: { tags?: Named[] }) {
  if (!tags) return null;
  return (
    <aside className="active-tags" aria-label="Active tags">
      <span className="label">Active tags</span>
      {tags.length === 0 ? (
        <p className="muted small">None yet</p>
      ) : (
        <ul>
          {tags.map((t) => (
            <li key={t.id} title={t.id}>
              {t.name}
            </li>
          ))}
        </ul>
      )}
    </aside>
  );
}

/** "Rolled 14 on 1d20", plus follow-up rolls, or "Chosen". */
export function OutcomeLine({ outcome }: { outcome?: StageOutcome }) {
  if (!outcome) return null;
  return (
    <div className="outcome">
      {outcome.chosen ? (
        <span>Chosen</span>
      ) : (
        <span>
          Rolled <strong>{outcome.roll}</strong> on {outcome.dice}
          {outcome.rerolls > 0 &&
            ` (after ${outcome.rerolls} reroll${outcome.rerolls === 1 ? "" : "s"})`}
        </span>
      )}
      {outcome.follow_ups.map((f) => (
        <span key={f.name} className="outcome-follow">
          {f.name}: rolled <strong>{f.roll}</strong> on {f.dice}
          {f.becomes ? ` → ${f.becomes}` : " → kept"}
        </span>
      ))}
    </div>
  );
}

/** A stage's legal results as a list of choices. */
export function ResultList({
  stage,
  disabled,
  onChoose,
}: {
  stage: StageView;
  disabled: boolean;
  onChoose: (resultId: string) => void;
}) {
  const legal = stage.options.filter((o) => o.legal);
  const hidden = stage.options.length - legal.length;
  const showFaces = legal.some((o) => o.range !== undefined);
  return (
    <div className="result-list" role="group" aria-label={`${stage.stage.name} results`}>
      {legal.map((option) => {
        const selected = stage.result?.id === option.result.id;
        return (
          <button
            key={option.result.id}
            type="button"
            className={`result-row ${showFaces ? "" : "no-faces"} ${selected ? "is-selected" : ""}`}
            aria-pressed={selected}
            disabled={disabled}
            onClick={() => onChoose(option.result.id)}
          >
            {showFaces && <span className="result-faces">{formatRange(option.range)}</span>}
            <span className="result-name">{option.result.name}</span>
            {option.tags.length > 0 && (
              <span className="result-tags">{option.tags.map((t) => t.name).join(", ")}</span>
            )}
          </button>
        );
      })}
      {legal.length === 0 && <p className="muted">No result can be chosen here.</p>}
      {hidden > 0 && (
        <p className="muted small">
          {hidden} other result{hidden === 1 ? " is" : "s are"} not available here.
        </p>
      )}
    </div>
  );
}

/** Vertical list of every stage with its status; the current one marked. */
export function StageRail({ stages, current }: { stages: StageView[]; current: number }) {
  return (
    <ol className="stage-rail" aria-label="Stages">
      {stages.map((s) => {
        const state =
          s.index === current
            ? "current"
            : s.status === "resolved"
              ? "done"
              : s.previous
                ? "redo"
                : "pending";
        return (
          <li
            key={s.index}
            className={`rail-${state}`}
            aria-current={s.index === current ? "step" : undefined}
          >
            <span className="rail-mark" aria-hidden="true" />
            <span className="rail-name">{s.stage.name}</span>
            {s.result && <span className="rail-result">{s.result.name}</span>}
          </li>
        );
      })}
    </ol>
  );
}

/** Inline confirmation for genuinely destructive actions. */
export function ConfirmBar({
  message,
  confirmLabel,
  onConfirm,
  onCancel,
}: {
  message: string;
  confirmLabel: string;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const cancel = useRef<HTMLButtonElement>(null);
  useEffect(() => cancel.current?.focus(), []);
  return (
    <div className="confirm-bar" role="alertdialog" aria-label={message}>
      <span>{message}</span>
      <div className="button-row">
        <button ref={cancel} type="button" className="btn" onClick={onCancel}>
          Keep it
        </button>
        <button type="button" className="btn btn-danger" onClick={onConfirm}>
          {confirmLabel}
        </button>
      </div>
    </div>
  );
}

/** Mirrors `settlements::MAX_NAME_LENGTH` in the core (a repository test keeps them equal). */
export const MAX_NAME_LENGTH = 80;

/**
 * Checks a settlement name; returns a message, or null if it is fine.
 * Counts characters (code points) exactly as the core does, not UTF-16 units.
 */
export function nameProblem(name: string): string | null {
  const trimmed = name.trim();
  if (trimmed.length === 0) return "Enter a name for the settlement.";
  if ([...trimmed].length > MAX_NAME_LENGTH)
    return `Keep the name to ${MAX_NAME_LENGTH} characters or fewer.`;
  return null;
}

/** Asks for the settlement name before saving. */
export function NameForm({
  busy,
  serverError,
  initialName = "",
  onSave,
  onCancel,
}: {
  busy: boolean;
  serverError: string | null;
  /** Pre-filled, e.g. with the name the name stage generated. */
  initialName?: string;
  onSave: (name: string) => void;
  onCancel: () => void;
}) {
  const [name, setName] = useState(initialName);
  const [problem, setProblem] = useState<string | null>(null);
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => input.current?.focus(), []);

  const submit = (event: FormEvent) => {
    event.preventDefault();
    const issue = nameProblem(name);
    setProblem(issue);
    if (!issue) onSave(name.trim());
  };

  const message = problem ?? serverError;
  return (
    <form className="name-form" onSubmit={submit} aria-label="Save settlement">
      <label htmlFor="settlement-name" className="setting-name">
        Settlement name
      </label>
      <div className="name-form-row">
        <input
          ref={input}
          id="settlement-name"
          className="text-input"
          value={name}
          maxLength={MAX_NAME_LENGTH + 20}
          aria-invalid={message ? true : undefined}
          aria-describedby={message ? "settlement-name-error" : undefined}
          onChange={(e) => {
            setName(e.target.value);
            if (problem) setProblem(null);
          }}
        />
        <button type="submit" className="btn btn-primary" disabled={busy}>
          Save
        </button>
        <button type="button" className="btn" onClick={onCancel}>
          Cancel
        </button>
      </div>
      {message && (
        <p id="settlement-name-error" className="field-error" role="alert">
          {message}
        </p>
      )}
    </form>
  );
}
