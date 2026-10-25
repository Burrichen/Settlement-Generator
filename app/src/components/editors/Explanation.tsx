// An effect or modifier in plain words, as the core explains it: what it
// does, what happens to a roll step by step, and what it affects.
import type { Explanation } from "../../api";

/** "Roll 1d12 → add 2 → cap at 12 → resolve result." */
export function RollPreview({ steps, label }: { steps: string[]; label?: string }) {
  if (steps.length === 0) return null;
  return (
    <ol className="roll-preview" aria-label={label ?? "What happens to the roll"}>
      {steps.map((step, i) => (
        <li key={i} className="roll-preview-step">
          {i > 0 && (
            <span className="roll-preview-arrow" aria-hidden="true">
              →{" "}
            </span>
          )}
          {step}
          {i === steps.length - 1 ? "." : ""}
        </li>
      ))}
    </ol>
  );
}

export function ExplanationView({ explanation }: { explanation: Explanation | null }) {
  if (!explanation) return null;
  return (
    <div
      className={`explanation ${explanation.planned ? "is-planned" : ""}`}
      aria-label="In plain words"
      role="note"
    >
      {explanation.planned && <span className="badge">Planned: documented, not run</span>}
      <p className="explanation-summary">{explanation.summary}</p>
      {explanation.preview && explanation.preview.length > 0 && (
        <RollPreview steps={explanation.preview} />
      )}
      {explanation.affects.length > 0 && (
        <p className="explanation-affects muted small">
          Affects: {explanation.affects.map((a) => a.name).join(", ")}
        </p>
      )}
    </div>
  );
}
