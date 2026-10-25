import { useEffect, useMemo, useState } from "react";
import {
  content,
  framework,
  transfer,
  type Collision,
  type ContentPack,
  type ImportAction,
  type ImportPlan,
  type PackItem,
  type PackKind,
} from "../api";
import { useApp } from "../app-state";
import { ScreenFrame } from "../components/ScreenFrame";
import { IssueList } from "../components/editors/common";

const KIND_LABEL: Record<PackKind, string> = {
  setting: "Setting",
  scale: "Scale",
  shared_result: "Shared result",
  object_type: "Object type",
  table: "Table",
  tag: "Tag",
  flow: "Flow",
};

const POLICIES: { value: Collision; label: string; help: string }[] = [
  {
    value: "rename",
    label: "Keep both",
    help: "Items whose ID is already used come in as new copies. Nothing you have changes.",
  },
  {
    value: "replace",
    label: "Replace mine",
    help: "Items whose ID is already used replace yours (a default becomes customised).",
  },
  {
    value: "skip",
    label: "Keep mine",
    help: "Items whose ID is already used are not imported; the file's references use yours.",
  },
];

function actionWords(action: ImportAction): string {
  if (action === "add") return "Add";
  if ("add_as" in action) return "Add as a new copy";
  if ("replace" in action) return `Replace “${action.replace.existing}”`;
  return `Skip (keep “${action.skip.existing}”)`;
}

/** Writes `pack` to the exports folder and says where. */
export async function savePack(pack: ContentPack, fileName: string): Promise<string> {
  const path = await transfer.writePack(pack, fileName);
  return `Saved to ${path}`;
}

/** One-click export of one item (and what it needs), from its editor. */
export function ExportButton({ item, name }: { item: PackItem; name: string }) {
  const [status, setStatus] = useState<{ text: string; error: boolean } | null>(null);
  return (
    <>
      <button
        type="button"
        className="btn btn-small"
        title="Save this (and anything custom it needs) as a content file to share or back up"
        onClick={async () => {
          try {
            const pack = await transfer.exportItems([item], true, name);
            setStatus({ text: await savePack(pack, name), error: false });
          } catch (e) {
            setStatus({ text: String(e), error: true });
          }
        }}
      >
        Export
      </button>
      {status && (
        <span
          role={status.error ? "alert" : "status"}
          className={`small ${status.error ? "is-error" : "muted"}`}
        >
          {status.text}
        </span>
      )}
    </>
  );
}

/** Import / Export: content files to share, back up or move between installs. */
export function TransferScreen() {
  const { back } = useApp();
  const [folder, setFolder] = useState<string | null>(null);
  useEffect(() => {
    transfer.exportsFolder().then(setFolder, () => setFolder(null));
  }, []);
  return (
    <ScreenFrame
      title="Import / Export"
      subtitle="Share tables, tags, settings and flows as files, or back up everything you made."
      backLabel="Settings"
      onBack={back}
    >
      <ExportSection folder={folder} />
      <ImportSection />
    </ScreenFrame>
  );
}

function ExportSection({ folder }: { folder: string | null }) {
  const [status, setStatus] = useState<{ text: string; error: boolean } | null>(null);
  const [choosing, setChoosing] = useState(false);
  const run = async (job: () => Promise<string>) => {
    try {
      setStatus({ text: await job(), error: false });
    } catch (e) {
      setStatus({ text: String(e), error: true });
    }
  };
  return (
    <section className="settings-section" aria-labelledby="export-title">
      <h2 id="export-title" className="label">
        Export
      </h2>
      <p className="muted small">
        Files are saved in {folder ? <code>{folder}</code> : "your Documents folder"}.
      </p>
      <div className="button-row">
        <button
          type="button"
          className="btn"
          onClick={() =>
            void run(async () =>
              savePack(await transfer.exportEverything("customised"), "My content"),
            )
          }
        >
          Export everything I made or changed
        </button>
        <button
          type="button"
          className="btn"
          onClick={() =>
            void run(async () =>
              savePack(await transfer.exportEverything("everything"), "Complete content pack"),
            )
          }
        >
          Export a complete content pack
        </button>
        <button
          type="button"
          className="btn"
          aria-expanded={choosing}
          onClick={() => setChoosing(!choosing)}
        >
          {choosing ? "Hide the list" : "Choose what to export…"}
        </button>
      </div>
      {choosing && <ChooseItems onDone={(text, error) => setStatus({ text, error })} />}
      {status && (
        <p
          role={status.error ? "alert" : "status"}
          className={`status-line ${status.error ? "is-error" : ""}`}
        >
          {status.text}
        </p>
      )}
    </section>
  );
}

interface Choice extends PackItem {
  name: string;
}

function ChooseItems({ onDone }: { onDone: (text: string, error: boolean) => void }) {
  const [choices, setChoices] = useState<Choice[]>([]);
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [filter, setFilter] = useState("");
  const [needs, setNeeds] = useState(true);
  const [name, setName] = useState("My export");

  useEffect(() => {
    void (async () => {
      const [overview, items] = await Promise.all([content.overview(), framework.items()]);
      const kindOf: Record<string, PackKind> = {
        shared_result: "shared_result",
        scale: "scale",
        setting: "setting",
        object_type: "object_type",
        flow: "flow",
      };
      setChoices([
        ...overview.tables
          .filter((t) => !t.deleted)
          .map((t) => ({ kind: "table" as const, id: t.id, name: t.name })),
        ...overview.tags
          .filter((t) => !t.deleted)
          .map((t) => ({ kind: "tag" as const, id: t.id, name: t.name })),
        ...items
          .filter((i) => !i.deleted)
          .map((i) => ({ kind: kindOf[i.kind], id: i.id, name: i.name })),
      ]);
    })().catch((e: unknown) => onDone(String(e), true));
  }, [onDone]);

  const key = (c: PackItem) => `${c.kind}:${c.id}`;
  const shown = useMemo(() => {
    const q = filter.trim().toLowerCase();
    return choices.filter((c) => !q || `${c.name} ${c.id}`.toLowerCase().includes(q));
  }, [choices, filter]);

  return (
    <div className="export-chooser">
      <input
        className="text-input"
        type="search"
        aria-label="Filter what to export"
        placeholder="Filter by name"
        value={filter}
        onChange={(e) => setFilter(e.target.value)}
      />
      <ul className="check-column" aria-label="What to export">
        {shown.map((c) => (
          <li key={key(c)}>
            <label className="check">
              <input
                type="checkbox"
                checked={picked.has(key(c))}
                onChange={(e) => {
                  const next = new Set(picked);
                  if (e.target.checked) next.add(key(c));
                  else next.delete(key(c));
                  setPicked(next);
                }}
              />
              {c.name} <span className="muted small">{KIND_LABEL[c.kind]}</span>
            </label>
          </li>
        ))}
      </ul>
      <label className="check-row">
        <input type="checkbox" checked={needs} onChange={(e) => setNeeds(e.target.checked)} />
        Include what they need (custom tags, tables and flows they refer to)
      </label>
      <label className="field">
        <span className="field-label">File name</span>
        <input className="text-input" value={name} onChange={(e) => setName(e.target.value)} />
      </label>
      <button
        type="button"
        className="btn btn-primary"
        disabled={picked.size === 0}
        onClick={async () => {
          const items = choices
            .filter((c) => picked.has(key(c)))
            .map(({ kind, id }) => ({ kind, id }));
          try {
            const pack = await transfer.exportItems(items, needs, name);
            onDone(await savePack(pack, name), false);
          } catch (e) {
            onDone(String(e), true);
          }
        }}
      >
        Export {picked.size} item{picked.size === 1 ? "" : "s"}
      </button>
    </div>
  );
}

function ImportSection() {
  const [text, setText] = useState<string | null>(null);
  const [fileName, setFileName] = useState("");
  const [policy, setPolicy] = useState<Collision>("rename");
  const [plan, setPlan] = useState<ImportPlan | null>(null);
  const [status, setStatus] = useState<{ text: string; error: boolean } | null>(null);
  const [pasting, setPasting] = useState(false);

  useEffect(() => {
    if (text === null) return;
    let live = true;
    transfer.planImport(text, policy).then(
      (p) => live && (setPlan(p), setStatus(null)),
      (e: unknown) => live && (setPlan(null), setStatus({ text: String(e), error: true })),
    );
    return () => {
      live = false;
    };
  }, [text, policy]);

  return (
    <section className="settings-section" aria-labelledby="import-title">
      <h2 id="import-title" className="label">
        Import
      </h2>
      <p className="muted small">
        Everything is checked before anything changes: a file that would break a reference is not
        imported.
      </p>
      <label className="field">
        <span className="field-label">Content file</span>
        <input
          type="file"
          accept=".json,application/json"
          aria-label="Content file to import"
          onChange={async (e) => {
            const file = e.target.files?.[0];
            if (!file) return;
            setFileName(file.name);
            setText(await file.text());
          }}
        />
      </label>
      <button
        type="button"
        className="btn btn-small"
        aria-expanded={pasting}
        onClick={() => setPasting(!pasting)}
      >
        {pasting ? "Hide" : "Or paste the file's text…"}
      </button>
      {pasting && (
        <textarea
          className="text-input text-area"
          aria-label="Content to import"
          onChange={(e) => {
            setFileName("pasted content");
            setText(e.target.value);
          }}
        />
      )}
      {text !== null && (
        <fieldset className="field">
          <legend className="field-label">If something with the same ID already exists</legend>
          {POLICIES.map((p) => (
            <label key={p.value} className="check-row">
              <input
                type="radio"
                name="collision"
                checked={policy === p.value}
                onChange={() => setPolicy(p.value)}
              />
              <span>
                {p.label} <span className="muted small">{p.help}</span>
              </span>
            </label>
          ))}
        </fieldset>
      )}
      {plan && (
        <div className="import-plan">
          <p className="field-label">
            {plan.name || fileName}: {plan.summary}
          </p>
          <table className="edit-grid" aria-label="What will be imported">
            <thead>
              <tr>
                <th scope="col">Item</th>
                <th scope="col">Kind</th>
                <th scope="col">What happens</th>
              </tr>
            </thead>
            <tbody>
              {plan.items.map((i) => (
                <tr key={`${i.kind}:${i.id}`}>
                  <th scope="row">{i.name}</th>
                  <td>{KIND_LABEL[i.kind]}</td>
                  <td>{actionWords(i.action)}</td>
                </tr>
              ))}
            </tbody>
          </table>
          {(plan.issues.length > 0 || plan.error) && (
            <IssueList issues={plan.issues} error={plan.error} />
          )}
          <button
            type="button"
            className="btn btn-primary"
            disabled={!plan.can_import}
            onClick={async () => {
              try {
                const done = await transfer.importPack(text!, policy);
                setStatus({ text: `Imported: ${done.summary}`, error: false });
                setPlan(null);
                setText(null);
              } catch (e) {
                setStatus({ text: String(e), error: true });
              }
            }}
          >
            Import
          </button>
        </div>
      )}
      {status && (
        <p
          role={status.error ? "alert" : "status"}
          className={`status-line ${status.error ? "is-error" : ""}`}
        >
          {status.text}
        </p>
      )}
    </section>
  );
}
