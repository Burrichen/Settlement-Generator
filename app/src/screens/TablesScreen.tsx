import { useCallback, useEffect, useRef, useState, type KeyboardEvent } from "react";
import { content, type Table, type TableDetail } from "../api";
import { useApp } from "../app-state";
import { ConfirmBar } from "../components/generation";
import { ScreenFrame } from "../components/ScreenFrame";
import { CatalogProvider, originLabel, useCatalog } from "../components/editors/common";
import { ItemList, useEntityEditor } from "../components/editors/EntityEditor";
import { TableForm } from "../components/editors/TableEditor";

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
  const { back, takeTarget } = useApp();
  const [wanted] = useState(() => takeTarget?.("table") ?? null);
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
      const id = wanted ?? first?.id;
      if (id) void editor.open(id);
    }
  }, [overview, editor, wanted]);

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
                onConfirm={() => {
                  // Discarding also drops the draft kept for this table.
                  const id = editor.draft?.id;
                  if (id)
                    void Promise.resolve()
                      .then(() => content.discardTableDraft(id))
                      .catch(() => {});
                  editor.confirmPending();
                }}
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
