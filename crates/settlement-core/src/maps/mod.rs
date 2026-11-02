//! Interactive settlement maps. See `docs/MAPS.md`.
//!
//! - Trading Posts, Villages and Towns get one map; Cities and Capitals get
//!   one independent map per recorded district, and no whole-city map.
//! - A map is derived from the saved settlement (never the other way round:
//!   mapping never changes a settlement result).
//! - Geometry is generated once from a saved seed and stored. Reopening
//!   reads it back; only an explicit regeneration replaces it.
//! - Editable metadata (building details, placements, tokens) is stored
//!   apart from geometry and refers to it by stable IDs. Building details
//!   are generated lazily, one building at a time, when it is first opened
//!   (see [`details`]).
//! - Engines sit behind [`MapEngineAdapter`]; the built-in engine is used
//!   (see `docs/MAPS.md` §Licensing for why settlemaker is not bundled).

pub mod builtin;
pub mod details;
pub mod directory;
pub mod engine;
pub mod export;
pub mod geometry;
pub mod migrate;
pub mod model;
pub mod params;
pub mod render;
pub mod rng;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::VERSION;
use crate::ids::MapId;
use crate::settlements::{Settlement, now};

pub use builtin::BuiltinEngine;
pub use details::{
    BuildingDetailsView, BuildingEdits, BuildingRecord, BuildingSummary, DetailsError,
    DetailsState, UserValue,
};
pub use directory::{
    AssignOptions, DistrictNames, LocationBook, LocationEdit, LocationEntry, LocationError,
    LocationOrigin, LocationRecord, PlaceOn, PoiEntry, markers,
};
pub use engine::{MapEngineAdapter, MapEngineError, run_engine};
pub use model::*;

/// Why maps could not be generated. Nothing is saved when this happens, and
/// the settlement itself is never affected.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MapError {
    #[error("The settlement has no Size yet. Roll its Size before generating maps.")]
    NoSize,
    #[error("No map layout is defined for the Size \u{201c}{0}\u{201d}.")]
    UnsupportedSize(String),
    #[error(
        "This {0} has no districts recorded yet. Roll its Notable Locations, which roll the \
         districts, before generating its maps."
    )]
    NoDistricts(String),
    #[error("There is no map `{0}` to change.")]
    UnknownMap(String),
    #[error("Map setting not accepted: {0}.")]
    InvalidParameter(String),
    #[error("The seed must be a whole number from 0 to {}.", rng::MAX_SEED)]
    SeedTooLarge,
    #[error("The map defaults could not be read: {0}")]
    Defaults(String),
    #[error("The map \u{201c}{map}\u{201d} could not be generated: {message}")]
    Engine { map: String, message: String },
}

/// Whether a settlement's saved maps still match it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MapStatus {
    NotGenerated,
    /// Generated from the settlement as it is now.
    Current,
    /// The settlement was edited since; the saved maps are kept as they are
    /// until the user regenerates them.
    Stale,
}

/// The status of `set` against the settlement as saved now: stale if any
/// saved map was drawn from facts that have changed since.
pub fn map_status(settlement: &Settlement, set: Option<&SettlementMapSet>) -> MapStatus {
    match set {
        Some(set) if !set.maps.is_empty() => {
            if stale_maps(settlement, set).is_empty() {
                MapStatus::Current
            } else {
                MapStatus::Stale
            }
        }
        _ => MapStatus::NotGenerated,
    }
}

/// The saved maps drawn from settlement facts that have changed since.
pub fn stale_maps(settlement: &Settlement, set: &SettlementMapSet) -> Vec<MapId> {
    let current = params::inputs_from_settlement(settlement)
        .ok()
        .map(|inputs| rng::fingerprint(&inputs));
    set.maps
        .iter()
        .filter(|m| current.as_deref() != Some(m.inputs_fingerprint.as_str()))
        .map(|m| m.id.clone())
        .collect()
}

/// Generates the maps of `settlement` with `engine`, entirely in memory:
/// every planned map, or only `options.only`. The caller saves the result;
/// on any error nothing has changed.
///
/// `previous` is the settlement's saved map set, if any: its seed is kept
/// (unless the options ask otherwise), its revision is increased, and its
/// editable metadata is carried over. With `only`, the other saved maps are
/// kept exactly as they are. Because each map's seed is derived from the set
/// seed and its own ID, a district generated on its own is the same as it
/// would have been generated with the others.
///
/// A saved map whose district no longer exists in the settlement is
/// dropped when maps are regenerated; its editable data is kept.
pub fn generate_map_set(
    engine: &dyn MapEngineAdapter,
    settlement: &Settlement,
    previous: Option<&SettlementMapSet>,
    options: &MapGenerationOptions,
) -> Result<SettlementMapSet, MapError> {
    let inputs = params::inputs_from_settlement(settlement)?;
    let seed = match options.seed {
        SeedChoice::Keep => previous.map_or_else(rng::random_seed, |p| p.seed),
        SeedChoice::Random => rng::random_seed(),
        SeedChoice::Fixed(seed) if seed <= rng::MAX_SEED => seed,
        SeedChoice::Fixed(_) => return Err(MapError::SeedTooLarge),
    };
    let overrides = options
        .overrides
        .clone()
        .or_else(|| previous.map(|p| p.overrides.clone()))
        .unwrap_or_default();
    let revision = previous.map_or(1, |p| p.revision + 1);
    let (layout, requests) =
        params::plan_requests(settlement, &inputs, &overrides, seed, revision)?;
    if let Some(only) = &options.only {
        if !requests.iter().any(|r| &r.map_id == only) {
            return Err(MapError::UnknownMap(only.to_string()));
        }
    }
    let fingerprint = rng::fingerprint(&inputs);
    let maps = requests
        .into_iter()
        .filter_map(|request| {
            if options
                .only
                .as_ref()
                .is_none_or(|only| &request.map_id == only)
            {
                return Some(Planned::Draw(request));
            }
            // Not asked for: keep the saved map, if there is one.
            previous
                .and_then(|p| p.map(&request.map_id))
                .map(|kept| Planned::Keep(kept.clone()))
        })
        .map(|planned| {
            let request = match planned {
                Planned::Draw(request) => request,
                Planned::Keep(map) => return Ok(map),
            };
            let result = run_engine(engine, &request)?;
            Ok(SettlementMap {
                id: request.map_id,
                settlement_id: request.settlement_id,
                scope: request.scope,
                label: request.label,
                revision,
                seed: request.seed,
                parameters: request.parameters,
                notable_locations: request.notable_locations,
                engine: result.engine,
                geometry_version: result.geometry_version,
                stats: result.stats,
                inputs_fingerprint: fingerprint.clone(),
                geometry: result.geometry,
            })
        })
        .collect::<Result<Vec<_>, MapError>>()?;
    Ok(SettlementMapSet {
        settlement_id: settlement.summary.id.clone(),
        format_version: MAP_FORMAT_VERSION,
        layout,
        revision,
        seed,
        overrides,
        inputs_fingerprint: fingerprint,
        inputs,
        generated_at: now(),
        app_version: VERSION.to_owned(),
        maps,
        metadata: previous.map(|p| p.metadata.clone()).unwrap_or_default(),
    })
}

/// One map in a generation: drawn now, or kept as saved.
enum Planned {
    Draw(MapGenerationRequest),
    Keep(SettlementMap),
}
