import {
  generation,
  type GenerationResponse,
  type LocationDetail,
  type LocationDetailOptions,
  type LocationsView,
  type Named,
  type NotableLocation,
  type NotableLocations,
  type ResultOption,
  type RollRecord,
  type StageView,
  type TableOptions,
} from "../api";

/** "District 2", from a location's district ID. */
function districtLabel(record: NotableLocations, district?: string) {
  return record.areas.find((a) => a.district?.id === district)?.district?.label;
}

/** "Specialist Artisan (Quality +1 step)": a shift waiting for Quality. */
export function qualityShiftLabel(location: NotableLocation) {
  const shift = location.quality_shift;
  if (!shift) return undefined;
  return `${shift.source.name} (Quality +${shift.steps} step${shift.steps === 1 ? "" : "s"})`;
}

/**
 * One line for a Notable Location, e.g. "Tavern / Inn · Hospitality · 4 spare
 * rooms" or "Magic Store (Weapons) · Trade / Shop · District 2".
 */
export function locationLine(record: NotableLocations, location: NotableLocation) {
  const subtype = location.subtype
    ? location.speciality
      ? `${location.subtype.name} (${location.speciality.name})`
      : location.subtype.name
    : undefined;
  const parts = [subtype, location.type.name];
  if (location.spare_rooms !== undefined) parts.push(`${location.spare_rooms} spare rooms`);
  parts.push(qualityShiftLabel(location));
  const district = districtLabel(record, location.district);
  if (district) parts.push(district);
  return parts.filter(Boolean).join(" · ");
}

/** The faces of a roll, or "chosen". */
function rolled(roll: RollRecord) {
  return roll.selected ? "chosen" : (roll.attempts ?? []).map((a) => a.roll.total).join(", ");
}

/** The Notable Locations as a numbered list (the guaranteed Tavern / Inn first). */
export function LocationsSummary({ record }: { record: NotableLocations }) {
  return (
    <div className="locations-summary">
      <span className="sheet-value">
        {record.locations.length} Notable Location{record.locations.length === 1 ? "" : "s"}
      </span>
      {record.districts && (
        <span className="sheet-detail">{record.districts.roll.result.name}</span>
      )}
      <ol className="location-list">
        {record.locations.map((l) => (
          <li key={l.id}>{locationLine(record, l)}</li>
        ))}
      </ol>
    </div>
  );
}

/** A select of a table's legal results, keeping the current one even if illegal. */
function ResultSelect({
  label,
  value,
  options,
  busy,
  onChange,
}: {
  label: string;
  value?: Named;
  options: ResultOption[];
  busy: boolean;
  onChange: (result: string) => void;
}) {
  return (
    <select
      className="select"
      aria-label={label}
      disabled={busy}
      value={value?.id ?? ""}
      onChange={(e) => onChange(e.target.value)}
    >
      {!value && (
        <option value="" disabled>
          Not rolled
        </option>
      )}
      {value && !options.some((o) => o.result.id === value.id) && (
        <option value={value.id} disabled>
          {value.name}
        </option>
      )}
      {options
        .filter((o) => o.legal || o.result.id === value?.id)
        .map((o) => (
          <option key={o.result.id} value={o.result.id}>
            {o.result.name}
          </option>
        ))}
    </select>
  );
}

/** One detail of a rolled location as it stands, with its table's label. */
interface DetailField {
  detail: Exclude<LocationDetail, "type">;
  label: string;
  table: TableOptions;
  value?: Named;
  note?: string;
}

/**
 * The details that apply to a rolled location: its subtype, then only what
 * that subtype needs (a Specialist Artisan's reroll, a Weaponsmith /
 * Armoursmith split, a Magic Store's speciality).
 */
function detailFields(location: NotableLocation, options?: LocationDetailOptions): DetailField[] {
  const rolls = location.subtype_rolls;
  const fields: DetailField[] = [];
  if (options?.subtype) {
    fields.push({
      detail: "subtype",
      label: options.subtype.table.name,
      table: options.subtype,
      value: rolls?.roll.result,
    });
  }
  if (options?.reroll && rolls) {
    const chain = [rolls.roll, ...(rolls.rerolls ?? [])].map(rolled).join(" → ");
    fields.push({
      detail: "reroll",
      label: `${rolls.roll.result.name} rolls again`,
      table: options.reroll,
      value: rolls.rerolls?.[rolls.rerolls.length - 1]?.result,
      note: `${options.reroll.table.name}: ${chain}.`,
    });
  }
  if (options?.split && rolls?.split) {
    fields.push({
      detail: "split",
      label: options.split.table.name,
      table: options.split,
      value: rolls.split.result,
      note: rolls.split.selected ? "Chosen." : `${rolls.split.dice}: ${rolled(rolls.split)}.`,
    });
  }
  if (options?.speciality) {
    fields.push({
      detail: "speciality",
      label: options.speciality.table.name,
      table: options.speciality,
      value: location.speciality,
    });
  }
  return fields;
}

/**
 * Manual controls for the Notable Locations stage: roll them all, then
 * choose (or re-roll) the District Count, each area's count, and each rolled
 * location's Type and the details that apply to it. Changing one re-rolls
 * what follows from it. The guaranteed Tavern / Inn is fixed.
 */
export function LocationsControls({
  stage,
  busy,
  onRoll,
  onAct,
}: {
  stage: StageView;
  busy: boolean;
  onRoll: () => void;
  onAct: (call: Promise<GenerationResponse>) => void;
}) {
  const view = stage.locations as LocationsView;
  const record = view.record;
  const { districts, count, types, details } = view.options;
  return (
    <div className="figures-controls">
      {stage.previous && stage.status === "pending" && (
        <p className="note">
          An earlier change affects this stage; it was <s>{stage.previous.name}</s>.
        </p>
      )}
      {stage.skipped === "not_recorded" && (
        <p className="note">Saved before Notable Locations existed.</p>
      )}
      {stage.can_roll && (
        <button type="button" className="btn btn-small" disabled={busy} onClick={onRoll}>
          {record ? "Re-roll all Notable Locations" : "Roll Notable Locations"}
        </button>
      )}
      {!stage.can_roll && !record && <p className="muted small">Resolve the Size first.</p>}
      {view.bare_minimum_buildings && (
        <p className="muted small">Bare Minimum Buildings: one extra Tavern / Inn is added.</p>
      )}
      {record && (
        <div className="figure-fields">
          {record.districts && districts && (
            <label className="figure-field">
              <span>District Count</span>
              <ResultSelect
                label="District Count"
                value={record.districts.roll.result}
                options={districts.options}
                busy={busy}
                onChange={(r) => onAct(generation.chooseDistrictCount(stage.index, r))}
              />
              <button
                type="button"
                className="btn btn-small"
                disabled={busy}
                onClick={() => onAct(generation.rollDistrictCount(stage.index))}
              >
                Re-roll
              </button>
            </label>
          )}
          {count &&
            record.areas.map((area, i) => {
              const label = `${area.district?.label ?? "Settlement"}: Notable Location Count`;
              return (
                <label key={area.district?.id ?? "settlement"} className="figure-field">
                  <span>{label}</span>
                  <ResultSelect
                    label={label}
                    value={area.count.roll.result}
                    options={count.options}
                    busy={busy}
                    onChange={(r) => onAct(generation.chooseLocationCount(stage.index, i, r))}
                  />
                  <button
                    type="button"
                    className="btn btn-small"
                    disabled={busy}
                    onClick={() => onAct(generation.rollLocationCount(stage.index, i))}
                  >
                    Re-roll
                  </button>
                </label>
              );
            })}
          {record.locations.map((location, i) => {
            const label = `Location ${i + 1}`;
            if (location.source === "bareMinimumBuildings") {
              return (
                <div key={location.id} className="figure-field">
                  <span>{label}</span>
                  <span>{locationLine(record, location)}</span>
                  <span className="figure-note">Added by Bare Minimum Buildings.</span>
                </div>
              );
            }
            const district = districtLabel(record, location.district);
            const shift = qualityShiftLabel(location);
            return [
              <label key={location.id} className="figure-field">
                <span>{district ? `${label} · ${district}` : label}</span>
                {types && (
                  <ResultSelect
                    label={`${label}: type`}
                    value={location.type}
                    options={types.options}
                    busy={busy}
                    onChange={(r) => onAct(generation.chooseLocationType(stage.index, i, r))}
                  />
                )}
                <button
                  type="button"
                  className="btn btn-small"
                  disabled={busy}
                  onClick={() => onAct(generation.rollLocationType(stage.index, i))}
                >
                  Re-roll
                </button>
              </label>,
              ...detailFields(location, details?.[i]).map((field) => (
                <label key={`${location.id}-${field.detail}`} className="figure-field">
                  <span>{`${label}: ${field.label}`}</span>
                  <ResultSelect
                    label={`${label}: ${field.detail}`}
                    value={field.value}
                    options={field.table.options}
                    busy={busy}
                    onChange={(r) =>
                      onAct(generation.chooseLocationDetail(stage.index, i, field.detail, r))
                    }
                  />
                  <button
                    type="button"
                    className="btn btn-small"
                    disabled={busy}
                    onClick={() =>
                      onAct(generation.rollLocationDetail(stage.index, i, field.detail))
                    }
                  >
                    Re-roll
                  </button>
                  {field.note && <span className="figure-note">{field.note}</span>}
                  {field.detail === "reroll" && shift && (
                    <span className="figure-note">
                      Quality +{location.quality_shift?.steps} step, once Quality is generated.
                    </span>
                  )}
                </label>
              )),
            ];
          })}
        </div>
      )}
    </div>
  );
}
