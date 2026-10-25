// The no-code table editor. The default view shows only what a new table
// needs: its name, its results, whether they are equally likely, and the
// dice. The Dice Assistant (in the core) lays results out on real tabletop
// dice and explains, in plain English, when no clean die fits. Everything
// else is under "Advanced", collapsed.
import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import {
  content,
  framework,
  type Coverage,
  type DiceAdvice,
  type DiceFit,
  type FrameworkItemListing,
  type Table,
  type TableDetail,
  type TableResult,
  type Usage,
} from "../../api";
import {
  CoverageStrip,
  IssueList,
  OptionalCondition,
  RangeInput,
  TableSelect,
  TagMultiSelect,
  TagSelect,
  chanceOf,
  originLabel,
  useCatalog,
} from "./common";
import type { useEntityEditor } from "./EntityEditor";
import { FollowUpsEditor, VariantsEditor } from "./TableAdvanced";
import { EditorActions } from "../../screens/TagsScreen";
import { TestPanel } from "./TestPanel";
import { UsageLinks } from "./UsageLinks";

type Editor = ReturnType<typeof useEntityEditor<Table, TableDetail>>;

/** The name the core gives the result that marks faces rolled again. */
export const ROLL_AGAIN_NAME = "Roll again";

export const isRollAgain = (r: TableResult) =>
  r.enabled === false &&
  r.name === ROLL_AGAIN_NAME &&
  (r.tags ?? []).length === 0 &&
  (r.then_roll ?? []).length === 0;

/** Sides of a single die (`1d10` → 10), or null for sums and modifiers. */
export function singleDie(dice: string | undefined): number | null {
  const match = /^\s*1?\s*d\s*(\d+)\s*$/i.exec(dice ?? "");
  return match ? Number(match[1]) : null;
}

/** Results that take part in the chances: enabled, and not "Roll again". */
export const entriesOf = (table: Table) =>
  table.results.filter((r) => (r.enabled ?? true) && !isRollAgain(r));

/** Each entry's relative chance: its number of faces on a single die. */
export function weightsOf(table: Table): number[] {
  const single = singleDie(table.dice) !== null;
  return entriesOf(table).map((r) => {
    if (!single || r.range === undefined) return 1;
    const [a, b] = typeof r.range === "number" ? [r.range, r.range] : r.range;
    return Math.max(1, b - a + 1);
  });
}

type ChanceMode = "equal" | "custom";

function modeOf(table: Table): ChanceMode {
  const weights = weightsOf(table);
  return weights.every((w) => w === weights[0]) ? "equal" : "custom";
}

/** Runs a core call, turning a missing or failing call into `null`. */
async function attempt<T>(call: () => Promise<T>): Promise<T | null> {
  try {
    return await call();
  } catch {
    return null;
  }
}

const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);

/**
 * Undo/redo over the editor's draft. Typing in one field within a second
 * counts as one change.
 */
function useHistory(editor: Editor) {
  const past = useRef<Table[]>([]);
  const future = useRef<Table[]>([]);
  const last = useRef<{ key: string; at: number } | null>(null);
  const [, bump] = useState(0);
  const draft = editor.draft!;
  const id = draft.id;
  useEffect(() => {
    past.current = [];
    future.current = [];
    last.current = null;
    bump((n) => n + 1);
  }, [id]);

  const set = useCallback(
    (next: Table, key = "") => {
      if (same(next, editor.draft)) return;
      const now = Date.now();
      const coalesce = key !== "" && last.current?.key === key && now - last.current.at < 1000;
      if (!coalesce && editor.draft) past.current.push(editor.draft);
      future.current = [];
      last.current = { key, at: now };
      editor.setDraft(next);
      bump((n) => n + 1);
    },
    [editor],
  );
  const undo = () => {
    const previous = past.current.pop();
    if (!previous || !editor.draft) return;
    future.current.push(editor.draft);
    last.current = null;
    editor.setDraft(previous);
    bump((n) => n + 1);
  };
  const redo = () => {
    const next = future.current.pop();
    if (!next || !editor.draft) return;
    past.current.push(editor.draft);
    last.current = null;
    editor.setDraft(next);
    bump((n) => n + 1);
  };
  return {
    set,
    undo,
    redo,
    canUndo: past.current.length > 0,
    canRedo: future.current.length > 0,
  };
}

/**
 * Keeps unsaved changes in the core as a draft (debounced), and offers a
 * draft kept from an earlier session.
 */
function useKeptDraft(editor: Editor) {
  const draft = editor.draft!;
  const [status, setStatus] = useState("");
  const [restorable, setRestorable] = useState<Table | null>(null);
  const checked = useRef<string | null>(null);

  useEffect(() => {
    if (checked.current === draft.id) return;
    checked.current = draft.id;
    setRestorable(null);
    setStatus("");
    void attempt(() => content.tableDraft(draft.id)).then((kept) => {
      if (kept && !same(kept, editor.draft)) setRestorable(kept);
    });
  }, [draft.id, editor.draft]);

  useEffect(() => {
    if (!editor.dirty) return;
    const timer = setTimeout(() => {
      void attempt(() => content.saveTableDraft(draft)).then((ok) => {
        if (ok !== null) setStatus("Unsaved changes kept as a draft");
      });
    }, 800);
    return () => clearTimeout(timer);
  }, [draft, editor.dirty]);

  return {
    status,
    restorable,
    dismiss: () => {
      setRestorable(null);
      void attempt(() => content.discardTableDraft(draft.id));
    },
    take: () => setRestorable(null),
  };
}

export function TableForm({
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
  const history = useHistory(editor);
  const kept = useKeptDraft(editor);
  const builtIn = editor.detail?.built_in ?? false;
  const origin = editor.detail?.origin ?? null;
  const usages = editor.detail?.usages ?? [];
  const set = (patch: Partial<Table>, key = "") => history.set({ ...draft, ...patch }, key);
  const check = editor.liveCheck;
  const coverage = check?.coverage ?? editor.detail?.coverage ?? [];
  const issues = check ? check.issues : (editor.detail?.issues ?? []);
  const hasErrors = Boolean(check?.error) || issues.some((i) => i.severity === "error");
  const base = coverage.find((c) => c.label === "Base layout");
  const rolled = draft.selection !== "choose";
  const [mode, setMode] = useState<ChanceMode>(() => modeOf(draft));
  const [advanced, setAdvanced] = useState(false);
  const [note, setNote] = useState<string | null>(null);

  useEffect(() => setMode(modeOf(draft)), [draft.id]);

  /** Lays `next`'s entries out on its dice with `weights` (the core fits). */
  const relayout = async (next: Table, weights?: number[]) => {
    const w = weights ?? (mode === "equal" ? entriesOf(next).map(() => 1) : weightsOf(next));
    if (!rolled || !next.dice || w.length === 0) return history.set(next);
    const fit = await attempt(() => content.diceFit(w, next.dice!));
    if (!fit) {
      setNote(
        `${next.dice} cannot hold ${w.length} results. The Dice Assistant below suggests dice that can.`,
      );
      return history.set(next);
    }
    const applied = await attempt(() =>
      content.applyDiceFit(
        next,
        entriesOf(next).map((r) => r.id),
        fit,
      ),
    );
    setNote(fit.kind === "approximate" ? fit.explanation : null);
    history.set(applied ?? next);
  };

  const applyFit = async (fit: DiceFit) => {
    const applied = await attempt(() =>
      content.applyDiceFit(
        draft,
        entriesOf(draft).map((r) => r.id),
        fit,
      ),
    );
    if (applied) {
      history.set(applied);
      setNote(null);
    }
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const ctrl = event.ctrlKey || event.metaKey;
    if (!ctrl) return;
    const target = event.target as HTMLElement;
    // Text fields keep their own undo while typing.
    if (target.tagName === "INPUT" && (target as HTMLInputElement).type === "text") return;
    if (event.key.toLowerCase() === "z" && !event.shiftKey) {
      event.preventDefault();
      history.undo();
    } else if (
      event.key.toLowerCase() === "y" ||
      (event.key.toLowerCase() === "z" && event.shiftKey)
    ) {
      event.preventDefault();
      history.redo();
    }
  };

  const errors = issues.filter((i) => i.severity === "error").length + (check?.error ? 1 : 0);
  const warnings = issues.filter((i) => i.severity === "warning").length;

  return (
    <div className="editor" onKeyDown={onKeyDown}>
      <header className="editor-header">
        <label className="field">
          <span className="field-label">Table name</span>
          <input
            className="text-input title-input"
            value={draft.name}
            onChange={(e) => set({ name: e.target.value }, "name")}
          />
        </label>
        <p className="saved-meta">
          ID <code>{draft.id}</code> ·{" "}
          {editor.isNew ? "New custom table (not saved)" : originLabel(origin, false)}
          {kept.status && <> · {kept.status}</>}
        </p>
        <div className="toolbar" role="toolbar" aria-label="Table tools">
          <button
            type="button"
            className="btn btn-small"
            disabled={!history.canUndo}
            onClick={history.undo}
            title="Ctrl+Z"
          >
            Undo
          </button>
          <button
            type="button"
            className="btn btn-small"
            disabled={!history.canRedo}
            onClick={history.redo}
            title="Ctrl+Y"
          >
            Redo
          </button>
          <button
            type="button"
            className="btn btn-small"
            disabled={editor.isNew}
            onClick={async () => {
              const copy = await attempt(() => content.copyTable(draft.id));
              if (copy) editor.startNew(copy);
            }}
          >
            Copy to a new table
          </button>
        </div>
        {kept.restorable && (
          <div className="note draft-note" role="status">
            You have unsaved changes to this table from earlier.{" "}
            <button
              type="button"
              className="btn btn-small"
              onClick={() => {
                history.set(kept.restorable!);
                kept.take();
              }}
            >
              Restore them
            </button>{" "}
            <button type="button" className="btn btn-small" onClick={kept.dismiss}>
              Discard them
            </button>
          </div>
        )}
        {draft.status === "incomplete" && (
          <p className="note">
            Marked incomplete{draft.status_note ? `: ${draft.status_note}` : "."} Generation stops
            here unless a result is chosen.
          </p>
        )}
        <ValidationSummary errors={errors} warnings={warnings} />
      </header>

      {rolled && (
        <section className="editor-section" aria-labelledby="table-chances">
          <h3 id="table-chances" className="label">
            Chances and dice
          </h3>
          <div className="field-grid">
            <div className="field" role="radiogroup" aria-label="Chance mode">
              <span className="field-label">Chance mode</span>
              <span className="segmented">
                <label>
                  <input
                    type="radio"
                    name="chance-mode"
                    checked={mode === "equal"}
                    onChange={() => {
                      setMode("equal");
                      void relayout(
                        draft,
                        entriesOf(draft).map(() => 1),
                      );
                    }}
                  />
                  <span>Equal</span>
                </label>
                <label>
                  <input
                    type="radio"
                    name="chance-mode"
                    checked={mode === "custom"}
                    onChange={() => setMode("custom")}
                  />
                  <span>Custom</span>
                </label>
              </span>
              <span className="field-help">
                {mode === "equal"
                  ? "Every result is equally likely."
                  : "Give each result a relative chance: 2 is twice as likely as 1."}
              </span>
            </div>
            <label className="field">
              <span className="field-label">Dice</span>
              <input
                className="text-input dice-input"
                value={draft.dice ?? ""}
                placeholder="e.g. 1d10"
                onChange={(e) => set({ dice: e.target.value || undefined }, "dice")}
                onBlur={() => void relayout(draft)}
              />
              <span className="field-help">
                Standard dice: d4, d6, d8, d10, d12, d20, d100, or sums such as 2d6.
              </span>
            </label>
          </div>
          {note && <p className="note">{note}</p>}
          <DiceAssistant
            table={draft}
            mode={mode}
            base={base}
            onUse={applyFit}
            onAdd={addResults}
          />
        </section>
      )}

      <ResultsSheet
        draft={draft}
        mode={mode}
        rolled={rolled}
        base={base}
        relayout={relayout}
        setTable={(t, key) => history.set(t, key)}
      />

      <TestPanel target={{ kind: "table", table: draft }} disabled={hasErrors} />

      {rolled && base && (
        <TestTable
          coverage={base}
          nameOf={(id) => draft.results.find((r) => r.id === id)?.name ?? id}
        />
      )}

      <section className="editor-section">
        <button
          type="button"
          className="link-button disclosure"
          aria-expanded={advanced}
          onClick={() => setAdvanced(!advanced)}
        >
          {advanced ? "▾" : "▸"} Advanced
        </button>
        {advanced && (
          <div className="advanced">
            <label className="field">
              <span className="field-label">Description</span>
              <textarea
                className="text-input text-area"
                value={draft.description ?? ""}
                onChange={(e) => set({ description: e.target.value }, "description")}
              />
            </label>
            <label className="field">
              <span className="field-label">How results are found</span>
              <select
                className="select select-inline"
                value={draft.selection ?? "roll"}
                disabled={builtIn}
                title={builtIn ? "Fixed for built-in tables." : undefined}
                onChange={(e) =>
                  set({ selection: e.target.value === "choose" ? "choose" : undefined })
                }
              >
                <option value="roll">Rolled with dice</option>
                <option value="choose">Always chosen</option>
              </select>
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
            <span className="field-label">When the table is rolled</span>
            <OptionalCondition
              label="Table eligibility"
              value={draft.eligibility}
              onChange={(eligibility) => set({ eligibility })}
              emptyLabel="Always rolled when reached"
            />
            {coverage.length > 0 && (
              <>
                <span className="field-label">Every face of the dice</span>
                {coverage.map((c: Coverage) => (
                  <CoverageStrip
                    key={c.label}
                    coverage={c}
                    nameOf={(id) => draft.results.find((r) => r.id === id)?.name ?? id}
                  />
                ))}
              </>
            )}
            {rolled && <VariantsEditor draft={draft} set={(p) => set(p)} />}
            {rolled && <FollowUpsEditor draft={draft} set={(p) => set(p)} />}
            <UsedBy usages={usages} />
          </div>
        )}
      </section>

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
          await attempt(() => content.discardTableDraft(draft.id));
          await afterStructuralChange(draft.id);
        }}
        onDelete={async () => {
          await content.deleteTable(draft.id);
          await afterStructuralChange();
        }}
        noun="table"
        usages={usages}
        exportItem={{ kind: "table", id: draft.id }}
      />
    </div>
  );

  async function addResults(count: number) {
    const added: TableResult[] = [];
    for (let i = 0; i < count; i++) {
      added.push({
        id: await content.newId("result"),
        name: `New result ${draft.results.length + i + 1}`,
        description: "",
        tags: [],
        enabled: true,
      });
    }
    await relayout({ ...draft, results: [...draft.results, ...added] });
  }
}

function ValidationSummary({ errors, warnings }: { errors: number; warnings: number }) {
  if (errors === 0 && warnings === 0) {
    return <p className="issues-ok small">No problems: this table can be saved.</p>;
  }
  const parts = [
    errors > 0 && `${errors} problem${errors === 1 ? "" : "s"} to fix before saving`,
    warnings > 0 && `${warnings} warning${warnings === 1 ? "" : "s"}`,
  ].filter(Boolean);
  return (
    <p className={errors > 0 ? "note validation-summary" : "muted small validation-summary"}>
      {parts.join(" · ")} (see Validation below).
    </p>
  );
}

function UsedBy({ usages }: { usages: Usage[] }) {
  return (
    <div className="used-by">
      <span className="field-label">Used by</span>
      {usages.length === 0 ? (
        <p className="muted small">Nothing uses this table, so it can be deleted.</p>
      ) : (
        <UsageLinks usages={usages} label="Used by" />
      )}
    </div>
  );
}

const pct = (p: number) => {
  const v = p * 100;
  return Math.abs(v - Math.round(v)) < 1e-9 ? `${Math.round(v)}%` : `${v.toFixed(2)}%`;
};

const faces = (r: number | [number, number]) =>
  typeof r === "number" || r[0] === r[1]
    ? `face ${typeof r === "number" ? r : r[0]}`
    : `faces ${r[0]}–${r[1]}`;

/** Plain-English problems with the current faces. */
function coverageProblems(coverage: Coverage | undefined, nameOf: (id: string) => string) {
  if (!coverage) return [];
  const out: string[] = [];
  if (coverage.dice_error) out.push(`The dice cannot be read: ${coverage.dice_error}.`);
  for (const r of coverage.uncovered) out.push(`No result is on ${faces(r)}.`);
  for (const r of coverage.overlapping) out.push(`More than one result is on ${faces(r)}.`);
  for (const r of coverage.impossible)
    out.push(`${nameOf(r.result)} is on ${faces(r.range)}, which ${coverage.dice} cannot roll.`);
  return out;
}

function DiceAssistant({
  table,
  mode,
  base,
  onUse,
  onAdd,
}: {
  table: Table;
  mode: ChanceMode;
  base: Coverage | undefined;
  onUse: (fit: DiceFit) => void;
  onAdd: (count: number) => void;
}) {
  const weights = useMemo(
    () => (mode === "equal" ? entriesOf(table).map(() => 1) : weightsOf(table)),
    [table, mode],
  );
  const key = weights.join(",");
  const [advice, setAdvice] = useState<DiceAdvice | null>(null);
  useEffect(() => {
    let live = true;
    void attempt(() => content.diceAdvice(weights)).then((a) => {
      if (live) setAdvice(a);
    });
    return () => {
      live = false;
    };
  }, [key]);
  const nameOf = (id: string) => table.results.find((r) => r.id === id)?.name ?? id;
  const problems = coverageProblems(base, nameOf);
  return (
    <div className="assistant" aria-label="Dice Assistant" role="region">
      <span className="field-label">Dice Assistant</span>
      {problems.length > 0 ? (
        <ul className="assistant-problems">
          {problems.map((p) => (
            <li key={p}>{p}</li>
          ))}
        </ul>
      ) : (
        base && <p className="issues-ok small">Every face is covered exactly once</p>
      )}
      {advice && (
        <>
          <p>{advice.summary}</p>
          <ul className="suggestions">
            {advice.suggestions.slice(0, 4).map((fit) => {
              const current =
                fit.dice === table.dice &&
                fit.kind !== "approximate" &&
                problems.length === 0 &&
                same(
                  fit.ranges,
                  entriesOf(table).map((r) => r.range),
                );
              return (
                <li key={`${fit.dice}-${fit.kind}`} className="suggestion">
                  <span className="suggestion-label">{fit.label}</span>
                  <span className="muted small">{fit.explanation}</span>
                  {current ? (
                    <span className="badge">In use</span>
                  ) : (
                    <button type="button" className="btn btn-small" onClick={() => onUse(fit)}>
                      Use {fit.label}
                    </button>
                  )}
                </li>
              );
            })}
          </ul>
          {advice.entry_changes?.map((change) => (
            <p key={change.explanation} className="muted small">
              {change.explanation}{" "}
              {change.kind === "add" && (
                <button type="button" className="btn btn-small" onClick={() => onAdd(change.count)}>
                  Add {change.count} result{change.count === 1 ? "" : "s"}
                </button>
              )}
            </p>
          ))}
        </>
      )}
    </div>
  );
}

// ----- results ------------------------------------------------------------------

function ResultsSheet({
  draft,
  mode,
  rolled,
  base,
  relayout,
  setTable,
}: {
  draft: Table;
  mode: ChanceMode;
  rolled: boolean;
  base: Coverage | undefined;
  relayout: (next: Table, weights?: number[]) => Promise<void>;
  setTable: (table: Table, key?: string) => void;
}) {
  const [filter, setFilter] = useState("");
  const [rowMessage, setRowMessage] = useState<string | null>(null);
  const [shared, setShared] = useState<FrameworkItemListing[]>([]);
  useEffect(() => {
    void attempt(() => framework.items()).then((items) =>
      setShared((items ?? []).filter((i) => i.kind === "shared_result" && !i.deleted)),
    );
  }, []);
  const valued = draft.results.some((r) => r.value !== undefined && r.value !== null);
  const auto = rolled && singleDie(draft.dice) !== null;
  const replace = (results: TableResult[]) => ({ ...draft, results });
  const update = (i: number, patch: Partial<TableResult>, key = "") =>
    setTable(
      replace(draft.results.map((r, j) => (j === i ? { ...r, ...patch } : r))),
      key ? `${draft.results[i].id}:${key}` : "",
    );
  const changeLayout = (results: TableResult[], weights?: number[]) => {
    const next = replace(results);
    if (auto) void relayout(next, weights);
    else setTable(next);
  };
  const move = (i: number, by: number) => {
    const j = i + by;
    if (j < 0 || j >= draft.results.length) return;
    const results = [...draft.results];
    [results[i], results[j]] = [results[j], results[i]];
    changeLayout(results);
  };
  const remove = async (i: number) => {
    const result = draft.results[i];
    const usages = await attempt(() => content.resultUsages(draft, result.id));
    if (usages && usages.length > 0) {
      setRowMessage(
        `“${result.name || result.id}” cannot be deleted while it is used by: ${usages
          .map((u) => u.description)
          .join("; ")}. Remove those uses first, or disable the result instead.`,
      );
      return;
    }
    setRowMessage(null);
    changeLayout(draft.results.filter((_, j) => j !== i));
  };
  const duplicate = async (i: number) => {
    const source = draft.results[i];
    const copy: TableResult = {
      ...source,
      id: await content.newId("result"),
      name: `Copy of ${source.name || source.id}`,
      unique: undefined,
      fallback: undefined,
    };
    const results = [...draft.results];
    results.splice(i + 1, 0, copy);
    changeLayout(results);
  };
  const setWeight = (i: number, weight: number) => {
    const entries = entriesOf(draft);
    const index = entries.findIndex((r) => r.id === draft.results[i].id);
    if (index < 0 || !auto) return;
    const weights = weightsOf(draft);
    weights[index] = Math.max(1, Math.trunc(weight) || 1);
    void relayout(draft, weights);
  };
  const shown = draft.results
    .map((r, i) => ({ r, i }))
    .filter(({ r }) => `${r.name} ${r.id}`.toLowerCase().includes(filter.trim().toLowerCase()));
  return (
    <section className="editor-section" aria-labelledby="table-results">
      <h3 id="table-results" className="label">
        Results
      </h3>
      {draft.results.length > 8 && (
        <input
          className="text-input list-filter"
          type="search"
          placeholder="Find a result"
          aria-label="Find a result"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
        />
      )}
      {rowMessage && (
        <p className="note" role="alert">
          {rowMessage}
        </p>
      )}
      <div className="edit-grid-scroll">
        <table className="edit-grid results-sheet">
          <thead>
            <tr>
              <th scope="col">
                <span className="visually-hidden">Order</span>
              </th>
              <th scope="col">Result</th>
              {rolled && <th scope="col">Dice range</th>}
              {rolled && <th scope="col">Chance</th>}
              {rolled && mode === "custom" && auto && <th scope="col">Relative chance</th>}
              {valued && <th scope="col">Value</th>}
              <th scope="col">Shared result</th>
              <th scope="col">On</th>
              <th scope="col">
                <span className="visually-hidden">Actions</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {shown.map(({ r: result, i }) => (
              <ResultRow
                key={result.id}
                result={result}
                index={i}
                count={draft.results.length}
                siblings={draft.results}
                rolled={rolled}
                weight={
                  mode === "custom" && auto && entriesOf(draft).some((e) => e.id === result.id)
                    ? weightsOf(draft)[entriesOf(draft).findIndex((e) => e.id === result.id)]
                    : undefined
                }
                showWeight={rolled && mode === "custom" && auto}
                valued={valued}
                chance={chanceOf(base, result.id)}
                shared={shared}
                onChange={(patch, key) => update(i, patch, key)}
                onLayoutChange={(patch) =>
                  changeLayout(draft.results.map((r, j) => (j === i ? { ...r, ...patch } : r)))
                }
                onWeight={(w) => setWeight(i, w)}
                onMove={(by) => move(i, by)}
                onDuplicate={() => void duplicate(i)}
                onRemove={() => void remove(i)}
                onShare={async () => {
                  const id = await content.newId("sharedResult");
                  const saved = await attempt(() =>
                    framework.saveItem("shared_result", {
                      id,
                      name: result.name,
                      description: result.description || undefined,
                      tags: result.tags?.length ? result.tags : undefined,
                      value: result.value,
                    }),
                  );
                  if (saved) {
                    setShared(saved.filter((s) => s.kind === "shared_result" && !s.deleted));
                    update(i, { shared: id });
                  } else {
                    setRowMessage("The shared result could not be created.");
                  }
                }}
              />
            ))}
          </tbody>
        </table>
      </div>
      <button
        type="button"
        className="btn btn-small"
        onClick={async () =>
          changeLayout([
            ...draft.results,
            {
              id: await content.newId("result"),
              name: "New result",
              description: "",
              tags: [],
              enabled: true,
            },
          ])
        }
      >
        Add result
      </button>
    </section>
  );
}

function ResultRow({
  result,
  index,
  count,
  siblings,
  rolled,
  weight,
  showWeight,
  valued,
  chance,
  shared,
  onChange,
  onLayoutChange,
  onWeight,
  onMove,
  onDuplicate,
  onRemove,
  onShare,
}: {
  result: TableResult;
  index: number;
  count: number;
  siblings: TableResult[];
  rolled: boolean;
  weight: number | undefined;
  showWeight: boolean;
  valued: boolean;
  chance: string;
  shared: FrameworkItemListing[];
  onChange: (patch: Partial<TableResult>, key?: string) => void;
  onLayoutChange: (patch: Partial<TableResult>) => void;
  onWeight: (weight: number) => void;
  onMove: (by: number) => void;
  onDuplicate: () => void;
  onRemove: () => void;
  onShare: () => void;
}) {
  const [open, setOpen] = useState(false);
  const sharedName = shared.find((s) => s.id === result.shared)?.name;
  const label = result.name || sharedName || result.id;
  const variants = result.label_variants ?? [];
  const thenRoll = result.then_roll ?? [];
  const setVariant = (i: number, patch: Partial<{ tag: string; name: string }>) =>
    onChange({ label_variants: variants.map((v, j) => (j === i ? { ...v, ...patch } : v)) });
  const columns = 5 + (rolled ? 2 : 0) + (showWeight ? 1 : 0) + (valued ? 1 : 0);
  return (
    <>
      <tr className={result.enabled === false ? "is-disabled" : undefined}>
        <td className="order-cell">
          <button
            type="button"
            className="icon-button"
            aria-label={`Move ${label} up`}
            disabled={index === 0}
            onClick={() => onMove(-1)}
          >
            ↑
          </button>
          <button
            type="button"
            className="icon-button"
            aria-label={`Move ${label} down`}
            disabled={index === count - 1}
            onClick={() => onMove(1)}
          >
            ↓
          </button>
        </td>
        <td>
          <input
            className="text-input"
            aria-label={`${label}: name`}
            value={result.name}
            placeholder={sharedName}
            onChange={(e) => onChange({ name: e.target.value }, "name")}
          />
        </td>
        {rolled && (
          <td>
            <RangeInput
              label={`${label} faces`}
              optional
              value={result.range}
              onChange={(range) => onChange({ range }, "range")}
            />
          </td>
        )}
        {rolled && <td className="chance">{chance}</td>}
        {showWeight && (
          <td>
            {weight !== undefined && (
              <input
                className="text-input number-input"
                type="number"
                min={1}
                step={1}
                aria-label={`${label}: relative chance`}
                value={weight}
                onChange={(e) => onWeight(Number(e.target.value))}
              />
            )}
          </td>
        )}
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
                onChange(
                  {
                    value: e.target.value === "" ? undefined : Math.trunc(Number(e.target.value)),
                  },
                  "value",
                )
              }
            />
          </td>
        )}
        <td>
          <select
            className="select select-inline"
            aria-label={`${label}: shared result`}
            value={result.shared ?? ""}
            onChange={(e) =>
              onChange(
                e.target.value
                  ? // Linking clears the fields the shared result supplies.
                    { shared: e.target.value, name: "", tags: [] }
                  : { shared: undefined, name: result.name || sharedName || "" },
              )
            }
          >
            <option value="">— its own —</option>
            {shared.map((s) => (
              <option key={s.id} value={s.id}>
                {s.name}
              </option>
            ))}
            {result.shared && !sharedName && <option value={result.shared}>{result.shared}</option>}
          </select>
        </td>
        <td>
          <input
            type="checkbox"
            aria-label={`${label}: enabled`}
            checked={result.enabled ?? true}
            onChange={(e) => onLayoutChange({ enabled: e.target.checked })}
          />
        </td>
        <td className="row-actions">
          <button
            type="button"
            className="link-button"
            aria-expanded={open}
            aria-label={`${label}: advanced options`}
            onClick={() => setOpen(!open)}
          >
            {open ? "▾" : "▸"} More
          </button>
          <button
            type="button"
            className="icon-button"
            aria-label={`Duplicate ${label}`}
            onClick={onDuplicate}
          >
            ⧉
          </button>
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
      {open && (
        <tr className="sub-row">
          <td colSpan={columns}>
            <div className="row-advanced">
              <span className="cell-sub">
                ID <code>{result.id}</code>
              </span>
              <label className="field">
                <span className="field-label">Description</span>
                <input
                  className="text-input"
                  aria-label={`${label}: description`}
                  value={result.description ?? ""}
                  onChange={(e) => onChange({ description: e.target.value }, "description")}
                />
              </label>
              <span className="field-label">Emits tags</span>
              <TagMultiSelect
                label={`${label} tags`}
                value={result.tags ?? []}
                onChange={(tags) => onChange({ tags })}
              />
              <span className="field-label">Eligible only when</span>
              <OptionalCondition
                label={`${label} eligibility`}
                value={result.eligibility}
                onChange={(eligibility) => onChange({ eligibility })}
              />
              <span className="field-label">Then roll</span>
              <span className="field-help">Tables rolled next whenever this result comes up.</span>
              {thenRoll.map((t, i) => (
                <span key={i} className="variant-row">
                  <TableSelect
                    label={`${label}: then roll ${i + 1}`}
                    value={t}
                    onChange={(id) =>
                      onChange({ then_roll: thenRoll.map((x, j) => (j === i ? id : x)) })
                    }
                  />
                  <button
                    type="button"
                    className="icon-button"
                    aria-label={`Remove then roll ${i + 1}`}
                    onClick={() => onChange({ then_roll: thenRoll.filter((_, j) => j !== i) })}
                  >
                    ×
                  </button>
                </span>
              ))}
              <button
                type="button"
                className="btn btn-small"
                onClick={() => onChange({ then_roll: [...thenRoll, ""] })}
              >
                Add a table to roll next
              </button>
              {!result.shared && (
                <button type="button" className="btn btn-small" onClick={onShare}>
                  Make a shared result from this
                </button>
              )}
              <span className="field-label">Uniqueness</span>
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
                Unique: given once, then it becomes its fallback (never a reroll)
              </label>
              {result.unique && (
                <label className="field">
                  <span className="field-label">Becomes, when already given</span>
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
              {!valued && (
                <label className="field">
                  <span className="field-label">Number it stands for</span>
                  <input
                    className="text-input number-input"
                    type="number"
                    min={1}
                    step={1}
                    aria-label={`${label}: value`}
                    value={result.value ?? ""}
                    onChange={(e) =>
                      onChange({
                        value:
                          e.target.value === "" ? undefined : Math.trunc(Number(e.target.value)),
                      })
                    }
                  />
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
            </div>
          </td>
        </tr>
      )}
    </>
  );
}

// ----- testing ------------------------------------------------------------------

/** "Test table": type what the physical dice showed and see the result. */
function TestTable({ coverage, nameOf }: { coverage: Coverage; nameOf: (id: string) => string }) {
  const [value, setValue] = useState("");
  const total = Number(value);
  const face = value.trim() === "" ? undefined : coverage.faces.find((f) => f.face === total);
  let answer = "";
  if (value.trim() !== "") {
    if (!Number.isInteger(total) || !face) {
      answer = `${coverage.dice} cannot roll ${value}.`;
    } else if (face.claimed_by.length === 0) {
      answer = `No result is on ${total}: add one, or let the Dice Assistant lay the table out.`;
    } else if (face.claimed_by.length > 1) {
      answer = `${total} is claimed by ${face.claimed_by.map(nameOf).join(" and ")}: only one result can have a face.`;
    } else {
      answer = `${total} gives ${nameOf(face.claimed_by[0])} (${pct(face.chance)} to roll exactly ${total}).`;
    }
  }
  return (
    <section className="editor-section" aria-labelledby="table-test">
      <h3 id="table-test" className="label">
        Test table
      </h3>
      <label className="field">
        <span className="field-label">What did the dice show?</span>
        <input
          className="text-input number-input"
          inputMode="numeric"
          aria-label="Dice total to test"
          value={value}
          onChange={(e) => setValue(e.target.value)}
        />
      </label>
      {answer && <p role="status">{answer}</p>}
    </section>
  );
}
