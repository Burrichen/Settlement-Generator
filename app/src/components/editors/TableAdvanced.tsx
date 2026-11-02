// Advanced parts of the table editor: tag-driven variants and follow-up
// rolls. Shown collapsed under "Advanced" in the table editor.
import {
  content,
  toPair,
  type FollowUpRoll,
  type RangeAssignment,
  type Table,
  type TableVariant,
} from "../../api";
import { OptionalCondition, RangeInput } from "./common";

// ----- variants ----------------------------------------------------------------

export function VariantsEditor({
  draft,
  set,
}: {
  draft: Table;
  set: (patch: Partial<Table>) => void;
}) {
  const variants = draft.variants ?? [];
  const update = (i: number, next: TableVariant) =>
    set({ variants: variants.map((v, j) => (j === i ? next : v)) });
  return (
    <section className="editor-section" aria-labelledby="table-variants">
      <h3 id="table-variants" className="label">
        Tag-driven variants
      </h3>
      <p className="muted small">
        A variant replaces the dice and/or ranges when its condition holds (first match wins).
      </p>
      {variants.map((variant, i) => (
        <fieldset key={variant.id} className="sub-editor">
          <legend>
            {variant.name || variant.id} <code>{variant.id}</code>
          </legend>
          <div className="field-grid">
            <label className="field">
              <span className="field-label">Name</span>
              <input
                className="text-input"
                value={variant.name}
                onChange={(e) => update(i, { ...variant, name: e.target.value })}
              />
            </label>
            <label className="field">
              <span className="field-label">Dice</span>
              <input
                className="text-input dice-input"
                value={variant.dice ?? ""}
                placeholder={`Same as table (${draft.dice ?? "none"})`}
                onChange={(e) => update(i, { ...variant, dice: e.target.value || undefined })}
              />
            </label>
          </div>
          <span className="field-label">Used when</span>
          <OptionalCondition
            label={`${variant.name} condition`}
            value={variant.when}
            onChange={(when) => update(i, { ...variant, when })}
            emptyLabel="Only when a tag effect asks for it"
          />
          <VariantRanges draft={draft} variant={variant} onChange={(v) => update(i, v)} />
          <button
            type="button"
            className="btn btn-small btn-quiet-danger"
            onClick={() => set({ variants: variants.filter((_, j) => j !== i) })}
          >
            Remove variant
          </button>
        </fieldset>
      ))}
      <button
        type="button"
        className="btn btn-small"
        onClick={async () =>
          set({
            variants: [...variants, { id: await content.newId("variant"), name: "New variant" }],
          })
        }
      >
        Add variant
      </button>
    </section>
  );
}

function VariantRanges({
  draft,
  variant,
  onChange,
}: {
  draft: Table;
  variant: TableVariant;
  onChange: (variant: TableVariant) => void;
}) {
  const custom = variant.ranges !== undefined;
  const ranges = variant.ranges ?? [];
  const rangeOf = (id: string) => ranges.find((a) => a.result === id)?.range;
  const setRange = (id: string, range: RangeAssignment["range"] | undefined) => {
    const rest = ranges.filter((a) => a.result !== id);
    const next = range === undefined ? rest : [...rest, { result: id, range }];
    // Keep the table's result order.
    next.sort(
      (a, b) =>
        draft.results.findIndex((r) => r.id === a.result) -
        draft.results.findIndex((r) => r.id === b.result),
    );
    onChange({ ...variant, ranges: next });
  };
  return (
    <div className="variant-ranges">
      <label className="check">
        <input
          type="checkbox"
          checked={custom}
          onChange={(e) =>
            onChange({
              ...variant,
              ranges: e.target.checked
                ? draft.results
                    .filter((r) => r.range !== undefined)
                    .map((r) => ({ result: r.id, range: r.range! }))
                : undefined,
            })
          }
        />
        Use its own ranges
      </label>
      {custom && (
        <ul className="range-list">
          {draft.results.map((r) => (
            <li key={r.id}>
              <span className="range-name">{r.name}</span>
              <RangeInput
                label={`${variant.name}: ${r.name} faces`}
                optional
                value={rangeOf(r.id)}
                onChange={(range) => setRange(r.id, range)}
              />
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

// ----- follow-up rolls -----------------------------------------------------------

export function FollowUpsEditor({
  draft,
  set,
}: {
  draft: Table;
  set: (patch: Partial<Table>) => void;
}) {
  const followUps = draft.follow_ups ?? [];
  const update = (i: number, next: FollowUpRoll) =>
    set({ follow_ups: followUps.map((f, j) => (j === i ? next : f)) });
  return (
    <section className="editor-section" aria-labelledby="table-follow-ups">
      <h3 id="table-follow-ups" className="label">
        Follow-up rolls
      </h3>
      <p className="muted small">
        A second physical roll made after certain results (optionally only when tags are present),
        which can change the result.
      </p>
      {followUps.map((f, i) => (
        <fieldset key={f.id} className="sub-editor">
          <legend>
            {f.name || f.id} <code>{f.id}</code>
          </legend>
          <div className="field-grid">
            <label className="field">
              <span className="field-label">Name</span>
              <input
                className="text-input"
                value={f.name}
                onChange={(e) => update(i, { ...f, name: e.target.value })}
              />
            </label>
            <label className="field">
              <span className="field-label">Dice</span>
              <input
                className="text-input dice-input"
                value={f.dice}
                onChange={(e) => update(i, { ...f, dice: e.target.value })}
              />
            </label>
          </div>
          <span className="field-label">Only when</span>
          <OptionalCondition
            label={`${f.name} condition`}
            value={f.when}
            onChange={(when) => update(i, { ...f, when })}
          />
          <span className="field-label">After these results</span>
          <span className="check-list" role="group" aria-label={`${f.name}: after results`}>
            {draft.results.map((r) => (
              <label key={r.id} className="check">
                <input
                  type="checkbox"
                  checked={f.on_results.includes(r.id)}
                  onChange={(e) =>
                    update(i, {
                      ...f,
                      on_results: e.target.checked
                        ? [...f.on_results, r.id]
                        : f.on_results.filter((x) => x !== r.id),
                    })
                  }
                />
                {r.name}
              </label>
            ))}
          </span>
          <span className="field-label">Outcomes</span>
          <ul className="range-list">
            {f.outcomes.map((o, k) => (
              <li key={k}>
                <RangeInput
                  label={`${f.name} outcome ${k + 1} faces`}
                  value={o.range}
                  onChange={(range) =>
                    update(i, {
                      ...f,
                      outcomes: f.outcomes.map((x, j) =>
                        j === k ? { ...x, range: range ?? toPair(x.range) } : x,
                      ),
                    })
                  }
                />
                <select
                  className="select select-inline"
                  aria-label={`${f.name} outcome ${k + 1} result`}
                  value={o.becomes ?? ""}
                  onChange={(e) =>
                    update(i, {
                      ...f,
                      outcomes: f.outcomes.map((x, j) =>
                        j === k ? { ...x, becomes: e.target.value || undefined } : x,
                      ),
                    })
                  }
                >
                  <option value="">Keep the original result</option>
                  {draft.results.map((r) => (
                    <option key={r.id} value={r.id}>
                      Change to {r.name}
                    </option>
                  ))}
                </select>
                <button
                  type="button"
                  className="icon-button"
                  aria-label={`Remove ${f.name} outcome ${k + 1}`}
                  onClick={() =>
                    update(i, { ...f, outcomes: f.outcomes.filter((_, j) => j !== k) })
                  }
                >
                  ×
                </button>
              </li>
            ))}
          </ul>
          <div className="button-row">
            <button
              type="button"
              className="btn btn-small"
              onClick={() => update(i, { ...f, outcomes: [...f.outcomes, { range: 1 }] })}
            >
              Add outcome
            </button>
            <button
              type="button"
              className="btn btn-small btn-quiet-danger"
              onClick={() => set({ follow_ups: followUps.filter((_, j) => j !== i) })}
            >
              Remove follow-up roll
            </button>
          </div>
        </fieldset>
      ))}
      <button
        type="button"
        className="btn btn-small"
        onClick={async () =>
          set({
            follow_ups: [
              ...followUps,
              {
                id: await content.newId("followUp"),
                name: "New follow-up roll",
                on_results: [],
                dice: "1d4",
                outcomes: [{ range: [1, 4] }],
              },
            ],
          })
        }
      >
        Add follow-up roll
      </button>
    </section>
  );
}
