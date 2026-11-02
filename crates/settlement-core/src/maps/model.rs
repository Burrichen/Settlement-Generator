//! The map data model. See `docs/MAPS.md`.
//!
//! Two kinds of data are kept strictly apart:
//!
//! - **Procedural geometry** ([`SettlementMap`], [`MapGeometry`]): what the
//!   engine produced from a seed and parameters. It is stored as generated
//!   and never edited; reopening a settlement reads it back rather than
//!   rerunning the engine. Only an explicit regeneration replaces it.
//! - **Editable metadata** ([`MapMetadata`], and building details in
//!   [`super::details`]): notable location placements, user tokens and
//!   building details. It refers to geometry only by stable IDs.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::engine::Named;
use crate::ids::{BuildingId, MapId, PoiId, ResultId, TagId};
use crate::locations::District;
use crate::settlements::SettlementId;

use super::geometry::{Bounds, Point};

/// Version of the stored map documents (map set header, each map, and the
/// metadata). Readers refuse a newer version and upgrade older ones; see
/// [`super::migrate`].
pub const MAP_FORMAT_VERSION: u32 = 1;

/// Version of the geometry contract: coordinate frame, units and feature
/// shapes (see [`super::geometry`]). Changes only if saved geometry would be
/// read differently.
pub const GEOMETRY_VERSION: u32 = 1;

// ----- inputs -------------------------------------------------------------------

/// The settlement facts a map is derived from, copied from the saved
/// snapshot as recorded (never re-read from current content). Absent facts
/// are `None`: nothing is invented to fill them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapInputs {
    pub settlement_name: String,
    pub size: Named<ResultId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<Named<ResultId>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<Named<ResultId>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connections: Option<Named<ResultId>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<Named<ResultId>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<Named<ResultId>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub age: Option<Named<ResultId>>,
    /// Every tag the settlement's stages activated, in stage order.
    pub tags: Vec<TagId>,
    /// The districts recorded by the Notable Locations stage (Cities and
    /// Capitals); empty otherwise.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub districts: Vec<District>,
    /// The settlement's population, if content ever records one. No current
    /// table does, so this is always `None` and no map parameter is derived
    /// from population.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub population: Option<u32>,
}

/// How a settlement is mapped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MapLayout {
    /// One map of the whole settlement (Trading Post, Village, Town).
    Single,
    /// One independent map per district, and no whole-settlement map
    /// (City, Capital).
    PerDistrict,
}

/// What one map shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MapScope {
    Settlement,
    District { district: District },
}

/// Water on a map. Only kinds the engine draws are listed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaterKind {
    None,
    Coast,
    River,
    Lake,
}

/// The ground drawn under a map: a palette key, not a simulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroundKind {
    Grass,
    Farmland,
    Woodland,
    Heath,
    Hills,
    Rock,
    Sand,
    Marsh,
    Snow,
    Ice,
    Jungle,
    Wasteland,
    Crystal,
    Ichor,
    Cavern,
}

/// The layout parameters of one map: map defaults (`data/map-defaults.json`)
/// chosen by the settlement's tags, then any manual overrides. These are
/// drawing choices, not settlement facts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapParameters {
    /// Buildings the engine tries to place.
    pub building_count: u32,
    /// Buildings stay within this distance of the centre.
    pub radius_m: f64,
    /// Roads leaving the map.
    pub approaches: u32,
    pub walls: bool,
    pub water: WaterKind,
    pub ground: GroundKind,
    /// 0 = regular and planned, 1 = organic and crooked.
    pub irregularity: f64,
    /// 0–1: tree cover around the built area.
    pub trees: f64,
    /// Field plots around the built area.
    pub fields: bool,
    /// An open central square.
    pub square: bool,
}

/// Manual changes to [`MapParameters`]. Each field left `None` keeps the
/// default.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MapParameterOverrides {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub building_count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radius_m: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approaches: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub walls: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub water: Option<WaterKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ground: Option<GroundKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub irregularity: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trees: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fields: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub square: Option<bool>,
}

impl MapParameterOverrides {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Applies the fields that are set.
    pub fn apply(&self, p: &mut MapParameters) {
        macro_rules! set {
            ($($f:ident),*) => { $(if let Some(v) = self.$f { p.$f = v; })* };
        }
        set!(
            building_count,
            radius_m,
            approaches,
            walls,
            water,
            ground,
            irregularity,
            trees,
            fields,
            square
        );
    }
}

/// Manual overrides for a map set: for every map, then per map (which win).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MapOverrides {
    #[serde(default, skip_serializing_if = "MapParameterOverrides::is_empty")]
    pub all: MapParameterOverrides,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub maps: BTreeMap<MapId, MapParameterOverrides>,
}

/// Which seed a generation uses.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "seed", rename_all = "snake_case")]
pub enum SeedChoice {
    /// The map set's saved seed, or a new one if there is none.
    #[default]
    Keep,
    /// A new random seed.
    Random,
    /// This seed (at most [`super::rng::MAX_SEED`]).
    Fixed(u64),
}

/// What the user asked for when (re)generating a settlement's maps.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MapGenerationOptions {
    #[serde(default)]
    pub seed: SeedChoice,
    /// Replaces the saved overrides when given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overrides: Option<MapOverrides>,
    /// Generate only this map (for example one district), keeping the
    /// settlement's other saved maps exactly as they are.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub only: Option<MapId>,
}

/// Everything an engine needs to draw one map.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapGenerationRequest {
    pub settlement_id: SettlementId,
    pub map_id: MapId,
    pub scope: MapScope,
    pub label: String,
    /// The map set revision; building IDs include it.
    pub revision: u32,
    /// This map's own seed, derived from the map set seed and `map_id`.
    pub seed: u64,
    pub parameters: MapParameters,
    pub inputs: MapInputs,
    /// IDs of the Notable Locations in this map's area.
    pub notable_locations: Vec<String>,
}

// ----- engine output ----------------------------------------------------------

/// The engine that drew a map, recorded with it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineInfo {
    /// Stable engine ID, e.g. `builtin.medieval`.
    pub id: String,
    /// The engine's algorithm version. The same seed and parameters give the
    /// same geometry only under the same version.
    pub version: String,
    /// Features this engine can produce. Interfaces must not offer others.
    pub features: Vec<MapFeature>,
}

/// Map features an engine may support.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MapFeature {
    Buildings,
    Streets,
    Walls,
    Gates,
    Coast,
    River,
    Lake,
    Bridges,
    Trees,
    Fields,
    Squares,
}

/// One engine run's output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapGenerationResult {
    pub engine: EngineInfo,
    pub geometry_version: u32,
    pub geometry: MapGeometry,
    pub stats: MapStats,
}

/// Honest accounting of what was placed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapStats {
    pub buildings_requested: u32,
    pub buildings_placed: u32,
}

impl MapStats {
    /// Buildings that did not fit.
    pub fn shortfall(&self) -> u32 {
        self.buildings_requested
            .saturating_sub(self.buildings_placed)
    }
}

/// The procedural geometry of one map. Immutable once generated.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapGeometry {
    /// The drawn extent, in the frame of [`super::geometry`].
    pub bounds: Bounds,
    pub buildings: Vec<MapBuilding>,
    pub streets: Vec<Street>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub walls: Vec<Wall>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub water: Vec<WaterFeature>,
    pub terrain: Terrain,
}

/// Rough size of a building footprint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildingSize {
    Small,
    Medium,
    Large,
}

/// One building's geometry. Everything editable about it lives in
/// [`super::details::BuildingRecord`], keyed by `id`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapBuilding {
    pub id: BuildingId,
    pub map_id: MapId,
    pub settlement_id: SettlementId,
    /// Closed ring (first point not repeated); also the hit-test shape.
    pub footprint: Vec<Point>,
    pub centroid: Point,
    pub area_m2: f64,
    pub size: BuildingSize,
    /// The street it faces.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub street: Option<String>,
}

impl MapBuilding {
    /// Whether `p` falls on this building (hit-testing).
    pub fn contains(&self, p: Point) -> bool {
        super::geometry::contains(&self.footprint, p)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreetKind {
    /// A road leaving the map.
    Road,
    Street,
    Lane,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Street {
    pub id: String,
    pub kind: StreetKind,
    pub width_m: f64,
    /// Polyline; a ring street repeats its first point at the end.
    pub path: Vec<Point>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Wall {
    pub id: String,
    /// Wall runs (polylines). A ring is split where it would stand in water.
    pub segments: Vec<Vec<Point>>,
    pub gates: Vec<Point>,
    pub towers: Vec<Point>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WaterFeature {
    pub id: String,
    pub kind: WaterKind,
    /// Filled area, clipped to the map bounds.
    pub area: Vec<Point>,
    /// A river's centreline.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub centreline: Vec<Point>,
    /// Where streets cross a river.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bridges: Vec<Point>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Terrain {
    pub ground: GroundKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trees: Vec<TreeCluster>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<AreaFeature>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub squares: Vec<AreaFeature>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TreeCluster {
    pub centre: Point,
    pub radius_m: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AreaFeature {
    pub id: String,
    pub area: Vec<Point>,
}

// ----- stored maps ------------------------------------------------------------

/// One generated map: what was asked for, who drew it, and the geometry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettlementMap {
    pub id: MapId,
    pub settlement_id: SettlementId,
    pub scope: MapScope,
    pub label: String,
    pub revision: u32,
    pub seed: u64,
    pub parameters: MapParameters,
    /// Notable Locations in this map's area (by ID).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notable_locations: Vec<String>,
    pub engine: EngineInfo,
    pub geometry_version: u32,
    pub stats: MapStats,
    /// Fingerprint of the settlement facts this map was drawn from, so each
    /// map can tell when the settlement has changed since.
    pub inputs_fingerprint: String,
    pub geometry: MapGeometry,
}

/// A district's map): a [`SettlementMap`] whose scope is a district. Cities
/// and Capitals have one per district and no whole-settlement map.
pub type DistrictMap = SettlementMap;

impl SettlementMap {
    /// The district, for a district map.
    pub fn district(&self) -> Option<&District> {
        match &self.scope {
            MapScope::District { district } => Some(district),
            MapScope::Settlement => None,
        }
    }

    pub fn building(&self, id: &BuildingId) -> Option<&MapBuilding> {
        self.geometry.buildings.iter().find(|b| &b.id == id)
    }

    /// The building at `p`, if any (hit-testing).
    pub fn building_at(&self, p: Point) -> Option<&MapBuilding> {
        self.geometry.buildings.iter().find(|b| b.contains(p))
    }
}

/// All maps of one settlement, generated together from one seed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettlementMapSet {
    pub settlement_id: SettlementId,
    pub format_version: u32,
    pub layout: MapLayout,
    /// Increases with every regeneration, starting at 1.
    pub revision: u32,
    pub seed: u64,
    #[serde(default)]
    pub overrides: MapOverrides,
    pub inputs: MapInputs,
    /// Fingerprint of `inputs`: tells when the settlement has changed since.
    pub inputs_fingerprint: String,
    #[serde(with = "time::serde::rfc3339")]
    pub generated_at: OffsetDateTime,
    pub app_version: String,
    /// One map, or one per district in district order.
    pub maps: Vec<SettlementMap>,
    #[serde(default)]
    pub metadata: MapMetadata,
}

impl SettlementMapSet {
    pub fn map(&self, id: &MapId) -> Option<&SettlementMap> {
        self.maps.iter().find(|m| &m.id == id)
    }
}

// ----- editable metadata ------------------------------------------------------

/// Map-set-wide editable data, stored with the map set. Empty for now:
/// everything the user edits is stored one row per item instead, so an edit
/// never rewrites the maps (building details, the Notable Location
/// directory, points of interest, district names and saved views; see
/// [`super::details`] and [`super::directory`]).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MapMetadata {}

/// A point of interest: a user token at a position on one map, independent
/// of buildings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PointOfInterest {
    pub id: PoiId,
    /// Its marker number, from the same settlement-wide sequence as Notable
    /// Locations, so numbers never clash.
    pub number: u32,
    pub map_id: MapId,
    pub position: Point,
    pub label: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
}

/// A numbered marker drawn on a map, derived for display from the location
/// directory and tokens. Its number is the location's persistent marker
/// number, never its position in a list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapMarker {
    pub number: u32,
    pub map_id: MapId,
    pub position: Point,
    pub target: MarkerTarget,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MarkerTarget {
    NotableLocation {
        location_id: String,
        building_id: BuildingId,
    },
    PointOfInterest {
        poi_id: PoiId,
    },
}

/// One line of a map's legend, matching a [`MapMarker`]'s number.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapLegendEntry {
    pub number: u32,
    pub label: String,
    /// The location's type, e.g. "Hospitality".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location_type: Option<String>,
    /// The district of the map it is on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub district: Option<District>,
    pub target: MarkerTarget,
}

/// Where a map was last left: its centre in map metres, and its zoom
/// relative to the map's default view (1 = the default), so it fits any
/// window size.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedView {
    pub map_id: MapId,
    pub centre: Point,
    pub zoom: f64,
}

/// What kind of stored map data could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DamagedKind {
    /// The map set itself (every map of the settlement).
    Maps,
    Building,
    Location,
    Poi,
}

/// Stored map data that could not be read (damaged, or from a newer
/// version). The settlement and everything else still open; the user can
/// discard it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DamagedRecord {
    pub kind: DamagedKind,
    /// The building, location or point ID; empty for the map set.
    pub id: String,
    pub problem: String,
}
