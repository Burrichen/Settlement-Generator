//! From a saved settlement to map requests: which facts a map uses, which
//! maps a settlement gets, and each map's layout parameters.
//!
//! Parameters come from `data/map-defaults.json` (compiled in): a base, the
//! Size's layout and defaults, then rules chosen by the settlement's tags,
//! in file order (later rules win). Manual overrides are applied last.

use std::sync::OnceLock;

use serde::Deserialize;

use crate::content::Condition;
use crate::engine::Named;
use crate::ids::{MapId, ResultId, TableId, TagId};
use crate::locations::District;
use crate::settlements::{Settlement, SettlementSnapshot};

use super::MapError;
use super::model::{
    MapGenerationRequest, MapInputs, MapLayout, MapOverrides, MapParameterOverrides, MapParameters,
    MapScope,
};
use super::rng::derive_seed;

const DEFAULTS_JSON: &str = include_str!("../../data/map-defaults.json");

/// Format of `map-defaults.json` this build reads.
pub const MAP_DEFAULTS_FORMAT_VERSION: u32 = 1;

/// Limits on parameters, so a typed override cannot hang the engine.
pub const MAX_BUILDINGS: u32 = 600;
pub const MIN_RADIUS_M: f64 = 20.0;
pub const MAX_RADIUS_M: f64 = 600.0;
pub const MAX_APPROACHES: u32 = 8;

#[derive(Debug, Deserialize)]
pub struct MapDefaults {
    pub format_version: u32,
    #[serde(default)]
    pub description: String,
    pub inputs: InputStages,
    pub base: MapParameters,
    pub sizes: Vec<SizeDefaults>,
    pub rules: Vec<MapRule>,
}

/// The stage tables (as named in the sequence) each map input is read from.
#[derive(Debug, Deserialize)]
pub struct InputStages {
    pub size: TableId,
    pub region: TableId,
    pub environment: TableId,
    pub connections: TableId,
    pub origin: TableId,
    pub purpose: TableId,
    pub age: TableId,
}

#[derive(Debug, Deserialize)]
pub struct SizeDefaults {
    pub when: Condition,
    pub layout: MapLayout,
    #[serde(default)]
    pub set: MapParameterOverrides,
}

#[derive(Debug, Deserialize)]
pub struct MapRule {
    pub when: Condition,
    pub set: MapParameterOverrides,
}

impl MapDefaults {
    /// Every tag the defaults refer to (for checking against content).
    pub fn referenced_tags(&self) -> Vec<TagId> {
        fn walk(c: &Condition, out: &mut Vec<TagId>) {
            match c {
                Condition::HasTag(t) => out.push(t.clone()),
                Condition::All(cs) | Condition::Any(cs) => cs.iter().for_each(|c| walk(c, out)),
                Condition::Not(c) => walk(c, out),
            }
        }
        let mut out = Vec::new();
        for c in self
            .sizes
            .iter()
            .map(|s| &s.when)
            .chain(self.rules.iter().map(|r| &r.when))
        {
            walk(c, &mut out);
        }
        out
    }
}

/// The compiled-in map defaults.
pub fn map_defaults() -> Result<&'static MapDefaults, MapError> {
    static DEFAULTS: OnceLock<Result<MapDefaults, String>> = OnceLock::new();
    DEFAULTS
        .get_or_init(|| {
            let d: MapDefaults = serde_json::from_str(DEFAULTS_JSON).map_err(|e| e.to_string())?;
            if d.format_version != MAP_DEFAULTS_FORMAT_VERSION {
                return Err(format!("unsupported format {}", d.format_version));
            }
            validate_parameters(&d.base).map_err(|e| e.to_string())?;
            Ok(d)
        })
        .as_ref()
        .map_err(|e| MapError::Defaults(e.clone()))
}

/// The map inputs of a saved settlement, read from its snapshot as
/// recorded.
pub fn inputs_from_settlement(settlement: &Settlement) -> Result<MapInputs, MapError> {
    let defaults = map_defaults()?;
    let snapshot = &settlement.snapshot;
    let stages = snapshot.display_stages();
    let result_of = |table: &TableId| -> Option<Named<ResultId>> {
        stages
            .iter()
            .find(|s| &s.stage_table == table && !s.skipped)
            .and_then(|s| s.result.clone())
    };
    let size = result_of(&defaults.inputs.size).ok_or(MapError::NoSize)?;
    let mut tags: Vec<TagId> = Vec::new();
    for tag in stages.iter().flat_map(|s| s.tags.iter()) {
        if !tags.contains(&tag.id) {
            tags.push(tag.id.clone());
        }
    }
    Ok(MapInputs {
        settlement_name: settlement.summary.name.clone(),
        size,
        region: result_of(&defaults.inputs.region),
        environment: result_of(&defaults.inputs.environment),
        connections: result_of(&defaults.inputs.connections),
        origin: result_of(&defaults.inputs.origin),
        purpose: result_of(&defaults.inputs.purpose),
        age: result_of(&defaults.inputs.age),
        tags,
        districts: districts(snapshot),
        population: None,
    })
}

fn districts(snapshot: &SettlementSnapshot) -> Vec<District> {
    snapshot
        .notable_locations()
        .map(|l| l.areas.iter().filter_map(|a| a.district.clone()).collect())
        .unwrap_or_default()
}

/// How a settlement with these inputs is mapped.
pub fn layout_for(inputs: &MapInputs) -> Result<MapLayout, MapError> {
    Ok(size_defaults(map_defaults()?, inputs)?.layout)
}

fn size_defaults<'d>(
    defaults: &'d MapDefaults,
    inputs: &MapInputs,
) -> Result<&'d SizeDefaults, MapError> {
    let has = |t: &TagId| inputs.tags.contains(t);
    defaults
        .sizes
        .iter()
        .find(|s| s.when.evaluate(&has))
        .ok_or_else(|| MapError::UnsupportedSize(inputs.size.name.clone()))
}

/// The default parameters (before manual overrides) for these inputs.
pub fn default_parameters(inputs: &MapInputs) -> Result<MapParameters, MapError> {
    let defaults = map_defaults()?;
    let has = |t: &TagId| inputs.tags.contains(t);
    let mut p = defaults.base.clone();
    size_defaults(defaults, inputs)?.set.apply(&mut p);
    for rule in &defaults.rules {
        if rule.when.evaluate(&has) {
            rule.set.apply(&mut p);
        }
    }
    Ok(p)
}

/// Refuses parameters an engine cannot sensibly draw.
pub fn validate_parameters(p: &MapParameters) -> Result<(), MapError> {
    let bad = |what: &str| Err(MapError::InvalidParameter(what.to_owned()));
    if p.building_count == 0 || p.building_count > MAX_BUILDINGS {
        return bad(&format!("buildings must be between 1 and {MAX_BUILDINGS}"));
    }
    if !(MIN_RADIUS_M..=MAX_RADIUS_M).contains(&p.radius_m) {
        return bad(&format!(
            "radius must be between {MIN_RADIUS_M} and {MAX_RADIUS_M} metres"
        ));
    }
    if p.approaches > MAX_APPROACHES {
        return bad(&format!("roads must be at most {MAX_APPROACHES}"));
    }
    if !(0.0..=1.0).contains(&p.irregularity) {
        return bad("irregularity must be between 0 and 1");
    }
    if !(0.0..=1.0).contains(&p.trees) {
        return bad("tree cover must be between 0 and 1");
    }
    Ok(())
}

/// One map a settlement gets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedMap {
    pub id: MapId,
    pub scope: MapScope,
    pub label: String,
}

/// The maps a settlement gets.
pub fn planned_maps(inputs: &MapInputs) -> Result<(MapLayout, Vec<PlannedMap>), MapError> {
    let layout = layout_for(inputs)?;
    let maps = match layout {
        MapLayout::Single => vec![PlannedMap {
            id: MapId::new("map-settlement"),
            scope: MapScope::Settlement,
            label: inputs.settlement_name.clone(),
        }],
        MapLayout::PerDistrict => {
            if inputs.districts.is_empty() {
                return Err(MapError::NoDistricts(inputs.size.name.clone()));
            }
            inputs
                .districts
                .iter()
                .map(|d| PlannedMap {
                    id: MapId::new(format!("map-{}", d.id)),
                    scope: MapScope::District {
                        district: d.clone(),
                    },
                    label: d.label.clone(),
                })
                .collect()
        }
    };
    Ok((layout, maps))
}

/// One request per planned map.
pub fn plan_requests(
    settlement: &Settlement,
    inputs: &MapInputs,
    overrides: &MapOverrides,
    set_seed: u64,
    revision: u32,
) -> Result<(MapLayout, Vec<MapGenerationRequest>), MapError> {
    let (layout, maps) = planned_maps(inputs)?;
    if let Some(unknown) = overrides
        .maps
        .keys()
        .find(|id| !maps.iter().any(|m| &m.id == *id))
    {
        return Err(MapError::UnknownMap(unknown.to_string()));
    }
    let defaults = default_parameters(inputs)?;
    let locations = settlement.snapshot.notable_locations();
    let requests = maps
        .into_iter()
        .map(
            |PlannedMap {
                 id: map_id,
                 scope,
                 label,
             }| {
                let mut parameters = defaults.clone();
                overrides.all.apply(&mut parameters);
                if let Some(o) = overrides.maps.get(&map_id) {
                    o.apply(&mut parameters);
                }
                validate_parameters(&parameters)?;
                let district = match &scope {
                    MapScope::District { district } => Some(district.id.as_str()),
                    MapScope::Settlement => None,
                };
                let notable_locations = locations
                    .map(|l| {
                        l.locations
                            .iter()
                            // A settlement mapped whole holds all of them; a
                            // district map holds its own. The settlement-level
                            // guaranteed Tavern of a City is not yet placed.
                            .filter(|loc| district.is_none() || loc.district.as_deref() == district)
                            .map(|loc| loc.id.clone())
                            .collect()
                    })
                    .unwrap_or_default();
                Ok(MapGenerationRequest {
                    settlement_id: settlement.summary.id.clone(),
                    seed: derive_seed(set_seed, map_id.as_str()),
                    map_id,
                    scope,
                    label,
                    revision,
                    parameters,
                    inputs: inputs.clone(),
                    notable_locations,
                })
            },
        )
        .collect::<Result<Vec<_>, MapError>>()?;
    Ok((layout, requests))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_parse_and_cover_every_size() {
        let d = map_defaults().unwrap();
        for size in [
            "size.tradingPost",
            "size.village",
            "size.town",
            "size.city",
            "size.capital",
        ] {
            let has = |t: &TagId| t.as_str() == size;
            assert!(d.sizes.iter().any(|s| s.when.evaluate(&has)), "{size}");
        }
    }
}
