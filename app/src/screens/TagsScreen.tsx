import { useCallback, useEffect, useRef, useState, type KeyboardEvent } from "react";
import {
  content,
  type EffectKindSchema,
  type Tag,
  type TagDetail,
  type TagInsight,
  type TagUsage,
  type PackItem,
} from "../api";
import { useApp } from "../app-state";
import { ConfirmBar } from "../components/generation";
import { ScreenFrame } from "../components/ScreenFrame";
import {
  CatalogProvider,
  IssueList,
  originLabel,
  useCatalog,
  useDraftCheck,
} from "../components/editors/common";
import { blankEffect, EffectEditor, KindOptions } from "../components/editors/EffectEditor";
import { RollPreview } from "../components/editors/Explanation";
import { DeleteBlocked, UsageLinks, type UsageLike } from "../components/editors/UsageLinks";
import { ExportButton } from "./TransferScreen";
import { ItemList, useEntityEditor } from "../components/editors/EntityEditor";

/** View Tags: every effective tag (defaults merged with customisations). */
export function TagsScreen() {
  return (
    <CatalogProvider>
      <TagsWorkspace />
    </CatalogProvider>
  );
}

const normalizeTag = (t: Tag): Tag => ({
  ...t,
  description: t.description ?? "",
  effects: t.effects ?? [],
  enabled: t.enabled ?? true,
});

function TagsWorkspace() {
  const { back, takeTarget } = useApp();
  const [wanted] = useState(() => takeTarget?.("tag") ?? null);
  const { overview, refresh } = useCatalog();
  const [filter, setFilter] = useState("");
  const [schema, setSchema] = useState<EffectKindSchema[]>([]);
  const [confirm, setConfirm] = useState<null | "delete">(null);
  const [deletedId, setDeletedId] = useState<string | null>(null);
  const opened = useRef(false);

  useEffect(() => {
    content.effectSchema().then(setSchema, () => setSchema([]));
  }, []);

  const editor = useEntityEditor<Tag, TagDetail>({
    load: content.tag,
    entityOf: useCallback((d: TagDetail) => normalizeTag(d.tag), []),
    check: content.checkTag,
    save: content.saveTag,
  });

  // Select the first tag once the list arrives.
  useEffect(() => {
    if (!opened.current && overview?.tags.length) {
      opened.current = true;
      const first = overview.tags.find((t) => !t.deleted);
      const id = wanted ?? first?.id;
      if (id) void editor.open(id);
    }
  }, [overview, editor, wanted]);

  const listItem = deletedId ? overview?.tags.find((t) => t.id === deletedId) : undefined;
  const deletedDefault = listItem?.deleted ?? false;

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape" && editor.dirty) {
      event.preventDefault();
      editor.guard(back);
    }
  };

  const afterStructuralChange = async (select?: string) => {
    const next = await refresh();
    setDeletedId(null);
    const target = select ?? next?.tags.find((t) => !t.deleted)?.id;
    if (target) await editor.open(target);
    else editor.clear();
  };

  return (
    <div onKeyDown={onKeyDown}>
      <ScreenFrame title="Tags" backLabel="Settings" onBack={() => editor.guard(back)} wide>
        <div className="view-layout">
          <ItemList
            label="Tags"
            filter={filter}
            onFilter={setFilter}
            selectedId={deletedId ?? editor.selectedId}
            onSelect={(id) => {
              const item = overview?.tags.find((t) => t.id === id);
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
            items={(overview?.tags ?? []).map((t) => ({
              id: t.id,
              name: t.name,
              sub: t.id,
              badges: [
                originLabel(t.origin, t.deleted),
                ...(!t.enabled && !t.deleted ? ["Disabled"] : []),
              ].filter(Boolean),
              muted: !t.enabled || t.deleted,
              problem: (overview?.issues ?? []).some(
                (i) => i.severity === "error" && i.location.tag === t.id,
              ),
            }))}
            footer={
              <button
                type="button"
                className="btn btn-small"
                onClick={() =>
                  content.newTag().then(
                    (t) => editor.startNew(normalizeTag(t)),
                    (e: unknown) => editor.setMessage({ text: String(e), error: true }),
                  )
                }
              >
                New tag
              </button>
            }
          />
          <section className="saved-detail-pane" aria-label="Tag editor">
            {editor.pending && (
              <ConfirmBar
                message="Discard your unsaved changes?"
                confirmLabel="Discard changes"
                onConfirm={editor.confirmPending}
                onCancel={editor.cancelPending}
              />
            )}
            {deletedDefault && listItem ? (
              <div className="editor">
                <h2 className="display saved-title">{listItem.name}</h2>
                <p className="muted">
                  This default tag was deleted. Restoring it brings back the installed default.
                </p>
                <button
                  type="button"
                  className="btn"
                  onClick={async () => {
                    await content
                      .resetTag(listItem.id)
                      .catch((e) => editor.setMessage({ text: String(e), error: true }));
                    await afterStructuralChange(listItem.id);
                  }}
                >
                  Restore default
                </button>
              </div>
            ) : editor.draft ? (
              <TagForm
                key={editor.draft.id}
                editor={editor}
                schema={schema}
                builtIn={editor.detail?.built_in ?? false}
                origin={editor.detail?.origin ?? null}
                confirm={confirm}
                setConfirm={setConfirm}
                afterStructuralChange={afterStructuralChange}
              />
            ) : (
              <p className="muted">Choose a tag to edit.</p>
            )}
          </section>
        </div>
      </ScreenFrame>
    </div>
  );
}

function TagForm({
  editor,
  schema,
  builtIn,
  origin,
  confirm,
  setConfirm,
  afterStructuralChange,
}: {
  editor: ReturnType<typeof useEntityEditor<Tag, TagDetail>>;
  schema: EffectKindSchema[];
  builtIn: boolean;
  origin: string | null;
  confirm: null | "delete";
  setConfirm: (v: null | "delete") => void;
  afterStructuralChange: (select?: string) => Promise<void>;
}) {
  const { refresh } = useCatalog();
  const draft = editor.draft!;
  const set = (patch: Partial<Tag>) => editor.setDraft({ ...draft, ...patch });
  const effects = draft.effects ?? [];
  const check = editor.liveCheck;
  const issues = check ? check.issues : (editor.detail?.issues ?? []);
  const hasErrors = Boolean(check?.error) || issues.some((i) => i.severity === "error");
  const insight = useDraftCheck<Tag, TagInsight>(draft, content.explainTag, 200);

  return (
    <div className="editor">
      <header className="editor-header">
        <h2 className="display saved-title">{draft.name || "Unnamed tag"}</h2>
        <p className="saved-meta">
          ID <code>{draft.id}</code> ·{" "}
          {editor.isNew ? "New custom tag (not saved)" : originLabel(origin, false)}
        </p>
      </header>

      <section className="editor-section" aria-labelledby="tag-basics">
        <h3 id="tag-basics" className="label">
          Basics
        </h3>
        <label className="field">
          <span className="field-label">Name</span>
          <input
            className="text-input"
            value={draft.name}
            onChange={(e) => set({ name: e.target.value })}
          />
          <span className="field-help">
            Renaming is safe: everything refers to the ID, not the name.
          </span>
        </label>
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
            checked={draft.enabled ?? true}
            onChange={(e) => set({ enabled: e.target.checked })}
          />
          Enabled
          <span className="field-help">
            {" "}
            Disabled tags are never applied; their effects do not run.
          </span>
        </label>
      </section>

      <section className="editor-section" aria-labelledby="tag-effects">
        <h3 id="tag-effects" className="label">
          Effects
        </h3>
        {insight?.informational && (
          <p className="muted small" role="note">
            {effects.length === 0
              ? "This tag has no effects: it is informational only. It labels the settlement, and conditions (in tables and flows) can still test it."
              : "Every effect is planned, so the tag is informational only for now: it labels the settlement, and conditions can still test it."}
          </p>
        )}
        {effects.map((effect, i) => (
          <EffectEditor
            key={i}
            effect={effect}
            schema={schema}
            tag={draft}
            label={`Effect ${i + 1}`}
            onChange={(next) => set({ effects: effects.map((e, j) => (j === i ? next : e)) })}
            onRemove={() => set({ effects: effects.filter((_, j) => j !== i) })}
          />
        ))}
        {schema.length > 0 && (
          <select
            className="select select-inline"
            aria-label="Add effect"
            value=""
            onChange={(e) => {
              const kind = schema.find((s) => s.kind === e.target.value);
              if (kind) set({ effects: [...effects, blankEffect(kind)] });
            }}
          >
            <option value="">Add effect…</option>
            <KindOptions schema={schema} />
          </select>
        )}
        {insight && insight.combined.length > 0 && (
          <div className="combined-previews">
            <h4 className="label">Together on one roll</h4>
            {insight.combined.map((p) => (
              <div key={p.table.id}>
                <p className="field-label">{p.table.name}</p>
                <RollPreview
                  steps={p.steps}
                  label={`What this tag does to a roll of ${p.table.name}`}
                />
              </div>
            ))}
          </div>
        )}
      </section>

      {editor.detail && <UsageSection usages={editor.detail.usages} />}

      <section className="editor-section" aria-labelledby="tag-problems">
        <h3 id="tag-problems" className="label">
          Validation
        </h3>
        <IssueList issues={issues} error={check?.error} />
      </section>

      <EditorActions
        editor={editor}
        hasErrors={hasErrors}
        canReset={builtIn && origin === "modified"}
        canDelete={!editor.isNew}
        confirm={confirm}
        setConfirm={setConfirm}
        onSaved={() => void refresh()}
        onReset={async () => {
          await content.resetTag(draft.id);
          await afterStructuralChange(draft.id);
        }}
        onDelete={async () => {
          await content.deleteTag(draft.id);
          await afterStructuralChange();
        }}
        noun="tag"
        usages={editor.detail?.usages ?? []}
        exportItem={{ kind: "tag", id: draft.id }}
      />
    </div>
  );
}

const USAGE_GROUPS: { kind: TagUsage["kind"]; title: string }[] = [
  { kind: "emitted_by", title: "Given by" },
  { kind: "added_by", title: "Added by" },
  { kind: "removed_by", title: "Removed by" },
  { kind: "checked_by", title: "Checked by" },
];

/** Everything that gives, adds, removes or checks the tag. */
function UsageSection({ usages }: { usages: TagUsage[] }) {
  return (
    <section className="editor-section" aria-labelledby="tag-usages">
      <h3 id="tag-usages" className="label">
        Dependencies
      </h3>
      {usages.length === 0 ? (
        <p className="muted small">Nothing uses this tag yet; it can be deleted safely.</p>
      ) : (
        USAGE_GROUPS.filter((g) => usages.some((u) => u.kind === g.kind)).map((g) => (
          <div key={g.kind}>
            <p className="field-label">{g.title}</p>
            <UsageLinks
              label={g.title}
              usages={usages.filter((u) => u.kind === g.kind)}
              strip={/^(Given|Added|Removed|Checked) by /}
            />
          </div>
        ))
      )}
    </section>
  );
}

/** Save / Revert / Reset / Delete, shared by both editors. */
export function EditorActions<T extends { id: string; name: string }, D>({
  editor,
  hasErrors,
  canReset,
  canDelete,
  confirm,
  setConfirm,
  onSaved,
  onReset,
  onDelete,
  noun,
  usages = [],
  exportItem,
}: {
  editor: ReturnType<typeof useEntityEditor<T, D>>;
  hasErrors: boolean;
  canReset: boolean;
  canDelete: boolean;
  confirm: null | "delete";
  setConfirm: (v: null | "delete") => void;
  onSaved: () => void;
  onReset: () => Promise<void>;
  onDelete: () => Promise<void>;
  noun: string;
  /** What uses the item: deleting is blocked, showing these, while any remain. */
  usages?: UsageLike[];
  /** Offers a one-click export of the saved item. */
  exportItem?: PackItem;
}) {
  const [blocked, setBlocked] = useState(false);
  const run = async (action: () => Promise<void>) => {
    editor.setBusy(true);
    try {
      await action();
    } catch (e) {
      editor.setMessage({ text: String(e), error: true });
    } finally {
      editor.setBusy(false);
    }
  };
  return (
    <div className="editor-actions">
      {blocked ? (
        <DeleteBlocked noun={noun} usages={usages} onClose={() => setBlocked(false)} />
      ) : confirm === "delete" ? (
        <ConfirmBar
          message={`Delete the ${noun} “${editor.draft?.name}”? A default can be restored later; a custom one cannot.`}
          confirmLabel="Delete"
          onConfirm={() => {
            setConfirm(null);
            void run(onDelete);
          }}
          onCancel={() => setConfirm(null)}
        />
      ) : (
        <div className="action-bar">
          <button
            type="button"
            className="btn btn-primary"
            disabled={editor.busy || !editor.dirty || hasErrors}
            onClick={async () => {
              if (await editor.saveDraft()) onSaved();
            }}
          >
            Save changes
          </button>
          <button
            type="button"
            className="btn"
            disabled={editor.busy || !editor.dirty || editor.isNew}
            onClick={editor.revert}
          >
            Revert
          </button>
          {exportItem && !editor.isNew && (
            <ExportButton item={exportItem} name={editor.draft?.name ?? exportItem.id} />
          )}
          {canReset && (
            <button
              type="button"
              className="btn"
              disabled={editor.busy}
              onClick={() => editor.guard(() => void run(onReset))}
            >
              Reset to default
            </button>
          )}
          {canDelete && (
            <button
              type="button"
              className="btn btn-quiet-danger"
              disabled={editor.busy}
              onClick={() => (usages.length > 0 ? setBlocked(true) : setConfirm("delete"))}
            >
              Delete
            </button>
          )}
        </div>
      )}
      {hasErrors && editor.dirty && <p className="muted small">Fix the errors above to save.</p>}
      {editor.message && (
        <p
          className={`status-line ${editor.message.error ? "is-error" : ""}`}
          role={editor.message.error ? "alert" : "status"}
        >
          {editor.message.text}
        </p>
      )}
    </div>
  );
}
