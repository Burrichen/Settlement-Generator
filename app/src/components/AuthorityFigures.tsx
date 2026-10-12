import { useState } from "react";
import type {
  AuthorityFigure,
  AuthorityFigures,
  FiguresView,
  GenerationResponse,
  StageView,
} from "../api";
import { generation } from "../api";
import { TagList } from "./generation";

/** Shown on one line under the name, in this order. */
const OUTLINE = ["species", "gender", "age"];
/** Shown as labelled lines. */
const DETAILS: [string, string][] = [
  ["disposition", "Disposition"],
  ["motive", "Motive"],
  ["reputation", "Reputation"],
  ["flaw", "Flaw"],
];

const result = (figure: AuthorityFigure, field: string) =>
  figure.fields.find((f) => f.field === field)?.roll.result.name;

function heading(figure: AuthorityFigure) {
  return figure.role ? `${figure.role.name} ${figure.label}` : figure.label;
}

/** "Mila Varen ↔ Orven Daal". */
function pairLabel(record: AuthorityFigures, a: string, b: string) {
  const label = (id: string) => record.figures.find((f) => f.id === id)?.label ?? id;
  return `${label(a)} ↔ ${label(b)}`;
}

/** The Authority Figures and their relationships, compactly. */
export function FiguresSummary({
  record,
  showTags,
}: {
  record: AuthorityFigures;
  showTags?: boolean;
}) {
  const relationships = record.relationships ?? [];
  return (
    <div className="figures">
      {record.figures.map((figure) => (
        <div key={figure.id} className="figure">
          <span className="figure-name">{heading(figure)}</span>
          {figure.name.source === "missing" && (
            <span className="figure-note">{figure.name.reason}</span>
          )}
          <span className="figure-outline">
            {OUTLINE.map((f) => result(figure, f))
              .filter(Boolean)
              .join(" • ")}
          </span>
          {DETAILS.map(([field, label]) => (
            <span key={field} className="figure-detail">
              {label}: {result(figure, field) ?? "—"}
            </span>
          ))}
          {showTags && figure.name.source === "generated" && (
            <span className="figure-note">
              Named from {figure.name.scheme.name}
              {(figure.name.parts ?? []).map((p) => ` · ${p.label}: ${p.result.name}`).join("")}
            </span>
          )}
          {showTags && <TagList tags={figure.tags} />}
        </div>
      ))}
      {relationships.length > 0 && (
        <div className="figure-relationships">
          <span className="label">Relationships</span>
          {relationships.map((r) => (
            <span key={`${r.a}-${r.b}`} className="figure-detail">
              {pairLabel(record, r.a, r.b)} — {r.roll.result.name}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}

/**
 * Manual controls for the Authority Figures stage: generate them all, then
 * choose any figure's details or relationship, or name a figure.
 */
export function FiguresControls({
  stage,
  busy,
  onRoll,
  onAct,
}: {
  stage: StageView;
  busy: boolean;
  onRoll: () => void;
  onAct: (call: Promise<GenerationResponse>) => void;
}) {
  const view = stage.figures as FiguresView;
  const record = view.record;
  return (
    <div className="figures-controls">
      {stage.previous && stage.status === "pending" && (
        <p className="note">
          An earlier change affects this stage; it was <s>{stage.previous.name}</s>.
        </p>
      )}
      {stage.skipped === "not_recorded" && (
        <p className="note">Saved before Authority Figures existed.</p>
      )}
      {stage.skipped === "not_eligible" && (
        <p className="note">There is no Authority to represent.</p>
      )}
      {stage.can_roll && (
        <button type="button" className="btn btn-small" disabled={busy} onClick={onRoll}>
          {record ? "Reroll all figures" : "Generate figures"}
        </button>
      )}
      {!stage.can_roll && !record && (
        <p className="muted small">Resolve the Authority and its Authority Size first.</p>
      )}
      {record?.figures.map((figure, i) => (
        <FigureEditor
          key={figure.id}
          stage={stage.index}
          index={i}
          figure={figure}
          view={view}
          busy={busy}
          onAct={onAct}
        />
      ))}
      {record && (record.relationships ?? []).length > 0 && (
        <fieldset className="sub-editor figure-editor">
          <legend>Relationships</legend>
          {(record.relationships ?? []).map((r, i) => {
            const label = pairLabel(record, r.a, r.b);
            return (
              <label key={`${r.a}-${r.b}`} className="figure-field">
                <span>{label}</span>
                <select
                  className="select"
                  aria-label={`${label}: relationship`}
                  disabled={busy}
                  value={r.roll.result.id}
                  onChange={(e) =>
                    onAct(generation.chooseRelationship(stage.index, i, e.target.value))
                  }
                >
                  {view.relationship_options
                    .filter((o) => o.legal || o.result.id === r.roll.result.id)
                    .map((o) => (
                      <option key={o.result.id} value={o.result.id}>
                        {o.result.name}
                      </option>
                    ))}
                </select>
              </label>
            );
          })}
        </fieldset>
      )}
    </div>
  );
}

function FigureEditor({
  stage,
  index,
  figure,
  view,
  busy,
  onAct,
}: {
  stage: number;
  index: number;
  figure: AuthorityFigure;
  view: FiguresView;
  busy: boolean;
  onAct: (call: Promise<GenerationResponse>) => void;
}) {
  const [name, setName] = useState("");
  return (
    <fieldset className="sub-editor figure-editor">
      <legend>{heading(figure)}</legend>
      {figure.name.source === "missing" && (
        <p className="note small">{figure.name.reason} Type a name instead.</p>
      )}
      <form
        className="figure-name-form"
        onSubmit={(e) => {
          e.preventDefault();
          if (name.trim()) onAct(generation.figureName(stage, index, name));
          setName("");
        }}
      >
        <input
          className="text-input"
          aria-label={`${figure.label}: type a name`}
          placeholder="Type a name"
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
        <button type="submit" className="btn btn-small" disabled={busy || !name.trim()}>
          Set name
        </button>
        {figure.name.source !== "missing" && (
          <button
            type="button"
            className="btn btn-small"
            disabled={busy}
            onClick={() => onAct(generation.rollFigureName(stage, index))}
          >
            Roll name
          </button>
        )}
      </form>
      <div className="figure-fields">
        {(view.name_parts?.[index] ?? []).map((p) => {
          const current =
            figure.name.source === "generated" ? figure.name.rolls[p.part + 1]?.result : undefined;
          return (
            <label key={`name-${p.part}`} className="figure-field">
              <span>{p.label}</span>
              <select
                className="select"
                aria-label={`${figure.label}: ${p.label}`}
                disabled={busy}
                value={current?.id ?? ""}
                onChange={(e) =>
                  onAct(generation.chooseFigureNamePart(stage, index, p.part, e.target.value))
                }
              >
                {p.options
                  .filter((o) => o.legal || o.result.id === current?.id)
                  .map((o) => (
                    <option key={o.result.id} value={o.result.id}>
                      {o.result.name}
                    </option>
                  ))}
              </select>
            </label>
          );
        })}
        {view.fields.map((f) => {
          const current = figure.fields.find((x) => x.field === f.field)?.roll.result;
          return (
            <label key={f.field} className="figure-field">
              <span>{f.label}</span>
              <select
                className="select"
                aria-label={`${figure.label}: ${f.label}`}
                disabled={busy}
                value={current?.id ?? ""}
                onChange={(e) =>
                  onAct(generation.chooseFigureField(stage, index, f.field, e.target.value))
                }
              >
                {current && !f.options.some((o) => o.result.id === current.id) && (
                  <option value={current.id} disabled>
                    {current.name}
                  </option>
                )}
                {f.options
                  .filter((o) => o.legal || o.result.id === current?.id)
                  .map((o) => (
                    <option key={o.result.id} value={o.result.id}>
                      {o.result.name}
                    </option>
                  ))}
              </select>
            </label>
          );
        })}
      </div>
    </fieldset>
  );
}
