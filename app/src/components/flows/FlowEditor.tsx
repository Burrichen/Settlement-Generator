// The visual flow editor: the node graph, the selected step's settings, the
// flow's pseudocode and its problems. Structural changes are edits the core
// applies; the editor keeps undo/redo and an autosaved draft.
import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import {
  flows,
  framework,
  type Flow,
  type FlowCheck,
  type FlowContext,
  type FlowEdit,
  type FlowPosition,
  type FlowStep,
  type Origin,
  type StepKind,
  type Usage,
} from "../../api";
import { ConfirmBar } from "../generation";
import { originLabel, useDraftCheck } from "../editors/common";
import { FlowCanvas, type CanvasHandle, type DropTarget } from "./FlowCanvas";
import { isDetached, layoutFlow, listsOf, positionOf, slotKey, stepsById } from "./layout";
import { StepInspector } from "./StepInspector";
import { TestPanel } from "../editors/TestPanel";
import { DeleteBlocked, UsageLinks } from "../editors/UsageLinks";
import { ExportButton } from "../../screens/TransferScreen";

const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);

/** `flow` with step `next` (same ID) in place of the old one. */
function replaceStep(flow: Flow, next: FlowStep): Flow {
  const inList = (steps: FlowStep[]): FlowStep[] =>
    steps.map((s) => {
      if (s.id === next.id) return next;
      switch (s.kind) {
        case "repeat":
          return { ...s, steps: inList(s.steps) };
        case "for_each":
        case "create_object":
          return s.steps ? { ...s, steps: inList(s.steps) } : s;
        case "branch":
          return {
            ...s,
            cases: s.cases.map((c) => ({ ...c, steps: inList(c.steps) })),
            otherwise: s.otherwise ? inList(s.otherwise) : s.otherwise,
          };
        default:
          return s;
      }
    });
  return {
    ...flow,
    steps: inList(flow.steps),
    editor: flow.editor?.detached
      ? { ...flow.editor, detached: inList(flow.editor.detached) }
      : flow.editor,
  };
}

/** Undo/redo over the flow; typing in one field within a second is one change. */
function useHistory(draft: Flow, setDraft: (f: Flow) => void) {
  const past = useRef<Flow[]>([]);
  const future = useRef<Flow[]>([]);
  const last = useRef<{ key: string; at: number } | null>(null);
  const [, bump] = useState(0);
  const commit = useCallback(
    (next: Flow, key = "") => {
      if (same(next, draft)) return;
      const now = Date.now();
      const coalesce = key !== "" && last.current?.key === key && now - last.current.at < 1000;
      if (!coalesce) past.current.push(draft);
      future.current = [];
      last.current = { key, at: now };
      setDraft(next);
      bump((n) => n + 1);
    },
    [draft, setDraft],
  );
  const undo = () => {
    const previous = past.current.pop();
    if (!previous) return;
    future.current.push(draft);
    last.current = null;
    setDraft(previous);
    bump((n) => n + 1);
  };
  const redo = () => {
    const next = future.current.pop();
    if (!next) return;
    past.current.push(draft);
    last.current = null;
    setDraft(next);
    bump((n) => n + 1);
  };
  return {
    commit,
    undo,
    redo,
    canUndo: past.current.length > 0,
    canRedo: future.current.length > 0,
  };
}

export function FlowEditor({
  initial,
  saved,
  origin,
  onSaved,
  onOpenCopy,
  onStructuralChange,
}: {
  /** The flow to edit (a restored draft, a new flow or the saved flow). */
  initial: Flow;
  /** The stored flow, or null for a new one. */
  saved: Flow | null;
  origin: Origin | null;
  onSaved: (flow: Flow) => void;
  /** Opens an unsaved copy in the editor. */
  onOpenCopy: (flow: Flow) => void;
  /** After a delete or restore: reload the list. */
  onStructuralChange: (select?: string) => void;
}) {
  const [draft, setDraft] = useState<Flow>(initial);
  const [stored, setStored] = useState<Flow | null>(saved);
  const history = useHistory(draft, setDraft);
  const [selected, setSelected] = useState<string | null>(null);
  const [message, setMessage] = useState<{ text: string; error?: boolean } | null>(null);
  const [search, setSearch] = useState("");
  const [matchIndex, setMatchIndex] = useState(0);
  const [panel, setPanel] = useState<"pseudocode" | "problems" | "test">("pseudocode");
  const [confirm, setConfirm] = useState<null | "delete" | "reset">(null);
  const [blocked, setBlocked] = useState(false);
  const [usages, setUsages] = useState<Usage[]>([]);
  useEffect(() => {
    if (!stored) return;
    flows.usages(stored.id).then(setUsages, () => setUsages([]));
  }, [stored]);
  const [keptStatus, setKeptStatus] = useState("");
  const [busy, setBusy] = useState(false);
  const canvas = useRef<CanvasHandle>(null);

  const check = useDraftCheck<Flow, FlowCheck>(draft, flows.check);
  const context = useDraftCheck<Flow, FlowContext>(draft, flows.context, 300);
  const dirty = stored === null || !same(draft, stored);

  // Keep unsaved changes as a draft in the core.
  useEffect(() => {
    if (!dirty) return;
    const timer = setTimeout(() => {
      flows.saveDraft(draft).then(
        () => setKeptStatus("Unsaved changes kept as a draft"),
        () => undefined,
      );
    }, 800);
    return () => clearTimeout(timer);
  }, [draft, dirty]);

  const texts = useMemo(() => new Map((check?.nodes ?? []).map((n) => [n.step, n])), [check]);
  const layout = useMemo(
    () => layoutFlow(draft, (step, i) => texts.get(step)?.cases?.[i] ?? `case ${i + 1}`),
    [draft, texts],
  );
  const steps = useMemo(() => stepsById(draft), [draft]);
  const problems = useMemo(() => {
    const out = new Map<string, "error" | "warning">();
    for (const issue of check?.issues ?? []) {
      const step = issue.location.step;
      if (!step || issue.location.flow !== draft.id) continue;
      if (issue.severity === "error" || !out.has(step)) out.set(step, issue.severity);
    }
    return out;
  }, [check, draft.id]);
  const matches = useMemo(() => {
    const q = search.trim().toLowerCase();
    if (!q) return [] as string[];
    return layout.nodes
      .filter((n) => {
        const t = texts.get(n.id);
        return `${n.id} ${n.step.name ?? ""} ${t?.kind ?? ""} ${t?.text ?? ""} ${t?.when ?? ""}`
          .toLowerCase()
          .includes(q);
      })
      .map((n) => n.id);
  }, [search, layout, texts]);

  const errors =
    (check?.issues.filter((i) => i.severity === "error").length ?? 0) + (check?.error ? 1 : 0);
  const warnings = check?.issues.filter((i) => i.severity === "warning").length ?? 0;

  /** Applies core edits in order, as one undoable change. */
  const edit = async (...edits: FlowEdit[]) => {
    let next = draft;
    try {
      for (const e of edits) next = await flows.edit(next, e);
    } catch (e) {
      setMessage({ text: String(e), error: true });
      return null;
    }
    setMessage(null);
    history.commit(next);
    return next;
  };

  const select = (id: string | null) => {
    setSelected(id);
    if (id) canvas.current?.centerOn(id);
  };

  const onAdd = async (at: FlowPosition, kind: StepKind) => {
    const before = new Set(stepsById(draft).keys());
    const next = await edit({ op: "add", kind, at });
    if (next) {
      const added = [...stepsById(next).keys()].find((id) => !before.has(id));
      if (added) setSelected(added);
    }
  };

  const onDrop = (id: string, target: DropTarget) => {
    const detached = isDetached(draft, id);
    if ("port" in target) {
      if (detached) return void edit({ op: "attach", step: id, to: target.port });
      const from = positionOf(draft, id);
      let to = target.port;
      if (from && slotKey(from) === slotKey(to)) {
        // Dropping just before or after itself changes nothing.
        if (to.index === from.index || to.index === from.index + 1) return;
        if (to.index > from.index) to = { ...to, index: to.index - 1 };
      }
      return void edit({ op: "move", step: id, to });
    }
    if (detached) return void edit({ op: "place", step: id, at: target.at });
    void edit({ op: "detach", step: id }, { op: "place", step: id, at: target.at });
  };

  const moveBy = (id: string, delta: number) => {
    const from = positionOf(draft, id);
    if (!from) return;
    const index = from.index + delta;
    const length = listLength(draft, from);
    if (index < 0 || index >= length) return;
    void edit({ op: "move", step: id, to: { ...from, index } });
  };

  const removeStep = async (id: string) => {
    if (await edit({ op: "remove", step: id })) setSelected(null);
  };

  const duplicateStep = async (id: string) => {
    if (isDetached(draft, id)) return;
    const before = new Set(stepsById(draft).keys());
    const next = await edit({ op: "duplicate", step: id });
    const copy = next ? [...stepsById(next).keys()].find((s) => !before.has(s)) : undefined;
    if (copy) setSelected(copy);
  };

  const save = async () => {
    setBusy(true);
    try {
      const next = await flows.save(draft);
      setStored(next);
      setDraft(next);
      setKeptStatus("");
      setMessage({ text: "Flow saved." });
      onSaved(next);
    } catch (e) {
      setMessage({ text: String(e), error: true });
    } finally {
      setBusy(false);
    }
  };

  const discard = () => {
    void flows.discardDraft(draft.id).catch(() => undefined);
    setKeptStatus("");
    if (stored) history.commit(stored);
  };

  const duplicateFlow = async () => {
    try {
      if (!dirty && stored) return onOpenCopy(await flows.duplicate(draft.id));
      const fresh = await flows.create();
      onOpenCopy({ ...draft, id: fresh.id, name: `Copy of ${draft.name}` });
    } catch (e) {
      setMessage({ text: String(e), error: true });
    }
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const target = event.target as HTMLElement;
    const typing =
      target.tagName === "TEXTAREA" ||
      (target.tagName === "INPUT" &&
        !["checkbox", "radio"].includes((target as HTMLInputElement).type));
    const ctrl = event.ctrlKey || event.metaKey;
    const key = event.key.toLowerCase();
    if (ctrl && key === "z" && !event.shiftKey && !typing) {
      event.preventDefault();
      history.undo();
    } else if (ctrl && (key === "y" || (key === "z" && event.shiftKey)) && !typing) {
      event.preventDefault();
      history.redo();
    } else if (ctrl && key === "d" && selected) {
      event.preventDefault();
      void duplicateStep(selected);
    } else if (ctrl && key === "s") {
      event.preventDefault();
      if (errors === 0 && dirty) void save();
    } else if (event.key === "Delete" && selected && !typing && target.tagName !== "SELECT") {
      event.preventDefault();
      void removeStep(selected);
    }
  };

  const nextMatch = () => {
    if (matches.length === 0) return;
    const i = matchIndex % matches.length;
    setMatchIndex(i + 1);
    select(matches[i]);
  };

  const selectedStep = selected ? steps.get(selected) : undefined;

  return (
    <div className="editor flow-editor" onKeyDown={onKeyDown}>
      <header className="editor-header">
        <div className="field-grid">
          <label className="field">
            <span className="field-label">Flow name</span>
            <input
              className="text-input title-input"
              value={draft.name}
              onChange={(e) => history.commit({ ...draft, name: e.target.value }, "name")}
            />
          </label>
          <label className="field">
            <span className="field-label">Description</span>
            <input
              className="text-input"
              value={draft.description ?? ""}
              onChange={(e) =>
                history.commit(
                  { ...draft, description: e.target.value || undefined },
                  "description",
                )
              }
            />
          </label>
        </div>
        <p className="saved-meta">
          ID <code>{draft.id}</code> ·{" "}
          {stored === null ? "New custom flow (not saved)" : originLabel(origin, false)}
          {dirty && stored !== null && <> · Unsaved changes</>}
          {keptStatus && <> · {keptStatus}</>}
        </p>
        {usages.length > 0 && (
          <details className="used-by">
            <summary className="field-label">
              Used by {usages.length} place{usages.length === 1 ? "" : "s"}
            </summary>
            <UsageLinks usages={usages} label="Used by" />
          </details>
        )}
        <div className="toolbar" role="toolbar" aria-label="Flow tools">
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
          <button type="button" className="btn btn-small" onClick={() => canvas.current?.fit()}>
            Fit to view
          </button>
          <button
            type="button"
            className="btn btn-small"
            aria-label="Zoom out"
            onClick={() => canvas.current?.zoomBy(1 / 1.2)}
          >
            −
          </button>
          <button
            type="button"
            className="btn btn-small"
            onClick={() => canvas.current?.actualSize()}
          >
            100%
          </button>
          <button
            type="button"
            className="btn btn-small"
            aria-label="Zoom in"
            onClick={() => canvas.current?.zoomBy(1.2)}
          >
            +
          </button>
          <input
            className="text-input flow-search"
            type="search"
            placeholder="Find a step"
            aria-label="Find a step"
            value={search}
            onChange={(e) => {
              setSearch(e.target.value);
              setMatchIndex(0);
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                nextMatch();
              }
            }}
          />
          {search.trim() && (
            <span className="muted flow-search-count" aria-live="polite">
              {matches.length === 0
                ? "No matches"
                : `${matches.length} match${matches.length === 1 ? "" : "es"}`}
            </span>
          )}
        </div>
        <div className="toolbar" role="toolbar" aria-label="Save tools">
          <button
            type="button"
            className="btn btn-primary btn-small"
            disabled={!dirty || errors > 0 || busy}
            onClick={() => void save()}
            title="Ctrl+S"
          >
            Save flow
          </button>
          {stored !== null && (
            <button type="button" className="btn btn-small" disabled={!dirty} onClick={discard}>
              Discard changes
            </button>
          )}
          <button type="button" className="btn btn-small" onClick={() => void duplicateFlow()}>
            Duplicate flow
          </button>
          {stored !== null && (
            <ExportButton item={{ kind: "flow", id: draft.id }} name={draft.name} />
          )}
          {origin === "modified" && (
            <button type="button" className="btn btn-small" onClick={() => setConfirm("reset")}>
              Restore default
            </button>
          )}
          {stored !== null && (
            <button
              type="button"
              className="btn btn-small btn-quiet-danger"
              onClick={() => (usages.length > 0 ? setBlocked(true) : setConfirm("delete"))}
            >
              Delete flow
            </button>
          )}
        </div>
        {blocked && <DeleteBlocked noun="flow" usages={usages} onClose={() => setBlocked(false)} />}
        {confirm && (
          <ConfirmBar
            message={
              confirm === "delete"
                ? `Delete the flow “${draft.name}”? Anything that runs it must be changed first.`
                : "Restore this flow to its installed default? Your changes to it are removed."
            }
            confirmLabel={confirm === "delete" ? "Delete flow" : "Restore default"}
            onCancel={() => setConfirm(null)}
            onConfirm={async () => {
              const action = confirm;
              setConfirm(null);
              try {
                if (action === "delete") await flows.remove(draft.id);
                else await framework.resetItem("flow", draft.id);
                await flows.discardDraft(draft.id).catch(() => undefined);
                onStructuralChange(action === "reset" ? draft.id : undefined);
              } catch (e) {
                setMessage({ text: String(e), error: true });
              }
            }}
          />
        )}
        {message && (
          <p
            role={message.error ? "alert" : "status"}
            className={`status-line ${message.error ? "is-error" : ""}`}
          >
            {message.text}
          </p>
        )}
        <p className="validation-summary" aria-live="polite">
          {errors === 0 && warnings === 0
            ? "No problems found."
            : `${errors} error${errors === 1 ? "" : "s"}, ${warnings} warning${warnings === 1 ? "" : "s"}.`}
          {errors > 0 && " Fix the errors to save."}
        </p>
      </header>

      <div className="flow-workspace">
        <FlowCanvas
          ref={canvas}
          layout={layout}
          texts={texts}
          problems={problems}
          selected={selected}
          matches={new Set(matches)}
          kinds={context?.kinds ?? []}
          onSelect={setSelected}
          onAdd={(at, kind) => void onAdd(at, kind)}
          onDrop={onDrop}
        />
        <aside className="flow-side" aria-label="Selected step">
          {selectedStep && context ? (
            <StepInspector
              key={selectedStep.id}
              step={selectedStep}
              text={texts.get(selectedStep.id)}
              context={context}
              detached={isDetached(draft, selectedStep.id)}
              onChange={(next, key) =>
                history.commit(replaceStep(draft, next), key ? `${next.id}:${key}` : "")
              }
              actions={{
                duplicate: () => void duplicateStep(selectedStep.id),
                remove: () => void removeStep(selectedStep.id),
                detach: () => void edit({ op: "detach", step: selectedStep.id }),
                moveBy: (delta) => moveBy(selectedStep.id, delta),
                addCase: () => void edit({ op: "add_case", step: selectedStep.id }),
                removeCase: (index) =>
                  void edit({ op: "remove_case", step: selectedStep.id, case: index }),
              }}
            />
          ) : (
            <div className="inspector">
              <p className="muted">
                Select a step to change it. Click a <strong>+</strong> to add a step there; drag a
                step onto a <strong>+</strong> to move it, or onto empty space to disconnect it.
              </p>
            </div>
          )}
        </aside>
      </div>

      <section className="flow-panels">
        <div className="tabs" role="tablist" aria-label="Flow summary">
          <button
            type="button"
            role="tab"
            aria-selected={panel === "pseudocode"}
            className={`tab ${panel === "pseudocode" ? "is-active" : ""}`}
            onClick={() => setPanel("pseudocode")}
          >
            Pseudocode
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={panel === "problems"}
            className={`tab ${panel === "problems" ? "is-active" : ""}`}
            onClick={() => setPanel("problems")}
          >
            Problems ({errors + warnings})
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={panel === "test"}
            className={`tab ${panel === "test" ? "is-active" : ""}`}
            onClick={() => setPanel("test")}
          >
            Test
          </button>
        </div>
        {panel === "test" && (
          <TestPanel target={{ kind: "flow", flow: draft }} disabled={errors > 0} />
        )}
        {panel === "test" ? null : panel === "pseudocode" ? (
          <ol className="pseudocode" aria-label="Pseudocode" aria-readonly="true">
            {(check?.pseudocode ?? []).map((line, i) => (
              <li
                key={i}
                className={`pseudo-line ${line.step && line.step === selected ? "is-selected" : ""}`}
                style={{ paddingLeft: `${line.depth * 1.5}em` }}
                onClick={() => line.step && select(line.step)}
              >
                {line.text}
              </li>
            ))}
          </ol>
        ) : (
          <ul className="issues" aria-label="Problems">
            {check?.error && <li className="issue issue-error">{check.error}</li>}
            {(check?.issues ?? []).map((issue, i) => (
              <li key={i} className={`issue issue-${issue.severity}`}>
                <span className="issue-level">
                  {issue.severity === "error" ? "Error" : "Warning"}
                </span>{" "}
                {issue.location.step && issue.location.flow === draft.id ? (
                  <button
                    type="button"
                    className="link-button"
                    onClick={() => select(issue.location.step!)}
                  >
                    {issue.message}
                  </button>
                ) : (
                  issue.message
                )}
              </li>
            ))}
            {!check?.error && (check?.issues.length ?? 0) === 0 && (
              <li className="issues-ok">No problems found.</li>
            )}
          </ul>
        )}
      </section>
    </div>
  );
}

/** How many steps the list at `at` holds. */
function listLength(flow: Flow, at: FlowPosition): number {
  if (at.list === "top") return flow.steps.length;
  const owner = stepsById(flow).get(at.step);
  if (!owner) return 0;
  const lists = listsOf(owner);
  const key = slotKey(at);
  return lists.find((l) => slotKey(l.slot) === key)?.steps.length ?? 0;
}
