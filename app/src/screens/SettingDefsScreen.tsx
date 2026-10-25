import { useCallback, useEffect, useRef, useState, type KeyboardEvent } from "react";
import {
  framework,
  type FrameworkItemListing,
  type SettingDef,
  type SettingDetail,
  type SettingOption,
  type SettingType,
} from "../api";
import { useApp } from "../app-state";
import { ConfirmBar } from "../components/generation";
import { ScreenFrame } from "../components/ScreenFrame";
import { IssueList, originLabel } from "../components/editors/common";
import { ItemList, useEntityEditor } from "../components/editors/EntityEditor";
import { EditorActions } from "./TagsScreen";
import { UsageLinks } from "../components/editors/UsageLinks";

const KINDS: { kind: SettingType; label: string; help: string }[] = [
  { kind: "toggle", label: "On/off", help: "A switch, e.g. “Include ruins”." },
  { kind: "number", label: "Number", help: "A whole number, optionally within a range." },
  { kind: "choice", label: "Dropdown", help: "One of a list of options." },
  { kind: "multi_select", label: "Multi-select", help: "Any number of a list of options." },
  { kind: "text", label: "Text", help: "Free text, up to 200 characters." },
];

const kindLabel = (kind: SettingType) => KINDS.find((k) => k.kind === kind)?.label ?? kind;

/** A new option ID from its name, unique among `taken`. */
function optionId(name: string, taken: string[]): string {
  const base =
    name
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "") || "option";
  let id = base;
  for (let n = 2; taken.includes(id); n++) id = `${base}-${n}`;
  return id;
}

/** View Generation Settings: the settings content defines, which flow and table conditions test. */
export function SettingDefsScreen() {
  const { back, takeTarget } = useApp();
  const [wanted] = useState(() => takeTarget?.("setting") ?? null);
  const [items, setItems] = useState<FrameworkItemListing[]>([]);
  const [filter, setFilter] = useState("");
  const [confirm, setConfirm] = useState<null | "delete">(null);
  const [deletedId, setDeletedId] = useState<string | null>(null);
  const [newKind, setNewKind] = useState<SettingType | "">("");
  const opened = useRef(false);

  const editor = useEntityEditor<SettingDef, SettingDetail>({
    load: framework.settingDetail,
    entityOf: useCallback((d: SettingDetail) => d.setting, []),
    check: framework.checkSetting,
    save: framework.saveSetting,
  });

  const refresh = useCallback(async () => {
    try {
      const all = await framework.items();
      const list = all.filter((i) => i.kind === "setting");
      setItems(list);
      return list;
    } catch (e) {
      editor.setMessage({ text: String(e), error: true });
      return null;
    }
  }, [editor]);

  useEffect(() => {
    if (opened.current) return;
    opened.current = true;
    void refresh().then((list) => {
      const first = list?.find((i) => !i.deleted);
      const id = wanted ?? first?.id;
      if (id) void editor.open(id);
    });
  }, [refresh, editor, wanted]);

  const afterStructuralChange = async (select?: string) => {
    const next = await refresh();
    setDeletedId(null);
    const target = select ?? next?.find((t) => !t.deleted)?.id;
    if (target) await editor.open(target);
    else editor.clear();
  };

  const deleted = deletedId ? items.find((i) => i.id === deletedId) : undefined;

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape" && editor.dirty) {
      event.preventDefault();
      editor.guard(back);
    }
  };

  return (
    <div onKeyDown={onKeyDown}>
      <ScreenFrame
        title="Generation Settings"
        backLabel="Settings"
        onBack={() => editor.guard(back)}
        wide
      >
        <div className="view-layout">
          <ItemList
            label="Generation settings"
            filter={filter}
            onFilter={setFilter}
            selectedId={deletedId ?? editor.selectedId}
            onSelect={(id) => {
              const item = items.find((t) => t.id === id);
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
            items={items.map((t) => ({
              id: t.id,
              name: t.name,
              sub: t.id,
              badges: [originLabel(t.origin ?? null, t.deleted)].filter(Boolean),
              muted: t.deleted,
              problem: false,
            }))}
            footer={
              <select
                className="select select-inline"
                aria-label="New setting"
                value={newKind}
                onChange={(e) => {
                  const kind = e.target.value as SettingType;
                  setNewKind("");
                  if (!kind) return;
                  framework.newSetting(kind).then(
                    (s) => editor.startNew(s),
                    (err: unknown) => editor.setMessage({ text: String(err), error: true }),
                  );
                }}
              >
                <option value="">New setting…</option>
                {KINDS.map((k) => (
                  <option key={k.kind} value={k.kind}>
                    {k.label}
                  </option>
                ))}
              </select>
            }
          />
          <section className="saved-detail-pane" aria-label="Setting editor">
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
                  This default setting was deleted. Restoring it brings back the installed default.
                </p>
                <button
                  type="button"
                  className="btn"
                  onClick={async () => {
                    try {
                      await framework.resetItem("setting", deleted.id);
                    } catch (e) {
                      editor.setMessage({ text: String(e), error: true });
                    }
                    await afterStructuralChange(deleted.id);
                  }}
                >
                  Restore default
                </button>
              </div>
            ) : editor.draft ? (
              <SettingForm
                key={editor.draft.id}
                editor={editor}
                confirm={confirm}
                setConfirm={setConfirm}
                afterStructuralChange={afterStructuralChange}
                onSaved={() => void refresh()}
              />
            ) : (
              <p className="muted">Choose a setting to edit, or create one.</p>
            )}
          </section>
        </div>
      </ScreenFrame>
    </div>
  );
}

function SettingForm({
  editor,
  confirm,
  setConfirm,
  afterStructuralChange,
  onSaved,
}: {
  editor: ReturnType<typeof useEntityEditor<SettingDef, SettingDetail>>;
  confirm: null | "delete";
  setConfirm: (v: null | "delete") => void;
  afterStructuralChange: (select?: string) => Promise<void>;
  onSaved: () => void;
}) {
  const draft = editor.draft!;
  const set = (patch: Partial<SettingDef>) => editor.setDraft({ ...draft, ...patch } as SettingDef);
  const check = editor.liveCheck;
  const issues = check ? check.issues : (editor.detail?.issues ?? []);
  const hasErrors = Boolean(check?.error) || issues.some((i) => i.severity === "error");
  const builtIn = editor.detail?.built_in ?? false;
  const origin = editor.detail?.origin ?? null;
  const usages = editor.detail?.usages ?? [];

  const changeKind = async (kind: SettingType) => {
    if (kind === draft.kind) return;
    try {
      const template = await framework.newSetting(kind);
      const { id, name, description, status, note } = draft;
      editor.setDraft({ ...template, id, name, description, status, note } as SettingDef);
    } catch (e) {
      editor.setMessage({ text: String(e), error: true });
    }
  };

  return (
    <div className="editor">
      <header className="editor-header">
        <h2 className="display saved-title">{draft.name || "Unnamed setting"}</h2>
        <p className="saved-meta">
          ID <code>{draft.id}</code> · {kindLabel(draft.kind)} ·{" "}
          {editor.isNew ? "New custom setting (not saved)" : originLabel(origin, false)}
        </p>
      </header>

      <section className="editor-section" aria-labelledby="setting-basics">
        <h3 id="setting-basics" className="label">
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
            Renaming is safe: conditions refer to the ID, not the name.
          </span>
        </label>
        <label className="field">
          <span className="field-label">Description</span>
          <textarea
            className="text-input text-area"
            value={draft.description ?? ""}
            onChange={(e) => set({ description: e.target.value || undefined })}
          />
        </label>
        <label className="field">
          <span className="field-label">Kind</span>
          <select
            className="select select-inline"
            value={draft.kind}
            onChange={(e) => void changeKind(e.target.value as SettingType)}
          >
            {KINDS.map((k) => (
              <option key={k.kind} value={k.kind}>
                {k.label}
              </option>
            ))}
          </select>
          <span className="field-help">{KINDS.find((k) => k.kind === draft.kind)?.help}</span>
        </label>
        <label className="check-row">
          <input
            type="checkbox"
            checked={draft.status === "planned"}
            onChange={(e) => set({ status: e.target.checked ? "planned" : undefined })}
          />
          Planned: documented here, but not offered; generation uses its default
        </label>
        {draft.status === "planned" && (
          <label className="field">
            <span className="field-label">Note</span>
            <input
              className="text-input"
              value={draft.note ?? ""}
              onChange={(e) => set({ note: e.target.value || undefined })}
            />
          </label>
        )}
      </section>

      <section className="editor-section" aria-labelledby="setting-values">
        <h3 id="setting-values" className="label">
          Values
        </h3>
        <KindFields draft={draft} set={set} />
      </section>

      <section className="editor-section" aria-labelledby="setting-usages">
        <h3 id="setting-usages" className="label">
          Dependencies
        </h3>
        {usages.length === 0 ? (
          <p className="muted small">
            Nothing tests this setting yet. Use it in a table or flow condition (“WHEN {draft.name}{" "}
            …”), or with a Read Setting step.
          </p>
        ) : (
          <UsageLinks usages={usages} label="Used by" />
        )}
      </section>

      <section className="editor-section" aria-labelledby="setting-problems">
        <h3 id="setting-problems" className="label">
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
        onSaved={onSaved}
        onReset={async () => {
          await framework.resetItem("setting", draft.id);
          await afterStructuralChange(draft.id);
        }}
        onDelete={async () => {
          await framework.deleteSetting(draft.id);
          await afterStructuralChange();
        }}
        noun="setting"
        usages={usages}
        exportItem={{ kind: "setting", id: draft.id }}
      />
    </div>
  );
}

const num = (v: string): number | undefined => (v.trim() === "" ? undefined : Number(v));

function KindFields({ draft, set }: { draft: SettingDef; set: (p: Partial<SettingDef>) => void }) {
  switch (draft.kind) {
    case "toggle":
      return (
        <label className="check-row">
          <input
            type="checkbox"
            checked={draft.default ?? false}
            onChange={(e) => set({ default: e.target.checked } as Partial<SettingDef>)}
          />
          On by default
        </label>
      );
    case "number":
      return (
        <div className="field-grid">
          {(["default", "min", "max"] as const).map((key) => (
            <label key={key} className="field">
              <span className="field-label">
                {key === "default"
                  ? "Default"
                  : key === "min"
                    ? "Lowest (optional)"
                    : "Highest (optional)"}
              </span>
              <input
                className="num-input"
                type="number"
                value={draft[key] ?? ""}
                onChange={(e) => set({ [key]: num(e.target.value) } as Partial<SettingDef>)}
              />
            </label>
          ))}
        </div>
      );
    case "text":
      return (
        <label className="field">
          <span className="field-label">Default</span>
          <input
            className="text-input"
            maxLength={200}
            value={draft.default ?? ""}
            onChange={(e) => set({ default: e.target.value } as Partial<SettingDef>)}
          />
        </label>
      );
    case "choice":
    case "multi_select":
      return <OptionsEditor draft={draft} set={set} />;
  }
}

function OptionsEditor({
  draft,
  set,
}: {
  draft: Extract<SettingDef, { kind: "choice" | "multi_select" }>;
  set: (p: Partial<SettingDef>) => void;
}) {
  const options = draft.options;
  const multi = draft.kind === "multi_select";
  const defaults: string[] = multi ? (draft.default ?? []) : [draft.default];
  const setOptions = (next: SettingOption[], nextDefaults = defaults) => {
    const kept = nextDefaults.filter((d) => next.some((o) => o.id === d));
    set(
      (multi
        ? { options: next, default: kept }
        : { options: next, default: kept[0] ?? next[0]?.id ?? "" }) as Partial<SettingDef>,
    );
  };
  return (
    <div>
      <table className="edit-grid" aria-label="Options">
        <thead>
          <tr>
            <th scope="col">Option</th>
            <th scope="col">{multi ? "Ticked by default" : "Default"}</th>
            <th scope="col">ID</th>
            <th scope="col">
              <span className="sr-only">Actions</span>
            </th>
          </tr>
        </thead>
        <tbody>
          {options.map((o, i) => (
            <tr key={o.id}>
              <td>
                <input
                  className="text-input"
                  aria-label={`Option ${i + 1} name`}
                  value={o.name}
                  onChange={(e) =>
                    setOptions(
                      options.map((x, j) => (j === i ? { ...x, name: e.target.value } : x)),
                    )
                  }
                />
              </td>
              <td>
                <input
                  type={multi ? "checkbox" : "radio"}
                  name={`default-${draft.id}`}
                  aria-label={`${o.name} is a default`}
                  checked={defaults.includes(o.id)}
                  onChange={(e) =>
                    setOptions(
                      options,
                      multi
                        ? e.target.checked
                          ? [...defaults, o.id]
                          : defaults.filter((d) => d !== o.id)
                        : [o.id],
                    )
                  }
                />
              </td>
              <td>
                <code className="muted small">{o.id}</code>
              </td>
              <td>
                <button
                  type="button"
                  className="btn btn-small btn-quiet-danger"
                  aria-label={`Remove ${o.name}`}
                  disabled={options.length === 1}
                  onClick={() => setOptions(options.filter((_, j) => j !== i))}
                >
                  ×
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <button
        type="button"
        className="btn btn-small"
        onClick={() => {
          const name = `Option ${options.length + 1}`;
          setOptions([
            ...options,
            {
              id: optionId(
                name,
                options.map((o) => o.id),
              ),
              name,
            },
          ]);
        }}
      >
        Add option
      </button>
      <p className="field-help">
        IDs are fixed when an option is added, so renaming an option keeps every condition working.
      </p>
    </div>
  );
}
