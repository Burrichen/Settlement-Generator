import { useCallback, useEffect, useState } from "react";
import { flows, framework, type Flow, type FrameworkItemListing } from "../api";
import { useApp } from "../app-state";
import { ConfirmBar } from "../components/generation";
import { ScreenFrame } from "../components/ScreenFrame";
import { originLabel } from "../components/editors/common";
import { ItemList } from "../components/editors/EntityEditor";
import { FlowEditor } from "../components/flows/FlowEditor";

interface Open {
  /** What the editor starts from (a kept draft, a copy or the saved flow). */
  initial: Flow;
  saved: Flow | null;
  /** A kept draft was found for the saved flow. */
  restored: boolean;
  /** Bumped to start the editor afresh. */
  rev: number;
}

/** View Flows: every flow, edited as a node graph. */
export function FlowsScreen() {
  const { back, takeTarget } = useApp();
  const [wanted] = useState(() => takeTarget?.("flow") ?? null);
  const [items, setItems] = useState<FrameworkItemListing[] | null>(null);
  const [filter, setFilter] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [open, setOpen] = useState<Open | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const all = await framework.items();
      const list = all.filter((i) => i.kind === "flow");
      setItems(list);
      return list;
    } catch (e) {
      setError(`The flows could not be loaded: ${String(e)}`);
      return null;
    }
  }, []);

  const load = useCallback(async (id: string) => {
    setSelectedId(id);
    setError(null);
    try {
      const saved = await flows.get(id);
      const kept = await flows.draft(id).catch(() => null);
      const restored = kept !== null && JSON.stringify(kept) !== JSON.stringify(saved);
      setOpen({ initial: restored ? kept! : saved, saved, restored, rev: 0 });
    } catch (e) {
      setOpen(null);
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void refresh().then((list) => {
      const first = list?.find((i) => !i.deleted);
      const id = wanted ?? first?.id;
      if (id) void load(id);
    });
  }, [refresh, load, wanted]);

  const selected = items?.find((i) => i.id === selectedId);

  return (
    <ScreenFrame title="Flows" backLabel="Settings" onBack={back} wide>
      {error && (
        <p role="alert" className="status-line is-error">
          {error}
        </p>
      )}
      <div className="view-layout">
        <ItemList
          label="Flows"
          filter={filter}
          onFilter={setFilter}
          selectedId={selectedId}
          onSelect={(id) => {
            const item = items?.find((i) => i.id === id);
            if (item?.deleted) {
              setSelectedId(id);
              setOpen(null);
            } else void load(id);
          }}
          items={(items ?? []).map((f) => ({
            id: f.id,
            name: f.name,
            sub: f.id,
            badges: [originLabel(f.origin ?? null, f.deleted)].filter(Boolean),
            muted: f.deleted,
            problem: false,
          }))}
          footer={
            <button
              type="button"
              className="btn btn-small"
              onClick={() =>
                flows.create().then(
                  (flow) => {
                    setSelectedId(flow.id);
                    setOpen({ initial: flow, saved: null, restored: false, rev: 0 });
                  },
                  (e: unknown) => setError(String(e)),
                )
              }
            >
              New flow
            </button>
          }
        />
        <section className="saved-detail-pane flow-pane" aria-label="Flow editor">
          {selected?.deleted ? (
            <div className="editor">
              <h2 className="display saved-title">{selected.name}</h2>
              <p className="muted">
                This default flow was deleted. Restoring it brings back the installed default.
              </p>
              <button
                type="button"
                className="btn"
                onClick={async () => {
                  try {
                    await framework.resetItem("flow", selected.id);
                    await refresh();
                    await load(selected.id);
                  } catch (e) {
                    setError(String(e));
                  }
                }}
              >
                Restore default
              </button>
            </div>
          ) : open ? (
            <>
              {open.restored && (
                <RestoredNote
                  onDiscard={async () => {
                    await flows.discardDraft(open.initial.id).catch(() => undefined);
                    setOpen({
                      initial: open.saved!,
                      saved: open.saved,
                      restored: false,
                      rev: open.rev + 1,
                    });
                  }}
                  onKeep={() => setOpen({ ...open, restored: false })}
                />
              )}
              <FlowEditor
                key={`${open.initial.id}:${open.rev}:${open.saved ? "saved" : "new"}`}
                initial={open.initial}
                saved={open.saved}
                origin={selected?.origin ?? null}
                onSaved={(flow) => {
                  void refresh();
                  setSelectedId(flow.id);
                }}
                onOpenCopy={(flow) => {
                  setSelectedId(flow.id);
                  setOpen({ initial: flow, saved: null, restored: false, rev: 0 });
                }}
                onStructuralChange={async (select) => {
                  const list = await refresh();
                  const target = select ?? list?.find((i) => !i.deleted)?.id;
                  if (target) await load(target);
                  else setOpen(null);
                }}
              />
            </>
          ) : (
            <p className="muted">Choose a flow to edit.</p>
          )}
        </section>
      </div>
    </ScreenFrame>
  );
}

function RestoredNote({ onDiscard, onKeep }: { onDiscard: () => void; onKeep: () => void }) {
  const [asking, setAsking] = useState(false);
  return asking ? (
    <ConfirmBar
      message="Discard the unsaved changes kept from earlier?"
      confirmLabel="Discard them"
      onConfirm={onDiscard}
      onCancel={() => setAsking(false)}
    />
  ) : (
    <div className="draft-note" role="status">
      <span>Unsaved changes from earlier were restored.</span>
      <button type="button" className="btn btn-small" onClick={onKeep}>
        OK
      </button>
      <button type="button" className="btn btn-small" onClick={() => setAsking(true)}>
        Discard them
      </button>
    </div>
  );
}
