// The name stage. Naming methods, rolls and the joining rules all come from
// the core; this only presents the stage and sends the user's choices.
import { useEffect, useState, type FormEvent } from "react";
import {
  formatRange,
  generation,
  type GenerationResponse,
  type NameAssembly,
  type NamingView,
  type StageView,
} from "../api";
import { MAX_NAME_LENGTH, nameProblem, OutcomeLine, ResultList } from "./generation";

export interface NameActions {
  onRoll: () => void;
  onChoose: (method: string) => void;
  onCustom: (name: string) => void;
  /** Runs a name-part action (roll or choose one part). */
  onAct: (call: Promise<GenerationResponse>) => void;
}

/** Name stage controls for the Incremental panel. */
export function NameControls({
  stage,
  busy,
  onRoll,
  onChoose,
  onCustom,
  onAct,
}: { stage: StageView; busy: boolean } & NameActions) {
  const naming = stage.naming as NamingView;
  const methods = stage.options.some((o) => o.legal);
  return (
    <div className="name-stage">
      <CurrentName stage={stage} />
      {stage.can_roll && (
        <div className="roll-row">
          <button type="button" className="btn btn-primary" disabled={busy} onClick={onRoll}>
            {naming.source === "generated" ? "Reroll name" : "Roll name"}
            {stage.dice && " "}
            {stage.dice && <span className="btn-detail">{stage.dice}</span>}
          </button>
          <OutcomeLine outcome={stage.outcome} />
        </div>
      )}
      <NameParts stage={stage} busy={busy} onAct={onAct} />
      {methods && (
        <>
          <span className="label results-label">Or choose a naming method</span>
          <ResultList stage={stage} disabled={busy} onChoose={onChoose} />
        </>
      )}
      <CustomNameForm stage={stage} busy={busy} onCustom={onCustom} />
      <NamingDetails naming={naming} />
    </div>
  );
}

/** The name, with how it was made, or why there is none. */
function CurrentName({ stage }: { stage: StageView }) {
  const naming = stage.naming as NamingView;
  return (
    <div className="name-current">
      {naming.name ? (
        <p className="display name-value">{naming.name}</p>
      ) : (
        <p className="muted">Not named yet.</p>
      )}
      <p className="muted small">
        {naming.source === "custom"
          ? "Typed name"
          : naming.source === "generated" && naming.method
            ? `${naming.scheme?.name ?? ""} naming · ${naming.method.name}`
            : null}
      </p>
      <NameNote stage={stage} />
    </div>
  );
}

/** Explains a name stage that cannot generate a name. */
function NameNote({ stage }: { stage: StageView }) {
  if (stage.skipped === "no_naming_scheme") {
    return (
      <p className="note">
        There is no naming data for this region yet. Type a name, or name the settlement when you
        save it.
      </p>
    );
  }
  if (stage.skipped === "not_recorded") {
    return <p className="note">Saved before settlement naming existed.</p>;
  }
  if (stage.previous && stage.status === "pending") {
    return (
      <p className="note">
        An earlier change affects the name. It was <s>{stage.previous.name}</s>.
      </p>
    );
  }
  return null;
}

/**
 * The parts of a generated name, each of which can be chosen or rolled
 * again on its own. Every option shows the name it would give, so approved
 * variants (e.g. Oswald: A → Oswalden, B → Oswaldwick) can be inspected.
 */
export function NameParts({
  stage,
  busy,
  onAct,
}: {
  stage: StageView;
  busy: boolean;
  onAct: (call: Promise<GenerationResponse>) => void;
}) {
  const parts = stage.naming?.part_options ?? [];
  if (parts.length === 0) return null;
  return (
    <fieldset className="name-parts">
      <legend className="label">Name parts</legend>
      {parts.map((part) => {
        const id = `name-part-${stage.index}-${part.part}`;
        const legal = part.options
          .map((o, i) => ({ option: o, preview: part.previews?.[i] ?? null }))
          .filter(({ option }) => option.legal);
        const current = part.current ?? "";
        return (
          <div key={part.part} className="name-part-row">
            <label htmlFor={id} className="small">
              {part.label}
            </label>
            <select
              id={id}
              className="select"
              disabled={busy}
              value={current}
              onChange={(e) =>
                e.target.value &&
                onAct(generation.chooseNamePart(stage.index, part.part, e.target.value))
              }
            >
              <option value="" disabled>
                Choose…
              </option>
              {legal.map(({ option, preview }) => (
                <option key={option.result.id} value={option.result.id}>
                  {option.result.name}
                  {preview && preview !== option.result.name ? ` → ${preview}` : ""}
                  {option.range !== undefined ? ` (${formatRange(option.range)})` : ""}
                </option>
              ))}
            </select>
            <button
              type="button"
              className="btn btn-small"
              disabled={busy}
              onClick={() => onAct(generation.rollNamePart(stage.index, part.part))}
            >
              Roll <span className="visually-hidden">{part.label}</span>
            </button>
          </div>
        );
      })}
    </fieldset>
  );
}

/** "Joined: compound", the exception used, or the approved name read. */
function assemblyText(assembly: NameAssembly): string {
  if (assembly.exception) {
    const note = assembly.exception.note ? ` (${assembly.exception.note})` : "";
    return `Spelling exception: ${assembly.exception.name}${note}`;
  }
  if (assembly.curated) {
    const c = assembly.curated;
    return `Approved name of ${c.source} (${c.table}) chosen by variant ${c.variant}`;
  }
  return `Joined: ${assembly.join}`;
}

/** Type a name: always possible, with or without naming data. */
function CustomNameForm({
  stage,
  busy,
  onCustom,
  compact = false,
}: {
  stage: StageView;
  busy: boolean;
  onCustom: (name: string) => void;
  compact?: boolean;
}) {
  const naming = stage.naming as NamingView;
  const [value, setValue] = useState(naming.name ?? "");
  const [problem, setProblem] = useState<string | null>(null);
  // Follow the stage when it is rolled or chosen elsewhere.
  useEffect(() => setValue(naming.name ?? ""), [naming.name]);

  const submit = (event: FormEvent) => {
    event.preventDefault();
    const issue = nameProblem(value);
    setProblem(issue);
    if (!issue) onCustom(value.trim());
  };
  const inputId = `custom-name-${stage.index}`;
  const unchanged = value.trim() === (naming.name ?? "");
  return (
    <form className={`custom-name ${compact ? "is-compact" : ""}`} onSubmit={submit}>
      <label htmlFor={inputId} className={compact ? "visually-hidden" : "label results-label"}>
        {compact ? `Type a name for ${stage.stage.name}` : "Or type a name"}
      </label>
      <div className="custom-name-row">
        <input
          id={inputId}
          className="text-input"
          value={value}
          placeholder="Type a name"
          maxLength={MAX_NAME_LENGTH + 20}
          aria-invalid={problem ? true : undefined}
          onChange={(e) => {
            setValue(e.target.value);
            if (problem) setProblem(null);
          }}
        />
        <button
          type="submit"
          className={`btn ${compact ? "btn-small" : ""}`}
          disabled={busy || unchanged}
        >
          Use name
        </button>
      </div>
      {problem && (
        <p className="field-error" role="alert">
          {problem}
        </p>
      )}
    </form>
  );
}

/** Scheme routing and every roll behind the name (Tag View only). */
export function NamingDetails({ naming }: { naming?: NamingView }) {
  const parts = naming?.parts ?? [];
  const routing = naming?.routing ?? [];
  if (!naming || (parts.length === 0 && routing.length === 0)) return null;
  return (
    <dl className="naming-details">
      {routing.length > 0 && (
        <>
          <dt>Naming scheme</dt>
          <dd>
            {naming.scheme?.name} — chosen by {routing.map((t) => t.name).join(" + ")}
          </dd>
        </>
      )}
      {parts.map((part) => (
        <div key={part.table.id} className="naming-part">
          <dt>{part.table.name}</dt>
          <dd>
            {part.result.name} <code className="muted">{part.result.id}</code>
            {part.roll && !part.chosen ? (
              <span className="muted">
                {" "}
                — rolled {part.roll} on {part.dice}
              </span>
            ) : (
              <span className="muted"> — chosen</span>
            )}
          </dd>
        </div>
      ))}
      {naming.assembly && (
        <>
          <dt>Assembly</dt>
          <dd>{assemblyText(naming.assembly)}</dd>
        </>
      )}
    </dl>
  );
}

/** The name stage as a row of the One-table grid (and edits). */
export function NameGridRow({
  stage,
  busy,
  onRoll,
  onChoose,
  onCustom,
  onAct,
}: { stage: StageView; busy: boolean } & NameActions) {
  const naming = stage.naming as NamingView;
  const legal = stage.options.filter((o) => o.legal);
  const selectId = `stage-select-${stage.index}`;
  return (
    <tr className={`stage-row name-row status-${stage.status}`}>
      <th scope="row">
        <span className="stage-row-name">{stage.stage.name}</span>
        {legal.length > 0 && <span className="stage-row-table">{stage.table.name}</span>}
      </th>
      <td>
        <CurrentName stage={stage} />
        {legal.length > 0 && (
          <select
            id={selectId}
            className="select"
            aria-label="Naming method"
            disabled={busy}
            value={naming.source === "generated" ? (naming.method?.id ?? "") : ""}
            onChange={(e) => e.target.value && onChoose(e.target.value)}
          >
            <option value="" disabled>
              Choose a naming method…
            </option>
            {legal.map((o) => (
              <option key={o.result.id} value={o.result.id}>
                {o.result.name}
              </option>
            ))}
          </select>
        )}
        <NameParts stage={stage} busy={busy} onAct={onAct} />
        <CustomNameForm stage={stage} busy={busy} onCustom={onCustom} compact />
        <NamingDetails naming={naming} />
      </td>
      <td className="roll-cell">
        {stage.can_roll && (
          <button type="button" className="btn btn-small" disabled={busy} onClick={onRoll}>
            {naming.source === "generated" ? "Reroll" : "Roll"}{" "}
            <span className="visually-hidden">{stage.stage.name}</span>
          </button>
        )}
        <OutcomeLine outcome={naming.source === "generated" ? stage.outcome : undefined} />
      </td>
    </tr>
  );
}
