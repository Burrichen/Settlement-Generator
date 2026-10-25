import { useId, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import type {
  BuildingSummary,
  MapBuilding,
  MapLegendEntry,
  SettlementMap,
  StreetKind,
} from "./types";

// ----- labels --------------------------------------------------------------

/** "Building 42", from the stable ID's sequence number. */
export function buildingLabel(b: MapBuilding): string {
  const number = Number(/(\d+)$/.exec(b.id)?.[1] ?? NaN);
  return Number.isFinite(number) ? `Building ${number}` : b.id;
}

const SIZE_LABEL = { small: "Small", medium: "Medium", large: "Large" } as const;
const STREET_KIND: Record<StreetKind, string> = { road: "Road", street: "Street", lane: "Lane" };

/** "Lane 12", from the street the building faces. */
export function streetLabel(map: SettlementMap, streetId: string | undefined): string | null {
  if (!streetId) return null;
  const street = map.geometry.streets.find((s) => s.id === streetId);
  const number = /(\d+)$/.exec(streetId)?.[1];
  return street ? `${STREET_KIND[street.kind]} ${number ?? ""}`.trim() : null;
}

export function buildingSummary(
  map: SettlementMap,
  b: MapBuilding,
  details?: BuildingSummary,
): string {
  const street = streetLabel(map, b.street);
  const kind = details?.building_type ?? SIZE_LABEL[b.size];
  return `${kind}${street ? ` · on ${street}` : ""}`;
}

// ----- collapsible panel ---------------------------------------------------

export function Panel({
  title,
  open,
  onToggle,
  children,
  className = "",
}: {
  title: string;
  open: boolean;
  onToggle: () => void;
  children: ReactNode;
  className?: string;
}) {
  const id = useId();
  return (
    <section className={`map-panel ${className} ${open ? "is-open" : ""}`} aria-label={title}>
      <h3 className="map-panel-heading">
        <button type="button" aria-expanded={open} aria-controls={id} onClick={onToggle}>
          <span aria-hidden="true" className="map-panel-chevron">
            {open ? "▾" : "▸"}
          </span>
          {title}
        </button>
      </h3>
      <div id={id} className="map-panel-body" hidden={!open}>
        {children}
      </div>
    </section>
  );
}

// ----- legend --------------------------------------------------------------

interface KeyItem {
  key: string;
  label: string;
  swatch: string;
}

/** The map key: only features this map actually has. */
function keyItems(map: SettlementMap): KeyItem[] {
  const g = map.geometry;
  const items: KeyItem[] = [{ key: "building", label: "Building", swatch: "building" }];
  items.push({ key: "selected", label: "Selected building", swatch: "selected" });
  const kinds = new Set(g.streets.map((s) => s.kind));
  if (kinds.has("road")) items.push({ key: "road", label: "Road", swatch: "road" });
  if (kinds.has("street")) items.push({ key: "street", label: "Street", swatch: "street" });
  if (kinds.has("lane")) items.push({ key: "lane", label: "Lane", swatch: "lane" });
  if (g.terrain.squares?.length) items.push({ key: "square", label: "Square", swatch: "square" });
  if (g.walls?.length) {
    items.push({ key: "wall", label: "Wall and towers", swatch: "wall" });
    if (g.walls.some((w) => w.gates.length)) {
      items.push({ key: "gate", label: "Gate", swatch: "gate" });
    }
  }
  for (const w of g.water ?? []) {
    const label = { coast: "Sea", river: "River", lake: "Lake", none: "Water" }[w.kind];
    if (!items.some((i) => i.key === w.kind)) items.push({ key: w.kind, label, swatch: "water" });
    if (w.bridges?.length && !items.some((i) => i.key === "bridge")) {
      items.push({ key: "bridge", label: "Bridge", swatch: "bridge" });
    }
  }
  if (g.terrain.fields?.length) items.push({ key: "field", label: "Field", swatch: "field" });
  if (g.terrain.trees?.length) items.push({ key: "trees", label: "Trees", swatch: "trees" });
  if (map.scope.kind === "district") {
    items.push({ key: "district", label: "District built area", swatch: "district" });
  }
  return items;
}

/**
 * The map key and this map's numbered locations. A numbered entry opens its
 * building: it is selected, centred and shown in the Location Inspector.
 */
export function Legend({
  map,
  entries,
  onGo,
}: {
  map: SettlementMap;
  entries: MapLegendEntry[];
  onGo: (entry: MapLegendEntry) => void;
}) {
  const [query, setQuery] = useState("");
  const id = useId();
  const q = query.trim().toLowerCase();
  const items = keyItems(map).filter((i) => i.label.toLowerCase().includes(q));
  const numbered = entries.filter(
    (e) =>
      [e.label, e.location_type, e.district?.label].some((t) => t?.toLowerCase().includes(q)) ||
      String(e.number) === q,
  );
  return (
    <div className="map-legend">
      <label htmlFor={id} className="visually-hidden">
        Search the legend
      </label>
      <input
        id={id}
        type="search"
        className="text-input map-search"
        placeholder="Search legend"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
      />
      <ul className="map-key" aria-label="Map key">
        {items.map((i) => (
          <li key={i.key}>
            <span className={`map-swatch swatch-${i.swatch}`} aria-hidden="true" />
            {i.label}
          </li>
        ))}
      </ul>
      <h4 className="map-subheading">Numbered locations</h4>
      {entries.length === 0 ? (
        <p className="muted small">No locations are marked on this map yet.</p>
      ) : (
        <ol className="map-numbered">
          {numbered.map((e) => (
            <li key={`${e.target.kind}-${e.number}`}>
              <button
                type="button"
                className="map-location-row"
                aria-label={`${e.number}: ${e.label}${e.location_type ? `, ${e.location_type}` : ""}${
                  e.district ? `, ${e.district.label}` : ""
                }`}
                onClick={() => onGo(e)}
              >
                <span
                  className={`map-marker-badge ${e.target.kind === "point_of_interest" ? "is-poi" : ""}`}
                >
                  {e.number}
                </span>
                <span className="map-location-text">
                  <span className="map-location-name">{e.label}</span>
                  {(e.location_type || e.district) && (
                    <span className="map-location-meta">
                      {[e.location_type, e.district?.label].filter(Boolean).join(" · ")}
                    </span>
                  )}
                </span>
              </button>
            </li>
          ))}
        </ol>
      )}
      {q && items.length === 0 && numbered.length === 0 && (
        <p className="muted small">Nothing in the legend matches “{query.trim()}”.</p>
      )}
    </div>
  );
}

// ----- building directory --------------------------------------------------

/** Rows rendered at once; a search narrows the rest. */
export const DIRECTORY_LIMIT = 150;

/**
 * Every building, searchable: the keyboard and screen-reader way to select
 * one. Up/Down move, Enter (or click) selects and centres the map.
 */
export function BuildingDirectory({
  map,
  summaries,
  selectedId,
  onChoose,
}: {
  map: SettlementMap;
  /** Generated buildings: their names and types are listed and searchable. */
  summaries: Map<string, BuildingSummary>;
  selectedId: string | null;
  onChoose: (b: MapBuilding) => void;
}) {
  const [query, setQuery] = useState("");
  const [active, setActive] = useState<string | null>(null);
  const listRef = useRef<HTMLUListElement>(null);
  const id = useId();
  const rows = useMemo(
    () =>
      map.geometry.buildings.map((b) => {
        const details = summaries.get(b.id);
        const number = buildingLabel(b);
        const label = details?.name ? `${details.name} (${number})` : number;
        const summary = buildingSummary(map, b, details);
        return { b, label, summary, text: `${label} ${summary}`.toLowerCase() };
      }),
    [map, summaries],
  );
  const q = query.trim().toLowerCase();
  const matches = q ? rows.filter((r) => r.text.includes(q)) : rows;
  const shown = matches.slice(0, DIRECTORY_LIMIT);
  const current = shown.find((r) => r.b.id === active) ?? shown.find((r) => r.b.id === selectedId);
  const focusRow = (buildingId: string) => {
    setActive(buildingId);
    listRef.current?.querySelector<HTMLElement>(`[data-building="${buildingId}"]`)?.focus();
  };

  const onKey = (event: KeyboardEvent<HTMLUListElement>) => {
    if (shown.length === 0) return;
    const index = Math.max(
      0,
      shown.findIndex((r) => r.b.id === current?.b.id),
    );
    let next: number | null = null;
    if (event.key === "ArrowDown") next = Math.min(index + 1, shown.length - 1);
    if (event.key === "ArrowUp") next = Math.max(index - 1, 0);
    if (event.key === "Home") next = 0;
    if (event.key === "End") next = shown.length - 1;
    if (next !== null) {
      event.preventDefault();
      focusRow(shown[next].b.id);
    }
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      onChoose(shown[index].b);
    }
  };

  return (
    <div className="map-directory">
      <label htmlFor={id} className="visually-hidden">
        Search buildings
      </label>
      <input
        id={id}
        type="search"
        className="text-input map-search"
        placeholder="Search buildings"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
      />
      <p className="muted small map-count" aria-live="polite">
        {matches.length === rows.length
          ? `${rows.length} buildings`
          : `${matches.length} of ${rows.length} buildings`}
        {matches.length > shown.length && ` · showing the first ${shown.length}`}
      </p>
      <ul
        ref={listRef}
        className="map-directory-list"
        role="listbox"
        aria-label="Buildings"
        onKeyDown={onKey}
      >
        {shown.map((r) => {
          const isCurrent = r.b.id === (current?.b.id ?? shown[0]?.b.id);
          return (
            <li
              key={r.b.id}
              data-building={r.b.id}
              role="option"
              aria-selected={r.b.id === selectedId}
              tabIndex={isCurrent ? 0 : -1}
              className={r.b.id === selectedId ? "is-selected" : ""}
              onClick={() => {
                setActive(r.b.id);
                onChoose(r.b);
              }}
            >
              <span className="map-directory-name">{r.label}</span>
              <span className="map-directory-meta">{r.summary}</span>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
