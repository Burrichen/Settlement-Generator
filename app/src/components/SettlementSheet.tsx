import type { AuthorityFigures, Named, NotableLocations } from "../api";
import { FiguresSummary } from "./AuthorityFigures";
import { LocationsSummary } from "./NotableLocations";
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
  /** Further lines of the value, from the stage's sub-stages. */
  details?: string[];
  /** The Authority Figures, on that stage's row. */
  figures?: AuthorityFigures;
  /** The Notable Locations, on that stage's row. */
  locations?: NotableLocations;
}

/** A stage to lay out on a sheet, before sub-stages are folded in. */
export interface SheetStage extends SheetRow {
  subStage?: boolean;
  skipped?: boolean;
}

/**
 * Folds each sub-stage into the stage before it, as a further line of its
 * value (e.g. "Local Council" then "4 Representatives"). Sub-stages that do
 * not apply are left out.
 */
export function foldSubStages(stages: SheetStage[]): SheetRow[] {
  const rows: SheetRow[] = [];
  for (const { subStage, skipped, ...row } of stages) {
    const parent = rows[rows.length - 1];
    if (!subStage || !parent) {
      rows.push(row);
    } else if (!skipped) {
      parent.details = [...(parent.details ?? []), row.value];
      if (row.tags) parent.tags = [...(parent.tags ?? []), ...row.tags];
    }
  }
  return rows;
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
              {row.figures ? (
                <FiguresSummary record={row.figures} showTags={row.tags !== undefined} />
              ) : row.locations ? (
                <LocationsSummary record={row.locations} />
              ) : (
                <span className="sheet-value">{row.value}</span>
              )}
              {row.details?.map((d, i) => (
                <span key={i} className="sheet-detail">
                  {d}
                </span>
              ))}
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
