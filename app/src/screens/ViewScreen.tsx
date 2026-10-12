import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type RefObject,
} from "react";
import {
  formatWhen,
  generation,
  saved,
  type GenerationResponse,
  type SavedSettlementView,
  type SettlementSummary,
} from "../api";
import { useApp } from "../app-state";
import { ConfirmBar, nameProblem, MAX_NAME_LENGTH } from "../components/generation";
import { ScreenFrame } from "../components/ScreenFrame";
import { SettlementSheet, foldSubStages } from "../components/SettlementSheet";
import { StageGrid } from "../components/StageGrid";

/**
 * Saved settlements: a list (left, ~25%) and the selected snapshot (right,
 * ~75%). Everything shown on the right comes from the saved snapshot, never
 * from current generator content.
 */
export function ViewScreen() {
  const { back, settings } = useApp();
  const tagView = settings?.tag_view ?? false;
  const [list, setList] = useState<SettlementSummary[] | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [detail, setDetail] = useState<SavedSettlementView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [editing, setEditing] = useState(false);
  const optionRefs = useRef(new Map<string, HTMLLIElement>());
  const actionsRef = useRef<HTMLDivElement>(null);

  const refreshList = useCallback(async (select?: string | null) => {
    try {
      const items = await saved.list();
      setList(items);
      setSelectedId((current) => {
        const wanted = select === undefined ? current : select;
        return items.some((s) => s.id === wanted) ? wanted : (items[0]?.id ?? null);
      });
    } catch (e) {
      setError(String(e));
      setList([]);
    }
  }, []);

  useEffect(() => {
    void refreshList();
  }, [refreshList]);

  // Load the selected snapshot; keep the previous one visible meanwhile.
  useEffect(() => {
    if (!selectedId) {
      setDetail(null);
      return;
    }
    let live = true;
    saved.get(selectedId).then(
      (d) => live && setDetail(d),
      (e: unknown) => live && setError(String(e)),
    );
    return () => {
      live = false;
    };
  }, [selectedId]);

  const select = (id: string) => {
    setSelectedId(id);
    setError(null);
    optionRefs.current.get(id)?.focus();
  };

  const onListKey = (event: KeyboardEvent<HTMLUListElement>) => {
    if (!list || list.length === 0) return;
    const index = Math.max(
      0,
      list.findIndex((s) => s.id === selectedId),
    );
    let next: number | null = null;
    if (event.key === "ArrowDown") next = Math.min(index + 1, list.length - 1);
    if (event.key === "ArrowUp") next = Math.max(index - 1, 0);
    if (event.key === "Home") next = 0;
    if (event.key === "End") next = list.length - 1;
    if (next !== null) {
      event.preventDefault();
      select(list[next].id);
    }
    if (event.key === "ArrowRight" || event.key === "Enter") {
      event.preventDefault();
      actionsRef.current?.querySelector("button")?.focus();
    }
  };

  const body =
    editing && detail ? (
      <EditPane
        key={detail.id}
        settlement={detail}
        onCancel={() => {
          setEditing(false);
          optionRefs.current.get(detail.id)?.focus();
        }}
        onSaved={async (updated) => {
          setEditing(false);
          setDetail(updated);
          await refreshList(updated.id);
          optionRefs.current.get(updated.id)?.focus();
        }}
      />
    ) : (
      <DetailPane
        detail={detail}
        tagView={tagView}
        actionsRef={actionsRef}
        error={error}
        onBackToList={() => selectedId && optionRefs.current.get(selectedId)?.focus()}
        onEdit={() => setEditing(true)}
        onFavourite={async (favourite) => {
          if (!detail) return;
          try {
            setDetail(await saved.setFavourite(detail.id, favourite));
            await refreshList(detail.id);
            setError(null);
          } catch (e) {
            setError(String(e));
          }
        }}
        onDelete={async () => {
          if (!detail || !list) return;
          try {
            await saved.remove(detail.id);
            const index = list.findIndex((s) => s.id === detail.id);
            const neighbour = list[index + 1] ?? list[index - 1];
            await refreshList(neighbour?.id ?? null);
            setError(null);
            if (neighbour) optionRefs.current.get(neighbour.id)?.focus();
          } catch (e) {
            setError(String(e));
          }
        }}
      />
    );

  return (
    <ScreenFrame title="Saved Settlements" backLabel="Main Menu" onBack={back} wide>
      {list === null ? (
        <p className="muted">Loading…</p>
      ) : list.length === 0 ? (
        <p className="empty-state">
          No saved settlements yet. Generate one from Start and save it.
        </p>
      ) : (
        <div className="view-layout">
          <nav
            className="saved-list-pane"
            aria-label="Saved settlements"
            // Another settlement cannot be picked mid-edit: Save or Cancel first.
            inert={editing}
          >
            <ul
              className="saved-list"
              role="listbox"
              aria-label="Saved settlements"
              onKeyDown={onListKey}
            >
              {list.map((item) => {
                const isSelected = item.id === selectedId;
                return (
                  <li
                    key={item.id}
                    ref={(el) => {
                      if (el) optionRefs.current.set(item.id, el);
                      else optionRefs.current.delete(item.id);
                    }}
                    role="option"
                    aria-selected={isSelected}
                    tabIndex={isSelected ? 0 : -1}
                    className={`saved-item ${isSelected ? "is-selected" : ""} ${
                      item.favourite ? "is-favourite" : ""
                    }`}
                    onClick={() => select(item.id)}
                  >
                    {item.favourite && (
                      <span className="star" aria-label="Favourite">
                        ★
                      </span>
                    )}
                    <span className="saved-name">{item.name}</span>
                  </li>
                );
              })}
            </ul>
          </nav>
          <section className="saved-detail-pane" aria-label="Selected settlement">
            {body}
          </section>
        </div>
      )}
    </ScreenFrame>
  );
}

function DetailPane({
  detail,
  tagView,
  actionsRef,
  error,
  onBackToList,
  onEdit,
  onFavourite,
  onDelete,
}: {
  detail: SavedSettlementView | null;
  tagView: boolean;
  actionsRef: RefObject<HTMLDivElement | null>;
  error: string | null;
  onBackToList: () => void;
  onEdit: () => void;
  onFavourite: (favourite: boolean) => void;
  onDelete: () => void;
}) {
  const [confirmDelete, setConfirmDelete] = useState(false);
  useEffect(() => setConfirmDelete(false), [detail?.id]);
  if (!detail) return <p className="muted">Loading…</p>;

  // Toolbar: Left/Right move between actions; Left from the first returns
  // to the list.
  const onActionsKey = (event: KeyboardEvent<HTMLDivElement>) => {
    const buttons = [...(actionsRef.current?.querySelectorAll("button") ?? [])];
    const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
    if (event.key === "ArrowRight" && index < buttons.length - 1) {
      event.preventDefault();
      buttons[index + 1].focus();
    }
    if (event.key === "ArrowLeft") {
      event.preventDefault();
      if (index > 0) buttons[index - 1].focus();
      else onBackToList();
    }
  };

  return (
    <div className="saved-detail">
      <header className="saved-detail-header">
        <h2 className={`display saved-title ${detail.favourite ? "is-favourite" : ""}`}>
          {detail.favourite && (
            <span className="star" aria-label="Favourite">
              ★{" "}
            </span>
          )}
          {detail.name}
        </h2>
        <p className="saved-meta">
          Generated with v{detail.app_version} · Created {formatWhen(detail.created_at)}
          {detail.updated_at !== detail.created_at && ` · Edited ${formatWhen(detail.updated_at)}`}
        </p>
        {confirmDelete ? (
          <ConfirmBar
            message={`Delete “${detail.name}”? This cannot be undone.`}
            confirmLabel="Delete"
            onConfirm={() => {
              setConfirmDelete(false);
              onDelete();
            }}
            onCancel={() => setConfirmDelete(false)}
          />
        ) : (
          <div
            ref={actionsRef}
            className="action-bar"
            role="toolbar"
            aria-label="Settlement actions"
            onKeyDown={onActionsKey}
          >
            <button type="button" className="btn" onClick={onEdit}>
              Edit
            </button>
            {detail.favourite ? (
              <button type="button" className="btn" onClick={() => onFavourite(false)}>
                Unfavourite
              </button>
            ) : (
              <>
                <button type="button" className="btn" onClick={() => onFavourite(true)}>
                  Favourite
                </button>
                <button
                  type="button"
                  className="btn btn-quiet-danger"
                  onClick={() => setConfirmDelete(true)}
                >
                  Delete
                </button>
              </>
            )}
          </div>
        )}
        {error && (
          <p className="alert alert-inline" role="alert">
            {error}
          </p>
        )}
      </header>
      <SettlementSheet
        label={`${detail.name} snapshot`}
        rows={foldSubStages(
          detail.stages.map((stage) => ({
            key: stage.stage_table,
            label: stage.label,
            value: stage.result?.name ?? (stage.skipped ? "Not applicable" : "—"),
            source: stage.table && stage.table.name !== stage.label ? stage.table.name : undefined,
            tags: tagView ? stage.tags : undefined,
            subStage: stage.sub_stage,
            skipped: stage.skipped,
            figures: stage.figures,
          })),
        )}
        meta={`Snapshot saved with v${detail.app_version}. Later changes to tables do not alter it.`}
      />
    </div>
  );
}

/**
 * Edits create an explicit revised snapshot. The saved results are reopened
 * as they are; only stages the user changes (and stages that depend on them)
 * are redone. Nothing is saved until "Save changes".
 */
function EditPane({
  settlement,
  onCancel,
  onSaved,
}: {
  settlement: SavedSettlementView;
  onCancel: () => void;
  onSaved: (updated: SavedSettlementView) => void;
}) {
  const [response, setResponse] = useState<GenerationResponse | null>(null);
  const [name, setName] = useState(settlement.name);
  const [busy, setBusy] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [dirty, setDirty] = useState(false);
  const [confirmingCancel, setConfirmingCancel] = useState(false);
  const nameInput = useRef<HTMLInputElement>(null);

  useEffect(() => {
    saved.editStart(settlement.id).then(
      (r) => {
        setResponse(r);
        setBusy(false);
        nameInput.current?.focus();
      },
      (e: unknown) => {
        setError(String(e));
        setBusy(false);
      },
    );
  }, [settlement.id]);

  const run = async (call: Promise<GenerationResponse>) => {
    setBusy(true);
    try {
      const next = await call;
      setResponse(next);
      setError(next.error);
      if (!next.error) setDirty(true);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const discard = async () => {
    await generation.discard().catch(() => undefined);
    onCancel();
  };

  // Leaving an edit with changes asks first, as in the terminal interface.
  const cancel = () => {
    if (changed) setConfirmingCancel(true);
    else void discard();
  };

  const save = async () => {
    const problem = nameProblem(name);
    if (problem) {
      setError(problem);
      nameInput.current?.focus();
      return;
    }
    setBusy(true);
    try {
      onSaved(await saved.editSave(name.trim()));
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  };

  // Esc cancels the edit rather than leaving the screen.
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      if (confirmingCancel) setConfirmingCancel(false);
      else cancel();
    }
  };

  const complete = response?.view.complete ?? false;
  const changed = dirty || name.trim() !== settlement.name;

  return (
    <div className="edit-pane" onKeyDown={onKeyDown}>
      <h2 className="display saved-title">Edit settlement</h2>
      <p className="muted small">
        Saved results stay exactly as they are unless you change them. If a change affects later
        stages, those stages must be redone before saving.
      </p>
      <label htmlFor="edit-name" className="setting-name">
        Name
      </label>
      <input
        ref={nameInput}
        id="edit-name"
        className="text-input"
        value={name}
        maxLength={MAX_NAME_LENGTH + 20}
        onChange={(e) => setName(e.target.value)}
      />
      {error && (
        <p className="alert alert-inline" role="alert">
          {error}
        </p>
      )}
      {response && (
        <StageGrid
          stages={response.view.stages}
          busy={busy}
          onRoll={(i) => run(generation.roll(i))}
          onChoose={(i, r) => run(generation.choose(i, r))}
          onCustomName={(i, name) => run(generation.customName(i, name))}
          onAct={run}
        />
      )}
      <div className="action-bar edit-actions">
        <button
          type="button"
          className="btn btn-primary"
          disabled={busy || !complete || !changed}
          onClick={save}
        >
          Save changes
        </button>
        <button type="button" className="btn" onClick={cancel}>
          Cancel
        </button>
        {!complete && response && <span className="muted small">Finish every stage to save.</span>}
      </div>
      {confirmingCancel && (
        <ConfirmBar
          message="Discard your changes to this settlement?"
          confirmLabel="Discard"
          onConfirm={() => void discard()}
          onCancel={() => setConfirmingCancel(false)}
        />
      )}
    </div>
  );
}
