// Shared list pane and editing state for the tag and table editors.
import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
} from "react";
import type { DraftCheck } from "../../api";
import { useDraftCheck } from "./common";

export interface ListItem {
  id: string;
  name: string;
  /** Secondary line (the stable ID). */
  sub: string;
  /** Short status text, e.g. "Custom", "Disabled". */
  badges: string[];
  muted: boolean;
  problem: boolean;
}

/** Keyboard-navigable list: Up/Down/Home/End move, click selects. */
export function ItemList({
  label,
  items,
  selectedId,
  onSelect,
  filter,
  onFilter,
  footer,
}: {
  label: string;
  items: ListItem[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  filter: string;
  onFilter: (value: string) => void;
  footer?: ReactNode;
}) {
  const refs = useRef(new Map<string, HTMLLIElement>());
  const shown = items.filter((i) =>
    `${i.name} ${i.sub}`.toLowerCase().includes(filter.trim().toLowerCase()),
  );
  const move = (id: string) => {
    onSelect(id);
    refs.current.get(id)?.focus();
  };
  const onKey = (event: KeyboardEvent<HTMLUListElement>) => {
    if (shown.length === 0) return;
    const index = Math.max(
      0,
      shown.findIndex((i) => i.id === selectedId),
    );
    let next: number | null = null;
    if (event.key === "ArrowDown") next = Math.min(index + 1, shown.length - 1);
    if (event.key === "ArrowUp") next = Math.max(index - 1, 0);
    if (event.key === "Home") next = 0;
    if (event.key === "End") next = shown.length - 1;
    if (next !== null) {
      event.preventDefault();
      move(shown[next].id);
    }
  };
  return (
    <nav className="saved-list-pane editor-list-pane" aria-label={label}>
      <input
        className="text-input list-filter"
        type="search"
        placeholder="Filter by name or ID"
        aria-label={`Filter ${label}`}
        value={filter}
        onChange={(e) => onFilter(e.target.value)}
      />
      <ul className="saved-list" role="listbox" aria-label={label} onKeyDown={onKey}>
        {shown.map((item) => {
          const selected = item.id === selectedId;
          return (
            <li
              key={item.id}
              ref={(el) => {
                if (el) refs.current.set(item.id, el);
                else refs.current.delete(item.id);
              }}
              role="option"
              aria-selected={selected}
              tabIndex={selected || (!selectedId && item === shown[0]) ? 0 : -1}
              className={`saved-item editor-item ${selected ? "is-selected" : ""} ${
                item.muted ? "is-muted" : ""
              }`}
              onClick={() => move(item.id)}
            >
              <span className="editor-item-name">
                {item.problem && (
                  <span className="problem-mark" aria-label="Has problems">
                    !{" "}
                  </span>
                )}
                {item.name}
              </span>
              <span className="editor-item-sub">
                {item.sub}
                {item.badges.length > 0 && ` · ${item.badges.join(" · ")}`}
              </span>
            </li>
          );
        })}
        {shown.length === 0 && <li className="muted small editor-empty">Nothing matches.</li>}
      </ul>
      {footer && <div className="list-footer">{footer}</div>}
    </nav>
  );
}

const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);

/**
 * Editing state for one entity: the saved version, the draft, live checks
 * against the core, and a guard against losing unsaved changes.
 */
export function useEntityEditor<T extends { id: string }, D>({
  load,
  entityOf,
  check,
  save,
}: {
  load: (id: string) => Promise<D>;
  entityOf: (detail: D) => T;
  check: (draft: T) => Promise<DraftCheck>;
  save: (draft: T) => Promise<D>;
}) {
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [detail, setDetail] = useState<D | null>(null);
  const [original, setOriginal] = useState<T | null>(null);
  const [draft, setDraft] = useState<T | null>(null);
  const [isNew, setIsNew] = useState(false);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<{ text: string; error: boolean } | null>(null);
  const [pending, setPending] = useState<(() => void) | null>(null);

  const dirty = draft !== null && (isNew || !same(draft, original));
  const live = useDraftCheck(dirty ? draft : null, check);
  const liveCheck = dirty ? live : null;

  const open = useCallback(
    async (id: string) => {
      setBusy(true);
      setMessage(null);
      try {
        const d = await load(id);
        const entity = entityOf(d);
        setDetail(d);
        setOriginal(entity);
        setDraft(entity);
        setIsNew(false);
        setSelectedId(id);
      } catch (e) {
        setMessage({ text: String(e), error: true });
      } finally {
        setBusy(false);
      }
    },
    [load, entityOf],
  );

  /** Runs `action` now, or after the user agrees to discard changes. */
  const guard = (action: () => void) => {
    if (dirty) setPending(() => action);
    else action();
  };

  const select = (id: string) => {
    if (id === selectedId && !isNew) return;
    guard(() => void open(id));
  };

  const startNew = (entity: T) => {
    guard(() => {
      setDetail(null);
      setOriginal(entity);
      setDraft(entity);
      setIsNew(true);
      setSelectedId(entity.id);
      setMessage(null);
    });
  };

  const saveDraft = async (): Promise<boolean> => {
    if (!draft) return false;
    setBusy(true);
    try {
      const d = await save(draft);
      const entity = entityOf(d);
      setDetail(d);
      setOriginal(entity);
      setDraft(entity);
      setIsNew(false);
      setMessage({ text: "Saved", error: false });
      return true;
    } catch (e) {
      setMessage({ text: String(e), error: true });
      return false;
    } finally {
      setBusy(false);
    }
  };

  const revert = () => {
    setDraft(original);
    setMessage(null);
  };

  /** Takes a newer saved detail (e.g. after a related save), keeping
   * unsaved changes to the draft. */
  const replaceDetail = (d: D) => {
    setDetail(d);
    if (!dirty) {
      const entity = entityOf(d);
      setOriginal(entity);
      setDraft(entity);
    }
  };

  const clear = () => {
    setSelectedId(null);
    setDetail(null);
    setOriginal(null);
    setDraft(null);
    setIsNew(false);
  };

  // Changing the draft clears any old "Saved" message.
  useEffect(() => {
    if (dirty) setMessage((m) => (m?.error ? m : null));
  }, [draft, dirty]);

  return {
    selectedId,
    detail,
    draft,
    setDraft,
    original,
    isNew,
    dirty,
    liveCheck,
    busy,
    setBusy,
    message,
    setMessage,
    pending,
    confirmPending: () => {
      const action = pending;
      setPending(null);
      setDraft(original);
      action?.();
    },
    cancelPending: () => setPending(null),
    guard,
    open,
    select,
    startNew,
    saveDraft,
    revert,
    clear,
    replaceDetail,
  };
}
