// The settings of the selected step. Every kind of step gets a small form;
// pickers list tables, flows, tags, settings and objects by name.
import { useEffect, useState, type ReactNode } from "react";
import {
  flows,
  type Count,
  type Explanation,
  type FlowContext,
  type FlowStep,
  type Modifier,
  type Named,
  type NodeText,
} from "../../api";
import { ExplanationView } from "../editors/Explanation";
import { ConditionBuilder, OptionalWhen } from "./ConditionBuilder";

/** Most levels a Run Subflow step may recurse (the core's subflow nesting limit). */
export const MAX_DEPTH = 16;

function Picker({
  label,
  value,
  options,
  onChange,
  placeholder = "Choose…",
}: {
  label: string;
  value: string;
  options: Named[];
  onChange: (id: string) => void;
  placeholder?: string;
}) {
  const known = options.some((o) => o.id === value);
  return (
    <label className="field">
      <span className="field-label">{label}</span>
      <select className="select" value={value} onChange={(e) => onChange(e.target.value)}>
        {value === "" && <option value="">{placeholder}</option>}
        {value !== "" && !known && <option value={value}>{value} (missing)</option>}
        {options.map((o) => (
          <option key={o.id} value={o.id}>
            {o.name}
          </option>
        ))}
      </select>
    </label>
  );
}

type ModifierKind = "shift" | "plus" | "minus" | "cap_at_highest" | "cap_at_lowest";

function modifierKind(m: Modifier): ModifierKind {
  if (typeof m === "string") return m;
  if ("shift" in m) return "shift";
  if ("plus" in m) return "plus";
  return "minus";
}

function modifierAmount(m: Modifier): number {
  if (typeof m === "string") return 1;
  if ("shift" in m) return m.shift;
  if ("plus" in m) return m.plus;
  return m.minus;
}

export function ModifierPicker({
  value,
  onChange,
  label,
}: {
  value: Modifier;
  onChange: (m: Modifier) => void;
  label: string;
}) {
  const kind = modifierKind(value);
  const amount = modifierAmount(value);
  const make = (k: ModifierKind, n: number): Modifier =>
    k === "cap_at_highest" || k === "cap_at_lowest"
      ? k
      : k === "shift"
        ? { shift: n }
        : k === "plus"
          ? { plus: Math.abs(n) || 1 }
          : { minus: Math.abs(n) || 1 };
  return (
    <span className="modifier-picker">
      <select
        className="select select-inline"
        aria-label={`${label} kind`}
        value={kind}
        onChange={(e) => onChange(make(e.target.value as ModifierKind, amount))}
      >
        <option value="shift">Shift along the scale</option>
        <option value="plus">Plus (add to the roll)</option>
        <option value="minus">Minus (take from the roll)</option>
        <option value="cap_at_highest">Cap at Highest</option>
        <option value="cap_at_lowest">Cap at Lowest</option>
      </select>
      {(kind === "shift" || kind === "plus" || kind === "minus") && (
        <input
          className="num-input"
          type="number"
          aria-label={`${label} amount`}
          value={amount}
          onChange={(e) => onChange(make(kind, Number(e.target.value) || 0))}
        />
      )}
      {kind === "shift" && (
        <span className="preset-row">
          {[-2, -1, 1, 2].map((n) => (
            <button
              key={n}
              type="button"
              className="btn btn-small"
              aria-pressed={amount === n}
              onClick={() => onChange({ shift: n })}
            >
              {n > 0 ? `+${n}` : n}
            </button>
          ))}
        </span>
      )}
    </span>
  );
}

/** The core's plain-English account of a modifier on `table`. */
function ModifierExplanation({ table, modifier }: { table: string; modifier: Modifier }) {
  const [explanation, setExplanation] = useState<Explanation | null>(null);
  const key = JSON.stringify([table, modifier]);
  useEffect(() => {
    if (!table) return setExplanation(null);
    let live = true;
    const timer = setTimeout(() => {
      flows.explainModifier(table, modifier).then(
        (e) => live && setExplanation(e),
        () => live && setExplanation(null),
      );
    }, 200);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [key]);
  return <ExplanationView explanation={explanation} />;
}

function CountEditor({
  value,
  onChange,
  context,
}: {
  value: Count;
  onChange: (c: Count) => void;
  context: FlowContext;
}) {
  const mode = "times" in value ? "times" : "value_of" in value ? "value_of" : "dice";
  const numeric = context.subjects.filter((s) => s.numeric);
  return (
    <div className="field">
      <span className="field-label">How many times</span>
      <div className="check-list" role="radiogroup" aria-label="How many times">
        {(
          [
            ["times", "A fixed number"],
            ["value_of", "A number from an earlier step"],
            ["dice", "Roll dice"],
          ] as const
        ).map(([m, text]) => (
          <label key={m} className="check">
            <input
              type="radio"
              name="count-mode"
              checked={mode === m}
              onChange={() =>
                onChange(
                  m === "times"
                    ? { times: 1 }
                    : m === "dice"
                      ? { dice: "1d4" }
                      : { value_of: { step: numeric[0]?.step ?? "" } },
                )
              }
            />
            {text}
          </label>
        ))}
      </div>
      {"times" in value && (
        <input
          className="num-input"
          type="number"
          min={0}
          aria-label="Number of times"
          value={value.times}
          onChange={(e) => onChange({ times: Math.max(0, Number(e.target.value) || 0) })}
        />
      )}
      {"dice" in value && (
        <input
          className="text-input dice-input"
          aria-label="Dice for the count"
          value={value.dice}
          onChange={(e) => onChange({ dice: e.target.value })}
        />
      )}
      {"value_of" in value && (
        <span className="cond-row">
          <select
            className="select select-inline"
            aria-label="Step whose number is used"
            value={value.value_of.step}
            onChange={(e) => onChange({ value_of: { ...value.value_of, step: e.target.value } })}
          >
            {value.value_of.step === "" && <option value="">Choose…</option>}
            {numeric.map((s) => (
              <option key={s.step} value={s.step}>
                {s.label}
              </option>
            ))}
          </select>
          <span className="cond-or">plus</span>
          <input
            className="num-input"
            type="number"
            aria-label="Added to the number"
            value={value.value_of.add ?? 0}
            onChange={(e) => {
              const add = Number(e.target.value) || 0;
              const next: { step: string; add?: number } = { step: value.value_of.step };
              if (add !== 0) next.add = add;
              onChange({ value_of: next });
            }}
          />
        </span>
      )}
    </div>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="inspector-section" aria-label={title}>
      <h4 className="label">{title}</h4>
      {children}
    </section>
  );
}

export function StepInspector({
  step,
  text,
  context,
  detached,
  onChange,
  actions,
}: {
  step: FlowStep;
  text: NodeText | undefined;
  context: FlowContext;
  detached: boolean;
  onChange: (step: FlowStep, key?: string) => void;
  actions: {
    duplicate: () => void;
    remove: () => void;
    detach: () => void;
    moveBy: (delta: number) => void;
    addCase: () => void;
    removeCase: (index: number) => void;
  };
}) {
  const set = (patch: Partial<FlowStep>, key?: string) =>
    onChange({ ...step, ...patch } as FlowStep, key);
  const settings = context.settings.map((s) => ({ id: s.id, name: s.name }));
  const editable = step.kind !== "builtin" && step.kind !== "name";

  return (
    <div className="inspector" aria-label="Step settings">
      <p className="inspector-kind label">{text?.kind ?? step.kind}</p>
      <p className="inspector-summary">{text?.text}</p>
      {detached && (
        <p className="field-help">
          Not connected: drag it onto a + in the flow to connect it. It never runs until then.
        </p>
      )}
      <div className="toolbar" role="toolbar" aria-label="Step tools">
        {!detached && (
          <>
            <button type="button" className="btn btn-small" onClick={() => actions.moveBy(-1)}>
              Move up
            </button>
            <button type="button" className="btn btn-small" onClick={() => actions.moveBy(1)}>
              Move down
            </button>
            <button
              type="button"
              className="btn btn-small"
              onClick={actions.duplicate}
              title="Ctrl+D"
            >
              Duplicate
            </button>
            <button type="button" className="btn btn-small" onClick={actions.detach}>
              Disconnect
            </button>
          </>
        )}
        <button
          type="button"
          className="btn btn-small btn-quiet-danger"
          onClick={actions.remove}
          title="Delete"
        >
          Delete step
        </button>
      </div>

      <label className="field">
        <span className="field-label">Label</span>
        <input
          className="text-input"
          value={step.name ?? ""}
          placeholder="Shown instead of the table's name"
          onChange={(e) =>
            set({ name: e.target.value === "" ? undefined : e.target.value }, "name")
          }
        />
      </label>

      {!editable && (
        <p className="field-help">
          This step is provided by the application; its own settings are edited on their own
          screens. Its label, condition and status can be changed here.
        </p>
      )}

      {step.kind === "roll" && (
        <>
          <Picker
            label="Table"
            value={step.table}
            options={context.tables}
            onChange={(table) => set({ table })}
          />
          <label className="field">
            <span className="field-label">Store on the object as (optional)</span>
            <input
              className="text-input"
              value={step.field ?? ""}
              onChange={(e) => set({ field: e.target.value || undefined }, "field")}
            />
          </label>
          <Section title="Modifiers on this roll">
            {(step.modifiers ?? []).map((m, i) => (
              <div key={i} className="effect">
                <ModifierPicker
                  label={`Modifier ${i + 1}`}
                  value={m.modifier}
                  onChange={(modifier) =>
                    set({
                      modifiers: step.modifiers!.map((x, j) => (j === i ? { ...x, modifier } : x)),
                    })
                  }
                />
                <ModifierExplanation table={step.table} modifier={m.modifier} />
                <OptionalWhen
                  value={m.when}
                  context={context}
                  label={`Modifier ${i + 1} condition`}
                  onChange={(when) =>
                    set({
                      modifiers: step.modifiers!.map((x, j) => {
                        if (j !== i) return x;
                        const next = { ...x, when };
                        if (when === undefined) delete next.when;
                        return next;
                      }),
                    })
                  }
                />
                <button
                  type="button"
                  className="btn btn-small btn-quiet-danger"
                  onClick={() => set({ modifiers: step.modifiers!.filter((_, j) => j !== i) })}
                >
                  Remove modifier
                </button>
              </div>
            ))}
            <button
              type="button"
              className="btn btn-small"
              onClick={() =>
                set({ modifiers: [...(step.modifiers ?? []), { modifier: { shift: 1 } }] })
              }
            >
              Add modifier
            </button>
          </Section>
        </>
      )}

      {step.kind === "run_flow" && (
        <>
          <Picker
            label="Flow to run"
            value={step.flow}
            options={context.flows}
            onChange={(flow) => set({ flow })}
          />
          <label className="check-row">
            <input
              type="checkbox"
              checked={step.max_depth !== undefined}
              onChange={(e) => set({ max_depth: e.target.checked ? 3 : undefined })}
            />
            Allow it to run itself (deliberate recursion)
          </label>
          {step.max_depth !== undefined && (
            <label className="field">
              <span className="field-label">Maximum depth</span>
              <input
                className="num-input"
                type="number"
                min={1}
                max={MAX_DEPTH}
                value={step.max_depth}
                onChange={(e) => set({ max_depth: Number(e.target.value) || 0 }, "depth")}
              />
              <span className="field-help">
                It stops quietly after this many levels (1–{MAX_DEPTH}).
              </span>
            </label>
          )}
        </>
      )}

      {step.kind === "branch" && (
        <Section title="Cases (the first that holds runs)">
          {step.cases.map((c, i) => (
            <div key={i} className="effect">
              <p className="field-label">{i === 0 ? "If" : "Otherwise if"}</p>
              <ConditionBuilder
                value={c.when}
                context={context}
                label={`Case ${i + 1} condition`}
                onChange={(when) =>
                  set({ cases: step.cases.map((x, j) => (j === i ? { ...x, when } : x)) })
                }
              />
              {step.cases.length > 1 && (
                <button
                  type="button"
                  className="btn btn-small btn-quiet-danger"
                  onClick={() => actions.removeCase(i)}
                >
                  Remove case {i + 1}
                </button>
              )}
            </div>
          ))}
          <button type="button" className="btn btn-small" onClick={actions.addCase}>
            Add case
          </button>
          <p className="field-help">
            Steps for each case, and for Otherwise, are added in the flow.
          </p>
        </Section>
      )}

      {step.kind === "create_object" && (
        <>
          <Picker
            label="Object type"
            value={step.object}
            options={context.objects}
            onChange={(object) => set({ object })}
          />
          <label className="check-row">
            <input
              type="checkbox"
              checked={step.share_tags ?? false}
              onChange={(e) => set({ share_tags: e.target.checked || undefined })}
            />
            Its tags also apply to the rest of the generation
          </label>
        </>
      )}

      {step.kind === "repeat" && (
        <CountEditor value={step.count} context={context} onChange={(count) => set({ count })} />
      )}

      {step.kind === "for_each" && (
        <>
          <Picker
            label="For each object of type"
            value={step.object}
            options={context.objects}
            onChange={(object) => set({ object })}
          />
          <label className="check-row">
            <input
              type="checkbox"
              checked={step.pairs ?? false}
              onChange={(e) => set({ pairs: e.target.checked || undefined })}
            />
            Once for every unique pair of them (e.g. a relationship per pair)
          </label>
        </>
      )}

      {step.kind === "set_value" && (
        <div className="field-grid">
          <label className="field">
            <span className="field-label">Field</span>
            <input
              className="text-input"
              value={step.field}
              onChange={(e) => set({ field: e.target.value }, "field")}
            />
          </label>
          <label className="field">
            <span className="field-label">Value</span>
            <input
              className="num-input"
              type="number"
              value={step.value}
              onChange={(e) => set({ value: Number(e.target.value) || 0 }, "value")}
            />
          </label>
        </div>
      )}

      {(step.kind === "add_tag" || step.kind === "remove_tag") && (
        <Picker
          label="Tag"
          value={step.tag}
          options={context.tags}
          onChange={(tag) => set({ tag })}
        />
      )}

      {step.kind === "apply_modifier" && (
        <>
          <Picker
            label="Every later roll of"
            value={step.table}
            options={context.tables}
            onChange={(table) => set({ table })}
          />
          <div className="field">
            <span className="field-label">Modifier</span>
            <ModifierPicker
              label="Modifier"
              value={step.modifier}
              onChange={(modifier) => set({ modifier })}
            />
            <ModifierExplanation table={step.table} modifier={step.modifier} />
          </div>
        </>
      )}

      {step.kind === "read_setting" && (
        <Picker
          label="Setting"
          value={step.setting}
          options={settings}
          onChange={(setting) => set({ setting })}
        />
      )}

      {step.kind === "secondary_roll" && (
        <div className="field-grid">
          <label className="field">
            <span className="field-label">Label</span>
            <input
              className="text-input"
              value={step.label}
              onChange={(e) => set({ label: e.target.value }, "label")}
            />
          </label>
          <label className="field">
            <span className="field-label">Dice</span>
            <input
              className="text-input dice-input"
              value={step.dice}
              onChange={(e) => set({ dice: e.target.value }, "dice")}
            />
          </label>
        </div>
      )}

      {step.kind === "stop" && (
        <p className="field-help">Ends the flow it is in. Steps after it never run.</p>
      )}

      <Section title="Condition">
        <OptionalWhen
          value={step.when}
          context={context}
          label="Step condition"
          onChange={(when) => {
            const next = { ...step, when } as FlowStep;
            if (when === undefined) delete next.when;
            onChange(next);
          }}
        />
      </Section>

      <Section title="Status">
        <label className="check-row">
          <input
            type="checkbox"
            checked={step.active ?? true}
            onChange={(e) => set({ active: e.target.checked ? undefined : false })}
          />
          Active
        </label>
        <label className="check-row">
          <input
            type="checkbox"
            checked={step.status === "planned"}
            onChange={(e) => set({ status: e.target.checked ? "planned" : undefined })}
          />
          Planned (a design note: never runs)
        </label>
        <label className="field">
          <span className="field-label">Description</span>
          <textarea
            className="text-input text-area"
            value={step.description ?? ""}
            onChange={(e) => set({ description: e.target.value || undefined }, "description")}
          />
        </label>
      </Section>
      <p className="saved-meta">
        Step ID <code>{step.id}</code>
      </p>
    </div>
  );
}
