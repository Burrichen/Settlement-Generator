import type { StageView } from "../api";
import { OutcomeLine, TagList } from "./generation";

/** Every stage in one table: choose a legal result or roll each stage. */
export function StageGrid({
  stages,
  busy,
  onRoll,
  onChoose,
}: {
  stages: StageView[];
  busy: boolean;
  onRoll: (stage: number) => void;
  onChoose: (stage: number, result: string) => void;
}) {
  return (
    <table className="stage-table-grid">
      <thead>
        <tr>
          <th scope="col">Stage</th>
          <th scope="col">Result</th>
          <th scope="col">Roll</th>
        </tr>
      </thead>
      <tbody>
        {stages.map((stage) => (
          <StageGridRow
            key={stage.index}
            stage={stage}
            busy={busy}
            onRoll={() => onRoll(stage.index)}
            onChoose={(r) => onChoose(stage.index, r)}
          />
        ))}
      </tbody>
    </table>
  );
}

function StageGridRow({
  stage,
  busy,
  onRoll,
  onChoose,
}: {
  stage: StageView;
  busy: boolean;
  onRoll: () => void;
  onChoose: (result: string) => void;
}) {
  const legal = stage.options.filter((o) => o.legal);
  const selectId = `stage-select-${stage.index}`;
  // A saved result can be one that is no longer offered (e.g. content has
  // since changed); keep it visible rather than silently replacing it.
  const currentMissing = stage.result && !legal.some((o) => o.result.id === stage.result?.id);
  return (
    <tr className={`stage-row status-${stage.status}`}>
      <th scope="row">
        <label htmlFor={selectId} className="stage-row-name">
          {stage.stage.name}
        </label>
        {stage.table.name !== stage.stage.name && (
          <span className="stage-row-table">{stage.table.name}</span>
        )}
      </th>
      <td>
        <select
          id={selectId}
          className="select"
          disabled={busy || legal.length === 0}
          value={stage.result?.id ?? ""}
          onChange={(e) => e.target.value && onChoose(e.target.value)}
        >
          <option value="" disabled>
            {stage.skipped ? "Skipped" : legal.length === 0 ? "Nothing to choose" : "Choose…"}
          </option>
          {currentMissing && stage.result && (
            <option value={stage.result.id} disabled>
              {stage.result.name}
            </option>
          )}
          {legal.map((o) => (
            <option key={o.result.id} value={o.result.id}>
              {o.result.name}
            </option>
          ))}
        </select>
        {stage.previous && stage.status === "pending" && (
          <span className="note small">
            Redo: was <s>{stage.previous.name}</s>
          </span>
        )}
        {stage.incomplete && <span className="note small">{stage.incomplete}</span>}
        <TagList tags={stage.tags} label="Emits" />
      </td>
      <td className="roll-cell">
        {stage.can_roll && (
          <button type="button" className="btn btn-small" disabled={busy} onClick={onRoll}>
            {stage.status === "resolved" ? "Reroll" : "Roll"}{" "}
            <span className="visually-hidden">{stage.stage.name}</span>
          </button>
        )}
        <OutcomeLine outcome={stage.outcome} />
      </td>
    </tr>
  );
}
