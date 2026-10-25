import { Fragment, useEffect, useId, useRef, useState, type KeyboardEvent } from "react";
import { formatWhen } from "../api";
import { buildingLabel, streetLabel } from "./MapPanels";
import { LocationForm, MarkerBadge } from "./NotableLocations";
import {
  MAX_DESCRIPTION_CHARS,
  MAX_LABEL_CHARS,
  MAX_NAME_CHARS,
  MAX_NOTES_CHARS,
  MAX_OCCUPANTS,
  type BuildingDetailsView,
  type BuildingEdits,
  type BuildingField,
  type DetailsState,
  type FieldOrigin,
  type LocationEdit,
  type LocationEntry,
  type MapBuilding,
  type SettlementMap,
} from "./types";

// ----- drafts ----------------------------------------------------------------

/** The editable fields generation fills in. Notes are the user's own. */
export const DETAIL_FIELDS = [
  "name",
  "building_type",
  "condition",
  "occupancy",
  "occupants",
  "description",
] as const;
export type DetailKey = (typeof DETAIL_FIELDS)[number];

const LABEL: Record<DetailKey, string> = {
  name: "Name or address",
  building_type: "Type",
  condition: "Condition",
  occupancy: "Occupancy",
  occupants: "Occupants",
  description: "Description",
};

const LIMIT: Record<Exclude<DetailKey, "occupants">, number> = {
  name: MAX_NAME_CHARS,
  building_type: MAX_LABEL_CHARS,
  condition: MAX_LABEL_CHARS,
  occupancy: MAX_LABEL_CHARS,
  description: MAX_DESCRIPTION_CHARS,
};

/** One field being edited: its text, and whether it is the user's value. */
interface DraftField {
  text: string;
  edited: boolean;
}

/** Unsaved changes to one building, kept until saved or cancelled. */
export interface Draft {
  /** The version the changes are made from. */
  version: number;
  fields: Record<DetailKey, DraftField>;
  notes: string;
}

function asText(value: string | number | undefined | null): string {
  return value === undefined || value === null ? "" : String(value);
}

function field(d: BuildingDetailsView, key: DetailKey): BuildingField<string | number> {
  return d[key] as BuildingField<string | number>;
}

export function draftFrom(d: BuildingDetailsView): Draft {
  const fields = Object.fromEntries(
    DETAIL_FIELDS.map((key) => {
      const f = field(d, key);
      return [key, { text: asText(f.value), edited: f.origin === "user" }];
    }),
  ) as Record<DetailKey, DraftField>;
  return { version: d.version, fields, notes: d.notes ?? "" };
}

/** Why the draft cannot be saved, by field. */
function problems(draft: Draft): Partial<Record<DetailKey | "notes", string>> {
  const out: Partial<Record<DetailKey | "notes", string>> = {};
  const occupants = draft.fields.occupants.text.trim();
  if (
    draft.fields.occupants.edited &&
    occupants !== "" &&
    !(/^\d+$/.test(occupants) && Number(occupants) <= MAX_OCCUPANTS)
  ) {
    out.occupants = `Enter a whole number from 0 to ${MAX_OCCUPANTS}, or leave it blank.`;
  }
  for (const key of DETAIL_FIELDS) {
    if (key === "occupants") continue;
    if (draft.fields[key].text.trim().length > LIMIT[key]) {
      out[key] = `Keep it to ${LIMIT[key]} characters or fewer.`;
    }
  }
  if (draft.notes.trim().length > MAX_NOTES_CHARS) {
    out.notes = `Keep notes to ${MAX_NOTES_CHARS} characters or fewer.`;
  }
  return out;
}

/** The user's values in a draft, as the core takes them. Blank is `null`. */
export function editsFrom(draft: Draft): BuildingEdits {
  const edits: BuildingEdits = {};
  for (const key of DETAIL_FIELDS) {
    const f = draft.fields[key];
    if (!f.edited) continue;
    const text = f.text.trim();
    if (key === "occupants") edits.occupants = { value: text === "" ? null : Number(text) };
    else edits[key] = { value: text === "" ? null : text };
  }
  const notes = draft.notes.trim();
  if (notes) edits.notes = notes;
  return edits;
}

/** The user's values as stored now (to compare with a draft, and to undo). */
export function editsOf(d: BuildingDetailsView): BuildingEdits {
  return editsFrom(draftFrom(d));
}

function same(a: BuildingEdits, b: BuildingEdits): boolean {
  const norm = (e: BuildingEdits) =>
    JSON.stringify(
      Object.keys(e)
        .sort()
        .map((k) => [k, e[k as keyof BuildingEdits]]),
    );
  return norm(a) === norm(b);
}

export function isDirty(draft: Draft, d: BuildingDetailsView): boolean {
  return !same(editsFrom(draft), editsOf(d));
}

/** "name or address, occupants": fields in a sentence. */
function fieldList(keys: DetailKey[]): string {
  return keys.map((k) => LABEL[k].toLowerCase()).join(", ");
}

/** The fields the user has changed (notes not counted). */
function editedFields(d: BuildingDetailsView): DetailKey[] {
  return DETAIL_FIELDS.filter((key) => field(d, key).origin === "user");
}

// ----- labels ----------------------------------------------------------------

const STATE_LABEL: Record<DetailsState, string> = {
  ungenerated: "Not generated",
  generated: "Generated",
  customised: "Customised",
};

const ORIGIN: Record<FieldOrigin, { label: string; title: string }> = {
  table: { label: "Rolled", title: "Rolled on a content table" },
  inferred: { label: "Inferred", title: "Worked out from the map" },
  not_rolled: { label: "Not rolled", title: "Its table does not apply to this building" },
  user: { label: "Edited", title: "Your value; regeneration keeps it" },
};

const SIZE_LABEL = { small: "Small", medium: "Medium", large: "Large" } as const;

const NOT_ROLLED = {
  inactive: "the table is switched off",
  not_eligible: "it does not apply to this building",
  missing: "the table is not in the content",
  incomplete: "the table is incomplete",
} as const;

/** How a generated value came about, in words. */
function sourceText(f: BuildingField<string | number>): string {
  const s = f.generated.source;
  switch (s.kind) {
    case "table": {
      const total = s.roll.attempts?.at(-1)?.roll.total;
      const dice = s.roll.dice ? ` ${s.roll.dice}` : "";
      return `Rolled${dice}${total !== undefined ? ` → ${total}` : ""} on ${s.roll.table.name}: ${s.roll.result.name}`;
    }
    case "not_rolled":
      return `Not rolled: ${NOT_ROLLED[s.reason]}`;
    case "inferred":
      return s.rule;
  }
}

/** Where the generated value came from (what a field shows when not edited). */
function generatedOrigin(f: BuildingField<string | number>): FieldOrigin {
  return f.generated.source.kind;
}

function Origin({ origin }: { origin: FieldOrigin }) {
  const o = ORIGIN[origin];
  return (
    <span className={`map-origin map-origin-${origin}`} title={o.title}>
      {o.label}
    </span>
  );
}

function Value({ f }: { f: BuildingField<string | number> }) {
  return f.value === undefined ? (
    <span className="muted">{f.origin === "user" ? "Left blank" : "—"}</span>
  ) : (
    <>{f.value}</>
  );
}

// ----- the inspector -----------------------------------------------------------

type Confirm =
  | { kind: "regenerate"; discard: boolean }
  | { kind: "reset" }
  | { kind: "discard" }
  | { kind: "unassign" };

/**
 * The Location Inspector: the selected building's identity, its details
 * (generated the first time it is opened) and the actions on them. Edits
 * are made in a draft and saved in one write; the map is never redrawn.
 */
export function LocationInspector({
  map,
  building,
  details,
  loading,
  error,
  draft,
  onDraft,
  onSave,
  onRegenerate,
  onReset,
  onRetry,
  onLocate,
  onClear,
  location,
  onMakeNotable,
  onEditLocation,
  onUnassign,
  onReassign,
}: {
  map: SettlementMap;
  building: MapBuilding | null;
  details: BuildingDetailsView | null;
  /** "Generating details…" or "Loading details…" while it is fetched. */
  loading: string | null;
  error: string | null;
  draft: Draft | undefined;
  onDraft: (draft: Draft | undefined) => void;
  /** Saves edits made from `version`; resolves whether they were saved. */
  onSave: (edits: BuildingEdits, version: number) => Promise<boolean>;
  onRegenerate: (discardEdits: boolean) => Promise<boolean>;
  onReset: () => Promise<boolean>;
  onRetry: () => void;
  onLocate: () => void;
  onClear: () => void;
  /** The Notable Location standing on this building, if any. */
  location?: LocationEntry;
  /** Opens the dialog that places a Notable Location here. */
  onMakeNotable: () => void;
  onEditLocation: (edit: LocationEdit) => Promise<boolean>;
  /** Takes the location off; it and the building's details are kept. */
  onUnassign: () => Promise<boolean>;
  /** Starts choosing another building for the location. */
  onReassign: () => void;
}) {
  const [editingLocation, setEditingLocation] = useState(false);
  const [confirm, setConfirm] = useState<Confirm | null>(null);
  const [pending, setPending] = useState(false);
  const [undo, setUndo] = useState<BuildingEdits | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const editRef = useRef<HTMLButtonElement>(null);

  // A new building starts afresh: nothing to confirm or undo.
  useEffect(() => {
    setConfirm(null);
    setUndo(null);
    setSaved(null);
    setEditingLocation(false);
  }, [building?.id]);

  if (!building) {
    return (
      <p className="muted small">
        Select a building on the map, or choose one from the building directory.
      </p>
    );
  }

  const label = buildingLabel(building);
  const street = streetLabel(map, building.street);
  const geometryFacts = (
    <>
      <dt>Size</dt>
      <dd>
        {SIZE_LABEL[building.size]} · {Math.round(building.area_m2)} m²
      </dd>
      {street && (
        <>
          <dt>Faces</dt>
          <dd>{street}</dd>
        </>
      )}
    </>
  );

  const header = (state: DetailsState, title: string) => (
    <div className="map-inspector-head">
      <h4 className="map-inspector-title">{title}</h4>
      <span className={`map-state map-state-${state}`}>{STATE_LABEL[state]}</span>
    </div>
  );

  if (!details) {
    return (
      <div className="map-inspector" aria-busy={loading !== null}>
        {header("ungenerated", label)}
        <dl className="map-facts">
          <dt>Building</dt>
          <dd>{label}</dd>
          <dt>{map.scope.kind === "district" ? "District" : "Map"}</dt>
          <dd>{map.label}</dd>
          {geometryFacts}
        </dl>
        {error ? (
          <div className="alert alert-inline" role="alert">
            <p className="map-inspector-error">{error}</p>
            <button type="button" className="btn btn-small" onClick={onRetry}>
              Try again
            </button>
          </div>
        ) : (
          <p className="muted small" role="status">
            {loading ?? "Details not generated yet."}
          </p>
        )}
        <div className="action-bar map-inspector-actions">
          <button type="button" className="btn btn-small" onClick={onLocate}>
            Locate on map
          </button>
          <button type="button" className="btn btn-small" onClick={onClear}>
            Clear selection
          </button>
        </div>
      </div>
    );
  }

  const d = details;
  const changed = editedFields(d);
  const title = d.name.value ?? label;

  const run = async (action: () => Promise<boolean>, message: string) => {
    setPending(true);
    try {
      const ok = await action();
      if (ok) setSaved(message);
      return ok;
    } finally {
      setPending(false);
    }
  };

  const save = async (next: Draft) => {
    const before = editsOf(d);
    const ok = await run(() => onSave(editsFrom(next), next.version), "Changes saved.");
    if (ok) {
      onDraft(undefined);
      setUndo(before);
      // Back to the details, with focus where editing started.
      requestAnimationFrame(() => editRef.current?.focus());
    }
  };

  const undoSave = async () => {
    if (!undo) return;
    const ok = await run(() => onSave(undo, d.version), "Change undone.");
    if (ok) setUndo(null);
  };

  if (draft) {
    return (
      <DetailsForm
        details={d}
        label={label}
        draft={draft}
        pending={pending}
        confirmDiscard={confirm?.kind === "discard"}
        onChange={onDraft}
        onSave={(next) => void save(next)}
        onCancel={(dirty) => (dirty ? setConfirm({ kind: "discard" }) : onDraft(undefined))}
        onDiscard={() => {
          setConfirm(null);
          onDraft(undefined);
        }}
        onKeepEditing={() => setConfirm(null)}
      />
    );
  }

  const fact = (key: DetailKey) => {
    const f = field(d, key);
    return (
      <>
        <dt>{LABEL[key]}</dt>
        <dd>
          <Value f={f} /> <Origin origin={f.origin} />
        </dd>
      </>
    );
  };

  return (
    <div className="map-inspector">
      {header(d.state, String(title))}
      <dl className="map-facts">
        <dt>Building</dt>
        <dd>{label}</dd>
        <dt>{d.district ? "District" : "Map"}</dt>
        <dd>{d.district?.label ?? d.map_label}</dd>
        {fact("name")}
        {fact("building_type")}
        {fact("condition")}
        {fact("occupancy")}
        {fact("occupants")}
        {geometryFacts}
        <dt>Notable location</dt>
        <dd>
          {location ? (
            <span className="map-inspector-location">
              <MarkerBadge number={location.marker} /> {location.name}
            </span>
          ) : (
            <span className="muted">None</span>
          )}
        </dd>
      </dl>

      {location && (
        <section className="map-inspector-linked" aria-label="Notable location">
          <h5 className="map-subheading">
            Notable location {location.marker !== undefined && `· marker ${location.marker}`}
          </h5>
          {editingLocation ? (
            <LocationForm
              location={location}
              busy={pending}
              onCancel={() => setEditingLocation(false)}
              onSave={async (edit) => {
                if (await run(() => onEditLocation(edit), "Location saved.")) {
                  setEditingLocation(false);
                }
              }}
            />
          ) : (
            <>
              <p className="map-inspector-text">
                <strong>{location.name}</strong>
                <br />
                <span className="muted">
                  {location.location_type}
                  {location.origin === "custom" && " · Custom"}
                </span>
              </p>
              {location.notes && (
                <p className="map-inspector-text map-inspector-notes">{location.notes}</p>
              )}
              {confirm?.kind === "unassign" ? (
                <div className="map-confirm" role="alertdialog" aria-label="Unassign location">
                  <p>
                    Take {location.name} off {label}? The location keeps its name, notes and marker
                    number {location.marker}, and this building keeps its details.
                  </p>
                  <ConfirmButtons
                    pending={pending}
                    danger
                    label="Unassign"
                    onConfirm={async () => {
                      if (await run(onUnassign, `${location.name} unassigned.`)) setConfirm(null);
                    }}
                    onCancel={() => setConfirm(null)}
                  />
                </div>
              ) : (
                !confirm && (
                  <div className="action-bar map-inspector-actions">
                    <button
                      type="button"
                      className="btn btn-small"
                      disabled={pending}
                      onClick={() => setEditingLocation(true)}
                    >
                      Edit location
                    </button>
                    <button
                      type="button"
                      className="btn btn-small"
                      disabled={pending}
                      onClick={onReassign}
                    >
                      Reassign…
                    </button>
                    <button
                      type="button"
                      className="btn btn-small"
                      disabled={pending}
                      onClick={() => setConfirm({ kind: "unassign" })}
                    >
                      Unassign…
                    </button>
                  </div>
                )
              )}
            </>
          )}
        </section>
      )}

      <h5 className="map-subheading">
        Description <Origin origin={d.description.origin} />
      </h5>
      <p className="map-inspector-text">
        <Value f={d.description} />
      </p>
      {d.notes && (
        <>
          <h5 className="map-subheading">Notes</h5>
          <p className="map-inspector-text map-inspector-notes">{d.notes}</p>
        </>
      )}

      {saved && (
        <p className="map-inspector-saved small" role="status">
          {saved}
          {undo && (
            <button
              type="button"
              className="btn btn-small"
              disabled={pending}
              onClick={() => void undoSave()}
            >
              Undo
            </button>
          )}
        </p>
      )}

      {confirm?.kind === "regenerate" && (
        <div className="map-confirm" role="alertdialog" aria-label="Regenerate details">
          <p>
            Roll the generated details of {label} again? Its type, condition, occupancy, occupants
            and description are rerolled and its address worked out again.
          </p>
          {changed.length > 0 && (
            <label className="check">
              <input
                type="checkbox"
                checked={confirm.discard}
                onChange={(e) => setConfirm({ kind: "regenerate", discard: e.target.checked })}
              />
              Also replace my changes ({fieldList(changed)})
            </label>
          )}
          <p className="muted small">
            {changed.length === 0
              ? "Your notes are kept."
              : confirm.discard
                ? "Your changes to these fields will be lost. Your notes are kept."
                : "Fields you changed, and your notes, are kept."}
          </p>
          <ConfirmButtons
            pending={pending}
            danger={confirm.discard}
            label={confirm.discard ? "Regenerate and replace my changes" : "Regenerate"}
            onConfirm={async () => {
              if (await run(() => onRegenerate(confirm.discard), "Details regenerated.")) {
                setConfirm(null);
                setUndo(null);
              }
            }}
            onCancel={() => setConfirm(null)}
          />
        </div>
      )}
      {confirm?.kind === "reset" && (
        <div className="map-confirm" role="alertdialog" aria-label="Reset generated details">
          <p>
            Give back the generated {changed.length === 1 ? "value" : "values"} of the{" "}
            {changed.length === 1 ? "field" : `${changed.length} fields`} you changed (
            {fieldList(changed)})? Your values will be lost. Your notes are kept.
          </p>
          <ConfirmButtons
            pending={pending}
            danger
            label="Reset to generated"
            onConfirm={async () => {
              const before = editsOf(d);
              if (await run(onReset, "Generated values restored.")) {
                setConfirm(null);
                setUndo(before);
              }
            }}
            onCancel={() => setConfirm(null)}
          />
        </div>
      )}

      {(!confirm || confirm.kind === "unassign") && (
        <div className="action-bar map-inspector-actions">
          <button
            ref={editRef}
            type="button"
            className="btn btn-small btn-primary"
            disabled={pending}
            onClick={() => {
              setSaved(null);
              onDraft(draftFrom(d));
            }}
          >
            Edit details
          </button>
          <button type="button" className="btn btn-small" onClick={onLocate}>
            Locate on map
          </button>
          <button
            type="button"
            className="btn btn-small"
            disabled={pending}
            onClick={() => setConfirm({ kind: "regenerate", discard: false })}
          >
            Regenerate…
          </button>
          <button
            type="button"
            className="btn btn-small"
            disabled={pending || changed.length === 0}
            title={
              changed.length === 0
                ? "Nothing to reset: no generated value has been changed"
                : "Give back the generated values"
            }
            onClick={() => setConfirm({ kind: "reset" })}
          >
            Reset…
          </button>
          <button
            type="button"
            className="btn btn-small"
            disabled={pending}
            title={
              location
                ? `Put another Notable Location here instead of ${location.name}`
                : "Link one of the settlement's Notable Locations to this building"
            }
            onClick={onMakeNotable}
          >
            Make Notable Location
          </button>
          <button type="button" className="btn btn-small" onClick={onClear}>
            Clear selection
          </button>
        </div>
      )}

      <details className="map-inspector-meta">
        <summary>Sources and record</summary>
        <dl className="map-facts map-facts-small">
          {DETAIL_FIELDS.map((key) => (
            <Fragment key={key}>
              <dt>{LABEL[key]}</dt>
              <dd>
                {sourceText(field(d, key))}
                {field(d, key).origin === "user" && (
                  <span className="muted">
                    {" "}
                    (generated: {asText(field(d, key).generated.value) || "blank"})
                  </span>
                )}
              </dd>
            </Fragment>
          ))}
          <dt>Building ID</dt>
          <dd>
            <code>{d.building_id}</code>
          </dd>
          <dt>Generated on</dt>
          <dd>
            {formatWhen(d.generated_at)}
            {d.generation > 1 && ` (generation ${d.generation})`} · v{d.app_version}
          </dd>
          {d.edited_at && (
            <>
              <dt>Last edited</dt>
              <dd>{formatWhen(d.edited_at)}</dd>
            </>
          )}
          {d.tags && d.tags.length > 0 && (
            <>
              <dt>Tags</dt>
              <dd>{d.tags.map((t) => t.name).join(", ")}</dd>
            </>
          )}
        </dl>
      </details>
    </div>
  );
}

function ConfirmButtons({
  pending,
  danger,
  label,
  onConfirm,
  onCancel,
}: {
  pending: boolean;
  danger: boolean;
  label: string;
  onConfirm: () => Promise<void>;
  onCancel: () => void;
}) {
  const cancel = useRef<HTMLButtonElement>(null);
  useEffect(() => cancel.current?.focus(), []);
  return (
    <div className="button-row">
      <button ref={cancel} type="button" className="btn btn-small" onClick={onCancel}>
        Cancel
      </button>
      <button
        type="button"
        className={`btn btn-small ${danger ? "btn-danger" : "btn-primary"}`}
        disabled={pending}
        onClick={() => void onConfirm()}
      >
        {label}
      </button>
    </div>
  );
}

// ----- the edit form ---------------------------------------------------------

function DetailsForm({
  details,
  label,
  draft,
  pending,
  confirmDiscard,
  onChange,
  onSave,
  onCancel,
  onDiscard,
  onKeepEditing,
}: {
  details: BuildingDetailsView;
  label: string;
  draft: Draft;
  pending: boolean;
  confirmDiscard: boolean;
  onChange: (draft: Draft) => void;
  onSave: (draft: Draft) => void;
  onCancel: (dirty: boolean) => void;
  onDiscard: () => void;
  onKeepEditing: () => void;
}) {
  const id = useId();
  const first = useRef<HTMLInputElement>(null);
  useEffect(() => first.current?.focus(), []);
  const errors = problems(draft);
  const dirty = isDirty(draft, details);
  const stale = draft.version !== details.version;
  const valid = Object.keys(errors).length === 0;

  const set = (key: DetailKey, text: string) =>
    onChange({ ...draft, fields: { ...draft.fields, [key]: { text, edited: true } } });
  const restoreGenerated = (key: DetailKey) =>
    onChange({
      ...draft,
      fields: {
        ...draft.fields,
        [key]: { text: asText(field(details, key).generated.value), edited: false },
      },
    });

  // Esc cancels (asking first if there are unsaved changes), and never
  // reaches the map, which would clear the selection.
  const onKeyDown = (event: KeyboardEvent<HTMLFormElement>) => {
    if (event.key !== "Escape") return;
    event.preventDefault();
    event.stopPropagation();
    if (confirmDiscard) onKeepEditing();
    else onCancel(dirty);
  };

  const input = (key: Exclude<DetailKey, "description">, list?: string[]) => {
    const f = draft.fields[key];
    const fieldId = `${id}-${key}`;
    const listId = list?.length ? `${fieldId}-choices` : undefined;
    const generated = asText(field(details, key).generated.value);
    return (
      <div className="field map-edit-field" key={key}>
        <div className="map-edit-label">
          <label className="field-label" htmlFor={fieldId}>
            {LABEL[key]}
          </label>
          <Origin origin={f.edited ? "user" : generatedOrigin(field(details, key))} />
          {f.edited && (
            <button
              type="button"
              className="btn-link small"
              onClick={() => restoreGenerated(key)}
              title={generated ? `Generated: ${generated}` : "Nothing was generated"}
            >
              Use generated
            </button>
          )}
        </div>
        <input
          ref={key === "name" ? first : undefined}
          id={fieldId}
          className="text-input map-edit-input"
          type="text"
          inputMode={key === "occupants" ? "numeric" : undefined}
          maxLength={key === "occupants" ? 6 : LIMIT[key]}
          list={listId}
          value={f.text}
          placeholder={f.edited ? "Left blank" : undefined}
          aria-invalid={errors[key] ? true : undefined}
          aria-describedby={errors[key] ? `${fieldId}-error` : undefined}
          onChange={(e) => set(key, e.target.value)}
        />
        {listId && (
          <datalist id={listId}>
            {list!.map((c) => (
              <option key={c} value={c} />
            ))}
          </datalist>
        )}
        {errors[key] && (
          <p className="field-error" id={`${fieldId}-error`}>
            {errors[key]}
          </p>
        )}
      </div>
    );
  };

  const descriptionId = `${id}-description`;
  const notesId = `${id}-notes`;
  return (
    <form
      className="map-inspector map-edit"
      aria-label={`Edit details of ${label}`}
      onKeyDown={onKeyDown}
      onSubmit={(e) => {
        e.preventDefault();
        if (valid && dirty && !pending) onSave(draft);
      }}
    >
      <div className="map-inspector-head">
        <h4 className="map-inspector-title">Editing {label}</h4>
        {dirty && <span className="map-state map-state-customised">Unsaved</span>}
      </div>
      <p className="muted small">
        A field you change is kept when details are regenerated. Leave a field empty to keep it
        blank.
      </p>
      {input("name")}
      {input("building_type", details.choices.building_type)}
      {input("condition", details.choices.condition)}
      {input("occupancy", details.choices.occupancy)}
      {input("occupants")}
      <div className="field map-edit-field">
        <div className="map-edit-label">
          <label className="field-label" htmlFor={descriptionId}>
            Description
          </label>
          {draft.fields.description.edited && (
            <button
              type="button"
              className="btn-link small"
              onClick={() => restoreGenerated("description")}
            >
              Use generated
            </button>
          )}
        </div>
        <textarea
          id={descriptionId}
          className="text-input text-area map-edit-input"
          rows={3}
          maxLength={MAX_DESCRIPTION_CHARS}
          value={draft.fields.description.text}
          onChange={(e) => set("description", e.target.value)}
        />
      </div>
      <div className="field map-edit-field">
        <label className="field-label" htmlFor={notesId}>
          Notes
        </label>
        <textarea
          id={notesId}
          className="text-input text-area map-edit-input"
          rows={4}
          maxLength={MAX_NOTES_CHARS}
          value={draft.notes}
          onChange={(e) => onChange({ ...draft, notes: e.target.value })}
        />
        <span className="field-help">Your own notes. Never generated or replaced.</span>
      </div>
      {stale && (
        <p className="field-error" role="alert">
          These details changed after you started editing. Saving will be refused; cancel and edit
          again.
        </p>
      )}
      {confirmDiscard ? (
        <div className="map-confirm" role="alertdialog" aria-label="Discard unsaved changes">
          <p>Discard your unsaved changes to {label}?</p>
          <div className="button-row">
            <button type="button" className="btn btn-small" onClick={onKeepEditing}>
              Keep editing
            </button>
            <button type="button" className="btn btn-small btn-danger" onClick={onDiscard}>
              Discard changes
            </button>
          </div>
        </div>
      ) : (
        <div className="action-bar map-inspector-actions">
          <button
            type="submit"
            className="btn btn-small btn-primary"
            disabled={!valid || !dirty || pending}
          >
            {pending ? "Saving…" : "Save changes"}
          </button>
          <button type="button" className="btn btn-small" onClick={() => onCancel(dirty)}>
            Cancel
          </button>
        </div>
      )}
    </form>
  );
}
