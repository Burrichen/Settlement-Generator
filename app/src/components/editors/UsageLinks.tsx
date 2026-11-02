// Where something is used, as links that open the dependent table, tag,
// flow or setting directly.
import { useApp, type ItemTarget } from "../../app-state";

/** A usage from the core: a description, and what it points at. */
export interface UsageLike {
  description: string;
  table?: string;
  tag?: string;
  flow?: string;
  location?: { table?: string; tag?: string; flow?: string };
}

/** The item a usage belongs to: its flow, else its table, else its tag. */
export function usageTarget(u: UsageLike): ItemTarget | null {
  const flow = u.flow ?? u.location?.flow;
  const table = u.table ?? u.location?.table;
  const tag = u.tag ?? u.location?.tag;
  if (flow) return { kind: "flow", id: flow };
  if (table) return { kind: "table", id: table };
  if (tag) return { kind: "tag", id: tag };
  return null;
}

export function UsageLinks({
  usages,
  label,
  strip,
}: {
  usages: UsageLike[];
  label: string;
  /** Text to leave off each description (e.g. a group heading's words). */
  strip?: RegExp;
}) {
  const { openItem } = useApp();
  return (
    <ul className="usage-list" aria-label={label}>
      {usages.map((u, i) => {
        const text = strip ? u.description.replace(strip, "") : u.description;
        const target = usageTarget(u);
        return (
          <li key={i}>
            {target && openItem ? (
              <button
                type="button"
                className="link-button usage-link"
                title={`Open this ${target.kind}`}
                onClick={() => openItem(target)}
              >
                {text}
              </button>
            ) : (
              text
            )}
          </li>
        );
      })}
    </ul>
  );
}

/** Shown instead of deleting something still in use. */
export function DeleteBlocked({
  noun,
  usages,
  onClose,
}: {
  noun: string;
  usages: UsageLike[];
  onClose: () => void;
}) {
  return (
    <div className="note delete-blocked" role="alert">
      <p>
        This {noun} cannot be deleted: it is still used by the places below. Change or remove those
        uses first. Nothing has been deleted.
      </p>
      <UsageLinks usages={usages} label={`Uses of this ${noun}`} />
      <button type="button" className="btn btn-small" onClick={onClose}>
        OK
      </button>
    </div>
  );
}
