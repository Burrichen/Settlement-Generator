import type { Named } from "../api";
import { TagList } from "./generation";
import { Ornament } from "./Ornament";

export interface SheetRow {
  key: string;
  label: string;
  value: string;
  /** The table actually used, when it differs from the stage. */
  source?: string;
  /** Only passed when Tag View is on. */
  tags?: Named[];
}

/** A settlement laid out stage by stage, between ornaments. */
export function SettlementSheet({
  rows,
  meta,
  label = "Settlement",
}: {
  rows: SheetRow[];
  meta: string;
  label?: string;
}) {
  return (
    <article className="settlement-sheet" aria-label={label}>
      <Ornament />
      <dl className="sheet-list">
        {rows.map((row) => (
          <div key={row.key} className="sheet-row">
            <dt className="label">{row.label}</dt>
            <dd>
              <span className="sheet-value">{row.value}</span>
              {row.source && <span className="sheet-source">{row.source}</span>}
              <TagList tags={row.tags} />
            </dd>
          </div>
        ))}
      </dl>
      <Ornament />
      <p className="sheet-meta">{meta}</p>
    </article>
  );
}
