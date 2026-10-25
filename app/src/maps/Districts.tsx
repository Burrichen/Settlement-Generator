import { useEffect, useId, useRef, useState, type KeyboardEvent } from "react";
import { ConfirmBar } from "../components/generation";
import { MarkerBadge } from "./NotableLocations";
import {
  MAX_DISTRICT_NAME_CHARS,
  MAX_LOCATION_NOTES_CHARS,
  MAX_POI_LABEL_CHARS,
  type MapAnnotations,
  type PlannedMapView,
  type PoiEntry,
} from "./types";

// ----- the district bar -------------------------------------------------------

/** "4 building records, 1 placed Notable Location and 2 points of interest". */
export function annotationText(a: MapAnnotations): string {
  const parts = [
    a.building_details &&
      `${a.building_details} building record${a.building_details === 1 ? "" : "s"}`,
    a.pois && `${a.pois} point${a.pois === 1 ? "" : "s"} of interest`,
  ].filter(Boolean) as string[];
  return parts.length === 0 ? "" : parts.join(" and ");
}

/**
 * The active district of a City or Capital: its name (renamable, never
 * changing its ID), deleting its map with a warning about what goes with it,
 * a search over the districts, and the way back to the settlement-wide
 * directory.
 */
export function DistrictBar({
  planned,
  active,
  busy,
  onSearch,
  onRename,
  onDeleteMap,
  onDirectory,
}: {
  planned: PlannedMapView[];
  active: PlannedMapView;
  busy: boolean;
  /** Switches to the first district matching the search. */
  onSearch: (id: string) => void;
  onRename: (name: string | null) => Promise<boolean>;
  onDeleteMap: () => void;
  onDirectory: () => void;
}) {
  const id = useId();
  const [query, setQuery] = useState("");
  const [renaming, setRenaming] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [name, setName] = useState("");
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => {
    setRenaming(false);
    setConfirmDelete(false);
  }, [active.id]);
  useEffect(() => {
    if (renaming) input.current?.focus();
  }, [renaming]);

  const q = query.trim().toLowerCase();
  const matches = q
    ? planned.filter(
        (p) =>
          p.label.toLowerCase().includes(q) || (p.default_label ?? "").toLowerCase().includes(q),
      )
    : [];
  const a = active.annotations;
  const extra = annotationText(a);

  const submit = async () => {
    if (await onRename(name.trim() || null)) setRenaming(false);
  };

  return (
    <div className="map-district-bar">
      <div className="map-district-row">
        <label htmlFor={`${id}-search`} className="visually-hidden">
          Search districts
        </label>
        <input
          id={`${id}-search`}
          type="search"
          className="text-input map-search map-district-search"
          placeholder="Search districts"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && matches[0]) {
              e.preventDefault();
              onSearch(matches[0].id);
              setQuery("");
            }
          }}
          aria-describedby={`${id}-results`}
        />
        <span id={`${id}-results`} className="muted small" aria-live="polite">
          {q && (matches.length === 0 ? "No district matches." : null)}
        </span>
        {q && matches.length > 0 && (
          <ul className="map-district-matches" aria-label="Matching districts">
            {matches.map((p) => (
              <li key={p.id}>
                <button
                  type="button"
                  className="btn btn-small"
                  onClick={() => {
                    onSearch(p.id);
                    setQuery("");
                  }}
                >
                  {p.label}
                  {p.default_label && <span className="muted"> ({p.default_label})</span>}
                </button>
              </li>
            ))}
          </ul>
        )}
        <span className="map-toolbar-gap" />
        <button type="button" className="btn btn-small" onClick={onDirectory}>
          Settlement directory
        </button>
      </div>

      {renaming ? (
        <form
          className="map-district-row"
          aria-label={`Rename ${active.label}`}
          onSubmit={(e) => {
            e.preventDefault();
            void submit();
          }}
          onKeyDown={(e: KeyboardEvent<HTMLFormElement>) => {
            if (e.key === "Escape") {
              e.preventDefault();
              e.stopPropagation();
              setRenaming(false);
            }
          }}
        >
          <label htmlFor={`${id}-name`} className="field-label">
            District name
          </label>
          <input
            ref={input}
            id={`${id}-name`}
            className="text-input map-edit-input map-district-name-input"
            maxLength={MAX_DISTRICT_NAME_CHARS}
            placeholder={active.default_label ?? active.label}
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
          <button type="submit" className="btn btn-small btn-primary" disabled={busy}>
            Save name
          </button>
          <button type="button" className="btn btn-small" onClick={() => setRenaming(false)}>
            Cancel
          </button>
          <span className="field-help">
            Leave empty for “{active.default_label ?? active.label}”. Its map and records are
            unchanged.
          </span>
        </form>
      ) : (
        <div className="map-district-row">
          <h3 className="map-district-title">
            {active.label}
            {active.default_label && <span className="muted small"> · {active.default_label}</span>}
          </h3>
          <button
            type="button"
            className="btn btn-small"
            onClick={() => {
              setName(active.default_label ? active.label : "");
              setRenaming(true);
            }}
          >
            Rename…
          </button>
          {active.generated && (
            <button
              type="button"
              className="btn btn-small"
              disabled={busy}
              onClick={() => setConfirmDelete(true)}
            >
              Delete map…
            </button>
          )}
        </div>
      )}

      {confirmDelete && (
        <ConfirmBar
          message={
            `Delete the map of ${active.label}?` +
            (extra ? ` Its ${extra} will be deleted too.` : "") +
            (a.locations
              ? ` ${a.locations} Notable Location${a.locations === 1 ? "" : "s"} on it will be taken off and kept, with ${a.locations === 1 ? "its number" : "their numbers"}.`
              : "") +
            " The district, its name and the other districts are kept, and the map can be drawn again."
          }
          confirmLabel="Delete map"
          onConfirm={() => {
            setConfirmDelete(false);
            onDeleteMap();
          }}
          onCancel={() => setConfirmDelete(false)}
        />
      )}
    </div>
  );
}

// ----- points of interest -------------------------------------------------------

/** Name and notes of a point of interest, for adding or editing one. */
export function PoiForm({
  title,
  initial,
  busy,
  submitLabel,
  onSave,
  onCancel,
}: {
  title: string;
  initial?: { label: string; notes: string };
  busy: boolean;
  submitLabel: string;
  onSave: (label: string, notes: string) => void;
  onCancel: () => void;
}) {
  const id = useId();
  const [label, setLabel] = useState(initial?.label ?? "");
  const [notes, setNotes] = useState(initial?.notes ?? "");
  const first = useRef<HTMLInputElement>(null);
  useEffect(() => first.current?.focus(), []);
  const valid = label.trim() !== "";
  return (
    <form
      className="map-location-form"
      aria-label={title}
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.preventDefault();
          e.stopPropagation();
          onCancel();
        }
      }}
      onSubmit={(e) => {
        e.preventDefault();
        if (valid && !busy) onSave(label.trim(), notes.trim());
      }}
    >
      <h4 className="map-inspector-title">{title}</h4>
      <div className="field">
        <label className="field-label" htmlFor={`${id}-label`}>
          Point name
        </label>
        <input
          ref={first}
          id={`${id}-label`}
          className="text-input map-edit-input"
          maxLength={MAX_POI_LABEL_CHARS}
          placeholder="e.g. Old well, Ambush site"
          value={label}
          onChange={(e) => setLabel(e.target.value)}
        />
      </div>
      <div className="field">
        <label className="field-label" htmlFor={`${id}-notes`}>
          Point notes
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
        <button type="submit" className="btn btn-small btn-primary" disabled={!valid || busy}>
          {busy ? "Saving…" : submitLabel}
        </button>
        <button type="button" className="btn btn-small" onClick={onCancel}>
          Cancel
        </button>
      </div>
    </form>
  );
}

/** The Location Inspector for a selected point of interest. */
export function PoiInspector({
  poi,
  busy,
  onSave,
  onMove,
  onDelete,
  onLocate,
  onClear,
}: {
  poi: PoiEntry;
  busy: boolean;
  onSave: (label: string, notes: string) => Promise<boolean>;
  /** Starts choosing a new position on the map. */
  onMove: () => void;
  onDelete: () => void;
  onLocate: () => void;
  onClear: () => void;
}) {
  const [editing, setEditing] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  useEffect(() => {
    setEditing(false);
    setConfirmDelete(false);
  }, [poi.id]);

  if (editing) {
    return (
      <PoiForm
        title={`Edit ${poi.label}`}
        initial={{ label: poi.label, notes: poi.notes ?? "" }}
        busy={busy}
        submitLabel="Save point"
        onCancel={() => setEditing(false)}
        onSave={async (label, notes) => {
          if (await onSave(label, notes)) setEditing(false);
        }}
      />
    );
  }
  return (
    <div className="map-inspector">
      <div className="map-inspector-head">
        <h4 className="map-inspector-title">{poi.label}</h4>
        <span className="map-state">Point of interest</span>
      </div>
      <dl className="map-facts">
        <dt>Marker</dt>
        <dd>
          <MarkerBadge number={poi.number} />
        </dd>
        <dt>{poi.district ? "District" : "Map"}</dt>
        <dd>{poi.district?.label ?? poi.map_label}</dd>
      </dl>
      {poi.notes && <p className="map-inspector-text map-inspector-notes">{poi.notes}</p>}
      {confirmDelete ? (
        <div className="map-confirm" role="alertdialog" aria-label="Delete point of interest">
          <p>
            Delete {poi.label}? Its number {poi.number} will not be given to anything else.
          </p>
          <div className="button-row">
            <button type="button" className="btn btn-small" onClick={() => setConfirmDelete(false)}>
              Cancel
            </button>
            <button
              type="button"
              className="btn btn-small btn-danger"
              disabled={busy}
              onClick={onDelete}
            >
              Delete point
            </button>
          </div>
        </div>
      ) : (
        <div className="action-bar map-inspector-actions">
          <button
            type="button"
            className="btn btn-small btn-primary"
            onClick={() => setEditing(true)}
          >
            Edit point
          </button>
          <button type="button" className="btn btn-small" onClick={onLocate}>
            Locate on map
          </button>
          <button type="button" className="btn btn-small" disabled={busy} onClick={onMove}>
            Move…
          </button>
          <button
            type="button"
            className="btn btn-small"
            disabled={busy}
            onClick={() => setConfirmDelete(true)}
          >
            Delete…
          </button>
          <button type="button" className="btn btn-small" onClick={onClear}>
            Clear selection
          </button>
        </div>
      )}
    </div>
  );
}
