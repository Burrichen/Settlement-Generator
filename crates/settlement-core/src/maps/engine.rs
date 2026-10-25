//! The engine boundary. The application depends only on
//! [`MapEngineAdapter`]; which engine draws the maps is an implementation
//! choice (see `docs/MAPS.md` for why the built-in engine is used and what
//! adopting another would require).

use std::collections::HashSet;
use std::panic::{AssertUnwindSafe, catch_unwind};

use thiserror::Error;

use super::MapError;
use super::geometry::{self, Point};
use super::model::{GEOMETRY_VERSION, MapGenerationRequest, MapGenerationResult};

/// A procedural map engine.
pub trait MapEngineAdapter: Send + Sync {
    /// The engine's ID, version and supported features.
    fn info(&self) -> super::model::EngineInfo;

    /// Draws one map. Must be deterministic: the same request under the same
    /// engine version gives the same result. Must not generate building
    /// details.
    fn generate(
        &self,
        request: &MapGenerationRequest,
    ) -> Result<MapGenerationResult, MapEngineError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MapEngineError {
    #[error("{0}")]
    Failed(String),
}

/// Runs `engine` for `request`, turning a panic into an error and checking
/// the result, so a misbehaving engine can never store bad geometry.
pub fn run_engine(
    engine: &dyn MapEngineAdapter,
    request: &MapGenerationRequest,
) -> Result<MapGenerationResult, MapError> {
    let outcome = catch_unwind(AssertUnwindSafe(|| engine.generate(request)));
    let result = match outcome {
        Ok(Ok(result)) => result,
        Ok(Err(e)) => {
            return Err(MapError::Engine {
                map: request.label.clone(),
                message: e.to_string(),
            });
        }
        Err(_) => {
            return Err(MapError::Engine {
                map: request.label.clone(),
                message: "the map engine stopped unexpectedly".into(),
            });
        }
    };
    validate_result(request, &result).map_err(|message| MapError::Engine {
        map: request.label.clone(),
        message,
    })?;
    Ok(result)
}

/// Engine-independent checks on a result.
pub fn validate_result(
    request: &MapGenerationRequest,
    result: &MapGenerationResult,
) -> Result<(), String> {
    if result.geometry_version != GEOMETRY_VERSION {
        return Err(format!(
            "geometry version {} is not supported (expected {GEOMETRY_VERSION})",
            result.geometry_version
        ));
    }
    let g = &result.geometry;
    let b = g.bounds;
    if !(b.width() > 0.0 && b.height() > 0.0) {
        return Err("the map has empty bounds".into());
    }
    if g.buildings.is_empty() {
        return Err("the map has no buildings".into());
    }
    let finite = |p: &Point| p.0.is_finite() && p.1.is_finite();
    let prefix = format!(
        "bld-{}-r{}-",
        map_key(request.map_id.as_str()),
        request.revision
    );
    let mut ids = HashSet::new();
    for building in &g.buildings {
        let id = building.id.as_str();
        if !building.id.is_valid() || !id.starts_with(&prefix) {
            return Err(format!("building ID `{id}` does not belong to this map"));
        }
        if !ids.insert(id) {
            return Err(format!("building ID `{id}` is used twice"));
        }
        if building.map_id != request.map_id || building.settlement_id != request.settlement_id {
            return Err(format!("building `{id}` names another map or settlement"));
        }
        if building.footprint.len() < 3
            || !building.footprint.iter().all(finite)
            || geometry::area(&building.footprint) <= 0.0
        {
            return Err(format!("building `{id}` has no footprint"));
        }
        if !building.footprint.iter().all(|p| b.contains(*p)) {
            return Err(format!("building `{id}` lies outside the map"));
        }
    }
    if result.stats.buildings_placed as usize != g.buildings.len() {
        return Err("the building count does not match the buildings".into());
    }
    Ok(())
}

/// The part of a map ID used in building IDs: `map-district-2` →
/// `district-2`.
pub fn map_key(map_id: &str) -> &str {
    map_id.strip_prefix("map-").unwrap_or(map_id)
}
