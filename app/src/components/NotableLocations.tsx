import {
  generation,
  type GenerationResponse,
  type LocationsView,
  type NotableLocation,
  type NotableLocations,
  type ResultOption,
  type StageView,
} from "../api";

/** "District 2", from a location's district ID. */
function districtLabel(record: NotableLocations, district?: string) {
  return record.areas.find((a) => a.district?.id === district)?.district?.label;
}

/** One line for a Notable Location: "Tavern · Hospitality · 4 spare rooms". */
export function locationLine(record: NotableLocations, location: NotableLocation) {
  const parts = [location.subtype?.name, location.type.name];
  if (location.spare_rooms !== undefined) parts.push(`${location.spare_rooms} spare rooms`);
  const district = districtLabel(record, location.district);
  if (district) parts.push(district);
  return parts.filter(Boolean).join(" · ");
}

/** The Notable Locations as a numbered list (the guaranteed Tavern first). */
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
  value: { id: string; name: string };
  options: ResultOption[];
  busy: boolean;
  onChange: (result: string) => void;
}) {
  return (
    <select
      className="select"
      aria-label={label}
      disabled={busy}
      value={value.id}
      onChange={(e) => onChange(e.target.value)}
    >
      {!options.some((o) => o.result.id === value.id) && (
        <option value={value.id} disabled>
          {value.name}
        </option>
      )}
      {options
        .filter((o) => o.legal || o.result.id === value.id)
        .map((o) => (
          <option key={o.result.id} value={o.result.id}>
            {o.result.name}
          </option>
        ))}
    </select>
  );
}

/**
 * Manual controls for the Notable Locations stage: roll them all, then
 * choose (or re-roll) the District Count, each area's count and each rolled
 * location's Type. The guaranteed Tavern is fixed.
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
  const { districts, count, types } = view.options;
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
        <p className="muted small">Bare Minimum Buildings: one extra Tavern is added.</p>
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
            return (
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
              </label>
            );
          })}
        </div>
      )}
    </div>
  );
}
