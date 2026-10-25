import { useEffect, useId, useMemo, useRef, useState, type KeyboardEvent, type Ref } from "react";
import type { District } from "../api";
import {
  MAX_LOCATION_NAME_CHARS,
  MAX_LOCATION_NOTES_CHARS,
  MAX_LOCATION_TYPE_CHARS,
  type AssignOptions,
  type LocationEdit,
  type LocationEntry,
  type LocationPlacement,
  type PoiEntry,
} from "./types";

// ----- shared bits -----------------------------------------------------------

/** "Building 12": the number in a building ID. */
export function buildingNumber(buildingId: string): string {
  const n = Number(/(\d+)$/.exec(buildingId)?.[1] ?? NaN);
  return Number.isFinite(n) ? `Building ${n}` : buildingId;
}

/** "Building 12, District 2". */
export function placementText(p: LocationPlacement): string {
  return `${buildingNumber(p.building_id)}, ${p.district?.label ?? p.map_label}`;
}

export function MarkerBadge({ number }: { number?: number }) {
  return number === undefined ? (
    <span className="map-marker-badge is-empty" aria-hidden="true">
      –
    </span>
  ) : (
    <span className="map-marker-badge" aria-label={`Marker ${number}`}>
      {number}
    </span>
  );
}

/** Whether a location is standing on a building that still exists. */
export function isPlaced(l: LocationEntry): l is LocationEntry & { placement: LocationPlacement } {
  return !!l.placement && !l.placement.missing;
}

function matches(l: LocationEntry, q: string): boolean {
  if (!q) return true;
  const text = [l.name, l.location_type, l.district?.label, l.marker && String(l.marker)]
    .filter(Boolean)
    .join(" ")
    .toLowerCase();
  return text.includes(q);
}

// ----- Make Notable Location -----------------------------------------------------

/** What the user chose in the dialog. */
export type LocationChoice =
  | { kind: "existing"; id: string; options: AssignOptions }
  | { kind: "custom"; edit: LocationEdit; options: AssignOptions };

const CUSTOM = "__custom__";

/**
 * Chooses the Notable Location for a building: one of the settlement's
 * locations (searchable), or a new custom one. Moving a location from
 * another building, or replacing this building's location, is spelled out
 * before it is confirmed; nothing happens until then.
 */
export function LocationDialog({
  buildingLabel,
  mapDistrict,
  buildingId,
  mapId,
  locations,
  busy,
  error,
  onConfirm,
  onCancel,
}: {
  buildingLabel: string;
  /** The district of the building's map, for district maps. */
  mapDistrict?: District;
  buildingId: string;
  mapId: string;
  locations: LocationEntry[];
  busy: boolean;
  error: string | null;
  onConfirm: (choice: LocationChoice) => void;
  onCancel: () => void;
}) {
  const titleId = useId();
  const searchId = useId();
  const [query, setQuery] = useState("");
  const [chosen, setChosen] = useState<string | null>(null);
  const [custom, setCustom] = useState({ name: "", type: "" });
  const search = useRef<HTMLInputElement>(null);
  useEffect(() => search.current?.focus(), []);

  const occupant = locations.find(
    (l) => isPlaced(l) && l.placement.building_id === buildingId && l.placement.map_id === mapId,
  );
  /** Why a location cannot stand on this building, if it cannot. */
  const blocked = (l: LocationEntry): string | null => {
    if (l.removed) return "No longer in the settlement";
    if (l.district && l.district.id !== mapDistrict?.id) return `Belongs to ${l.district.label}`;
    return null;
  };
  const q = query.trim().toLowerCase();
  const shown = useMemo(() => locations.filter((l) => matches(l, q)), [locations, q]);
  const selected = locations.find((l) => l.id === chosen);
  const here = selected && occupant?.id === selected.id;
  const moving = selected && isPlaced(selected) && !here;
  const replacing = !!occupant && !here && chosen !== null;

  const status = (l: LocationEntry) => {
    const why = blocked(l);
    if (why) return why;
    if (occupant?.id === l.id) return "On this building";
    if (isPlaced(l)) return `On ${placementText(l.placement)}`;
    if (l.placement?.missing) return "Its building was redrawn: place it again";
    return "Not placed";
  };

  const options: AssignOptions = { move_location: !!moving, replace_occupant: replacing };
  const customValid = custom.name.trim() !== "" || custom.type.trim() !== "";
  const canConfirm =
    !busy && !here && (chosen === CUSTOM ? customValid : !!selected && blocked(selected) === null);
  const confirmLabel = moving ? "Move here" : replacing ? "Replace" : "Make Notable Location";

  const confirm = () => {
    if (!canConfirm) return;
    if (chosen === CUSTOM) {
      onConfirm({
        kind: "custom",
        edit: {
          name: custom.name.trim() || null,
          location_type: custom.type.trim() || null,
          notes: "",
        },
        options,
      });
    } else if (selected) {
      onConfirm({ kind: "existing", id: selected.id, options });
    }
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      onCancel();
    }
  };

  const ids = [...shown.map((l) => l.id), CUSTOM];
  const onListKey = (event: KeyboardEvent<HTMLUListElement>) => {
    const index = ids.indexOf(chosen ?? "");
    let next: number | null = null;
    if (event.key === "ArrowDown") next = Math.min(index + 1, ids.length - 1);
    if (event.key === "ArrowUp") next = Math.max(index - 1, 0);
    if (event.key === "Home") next = 0;
    if (event.key === "End") next = ids.length - 1;
    if (next !== null) {
      event.preventDefault();
      setChosen(ids[next]);
    }
    if (event.key === "Enter") {
      event.preventDefault();
      confirm();
    }
  };

  return (
    <div className="map-dialog-backdrop" onKeyDown={onKeyDown}>
      <div className="map-dialog" role="dialog" aria-modal="true" aria-labelledby={titleId}>
        <h3 id={titleId} className="map-dialog-title">
          Make {buildingLabel} a Notable Location
        </h3>
        <label htmlFor={searchId} className="visually-hidden">
          Search Notable Locations
        </label>
        <input
          ref={search}
          id={searchId}
          type="search"
          className="text-input map-search"
          placeholder="Search by name, type or district"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <ul
          className="map-location-options"
          role="listbox"
          aria-label="Notable Locations"
          tabIndex={0}
          aria-activedescendant={chosen ? `${titleId}-${chosen}` : undefined}
          onKeyDown={onListKey}
        >
          {shown.map((l) => {
            const why = blocked(l);
            return (
              <li
                key={l.id}
                id={`${titleId}-${l.id}`}
                role="option"
                aria-selected={chosen === l.id}
                aria-disabled={why ? true : undefined}
                className={`${chosen === l.id ? "is-chosen" : ""} ${why ? "is-blocked" : ""}`}
                onClick={() => setChosen(l.id)}
              >
                <MarkerBadge number={l.marker} />
                <span className="map-location-text">
                  <span className="map-location-name">{l.name}</span>
                  <span className="map-location-meta">
                    {l.location_type}
                    {l.district && ` · ${l.district.label}`}
                    {l.origin === "custom" && " · Custom"}
                  </span>
                  <span className="map-location-status">{status(l)}</span>
                </span>
              </li>
            );
          })}
          {shown.length === 0 && (
            <li className="map-location-none" role="presentation">
              No Notable Location matches “{query.trim()}”.
            </li>
          )}
          <li
            id={`${titleId}-${CUSTOM}`}
            role="option"
            aria-selected={chosen === CUSTOM}
            className={chosen === CUSTOM ? "is-chosen" : ""}
            onClick={() => setChosen(CUSTOM)}
          >
            <span className="map-marker-badge is-empty" aria-hidden="true">
              +
            </span>
            <span className="map-location-text">
              <span className="map-location-name">Custom entry…</span>
              <span className="map-location-meta">A location of your own, such as a castle</span>
            </span>
          </li>
        </ul>

        {chosen === CUSTOM && (
          <div className="map-dialog-custom">
            <div className="field">
              <label className="field-label" htmlFor={`${titleId}-name`}>
                Name
              </label>
              <input
                id={`${titleId}-name`}
                className="text-input map-edit-input"
                maxLength={MAX_LOCATION_NAME_CHARS}
                value={custom.name}
                onChange={(e) => setCustom((c) => ({ ...c, name: e.target.value }))}
              />
            </div>
            <div className="field">
              <label className="field-label" htmlFor={`${titleId}-type`}>
                Type
              </label>
              <input
                id={`${titleId}-type`}
                className="text-input map-edit-input"
                maxLength={MAX_LOCATION_TYPE_CHARS}
                placeholder="e.g. Castle, Market, Shrine"
                value={custom.type}
                onChange={(e) => setCustom((c) => ({ ...c, type: e.target.value }))}
              />
            </div>
          </div>
        )}

        <div className="map-dialog-consequences" aria-live="polite">
          {here && <p>{selected!.name} is already on this building.</p>}
          {moving && (
            <p>
              {selected!.name} will move from {placementText(selected!.placement!)}. Its marker
              number{selected!.marker ? ` (${selected!.marker})` : ""} stays the same.
            </p>
          )}
          {replacing && occupant && (
            <p>
              {occupant.name} will be taken off this building. It is kept, with its number, and can
              be placed again.
            </p>
          )}
          {error && (
            <p className="field-error" role="alert">
              {error}
            </p>
          )}
        </div>

        <div className="action-bar map-dialog-actions">
          <button type="button" className="btn" onClick={onCancel}>
            Cancel
          </button>
          <button
            type="button"
            className={`btn ${moving || replacing ? "btn-danger" : "btn-primary"}`}
            disabled={!canConfirm}
            onClick={confirm}
          >
            {busy ? "Saving…" : confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}

// ----- editing a location ------------------------------------------------------

/** Renames a location and edits its notes (and a custom one's type). */
export function LocationForm({
  location,
  busy,
  onSave,
  onCancel,
}: {
  location: LocationEntry;
  busy: boolean;
  onSave: (edit: LocationEdit) => void;
  onCancel: () => void;
}) {
  const id = useId();
  const custom = location.origin === "custom";
  const [name, setName] = useState(location.renamed ? location.name : "");
  const [type, setType] = useState(custom ? location.location_type : "");
  const [notes, setNotes] = useState(location.notes ?? "");
  const first = useRef<HTMLInputElement>(null);
  useEffect(() => first.current?.focus(), []);
  const dirty =
    name !== (location.renamed ? location.name : "") ||
    notes !== (location.notes ?? "") ||
    (custom && type !== location.location_type);

  return (
    <form
      className="map-location-form"
      aria-label={`Edit ${location.name}`}
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.preventDefault();
          e.stopPropagation();
          onCancel();
        }
      }}
      onSubmit={(e) => {
        e.preventDefault();
        if (dirty && !busy) {
          onSave({
            name: name.trim() || null,
            location_type: custom ? type.trim() || null : null,
            notes: notes.trim(),
          });
        }
      }}
    >
      <div className="field">
        <label className="field-label" htmlFor={`${id}-name`}>
          Location name
        </label>
        <input
          ref={first}
          id={`${id}-name`}
          className="text-input map-edit-input"
          maxLength={MAX_LOCATION_NAME_CHARS}
          placeholder={location.default_name}
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
        <span className="field-help">Leave empty to use “{location.default_name}”.</span>
      </div>
      {custom && (
        <div className="field">
          <label className="field-label" htmlFor={`${id}-type`}>
            Location type
          </label>
          <input
            id={`${id}-type`}
            className="text-input map-edit-input"
            maxLength={MAX_LOCATION_TYPE_CHARS}
            value={type}
            onChange={(e) => setType(e.target.value)}
          />
        </div>
      )}
      <div className="field">
        <label className="field-label" htmlFor={`${id}-notes`}>
          Location notes
        </label>
        <textarea
          id={`${id}-notes`}
          className="text-input text-area map-edit-input"
          rows={3}
          maxLength={MAX_LOCATION_NOTES_CHARS}
          value={notes}
          onChange={(e) => setNotes(e.target.value)}
        />
      </div>
      <div className="action-bar map-inspector-actions">
        <button type="submit" className="btn btn-small btn-primary" disabled={!dirty || busy}>
          {busy ? "Saving…" : "Save location"}
        </button>
        <button type="button" className="btn btn-small" onClick={onCancel}>
          Cancel
        </button>
      </div>
    </form>
  );
}

// ----- the settlement-wide directory -----------------------------------------------

export type DirectoryScope = "district" | "settlement";

/** The district an entry belongs to or stands in. */
function districtOf(l: LocationEntry): District | undefined {
  return l.district ?? (isPlaced(l) ? l.placement.district : undefined);
}

/**
 * The settlement's directory: every Notable Location and point of interest,
 * searchable. For Cities and Capitals it is grouped by district and can be
 * narrowed to the district on screen. An entry on a map opens it there
 * (switching district if needed) and selects it.
 */
export function LocationDirectory({
  locations,
  pois,
  byDistrict,
  scope,
  onScope,
  currentDistrict,
  onGo,
  onGoPoi,
  searchRef,
}: {
  locations: LocationEntry[];
  pois: PoiEntry[];
  byDistrict: boolean;
  scope: DirectoryScope;
  onScope: (scope: DirectoryScope) => void;
  /** The district on screen, for the "This district" scope. */
  currentDistrict?: District;
  onGo: (location: LocationEntry & { placement: LocationPlacement }) => void;
  onGoPoi: (poi: PoiEntry) => void;
  searchRef?: Ref<HTMLInputElement>;
}) {
  const [query, setQuery] = useState("");
  const id = useId();
  const q = query.trim().toLowerCase();
  const narrowed = byDistrict && scope === "district";
  const inScope = (d: District | undefined) => !narrowed || d?.id === currentDistrict?.id;
  const shownLocations = locations.filter((l) => matches(l, q) && inScope(districtOf(l)));
  const shownPois = pois.filter(
    (p) =>
      inScope(p.district) &&
      (!q || [p.label, p.district?.label, String(p.number)].join(" ").toLowerCase().includes(q)),
  );
  type Group = { locations: LocationEntry[]; pois: PoiEntry[] };
  const groups = new Map<string, Group>();
  const group = (key: string) => {
    if (!groups.has(key)) groups.set(key, { locations: [], pois: [] });
    return groups.get(key)!;
  };
  const keyOf = (d: District | undefined) =>
    byDistrict && !narrowed ? (d?.label ?? "Whole settlement") : "";
  for (const l of shownLocations) group(keyOf(districtOf(l))).locations.push(l);
  for (const p of shownPois) group(keyOf(p.district)).pois.push(p);
  const placed = locations.filter(isPlaced).length;
  const empty = shownLocations.length === 0 && shownPois.length === 0;

  return (
    <div className="map-location-directory">
      {byDistrict && (
        <div className="map-scope" role="radiogroup" aria-label="Directory scope">
          {(["district", "settlement"] as const).map((s) => (
            <button
              key={s}
              type="button"
              role="radio"
              aria-checked={scope === s}
              className="btn btn-small"
              onClick={() => onScope(s)}
            >
              {s === "district" ? (currentDistrict?.label ?? "This district") : "Whole settlement"}
            </button>
          ))}
        </div>
      )}
      <label htmlFor={id} className="visually-hidden">
        Search the directory
      </label>
      <input
        ref={searchRef}
        id={id}
        type="search"
        className="text-input map-search"
        placeholder="Search locations and points"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
      />
      <p className="muted small map-count">
        {locations.length === 0 && pois.length === 0
          ? "Nothing here yet. Make a building a Notable Location, or add a point of interest."
          : `${placed} of ${locations.length} Notable Locations placed · ${pois.length} point${
              pois.length === 1 ? "" : "s"
            } of interest`}
      </p>
      {[...groups].map(([key, g]) => (
        <section key={key} aria-label={key || undefined}>
          {key && <h4 className="map-subheading">{key}</h4>}
          <ul className="map-location-list">
            {g.locations.map((l) => (
              <li key={l.id}>
                {isPlaced(l) ? (
                  <button
                    type="button"
                    className="map-location-row"
                    onClick={() => onGo(l)}
                    title={`Show on the map: ${placementText(l.placement)}`}
                  >
                    <MarkerBadge number={l.marker} />
                    <span className="map-location-text">
                      <span className="map-location-name">{l.name}</span>
                      <span className="map-location-meta">
                        {l.location_type} · {placementText(l.placement)}
                      </span>
                    </span>
                  </button>
                ) : (
                  <div className="map-location-row is-unplaced">
                    <MarkerBadge number={l.marker} />
                    <span className="map-location-text">
                      <span className="map-location-name">{l.name}</span>
                      <span className="map-location-meta">
                        {l.location_type} ·{" "}
                        {l.placement?.missing ? "Its building was redrawn" : "Not placed"}
                      </span>
                    </span>
                  </div>
                )}
              </li>
            ))}
            {g.pois.map((p) => (
              <li key={p.id}>
                <button
                  type="button"
                  className="map-location-row"
                  onClick={() => onGoPoi(p)}
                  title={`Show on the map: ${p.district?.label ?? p.map_label}`}
                >
                  <span className="map-marker-badge is-poi">{p.number}</span>
                  <span className="map-location-text">
                    <span className="map-location-name">{p.label}</span>
                    <span className="map-location-meta">
                      Point of interest · {p.district?.label ?? p.map_label}
                    </span>
                  </span>
                </button>
              </li>
            ))}
          </ul>
        </section>
      ))}
      {!empty || !(q || narrowed) ? null : (
        <p className="muted small">
          {q ? `Nothing matches “${query.trim()}”.` : "Nothing in this district yet."}
        </p>
      )}
    </div>
  );
}
