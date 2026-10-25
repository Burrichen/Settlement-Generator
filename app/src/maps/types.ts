// Settlement map types. These mirror the serialised structs in
// `settlement-core/src/maps/model.rs`; see docs/MAPS.md. The core generates
// and stores maps; the interface only displays them.
import type { District, Named, RollRecord } from "../api";

/** `[x, y]` in metres from the map centre; x east, y south (SVG). */
export type Point = [number, number];

export interface Bounds {
  min_x: number;
  min_y: number;
  max_x: number;
  max_y: number;
}

export type MapLayout = "single" | "per_district";
export type MapStatus = "not_generated" | "current" | "stale";
export type WaterKind = "none" | "coast" | "river" | "lake";
export type GroundKind =
  | "grass"
  | "farmland"
  | "woodland"
  | "heath"
  | "hills"
  | "rock"
  | "sand"
  | "marsh"
  | "snow"
  | "ice"
  | "jungle"
  | "wasteland"
  | "crystal"
  | "ichor"
  | "cavern";
export type MapFeature =
  | "buildings"
  | "streets"
  | "walls"
  | "gates"
  | "coast"
  | "river"
  | "lake"
  | "bridges"
  | "trees"
  | "fields"
  | "squares";
export type BuildingSize = "small" | "medium" | "large";
export type StreetKind = "road" | "street" | "lane";
/** Ungenerated buildings have no stored details; see BuildingDetailsView. */
export type DetailsState = "ungenerated" | "generated" | "customised";

export type MapScope = { kind: "settlement" } | { kind: "district"; district: District };

/** The settlement facts a map was derived from, as recorded. */
export interface MapInputs {
  settlement_name: string;
  size: Named;
  region?: Named;
  environment?: Named;
  connections?: Named;
  origin?: Named;
  purpose?: Named;
  age?: Named;
  tags: string[];
  districts?: District[];
  /** No table records population yet, so this is always absent. */
  population?: number;
}

export interface MapParameters {
  building_count: number;
  radius_m: number;
  approaches: number;
  walls: boolean;
  water: WaterKind;
  ground: GroundKind;
  irregularity: number;
  trees: number;
  fields: boolean;
  square: boolean;
}

export type MapParameterOverrides = Partial<MapParameters>;

export interface MapOverrides {
  all?: MapParameterOverrides;
  maps?: Record<string, MapParameterOverrides>;
}

export type SeedChoice =
  | { kind: "keep" }
  | { kind: "random" }
  /** At most 2^53 − 1. */
  | { kind: "fixed"; seed: number };

export interface MapGenerationOptions {
  seed?: SeedChoice;
  overrides?: MapOverrides;
  /** Generate only this map (e.g. one district), keeping the others. */
  only?: string;
}

export interface MapGenerationRequest {
  settlement_id: string;
  map_id: string;
  scope: MapScope;
  label: string;
  revision: number;
  seed: number;
  parameters: MapParameters;
  inputs: MapInputs;
  notable_locations: string[];
}

export interface EngineInfo {
  id: string;
  version: string;
  features: MapFeature[];
}

export interface MapStats {
  buildings_requested: number;
  buildings_placed: number;
}

export interface MapBuilding {
  id: string;
  map_id: string;
  settlement_id: string;
  /** Closed ring; also the hit-test shape. */
  footprint: Point[];
  centroid: Point;
  area_m2: number;
  size: BuildingSize;
  street?: string;
}

export interface Street {
  id: string;
  kind: StreetKind;
  width_m: number;
  path: Point[];
}

export interface Wall {
  id: string;
  segments: Point[][];
  gates: Point[];
  towers: Point[];
}

export interface WaterFeature {
  id: string;
  kind: WaterKind;
  area: Point[];
  centreline?: Point[];
  bridges?: Point[];
}

export interface TreeCluster {
  centre: Point;
  radius_m: number;
}

export interface AreaFeature {
  id: string;
  area: Point[];
}

export interface Terrain {
  ground: GroundKind;
  trees?: TreeCluster[];
  fields?: AreaFeature[];
  squares?: AreaFeature[];
}

export interface MapGeometry {
  bounds: Bounds;
  buildings: MapBuilding[];
  streets: Street[];
  walls?: Wall[];
  water?: WaterFeature[];
  terrain: Terrain;
}

export interface MapGenerationResult {
  engine: EngineInfo;
  geometry_version: number;
  geometry: MapGeometry;
  stats: MapStats;
}

export interface SettlementMap {
  id: string;
  settlement_id: string;
  scope: MapScope;
  label: string;
  revision: number;
  seed: number;
  parameters: MapParameters;
  notable_locations?: string[];
  engine: EngineInfo;
  geometry_version: number;
  stats: MapStats;
  /** The settlement facts this map was drawn from (to detect changes). */
  inputs_fingerprint: string;
  geometry: MapGeometry;
}

/** A City or Capital district's map. */
export type DistrictMap = SettlementMap & {
  scope: { kind: "district"; district: District };
};

export type MarkerTarget =
  | {
      kind: "notable_location";
      location_id: string;
      building_id: string;
    }
  | {
      kind: "point_of_interest";
      poi_id: string;
    };

export interface MapMarker {
  number: number;
  map_id: string;
  position: Point;
  target: MarkerTarget;
}

export interface MapLegendEntry {
  number: number;
  label: string;
  location_type?: string;
  district?: District;
  target: MarkerTarget;
}

/** Map-set-wide editable data: empty for now (edits are stored per item). */
export type MapMetadata = Record<string, never>;

export interface SettlementMapSet {
  settlement_id: string;
  format_version: number;
  layout: MapLayout;
  revision: number;
  seed: number;
  overrides: MapOverrides;
  inputs: MapInputs;
  inputs_fingerprint: string;
  generated_at: string;
  app_version: string;
  maps: SettlementMap[];
  metadata: MapMetadata;
}

/** A saved settlement's maps as the map screens show them. */
export interface SettlementMapsView {
  settlement_id: string;
  status: MapStatus;
  layout?: MapLayout;
  /** Why maps cannot be generated now; saved maps are still shown. */
  problem?: string;
  /** The engine new maps would be drawn with. */
  engine: EngineInfo;
  /** Every map the settlement gets, from its saved data, drawn or not. */
  planned: PlannedMapView[];
  maps?: SettlementMapSet;
  /** Markers and legend of each saved map. */
  legends?: MapLegendView[];
  /** Buildings on the saved maps whose details have been generated. */
  buildings?: BuildingSummary[];
  /** The settlement's Notable Location directory, placed or not. */
  locations?: LocationEntry[];
  /** Every point of interest on the saved maps, by number. */
  pois?: PoiEntry[];
  /** Where each map was last left. */
  views?: SavedView[];
  /** Stored map data that could not be read; it can be discarded. */
  damaged?: DamagedRecord[];
}

export type DamagedKind = "maps" | "building" | "location" | "poi";

export interface DamagedRecord {
  kind: DamagedKind;
  /** Empty for the map set. */
  id: string;
  problem: string;
}

// ----- exports (maps/render.rs, maps/export.rs) -------------------------------

export type ExportFormat = "svg" | "png" | "json";

export interface RenderOptions {
  markers: boolean;
  legend: boolean;
  /** Width of the map area in pixels. */
  width_px: number;
}

export interface MapExportRequest {
  /** One map; absent: every drawn map, each to its own file. */
  map?: string;
  formats: ExportFormat[];
  options: RenderOptions;
}

export interface ExportResult {
  dir: string;
  files: string[];
}

/** One map a settlement gets (for a City or Capital, one per district). */
export interface PlannedMapView {
  id: string;
  /** The district's name (the user's, if renamed), or the settlement's. */
  label: string;
  /** The district's name when not renamed; present only when renamed. */
  default_label?: string;
  scope: MapScope;
  generated: boolean;
  stale: boolean;
  /** What deleting this map would also delete or take off. */
  annotations: MapAnnotations;
}

export interface MapAnnotations {
  /** Buildings with stored details: deleted with the map. */
  building_details: number;
  /** Notable Locations placed on it: taken off, and kept. */
  locations: number;
  /** Points of interest on it: deleted with the map. */
  pois: number;
}

/** A point of interest: a user token at a position on one map. */
export interface PoiEntry {
  id: string;
  /** Shares the settlement-wide sequence with Notable Locations. */
  number: number;
  map_id: string;
  map_label: string;
  district?: District;
  position: Point;
  label: string;
  notes?: string;
}

/** Where a map was last left: centre in metres, zoom relative to its default view. */
export interface SavedView {
  map_id: string;
  centre: Point;
  zoom: number;
}

export const MAX_POI_LABEL_CHARS = 80;
export const MAX_DISTRICT_NAME_CHARS = 60;

export interface MapLegendView {
  map_id: string;
  markers: MapMarker[];
  entries: MapLegendEntry[];
}

// ----- building details (maps/details.rs) -------------------------------------

/** Edit limits; a core test keeps them equal to `maps::details`. */
export const MAX_NAME_CHARS = 120;
export const MAX_LABEL_CHARS = 80;
export const MAX_DESCRIPTION_CHARS = 1000;
export const MAX_NOTES_CHARS = 5000;
export const MAX_OCCUPANTS = 10000;

export type NotRolledReason = "inactive" | "not_eligible" | "missing" | "incomplete";

/** Where a generated value came from. */
export type GeneratedSource =
  | {
      kind: "table";
      /** The whole roll on the content table. */
      roll: RollRecord;
    }
  | {
      kind: "not_rolled";
      table: string;
      reason: NotRolledReason;
    }
  | {
      kind: "inferred";
      rule: string;
    };

export interface GeneratedValue<T> {
  value?: T;
  source: GeneratedSource;
}

/** Where a field's current value came from. */
export type FieldOrigin = "table" | "inferred" | "not_rolled" | "user";

/** One field: its value, its origin, and what generation gave. */
export interface BuildingField<T> {
  /** Absent when blank. */
  value?: T;
  origin: FieldOrigin;
  generated: GeneratedValue<T>;
}

/** Suggestions for the free-text fields, from the content tables. */
export interface DetailChoices {
  building_type: string[];
  condition: string[];
  occupancy: string[];
}

/** One building's details as the Location Inspector shows them. */
export interface BuildingDetailsView {
  settlement_id: string;
  map_id: string;
  building_id: string;
  map_label: string;
  district?: District;
  state: DetailsState;
  /** Sent back with a change, so a stale copy cannot overwrite a newer one. */
  version: number;
  generation: number;
  name: BuildingField<string>;
  building_type: BuildingField<string>;
  description: BuildingField<string>;
  condition: BuildingField<string>;
  occupancy: BuildingField<string>;
  occupants: BuildingField<number>;
  notes?: string;
  tags?: Named[];
  generated_at: string;
  app_version: string;
  edited_at?: string;
  choices: DetailChoices;
}

/** A value the user set; `value: null` is blank on purpose. */
export interface UserValue<T> {
  value: T | null;
}

/** The user's changes; a field left out keeps its generated value. */
export interface BuildingEdits {
  name?: UserValue<string>;
  building_type?: UserValue<string>;
  description?: UserValue<string>;
  condition?: UserValue<string>;
  occupancy?: UserValue<string>;
  occupants?: UserValue<number>;
  notes?: string;
}

/** A generated building, for lists. */
export interface BuildingSummary {
  map_id: string;
  building_id: string;
  state: DetailsState;
  name?: string;
  building_type?: string;
}

// ----- the Notable Location directory (maps/directory.rs) ----------------------

export const MAX_LOCATION_NAME_CHARS = 80;
export const MAX_LOCATION_TYPE_CHARS = 80;
export const MAX_LOCATION_NOTES_CHARS = 5000;

/** The settlement's own (from its snapshot), or added by the user. */
export type LocationOrigin = "settlement" | "custom";

/** Where a location stands. */
export interface LocationPlacement {
  map_id: string;
  building_id: string;
  map_label: string;
  district?: District;
  /** Its building was redrawn away: it must be placed again. */
  missing?: boolean;
}

/** One Notable Location of the settlement. */
export interface LocationEntry {
  id: string;
  origin: LocationOrigin;
  name: string;
  default_name: string;
  renamed?: boolean;
  location_type: string;
  /** The district it belongs to; it can only stand on that district's map. */
  district?: District;
  /** Persistent, settlement-wide; issued when first placed. */
  marker?: number;
  placement?: LocationPlacement;
  notes?: string;
  /** Sent back with a change; 0 when nothing is stored yet. */
  version: number;
  /** No longer in the settlement, kept for what the user set on it. */
  removed?: boolean;
}

export interface LocationEdit {
  /** null or blank: the default name. */
  name: string | null;
  /** Custom locations only. */
  location_type: string | null;
  notes: string;
}

/** Explicit consent for a placement that changes another one. */
export interface AssignOptions {
  move_location: boolean;
  replace_occupant: boolean;
}

export interface PlaceOn {
  map_id: string;
  building_id: string;
}

/**
 * The engine boundary, for a future engine that runs in the WebView. The
 * built-in engine runs in the Rust core, which validates and stores any
 * result; see docs/MAPS.md before adding one.
 */
export interface MapEngineAdapter {
  info(): EngineInfo;
  generate(request: MapGenerationRequest): Promise<MapGenerationResult>;
}
