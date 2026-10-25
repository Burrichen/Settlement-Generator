// Map fixtures for tests: square buildings on a grid, so positions and hits
// are easy to compute.
import type { District } from "../api";
import type {
  AssignOptions,
  BuildingDetailsView,
  LocationEdit,
  LocationEntry,
  PlaceOn,
  PoiEntry,
  Point,
  BuildingEdits,
  BuildingField,
  GeneratedValue,
  MapBuilding,
  MapScope,
  PlannedMapView,
  SettlementMap,
  SettlementMapSet,
  SettlementMapsView,
} from "./types";

export const SETTLEMENT = "stl-test";

export function square(mapId: string, n: number, cx: number, cy: number, s = 8): MapBuilding {
  const h = s / 2;
  return {
    id: `bld-${mapId.replace(/^map-/, "")}-r1-${String(n).padStart(4, "0")}`,
    map_id: mapId,
    settlement_id: SETTLEMENT,
    footprint: [
      [cx - h, cy - h],
      [cx + h, cy - h],
      [cx + h, cy + h],
      [cx - h, cy + h],
    ],
    centroid: [cx, cy],
    area_m2: s * s,
    size: n % 3 === 0 ? "large" : n % 2 === 0 ? "medium" : "small",
    street: "street-1",
  };
}

/** `count` buildings on a 12 m grid centred on the origin. */
export function grid(mapId: string, count: number): MapBuilding[] {
  const side = Math.ceil(Math.sqrt(count));
  const offset = ((side - 1) * 12) / 2;
  return Array.from({ length: count }, (_, i) =>
    square(mapId, i + 1, (i % side) * 12 - offset, Math.floor(i / side) * 12 - offset),
  );
}

export function makeMap(
  id: string,
  label: string,
  buildings: MapBuilding[],
  scope: MapScope = { kind: "settlement" },
): SettlementMap {
  const extent =
    Math.max(
      ...buildings.flatMap((b) => b.footprint.map(([x, y]) => Math.max(Math.abs(x), Math.abs(y)))),
    ) + 40;
  return {
    id,
    settlement_id: SETTLEMENT,
    scope,
    label,
    revision: 1,
    seed: 7,
    parameters: {
      building_count: buildings.length,
      radius_m: extent,
      approaches: 2,
      walls: true,
      water: "river",
      ground: "grass",
      irregularity: 0.4,
      trees: 0.3,
      fields: false,
      square: false,
    },
    engine: { id: "builtin.medieval", version: "1.0.0", features: ["buildings", "streets"] },
    geometry_version: 1,
    stats: { buildings_requested: buildings.length, buildings_placed: buildings.length },
    inputs_fingerprint: "f",
    geometry: {
      bounds: { min_x: -extent, min_y: -extent, max_x: extent, max_y: extent },
      buildings,
      streets: [
        {
          id: "street-1",
          kind: "road",
          width_m: 6,
          path: [
            [-extent, -20],
            [extent, -20],
          ],
        },
      ],
      walls: [
        {
          id: "wall-1",
          segments: [
            [
              [-30, -30],
              [30, -30],
            ],
          ],
          gates: [[0, -30]],
          towers: [],
        },
      ],
      water: [
        {
          id: "water-1",
          kind: "river",
          area: [
            [extent - 10, -extent],
            [extent, -extent],
            [extent, extent],
            [extent - 10, extent],
          ],
          bridges: [],
        },
      ],
      terrain: { ground: "grass", trees: [{ centre: [30, 30], radius_m: 5 }] },
    },
  };
}

export function district(n: number): District {
  return { id: `district-${n}`, label: `District ${n}` };
}

export function districtMap(n: number, count = 4): SettlementMap {
  const id = `map-district-${n}`;
  return makeMap(id, `District ${n}`, grid(id, count), { kind: "district", district: district(n) });
}

export function view(
  saved: SettlementMap[],
  planned: PlannedMapView[],
  extra: Partial<SettlementMapsView> = {},
): SettlementMapsView {
  const set: SettlementMapSet | undefined =
    saved.length > 0
      ? {
          settlement_id: SETTLEMENT,
          format_version: 1,
          layout: planned.some((p) => p.scope.kind === "district") ? "per_district" : "single",
          revision: 1,
          seed: 7,
          overrides: {},
          inputs: {
            settlement_name: "Testford",
            size: { id: "town", name: "Town" },
            tags: [],
          },
          inputs_fingerprint: "f",
          generated_at: "2026-10-13T10:00:00Z",
          app_version: "0.1.5",
          maps: saved,
          metadata: {},
        }
      : undefined;
  return {
    settlement_id: SETTLEMENT,
    status: saved.length ? "current" : "not_generated",
    layout: planned.some((p) => p.scope.kind === "district") ? "per_district" : "single",
    engine: { id: "builtin.medieval", version: "1.0.0", features: [] },
    planned,
    maps: set,
    legends: saved.map((m) => ({ map_id: m.id, markers: [], entries: [] })),
    ...extra,
  };
}

export function plannedSettlement(generated: boolean): PlannedMapView[] {
  return [
    {
      id: "map-settlement",
      label: "Testford",
      scope: { kind: "settlement" },
      generated,
      stale: false,
      annotations: { building_details: 0, locations: 0, pois: 0 },
    },
  ];
}

export function plannedDistricts(generated: boolean[]): PlannedMapView[] {
  return generated.map((g, i) => ({
    id: `map-district-${i + 1}`,
    label: `District ${i + 1}`,
    scope: { kind: "district", district: district(i + 1) },
    generated: g,
    stale: false,
    annotations: { building_details: 0, locations: 0, pois: 0 },
  }));
}

// ----- building details ---------------------------------------------------------

function rolled<T>(table: string, value: T): GeneratedValue<T> {
  return {
    value,
    source: {
      kind: "table",
      roll: {
        table: { id: table, name: table },
        dice: "1d10",
        attempts: [{ roll: { total: 4 } }],
        result: { id: String(value), name: String(value) },
      },
    },
  };
}

function shown<T>(generated: GeneratedValue<T>, edit?: { value: T | null }): BuildingField<T> {
  if (edit) return { value: edit.value ?? undefined, origin: "user", generated };
  return { value: generated.value, origin: generated.source.kind, generated };
}

/**
 * What the core returns for a building: fixed generated values, with
 * `edits` applied as the core applies them. Regenerated (generation 2 and
 * later) buildings are Workshops.
 */
export function fakeDetails(
  mapId: string,
  buildingId: string,
  edits: BuildingEdits = {},
  version = 1,
  generation = 1,
): BuildingDetailsView {
  const n = Number(/(\d+)$/.exec(buildingId)?.[1]);
  const name: GeneratedValue<string> = {
    value: `No. ${n}, Road 1`,
    source: { kind: "inferred", rule: "Address from the building's number and street." },
  };
  const district = /district-(\d+)/.exec(mapId);
  const customised = Object.keys(edits).length > 0;
  return {
    settlement_id: SETTLEMENT,
    map_id: mapId,
    building_id: buildingId,
    map_label: district ? `District ${district[1]}` : "Testford",
    district: district
      ? { id: `district-${district[1]}`, label: `District ${district[1]}` }
      : undefined,
    state: customised ? "customised" : "generated",
    version,
    generation,
    name: shown(name, edits.name),
    building_type: shown(
      rolled("Building Type", generation > 1 ? "Workshop" : "House"),
      edits.building_type,
    ),
    description: shown(rolled("Building Feature", "Ivy climbs one wall."), edits.description),
    condition: shown(rolled("Building Condition", "Sound"), edits.condition),
    occupancy: shown(rolled("Building Occupancy", "Occupied"), edits.occupancy),
    occupants: shown(rolled("Building Occupants", 3), edits.occupants),
    notes: edits.notes,
    generated_at: "2026-10-24T10:00:00Z",
    app_version: "0.1.5",
    edited_at: customised ? "2026-10-24T11:00:00Z" : undefined,
    choices: {
      building_type: ["House", "Shop", "Workshop"],
      condition: ["Worn", "Sound"],
      occupancy: ["Occupied", "Vacant"],
    },
  };
}

// ----- Notable Locations ----------------------------------------------------------

/** A settlement Notable Location, unplaced. */
export function location(n: number, extra: Partial<LocationEntry> = {}): LocationEntry {
  return {
    id: `notable-location-${String(n).padStart(2, "0")}`,
    origin: "settlement",
    name: extra.name ?? `Location ${n}`,
    default_name: `Location ${n}`,
    location_type: "Hospitality",
    version: 0,
    ...extra,
  };
}

/**
 * `v` with these locations, and legends derived from them as the core
 * derives them: each placed location's persistent number, at its building.
 */
export function withLocations(
  v: SettlementMapsView,
  locations: LocationEntry[],
): SettlementMapsView {
  const maps = v.maps?.maps ?? [];
  return {
    ...v,
    locations,
    legends: maps.map((m) => {
      const placed = locations
        .filter((l) => l.marker !== undefined && l.placement?.map_id === m.id)
        .map((l) => ({ l, b: m.geometry.buildings.find((b) => b.id === l.placement!.building_id) }))
        .filter((x) => x.b)
        .sort((a, b) => a.l.marker! - b.l.marker!);
      const district = m.scope.kind === "district" ? m.scope.district : undefined;
      const pois = (v.pois ?? []).filter((p) => p.map_id === m.id);
      const markers = [
        ...placed.map(({ l, b }) => ({
          number: l.marker!,
          map_id: m.id,
          position: b!.centroid,
          target: {
            kind: "notable_location" as const,
            location_id: l.id,
            building_id: b!.id,
          },
        })),
        ...pois.map((p) => ({
          number: p.number,
          map_id: m.id,
          position: p.position,
          target: { kind: "point_of_interest" as const, poi_id: p.id },
        })),
      ].sort((a, b) => a.number - b.number);
      const label = (n: number) => {
        const l = placed.find((x) => x.l.marker === n);
        return l
          ? { label: l.l.name, location_type: l.l.location_type }
          : { label: pois.find((p) => p.number === n)!.label };
      };
      return {
        map_id: m.id,
        markers,
        entries: markers.map((mk) => ({
          number: mk.number,
          ...label(mk.number),
          district,
          target: mk.target,
        })),
      };
    }),
  };
}

/**
 * The core's directory rules over a view, for mocking: refuses an
 * unintended move or replace and a wrong district, and issues marker
 * numbers once from a settlement-wide sequence.
 */
export class FakeDirectory {
  next = 1;
  custom = 1;
  constructor(public view: SettlementMapsView) {}

  private set(locations: LocationEntry[]) {
    this.view = withLocations(this.view, locations);
    return this.view;
  }

  private place(l: LocationEntry, target: PlaceOn): LocationEntry {
    const map = this.view.maps!.maps.find((m) => m.id === target.map_id)!;
    const district = map.scope.kind === "district" ? map.scope.district : undefined;
    return {
      ...l,
      version: l.version + 1,
      marker: l.marker ?? this.next++,
      placement: {
        map_id: target.map_id,
        building_id: target.building_id,
        map_label: map.label,
        district,
      },
    };
  }

  assign(id: string, target: PlaceOn, options: AssignOptions) {
    const all = this.view.locations ?? [];
    const l = all.find((x) => x.id === id)!;
    const map = this.view.maps!.maps.find((m) => m.id === target.map_id)!;
    if (l.district && (map.scope.kind !== "district" || map.scope.district.id !== l.district.id)) {
      throw `${l.name} belongs to ${l.district.label}.`;
    }
    if (l.placement && l.placement.building_id !== target.building_id && !options.move_location) {
      throw `${l.name} is already placed.`;
    }
    const occupant = all.find(
      (x) => x.id !== id && x.placement?.building_id === target.building_id,
    );
    if (occupant && !options.replace_occupant) throw "That building already holds a location.";
    return this.set(
      all.map((x) =>
        x.id === id
          ? this.place(x, target)
          : x.id === occupant?.id
            ? { ...x, placement: undefined, version: x.version + 1 }
            : x,
      ),
    );
  }

  unassign(id: string) {
    return this.set(
      (this.view.locations ?? []).map((x) =>
        x.id === id ? { ...x, placement: undefined, version: x.version + 1 } : x,
      ),
    );
  }

  update(id: string, version: number, edit: LocationEdit) {
    const all = this.view.locations ?? [];
    const l = all.find((x) => x.id === id)!;
    if (l.version !== version) throw "This location was changed elsewhere.";
    return this.set(
      all.map((x) =>
        x.id === id
          ? {
              ...x,
              name: edit.name ?? x.default_name,
              renamed: !!edit.name,
              notes: edit.notes,
              version: x.version + 1,
            }
          : x,
      ),
    );
  }

  private refresh(v: SettlementMapsView) {
    this.view = withLocations(v, v.locations ?? []);
    return this.view;
  }

  addPoi(mapId: string, position: Point, label: string, notes: string) {
    const map = this.view.maps!.maps.find((m) => m.id === mapId)!;
    const number = this.next++;
    const poi: PoiEntry = {
      id: `poi-${number}`,
      number,
      map_id: mapId,
      map_label: map.label,
      district: map.scope.kind === "district" ? map.scope.district : undefined,
      position,
      label,
      notes: notes || undefined,
    };
    return this.refresh({ ...this.view, pois: [...(this.view.pois ?? []), poi] });
  }

  private changePoi(id: string, change: (p: PoiEntry) => PoiEntry | null) {
    const pois = (this.view.pois ?? []).flatMap((p) => {
      if (p.id !== id) return [p];
      const next = change(p);
      return next ? [next] : [];
    });
    return this.refresh({ ...this.view, pois });
  }

  movePoi(id: string, position: Point) {
    return this.changePoi(id, (p) => ({ ...p, position }));
  }

  updatePoi(id: string, label: string, notes: string) {
    return this.changePoi(id, (p) => ({ ...p, label, notes: notes || undefined }));
  }

  deletePoi(id: string) {
    return this.changePoi(id, () => null);
  }

  renameDistrict(districtId: string, name: string | null) {
    const v = this.view;
    const relabel = (scope: SettlementMapsView["planned"][number]["scope"], fallback: string) =>
      scope.kind === "district" && scope.district.id === districtId
        ? { ...scope, district: { ...scope.district, label: name ?? fallback } }
        : scope;
    const planned = v.planned.map((p) => {
      if (p.scope.kind !== "district" || p.scope.district.id !== districtId) return p;
      const original = p.default_label ?? p.label;
      return {
        ...p,
        label: name ?? original,
        default_label: name ? original : undefined,
        scope: relabel(p.scope, original),
      };
    });
    const maps = v.maps && {
      ...v.maps,
      maps: v.maps.maps.map((m) => {
        if (m.scope.kind !== "district" || m.scope.district.id !== districtId) return m;
        const original = planned.find((p) => p.id === m.id)!;
        return { ...m, label: original.label, scope: original.scope };
      }),
    };
    const label = planned.find(
      (p) => p.scope.kind === "district" && p.scope.district.id === districtId,
    )!.label;
    const rename = <T extends { id: string; label: string } | undefined>(d: T) =>
      d && d.id === districtId ? { ...d, label } : d;
    const locations = (v.locations ?? []).map((l) => ({
      ...l,
      district: rename(l.district),
      placement: l.placement && {
        ...l.placement,
        district: rename(l.placement.district),
        map_label: l.placement.district?.id === districtId ? label : l.placement.map_label,
      },
    }));
    const pois = (v.pois ?? []).map((p) =>
      p.district?.id === districtId ? { ...p, district: rename(p.district), map_label: label } : p,
    );
    return this.refresh({ ...v, planned, maps, locations, pois });
  }

  deleteMap(mapId: string) {
    const v = this.view;
    return this.refresh({
      ...v,
      planned: v.planned.map((p) =>
        p.id === mapId
          ? { ...p, generated: false, annotations: { building_details: 0, locations: 0, pois: 0 } }
          : p,
      ),
      maps: v.maps && { ...v.maps, maps: v.maps.maps.filter((m) => m.id !== mapId) },
      pois: (v.pois ?? []).filter((p) => p.map_id !== mapId),
      locations: (v.locations ?? []).map((l) =>
        l.placement?.map_id === mapId ? { ...l, placement: undefined } : l,
      ),
      views: (v.views ?? []).filter((x) => x.map_id !== mapId),
    });
  }

  create(edit: LocationEdit, place: PlaceOn | null, options: AssignOptions) {
    const id = `custom-location-${this.custom++}`;
    const name = edit.name ?? edit.location_type ?? "Custom location";
    const entry: LocationEntry = {
      id,
      origin: "custom",
      name,
      default_name: edit.location_type ?? "Custom location",
      renamed: !!edit.name,
      location_type: edit.location_type ?? "Custom",
      version: 1,
    };
    this.set([...(this.view.locations ?? []), entry]);
    return place ? this.assign(id, place, options) : this.view;
  }
}
