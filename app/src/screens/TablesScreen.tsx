import { useCallback, useEffect, useRef, useState, type KeyboardEvent } from "react";
import {
  content,
  toPair,
  type Coverage,
  type FollowUpRoll,
  type Named,
  type RangeAssignment,
  type Table,
  type TableDetail,
  type TableResult,
  type TableVariant,
} from "../api";
import { useApp } from "../app-state";
import { ConfirmBar } from "../components/generation";
import { ScreenFrame } from "../components/ScreenFrame";
import {
  CatalogProvider,
  chanceOf,
  CoverageStrip,
  IssueList,
  OptionalCondition,
  originLabel,
  RangeInput,
  TagMultiSelect,
  TagSelect,
  useCatalog,
} from "../components/editors/common";
import { ItemList, useEntityEditor } from "../components/editors/EntityEditor";
import {
  ApprovedNamesEditor,
  approvedVariants,
  NamingMethodsEditor,
} from "../components/editors/NamingEditors";
import { EditorActions } from "./TagsScreen";

/** View Tables: every effective table, edited through one generic form. */
export function TablesScreen() {
  return (
    <CatalogProvider>
      <TablesWorkspace />
    </CatalogProvider>
  );
}

const normalizeTable = (t: Table): Table => ({
  ...t,
  description: t.description ?? "",
  active: t.active ?? true,
  results: t.results.map((r) => ({
    ...r,
    description: r.description ?? "",
    tags: r.tags ?? [],
    enabled: r.enabled ?? true,
  })),
  variants: t.variants ?? [],
  follow_ups: t.follow_ups ?? [],
});

function TablesWorkspace() {
  const { back } = useApp();
  const { overview, refresh } = useCatalog();
  const [filter, setFilter] = useState("");
  const [confirm, setConfirm] = useState<null | "delete">(null);
  const [confirmResetAll, setConfirmResetAll] = useState(false);
  const [deletedId, setDeletedId] = useState<string | null>(null);
  const opened = useRef(false);

  const editor = useEntityEditor<Table, TableDetail>({
    load: content.table,
    entityOf: useCallback((d: TableDetail) => normalizeTable(d.table), []),
    check: content.checkTable,
    save: content.saveTable,
  });

  useEffect(() => {
    if (!opened.current && overview?.tables.length) {
      opened.current = true;
      const first = overview.tables.find((t) => !t.deleted);
      if (first) void editor.open(first.id);
    }
  }, [overview, editor]);

  const deleted = deletedId ? overview?.tables.find((t) => t.id === deletedId) : undefined;

  const afterStructuralChange = async (select?: string) => {
    const next = await refresh();
    setDeletedId(null);
    const target = select ?? next?.tables.find((t) => !t.deleted)?.id;
    if (target) await editor.open(target);
    else editor.clear();
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape" && editor.dirty) {
      event.preventDefault();
      editor.guard(back);
    }
  };

  return (
    <div onKeyDown={onKeyDown}>
      <ScreenFrame title="Tables" backLabel="Settings" onBack={() => editor.guard(back)} wide>
        <div className="view-layout">
          <ItemList
            label="Tables"
            filter={filter}
            onFilter={setFilter}
            selectedId={deletedId ?? editor.selectedId}
            onSelect={(id) => {
              const item = overview?.tables.find((t) => t.id === id);
              if (item?.deleted) {
                editor.guard(() => {
                  editor.clear();
                  setDeletedId(id);
                });
              } else {
                setDeletedId(null);
                editor.select(id);
              }
            }}
            items={(overview?.tables ?? []).map((t) => ({
              id: t.id,
              name: t.name,
              sub: t.id,
              badges: [
                originLabel(t.origin, t.deleted),
                ...(t.incomplete ? ["Incomplete"] : []),
                ...(!t.active && !t.deleted ? ["Disabled"] : []),
              ].filter(Boolean),
              muted: !t.active || t.deleted,
              problem: t.errors > 0,
            }))}
            footer={
              <>
                <button
                  type="button"
                  className="btn btn-small"
                  onClick={() =>
                    content.newTable().then(
                      (t) => editor.startNew(normalizeTable(t)),
                      (e: unknown) => editor.setMessage({ text: String(e), error: true }),
                    )
                  }
                >
                  New table
                </button>
                {confirmResetAll ? (
                  <ConfirmBar
                    message="Restore all tables and tags to the installed defaults? Every customisation and custom item is removed."
                    confirmLabel="Restore all"
                    onConfirm={async () => {
                      setConfirmResetAll(false);
                      try {
                        await content.resetAll();
                      } catch (e) {
                        editor.setMessage({ text: String(e), error: true });
                        return;
                      }
                      editor.clear();
                      await afterStructuralChange(editor.selectedId ?? undefined).catch(() =>
                        afterStructuralChange(),
                      );
                    }}
                    onCancel={() => setConfirmResetAll(false)}
                  />
                ) : (
                  <button
                    type="button"
                    className="btn btn-small btn-quiet-danger"
                    onClick={() => editor.guard(() => setConfirmResetAll(true))}
                  >
                    Restore all defaults
                  </button>
                )}
              </>
            }
          />
          <section className="saved-detail-pane" aria-label="Table editor">
            {editor.pending && (
              <ConfirmBar
                message="Discard your unsaved changes?"
                confirmLabel="Discard changes"
                onConfirm={editor.confirmPending}
                onCancel={editor.cancelPending}
              />
            )}
            {deleted ? (
              <div className="editor">
                <h2 className="display saved-title">{deleted.name}</h2>
                <p className="muted">
                  This default table was deleted. Restoring it brings back the installed default.
                </p>
                <button
                  type="button"
                  className="btn"
                  onClick={async () => {
                    await content
                      .resetTable(deleted.id)
                      .catch((e) => editor.setMessage({ text: String(e), error: true }));
                    await afterStructuralChange(deleted.id);
                  }}
                >
                  Restore default
                </button>
              </div>
            ) : editor.draft ? (
              <TableForm
                key={editor.draft.id}
                editor={editor}
                confirm={confirm}
                setConfirm={setConfirm}
                afterStructuralChange={afterStructuralChange}
              />
            ) : (
              <p className="muted">Choose a table to edit.</p>
            )}
          </section>
        </div>
      </ScreenFrame>
    </div>
  );
}

type Editor = ReturnType<typeof useEntityEditor<Table, TableDetail>>;

function TableForm({
  editor,
  confirm,
  setConfirm,
  afterStructuralChange,
}: {
  editor: Editor;
  confirm: null | "delete";
  setConfirm: (v: null | "delete") => void;
  afterStructuralChange: (select?: string) => Promise<void>;
}) {
  const { refresh } = useCatalog();
  const draft = editor.draft!;
  const builtIn = editor.detail?.built_in ?? false;
  const origin = editor.detail?.origin ?? null;
  const set = (patch: Partial<Table>) => editor.setDraft({ ...draft, ...patch });
  const check = editor.liveCheck;
  const coverage = check?.coverage ?? editor.detail?.coverage ?? [];
  const issues = check ? check.issues : (editor.detail?.issues ?? []);
  const hasErrors = Boolean(check?.error) || issues.some((i) => i.severity === "error");
  const nameOf = (id: string) => draft.results.find((r) => r.id === id)?.name ?? id;
  const base = coverage.find((c) => c.label === "Base layout");
  const rolled = draft.selection !== "choose";
  const lockedForBuiltIn = builtIn ? "Fixed for built-in tables." : undefined;

  return (
    <div className="editor">
      <header className="editor-header">
        <h2 className="display saved-title">{draft.name || "Unnamed table"}</h2>
        <p className="saved-meta">
          ID <code>{draft.id}</code> ·{" "}
          {editor.isNew ? "New custom table (not saved)" : originLabel(origin, false)}
        </p>
        {draft.status === "incomplete" && (
          <p className="note">
            Marked incomplete{draft.status_note ? `: ${draft.status_note}` : "."} Generation stops
            here unless a result is chosen.
          </p>
        )}
      </header>

      <section className="editor-section" aria-labelledby="table-basics">
        <h3 id="table-basics" className="label">
          Basics
        </h3>
        <div className="field-grid">
          <label className="field">
            <span className="field-label">Name</span>
            <input
              className="text-input"
              value={draft.name}
              onChange={(e) => set({ name: e.target.value })}
            />
          </label>
          <label className="field">
            <span className="field-label">Dice</span>
            <input
              className="text-input dice-input"
              value={draft.dice ?? ""}
              placeholder={rolled ? "e.g. 1d20" : "Chosen, not rolled"}
              disabled={!rolled}
              onChange={(e) => set({ dice: e.target.value || undefined })}
            />
            <span className="field-help">
              d4, d6, d8, d10, d12, d20, d100, or sums such as 2d6 or 1d8+1d4.
            </span>
          </label>
          <label className="field">
            <span className="field-label">How results are found</span>
            <select
              className="select select-inline"
              value={draft.selection ?? "roll"}
              disabled={builtIn}
              title={lockedForBuiltIn}
              onChange={(e) =>
                set({ selection: e.target.value === "choose" ? "choose" : undefined })
              }
            >
              <option value="roll">Rolled with dice</option>
              <option value="choose">Always chosen</option>
            </select>
          </label>
        </div>
        <label className="field">
          <span className="field-label">Description</span>
          <textarea
            className="text-input text-area"
            value={draft.description ?? ""}
            onChange={(e) => set({ description: e.target.value })}
          />
        </label>
        <label className="toggle">
          <input
            type="checkbox"
            role="switch"
            checked={draft.active ?? true}
            onChange={(e) => set({ active: e.target.checked })}
          />
          Active
          <span className="field-help"> Inactive tables are skipped by generation.</span>
        </label>
      </section>

      {coverage.length > 0 && (
        <section className="editor-section" aria-labelledby="table-coverage">
          <h3 id="table-coverage" className="label">
            Dice coverage
          </h3>
          {coverage.map((c: Coverage) => (
            <CoverageStrip key={c.label} coverage={c} nameOf={nameOf} />
          ))}
        </section>
      )}

      <ResultsEditor
        draft={draft}
        set={set}
        base={base}
        rolled={rolled}
        approved={approvedVariants(editor.detail)}
      />

      <section className="editor-section" aria-labelledby="table-eligibility">
        <h3 id="table-eligibility" className="label">
          When the table is rolled
        </h3>
        <OptionalCondition
          label="Table eligibility"
          value={draft.eligibility}
          onChange={(eligibility) => set({ eligibility })}
          emptyLabel="Always rolled when reached"
        />
      </section>

      {rolled && <VariantsEditor draft={draft} set={set} />}
      {rolled && <FollowUpsEditor draft={draft} set={set} />}

      {editor.detail && (
        <NamingMethodsEditor
          detail={editor.detail}
          onSaved={(d) => {
            editor.replaceDetail(d);
            void refresh();
          }}
        />
      )}

      <section className="editor-section" aria-labelledby="table-problems">
        <h3 id="table-problems" className="label">
          Validation
        </h3>
        <IssueList issues={issues} error={check?.error} />
      </section>

      <EditorActions
        editor={editor}
        hasErrors={hasErrors}
        canReset={builtIn && origin === "modified"}
        canDelete={!builtIn && !editor.isNew}
        confirm={confirm}
        setConfirm={setConfirm}
        onSaved={() => void refresh()}
        onReset={async () => {
          await content.resetTable(draft.id);
          await afterStructuralChange(draft.id);
        }}
        onDelete={async () => {
          await content.deleteTable(draft.id);
          await afterStructuralChange();
        }}
        noun="table"
      />
    </div>
  );
}

// ----- results ----------------------------------------------------------------

function ResultsEditor({
  draft,
  set,
  base,
  rolled,
  approved,
}: {
  draft: Table;
  set: (patch: Partial<Table>) => void;
  base: Coverage | undefined;
  rolled: boolean;
  /** Variants each result needs an approved name for (curated naming). */
  approved: Named[];
}) {
  const update = (i: number, patch: Partial<TableResult>) =>
    set({ results: draft.results.map((r, j) => (j === i ? { ...r, ...patch } : r)) });
  // Tables whose results stand for numbers (e.g. Authority Size) show them.
  const valued = draft.results.some((r) => r.value !== undefined && r.value !== null);
  return (
    <section className="editor-section" aria-labelledby="table-results">
      <h3 id="table-results" className="label">
        Results
      </h3>
      <div className="edit-grid-scroll">
        <table className="edit-grid">
          <thead>
            <tr>
              {rolled && <th scope="col">Faces</th>}
              {rolled && <th scope="col">Chance</th>}
              <th scope="col">Result</th>
              {valued && <th scope="col">Value</th>}
              <th scope="col">Emits tags</th>
              <th scope="col">Enabled</th>
              <th scope="col">
                <span className="visually-hidden">Remove</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {draft.results.map((result, i) => (
              <ResultRow
                key={result.id}
                result={result}
                siblings={draft.results}
                rolled={rolled}
                valued={valued}
                chance={chanceOf(base, result.id)}
                approved={approved}
                onChange={(patch) => update(i, patch)}
                onRemove={() => set({ results: draft.results.filter((_, j) => j !== i) })}
              />
            ))}
          </tbody>
        </table>
      </div>
      <button
        type="button"
        className="btn btn-small"
        onClick={async () =>
          set({
            results: [
              ...draft.results,
              {
                id: await content.newId("result"),
                name: "New result",
                description: "",
                tags: [],
                enabled: true,
              },
            ],
          })
        }
      >
        Add result
      </button>
    </section>
  );
}

function ResultRow({
  result,
  siblings,
  rolled,
  valued,
  chance,
  approved,
  onChange,
  onRemove,
}: {
  result: TableResult;
  /** Every result of the table (fallback choices). */
  siblings: TableResult[];
  rolled: boolean;
  valued: boolean;
  chance: string;
  approved: Named[];
  onChange: (patch: Partial<TableResult>) => void;
  onRemove: () => void;
}) {
  const [open, setOpen] = useState(Boolean(result.eligibility));
  const roleOptionsSet = Boolean(result.unique || result.label_variants?.length);
  const [roleOpen, setRoleOpen] = useState(roleOptionsSet);
  const variants = result.label_variants ?? [];
  const setVariant = (i: number, patch: Partial<{ tag: string; name: string }>) =>
    onChange({
      label_variants: variants.map((v, j) => (j === i ? { ...v, ...patch } : v)),
    });
  const label = result.name || result.id;
  return (
    <>
      <tr>
        {rolled && (
          <td>
            <RangeInput
              label={`${label} faces`}
              optional
              value={result.range}
              onChange={(range) => onChange({ range })}
            />
          </td>
        )}
        {rolled && <td className="chance">{chance}</td>}
        <td>
          <input
            className="text-input"
            aria-label={`${label}: name`}
            value={result.name}
            onChange={(e) => onChange({ name: e.target.value })}
          />
          <span className="cell-sub">
            <code>{result.id}</code>
            <button
              type="button"
              className="link-button"
              aria-expanded={open}
              onClick={() => setOpen(!open)}
            >
              {result.eligibility ? "Eligibility rule" : "Add eligibility rule"}
            </button>
            <button
              type="button"
              className="link-button"
              aria-expanded={roleOpen}
              onClick={() => setRoleOpen(!roleOpen)}
            >
              Role options
            </button>
          </span>
        </td>
        {valued && (
          <td>
            <input
              className="text-input number-input"
              type="number"
              min={1}
              step={1}
              aria-label={`${label}: value`}
              value={result.value ?? ""}
              onChange={(e) =>
                onChange({
                  value: e.target.value === "" ? undefined : Math.trunc(Number(e.target.value)),
                })
              }
            />
          </td>
        )}
        <td>
          <TagMultiSelect
            label={`${label} tags`}
            value={result.tags ?? []}
            onChange={(tags) => onChange({ tags })}
          />
        </td>
        <td>
          <input
            type="checkbox"
            aria-label={`${label}: enabled`}
            checked={result.enabled ?? true}
            onChange={(e) => onChange({ enabled: e.target.checked })}
          />
        </td>
        <td>
          <button
            type="button"
            className="icon-button"
            aria-label={`Remove ${label}`}
            onClick={onRemove}
          >
            ×
          </button>
        </td>
      </tr>
      {(approved.length > 0 || Object.keys(result.approved_names ?? {}).length > 0) && (
        <tr className="sub-row">
          <td colSpan={(rolled ? 6 : 4) + (valued ? 1 : 0)}>
            <ApprovedNamesEditor
              result={result}
              variants={approved}
              onChange={(approved_names) => onChange({ approved_names })}
            />
          </td>
        </tr>
      )}
      {open && (
        <tr className="sub-row">
          <td colSpan={(rolled ? 6 : 4) + (valued ? 1 : 0)}>
            <span className="field-label">Eligible only when</span>
            <OptionalCondition
              label={`${label} eligibility`}
              value={result.eligibility}
              onChange={(eligibility) => onChange({ eligibility })}
            />
          </td>
        </tr>
      )}
      {roleOpen && (
        <tr className="sub-row">
          <td colSpan={(rolled ? 6 : 4) + (valued ? 1 : 0)}>
            <span className="field-help">
              Used when allocating Authority roles. A unique result can be held once; a repeat
              becomes its fallback, without a reroll.
            </span>
            <label className="check-row">
              <input
                type="checkbox"
                aria-label={`${label}: unique`}
                checked={result.unique ?? false}
                onChange={(e) =>
                  onChange(
                    e.target.checked
                      ? { unique: true }
                      : { unique: undefined, fallback: undefined },
                  )
                }
              />
              Unique
            </label>
            {result.unique && (
              <label className="field">
                <span className="field-label">Becomes, when already held</span>
                <select
                  className="select select-inline"
                  aria-label={`${label}: fallback`}
                  value={result.fallback ?? ""}
                  onChange={(e) => onChange({ fallback: e.target.value || undefined })}
                >
                  <option value="">Choose a repeatable result…</option>
                  {siblings
                    .filter((r) => r.id !== result.id && !r.unique)
                    .map((r) => (
                      <option key={r.id} value={r.id}>
                        {r.name}
                      </option>
                    ))}
                </select>
              </label>
            )}
            <span className="field-label">Display variants</span>
            {variants.map((v, i) => (
              <span key={i} className="variant-row">
                <TagSelect
                  label={`${label}: display variant ${i + 1} tag`}
                  value={v.tag}
                  onChange={(tag) => setVariant(i, { tag })}
                />
                <input
                  className="text-input"
                  aria-label={`${label}: display variant ${i + 1} label`}
                  value={v.name}
                  onChange={(e) => setVariant(i, { name: e.target.value })}
                />
                <button
                  type="button"
                  className="icon-button"
                  aria-label={`Remove display variant ${i + 1}`}
                  onClick={() => onChange({ label_variants: variants.filter((_, j) => j !== i) })}
                >
                  ×
                </button>
              </span>
            ))}
            <button
              type="button"
              className="btn btn-small"
              onClick={() => onChange({ label_variants: [...variants, { tag: "", name: "" }] })}
            >
              Add display variant
            </button>
          </td>
        </tr>
      )}
    </>
  );
}

// ----- variants ----------------------------------------------------------------

function VariantsEditor({ draft, set }: { draft: Table; set: (patch: Partial<Table>) => void }) {
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

function FollowUpsEditor({ draft, set }: { draft: Table; set: (patch: Partial<Table>) => void }) {
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
