import type { OutlineLine, StageView } from "../api";
import { RollDetails } from "./generation";

/** A flow stage's record as the core lays it out: one indented line each. */
export function FlowOutline({ lines }: { lines?: OutlineLine[] }) {
  if (!lines || lines.length === 0) return null;
  return (
    <ul className="flow-outline">
      {lines.map((line, i) => (
        <li key={i} style={{ paddingInlineStart: `${line.depth * 1.25}rem` }}>
          {line.value ? (
            <>
              <span className="muted">{line.label}: </span>
              <span>{line.value}</span>
            </>
          ) : (
            <span className="muted">{line.label}</span>
          )}
        </li>
      ))}
    </ul>
  );
}

/**
 * A flow stage (repeats, branches, objects, subflows): rolled as a whole,
 * with every roll and object shown below.
 */
export function FlowControls({
  stage,
  busy,
  onRoll,
}: {
  stage: StageView;
  busy: boolean;
  onRoll: () => void;
}) {
  return (
    <>
      {stage.previous && stage.status === "pending" && (
        <p className="note">
          An earlier change affects this stage. It was <s>{stage.previous.name}</s>.
        </p>
      )}
      {stage.skipped && <p className="note">This stage does not apply and is skipped.</p>}
      {!stage.skipped && (
        <div className="roll-row">
          <button
            type="button"
            className="btn btn-primary"
            disabled={busy || !stage.actionable}
            onClick={onRoll}
          >
            {stage.status === "resolved" ? "Roll again" : "Roll"}
          </button>
        </div>
      )}
      <FlowOutline lines={stage.flow?.outline} />
      <RollDetails traces={stage.trace} />
    </>
  );
}
