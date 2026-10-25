//! Settlement maps: which maps each Size gets, deterministic geometry,
//! building identity, bounds, small and large inputs, and failure safety.

use std::collections::HashSet;

use settlement_core::db::Database;
use settlement_core::dice::RandomFaces;
use settlement_core::engine::Selections;
use settlement_core::ids::{MapId, ResultId, TagId};
use settlement_core::maps::builtin::{BUILTIN_ENGINE_ID, BUILTIN_ENGINE_VERSION};
use settlement_core::maps::engine::validate_result;
use settlement_core::maps::params::{self, map_defaults};
use settlement_core::maps::rng::MAX_SEED;
use settlement_core::maps::{
    self, BuiltinEngine, EngineInfo, GEOMETRY_VERSION, MAP_FORMAT_VERSION, MapEngineAdapter,
    MapEngineError, MapError, MapGenerationOptions, MapGenerationRequest, MapGenerationResult,
    MapLayout, MapOverrides, MapParameterOverrides, MapScope, MapStatus, SeedChoice,
    SettlementMapSet, WaterKind,
};
use settlement_core::service::AppService;
use settlement_core::settlements::{Settlement, SettlementId};

const SIZES: [&str; 5] = ["tradingPost", "village", "town", "city", "capital"];

fn service() -> AppService {
    AppService::new(Database::open_in_memory().unwrap())
}

/// A complete settlement of `size`, generated automatically from `seed`.
fn generate(s: &AppService, size: &str, seed: u64) -> settlement_core::service::ActiveGeneration {
    let mut active = s.start_generation().unwrap();
    active
        .session
        .choose(0, ResultId::new("kurovia"), &mut RandomFaces::seeded(seed))
        .unwrap();
    let mut sizes = Selections::new();
    sizes.insert("table.size".into(), size.into());
    active
        .session
        .run_automatic(&sizes, &mut RandomFaces::seeded(seed + 1))
        .unwrap();
    active
}

fn saved(s: &mut AppService, size: &str, seed: u64) -> SettlementId {
    let active = generate(s, size, seed);
    s.save_new(&active, &format!("Test {size}")).unwrap().id
}

/// A saved settlement as the core reads it.
fn settlement(size: &str, seed: u64) -> Settlement {
    let active = generate(&service(), size, seed);
    Database::open_in_memory()
        .unwrap()
        .create_settlement(&format!("Test {size}"), active.session.to_snapshot())
        .unwrap()
}

fn options(seed: u64) -> MapGenerationOptions {
    MapGenerationOptions {
        seed: SeedChoice::Fixed(seed),
        overrides: None,
        only: None,
    }
}

fn with(overrides: MapParameterOverrides, seed: u64) -> MapGenerationOptions {
    MapGenerationOptions {
        seed: SeedChoice::Fixed(seed),
        overrides: Some(MapOverrides {
            all: overrides,
            maps: Default::default(),
        }),
        only: None,
    }
}

fn map_set(size: &str, seed: u64) -> SettlementMapSet {
    maps::generate_map_set(
        &BuiltinEngine,
        &settlement(size, seed),
        None,
        &options(seed),
    )
    .unwrap()
}

/// Every structural promise a map set makes.
fn assert_well_formed(set: &SettlementMapSet) {
    let mut ids = HashSet::new();
    for map in &set.maps {
        assert_eq!(map.settlement_id, set.settlement_id);
        assert_eq!(map.engine.id, BUILTIN_ENGINE_ID);
        assert_eq!(map.engine.version, BUILTIN_ENGINE_VERSION);
        assert_eq!(map.geometry_version, GEOMETRY_VERSION);
        let g = &map.geometry;
        assert!(!g.buildings.is_empty(), "{} has buildings", map.label);
        assert!(!g.streets.is_empty(), "{} has streets", map.label);
        assert_eq!(map.stats.buildings_placed as usize, g.buildings.len());
        let prefix = format!(
            "bld-{}-r{}-",
            map.id.as_str().trim_start_matches("map-"),
            set.revision
        );
        for b in &g.buildings {
            assert!(
                b.id.is_valid() && b.id.as_str().starts_with(&prefix),
                "{}",
                b.id
            );
            assert!(
                ids.insert(b.id.clone()),
                "{} is unique in the settlement",
                b.id
            );
            assert_eq!(b.map_id, map.id);
            assert_eq!(b.settlement_id, set.settlement_id);
            assert_eq!(b.footprint.len(), 4);
            assert!(b.area_m2 > 10.0, "{} has area", b.id);
            assert!(b.footprint.iter().all(|p| g.bounds.contains(*p)));
            // The footprint is the hit-test shape: its centre selects it.
            assert_eq!(map.building_at(b.centroid).map(|h| &h.id), Some(&b.id));
        }
        for s in &g.streets {
            assert!(s.path.len() >= 2 && s.width_m > 0.0, "{}", s.id);
            assert!(s.path.iter().all(|p| g.bounds.contains(*p)), "{}", s.id);
        }
        for w in &g.water {
            assert!(w.area.len() >= 3 && w.area.iter().all(|p| g.bounds.contains(*p)));
            for b in &g.buildings {
                assert!(
                    !settlement_core::maps::geometry::contains(&w.area, b.centroid),
                    "{} stands in water",
                    b.id
                );
            }
        }
    }
    assert_eq!(
        set.metadata,
        Default::default(),
        "no editable data is invented"
    );
}

// ----- which maps -------------------------------------------------------------

#[test]
fn trading_posts_villages_and_towns_get_one_settlement_map() {
    for size in ["tradingPost", "village", "town"] {
        let set = map_set(size, 11);
        assert_eq!(set.layout, MapLayout::Single, "{size}");
        assert_eq!(set.maps.len(), 1, "{size}");
        let map = &set.maps[0];
        assert_eq!(map.id, MapId::new("map-settlement"));
        assert_eq!(map.scope, MapScope::Settlement);
        assert_eq!(map.label, format!("Test {size}"));
        assert_well_formed(&set);
    }
}

#[test]
fn cities_and_capitals_get_one_map_per_recorded_district_and_no_city_map() {
    for size in ["city", "capital"] {
        for seed in [3, 8, 21] {
            let s = settlement(size, seed);
            let districts: Vec<_> = s
                .snapshot
                .notable_locations()
                .unwrap()
                .areas
                .iter()
                .filter_map(|a| a.district.clone())
                .collect();
            assert!(districts.len() >= 2, "{size} has districts");
            let set = maps::generate_map_set(&BuiltinEngine, &s, None, &options(seed)).unwrap();
            assert_eq!(set.layout, MapLayout::PerDistrict);
            assert_eq!(set.maps.len(), districts.len(), "{size} seed {seed}");
            assert!(set.maps.iter().all(|m| m.scope != MapScope::Settlement));
            for (map, district) in set.maps.iter().zip(&districts) {
                assert_eq!(map.district(), Some(district));
                assert_eq!(map.id.as_str(), format!("map-{}", district.id));
                assert_eq!(map.label, district.label);
                // Each district map holds its own Notable Locations.
                let own: Vec<String> = s
                    .snapshot
                    .notable_locations()
                    .unwrap()
                    .locations
                    .iter()
                    .filter(|l| l.district.as_deref() == Some(district.id.as_str()))
                    .map(|l| l.id.clone())
                    .collect();
                assert_eq!(map.notable_locations, own);
            }
            assert_well_formed(&set);
            // Districts are generated independently: different geometry.
            assert_ne!(set.maps[0].seed, set.maps[1].seed);
            assert_ne!(set.maps[0].geometry, set.maps[1].geometry);
        }
    }
}

#[test]
fn a_city_without_recorded_districts_is_refused_and_nothing_is_invented() {
    let mut s = settlement("city", 3);
    for stage in &mut s.snapshot.stages {
        stage.locations = None;
    }
    let error = maps::generate_map_set(&BuiltinEngine, &s, None, &options(1)).unwrap_err();
    assert!(matches!(error, MapError::NoDistricts(_)), "{error}");
}

#[test]
fn a_settlement_without_a_size_cannot_be_mapped() {
    let mut s = settlement("town", 3);
    s.snapshot
        .stages
        .retain(|st| st.stage_table.as_str() != "table.size");
    s.snapshot.log.events.clear();
    let error = maps::generate_map_set(&BuiltinEngine, &s, None, &options(1)).unwrap_err();
    assert_eq!(error, MapError::NoSize);
}

// ----- inputs and parameters --------------------------------------------------

#[test]
fn map_inputs_are_the_recorded_settlement_facts() {
    let s = settlement("town", 5);
    let inputs = params::inputs_from_settlement(&s).unwrap();
    let stage = |table: &str| {
        s.snapshot
            .stages
            .iter()
            .find(|st| st.stage_table.as_str() == table)
            .and_then(|st| st.result.clone())
    };
    assert_eq!(inputs.settlement_name, "Test town");
    assert_eq!(Some(inputs.size.clone()), stage("table.size"));
    assert_eq!(inputs.size.id.as_str(), "town");
    assert_eq!(inputs.region, stage("table.genericRegion"));
    assert_eq!(inputs.environment, stage("table.environment"));
    assert_eq!(inputs.connections, stage("table.connections"));
    assert_eq!(inputs.origin, stage("table.origin"));
    assert_eq!(inputs.age, stage("table.age"));
    assert!(inputs.tags.contains(&TagId::new("size.town")));
    assert!(inputs.districts.is_empty());
    // No table records population, so none is made up.
    assert_eq!(inputs.population, None);
}

#[test]
fn mapping_never_changes_the_settlement() {
    let mut s = service();
    let id = saved(&mut s, "capital", 4);
    let before = s.saved_get(&id).unwrap();
    s.maps_generate(&id, &options(9)).unwrap();
    assert_eq!(s.saved_get(&id).unwrap(), before);
}

#[test]
fn map_defaults_only_refer_to_tags_that_exist() {
    let content = settlement_core::bundled::bundled_content().unwrap();
    for tag in map_defaults().unwrap().referenced_tags() {
        assert!(
            content.tags.iter().any(|t| t.id == tag),
            "map-defaults.json refers to unknown tag {tag}"
        );
    }
}

#[test]
fn defaults_follow_the_settlement_and_overrides_win() {
    let s = settlement("village", 2);
    let inputs = params::inputs_from_settlement(&s).unwrap();
    let defaults = params::default_parameters(&inputs).unwrap();
    let connection = inputs.connections.as_ref().unwrap().id.as_str().to_owned();
    let expected_roads = match connection.as_str() {
        "isolated" => 1,
        "localRoad" => 2,
        "regionalRoad" => 3,
        "majorRoute" => 4,
        other => panic!("unexpected connection {other}"),
    };
    assert_eq!(defaults.approaches, expected_roads);
    assert_eq!(defaults.building_count, 32);

    let set = maps::generate_map_set(
        &BuiltinEngine,
        &s,
        None,
        &with(
            MapParameterOverrides {
                building_count: Some(12),
                walls: Some(true),
                ..Default::default()
            },
            2,
        ),
    )
    .unwrap();
    let p = &set.maps[0].parameters;
    assert_eq!(p.building_count, 12);
    assert!(p.walls);
    assert_eq!(
        p.approaches, expected_roads,
        "untouched values keep the default"
    );
}

#[test]
fn bad_overrides_and_seeds_are_refused() {
    let s = settlement("town", 1);
    let bad = |o: MapParameterOverrides| {
        maps::generate_map_set(&BuiltinEngine, &s, None, &with(o, 1)).unwrap_err()
    };
    for o in [
        MapParameterOverrides {
            building_count: Some(0),
            ..Default::default()
        },
        MapParameterOverrides {
            building_count: Some(10_000),
            ..Default::default()
        },
        MapParameterOverrides {
            radius_m: Some(5.0),
            ..Default::default()
        },
        MapParameterOverrides {
            irregularity: Some(1.5),
            ..Default::default()
        },
    ] {
        assert!(matches!(bad(o), MapError::InvalidParameter(_)));
    }
    let mut unknown = MapOverrides::default();
    unknown.maps.insert(
        MapId::new("map-district-9"),
        MapParameterOverrides::default(),
    );
    let error = maps::generate_map_set(
        &BuiltinEngine,
        &s,
        None,
        &MapGenerationOptions {
            seed: SeedChoice::Keep,
            overrides: Some(unknown),
            only: None,
        },
    )
    .unwrap_err();
    assert!(matches!(error, MapError::UnknownMap(_)));
    let error =
        maps::generate_map_set(&BuiltinEngine, &s, None, &options(MAX_SEED + 1)).unwrap_err();
    assert_eq!(error, MapError::SeedTooLarge);
}

// ----- determinism ------------------------------------------------------------

#[test]
fn the_same_seed_and_inputs_give_the_same_geometry() {
    for size in SIZES {
        let s = settlement(size, 6);
        let a = maps::generate_map_set(&BuiltinEngine, &s, None, &options(77)).unwrap();
        let b = maps::generate_map_set(&BuiltinEngine, &s, None, &options(77)).unwrap();
        assert_eq!(a.maps, b.maps, "{size}");
        assert_eq!(
            serde_json::to_string(&a.maps).unwrap(),
            serde_json::to_string(&b.maps).unwrap()
        );
        let c = maps::generate_map_set(&BuiltinEngine, &s, None, &options(78)).unwrap();
        assert_ne!(a.maps[0].geometry, c.maps[0].geometry, "{size}");
    }
}

#[test]
fn a_saved_seed_survives_a_javascript_round_trip() {
    let set = maps::generate_map_set(
        &BuiltinEngine,
        &settlement("village", 1),
        None,
        &MapGenerationOptions::default(),
    )
    .unwrap();
    assert!(set.seed <= MAX_SEED);
    assert!(set.maps.iter().all(|m| m.seed <= MAX_SEED));
    assert_eq!((set.seed as f64) as u64, set.seed);
}

#[test]
fn regenerating_keeps_the_seed_and_issues_new_building_ids() {
    let mut s = service();
    let id = saved(&mut s, "town", 2);
    let first = s.maps_generate(&id, &options(5)).unwrap().maps.unwrap();
    let again = s
        .maps_generate(&id, &MapGenerationOptions::default())
        .unwrap()
        .maps
        .unwrap();
    assert_eq!(again.revision, 2);
    assert_eq!(again.seed, first.seed, "Keep reuses the saved seed");
    let (a, b) = (&first.maps[0].geometry, &again.maps[0].geometry);
    assert_eq!(a.buildings.len(), b.buildings.len());
    for (x, y) in a.buildings.iter().zip(&b.buildings) {
        assert_eq!(x.footprint, y.footprint, "same geometry");
        assert_ne!(
            x.id, y.id,
            "an ID is never reused for a regenerated building"
        );
        assert!(y.id.as_str().contains("-r2-"));
    }
    let fresh = s
        .maps_generate(
            &id,
            &MapGenerationOptions {
                seed: SeedChoice::Random,
                overrides: None,
                only: None,
            },
        )
        .unwrap()
        .maps
        .unwrap();
    assert_eq!(fresh.revision, 3);
}

// ----- persistence ------------------------------------------------------------

#[test]
fn saved_maps_are_read_back_exactly_not_redrawn() {
    let mut s = service();
    let id = saved(&mut s, "capital", 12);
    let generated = s.maps_generate(&id, &options(3)).unwrap();
    assert_eq!(generated.status, MapStatus::Current);
    let loaded = s.maps_get(&id).unwrap();
    assert_eq!(loaded.maps, generated.maps);
    let set = loaded.maps.unwrap();
    assert_eq!(set.format_version, MAP_FORMAT_VERSION);
    assert_eq!(set.overrides, MapOverrides::default());
}

#[test]
fn maps_report_when_the_settlement_has_changed_since() {
    let s = settlement("town", 3);
    let set = maps::generate_map_set(&BuiltinEngine, &s, None, &options(1)).unwrap();
    assert_eq!(maps::map_status(&s, Some(&set)), MapStatus::Current);
    assert_eq!(maps::map_status(&s, None), MapStatus::NotGenerated);
    let mut edited = s.clone();
    edited.summary.name = "Renamed".into();
    assert_eq!(maps::map_status(&edited, Some(&set)), MapStatus::Stale);
}

#[test]
fn deleting_maps_or_their_settlement_removes_them() {
    let mut s = service();
    let id = saved(&mut s, "village", 2);
    s.maps_generate(&id, &options(1)).unwrap();
    let view = s.maps_delete(&id).unwrap();
    assert_eq!(view.status, MapStatus::NotGenerated);
    assert!(view.maps.is_none());

    s.maps_generate(&id, &options(1)).unwrap();
    s.saved_delete(&id).unwrap();
    assert!(
        s.maps_get(&id).is_err(),
        "the settlement and its maps are gone"
    );
}

#[test]
fn a_database_cascades_map_rows_with_their_settlement() {
    let mut db = Database::open_in_memory().unwrap();
    let s = settlement("town", 4);
    let stored = db.create_settlement("Cascade", s.snapshot.clone()).unwrap();
    let mut set = maps::generate_map_set(&BuiltinEngine, &stored, None, &options(1)).unwrap();
    set.settlement_id = stored.summary.id.clone();
    db.save_map_set(&set).unwrap();
    assert!(db.load_map_set(&stored.summary.id).unwrap().is_some());
    db.delete_settlement(&stored.summary.id).unwrap();
    assert!(db.load_map_set(&stored.summary.id).unwrap().is_none());
}

// ----- engine output ----------------------------------------------------------

#[test]
fn small_and_large_maps() {
    let s = settlement("town", 7);
    let one = maps::generate_map_set(
        &BuiltinEngine,
        &s,
        None,
        &with(
            MapParameterOverrides {
                building_count: Some(1),
                radius_m: Some(20.0),
                approaches: Some(0),
                ..Default::default()
            },
            1,
        ),
    )
    .unwrap();
    assert_eq!(one.maps[0].geometry.buildings.len(), 1);
    assert_well_formed(&one);

    let start = std::time::Instant::now();
    let big = maps::generate_map_set(
        &BuiltinEngine,
        &s,
        None,
        &with(
            MapParameterOverrides {
                building_count: Some(params::MAX_BUILDINGS),
                radius_m: Some(params::MAX_RADIUS_M),
                approaches: Some(params::MAX_APPROACHES),
                water: Some(WaterKind::None),
                ..Default::default()
            },
            1,
        ),
    )
    .unwrap();
    let elapsed = start.elapsed();
    let map = &big.maps[0];
    assert_eq!(map.stats.buildings_placed, params::MAX_BUILDINGS);
    assert_eq!(map.stats.shortfall(), 0);
    assert_well_formed(&big);
    assert!(elapsed.as_secs() < 20, "took {elapsed:?}");
}

#[test]
fn bounds_grow_with_the_settlement() {
    let width = |size| map_set(size, 4).maps[0].geometry.bounds.width();
    // The frame fits what was built (a linear village can be wider than a
    // compact walled town, so only clear cases are compared).
    assert!(width("tradingPost") < width("town"));
    assert!(width("tradingPost") < width("village"));
    assert!(width("town") < width("capital"));
}

#[test]
fn a_crowded_map_reports_its_shortfall_honestly() {
    let set = maps::generate_map_set(
        &BuiltinEngine,
        &settlement("village", 3),
        None,
        &with(
            MapParameterOverrides {
                building_count: Some(400),
                radius_m: Some(30.0),
                ..Default::default()
            },
            3,
        ),
    )
    .unwrap();
    let stats = set.maps[0].stats;
    assert_eq!(stats.buildings_requested, 400);
    assert!(stats.shortfall() > 0);
    assert_eq!(
        stats.buildings_placed as usize,
        set.maps[0].geometry.buildings.len()
    );
}

#[test]
fn water_walls_and_terrain_are_drawn_when_asked_for() {
    let s = settlement("town", 9);
    for (water, seed) in [
        (WaterKind::Coast, 1),
        (WaterKind::River, 2),
        (WaterKind::Lake, 3),
    ] {
        let set = maps::generate_map_set(
            &BuiltinEngine,
            &s,
            None,
            &with(
                MapParameterOverrides {
                    water: Some(water),
                    walls: Some(true),
                    approaches: Some(3),
                    fields: Some(true),
                    trees: Some(0.5),
                    ..Default::default()
                },
                seed,
            ),
        )
        .unwrap();
        assert_well_formed(&set);
        let g = &set.maps[0].geometry;
        assert_eq!(g.water.len(), 1, "{water:?}");
        assert_eq!(g.water[0].kind, water);
        if water == WaterKind::River {
            assert!(!g.water[0].centreline.is_empty());
        }
        assert_eq!(g.walls.len(), 1, "{water:?}");
        assert!(!g.walls[0].gates.is_empty(), "roads pass through gates");
        assert!(!g.terrain.trees.is_empty());
        assert!(!g.terrain.squares.is_empty(), "towns have a square");
    }

    let open = maps::generate_map_set(
        &BuiltinEngine,
        &s,
        None,
        &with(
            MapParameterOverrides {
                water: Some(WaterKind::None),
                walls: Some(false),
                trees: Some(0.0),
                ..Default::default()
            },
            1,
        ),
    )
    .unwrap();
    let g = &open.maps[0].geometry;
    assert!(g.water.is_empty() && g.walls.is_empty() && g.terrain.trees.is_empty());
}

#[test]
fn every_size_maps_across_many_seeds() {
    for size in SIZES {
        for seed in 0..6 {
            assert_well_formed(&map_set(size, seed));
        }
    }
}

/// The GUI's hand-written types must declare every field the core sends.
#[test]
fn the_gui_map_types_declare_every_serialised_field() {
    fn keys(v: &serde_json::Value, out: &mut std::collections::BTreeSet<String>) {
        match v {
            serde_json::Value::Object(m) => {
                for (k, v) in m {
                    out.insert(k.clone());
                    keys(v, out);
                }
            }
            serde_json::Value::Array(a) => a.iter().for_each(|v| keys(v, out)),
            _ => {}
        }
    }
    let mut s = service();
    let id = saved(&mut s, "town", 9);
    let view = s
        .maps_generate(
            &id,
            &with(
                MapParameterOverrides {
                    water: Some(WaterKind::River),
                    walls: Some(true),
                    fields: Some(true),
                    trees: Some(0.5),
                    ..Default::default()
                },
                1,
            ),
        )
        .unwrap();
    let mut found = std::collections::BTreeSet::new();
    keys(&serde_json::to_value(&view).unwrap(), &mut found);
    // `inputs.size` etc. are `Named` and `District`, declared in api.ts.
    for k in ["id", "name", "label"] {
        found.remove(k);
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let ts = std::fs::read_to_string(root.join("app/src/maps/types.ts")).unwrap();
    for key in found {
        assert!(
            ts.contains(&format!("  {key}: ")) || ts.contains(&format!("  {key}?: ")),
            "app/src/maps/types.ts does not declare `{key}`"
        );
    }
}

// ----- failure safety ---------------------------------------------------------

struct Failing;
impl MapEngineAdapter for Failing {
    fn info(&self) -> EngineInfo {
        EngineInfo {
            id: "test.failing".into(),
            version: "0".into(),
            features: Vec::new(),
        }
    }
    fn generate(&self, _: &MapGenerationRequest) -> Result<MapGenerationResult, MapEngineError> {
        Err(MapEngineError::Failed("out of ink".into()))
    }
}

struct Panicking;
impl MapEngineAdapter for Panicking {
    fn info(&self) -> EngineInfo {
        Failing.info()
    }
    fn generate(&self, _: &MapGenerationRequest) -> Result<MapGenerationResult, MapEngineError> {
        panic!("engine bug")
    }
}

/// Returns the built-in result for the first map, then empty geometry.
struct Broken;
impl MapEngineAdapter for Broken {
    fn info(&self) -> EngineInfo {
        BuiltinEngine.info()
    }
    fn generate(&self, r: &MapGenerationRequest) -> Result<MapGenerationResult, MapEngineError> {
        let mut result = BuiltinEngine.generate(r)?;
        if r.map_id.as_str() != "map-district-1" {
            result.geometry.buildings.clear();
            result.stats.buildings_placed = 0;
        }
        Ok(result)
    }
}

#[test]
fn a_failed_generation_changes_nothing() {
    let mut s = service();
    let id = saved(&mut s, "capital", 5);
    let settlement_before = s.saved_get(&id).unwrap();
    let maps_before = s.maps_generate(&id, &options(1)).unwrap().maps;

    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let outcomes = [
        s.maps_generate_with(&Failing, &id, &options(2)),
        s.maps_generate_with(&Panicking, &id, &options(2)),
        s.maps_generate_with(&Broken, &id, &options(2)),
    ];
    std::panic::set_hook(hook);

    for outcome in outcomes {
        let error = outcome.unwrap_err();
        assert!(error.contains("could not be generated"), "{error}");
    }
    assert_eq!(s.saved_get(&id).unwrap(), settlement_before);
    let after = s.maps_get(&id).unwrap();
    assert_eq!(
        after.maps, maps_before,
        "earlier maps are kept, revision unchanged"
    );
    assert_eq!(after.status, MapStatus::Current);
}

#[test]
fn a_failed_first_generation_leaves_no_maps() {
    let mut s = service();
    let id = saved(&mut s, "village", 5);
    assert!(s.maps_generate_with(&Failing, &id, &options(1)).is_err());
    let view = s.maps_get(&id).unwrap();
    assert_eq!(view.status, MapStatus::NotGenerated);
    assert!(view.maps.is_none());
}

#[test]
fn engine_output_is_checked_before_it_is_stored() {
    let s = settlement("town", 1);
    let inputs = params::inputs_from_settlement(&s).unwrap();
    let (_, requests) = params::plan_requests(&s, &inputs, &MapOverrides::default(), 1, 1).unwrap();
    let request = &requests[0];
    let good = BuiltinEngine.generate(request).unwrap();
    assert!(validate_result(request, &good).is_ok());

    let mut duplicate = good.clone();
    let first = duplicate.geometry.buildings[0].id.clone();
    duplicate.geometry.buildings[1].id = first;
    assert!(validate_result(request, &duplicate).is_err());

    let mut foreign = good.clone();
    foreign.geometry.buildings[0].map_id = MapId::new("map-elsewhere");
    assert!(validate_result(request, &foreign).is_err());

    let mut outside = good.clone();
    outside.geometry.buildings[0].footprint[0].0 = 1.0e9;
    assert!(validate_result(request, &outside).is_err());

    let mut miscounted = good;
    miscounted.stats.buildings_placed += 1;
    assert!(validate_result(request, &miscounted).is_err());
}

// ----- single districts -------------------------------------------------------

fn only(map: &str) -> MapGenerationOptions {
    MapGenerationOptions {
        seed: SeedChoice::Fixed(42),
        overrides: None,
        only: Some(MapId::new(map)),
    }
}

#[test]
fn a_district_can_be_generated_on_its_own() {
    let mut s = service();
    let id = saved(&mut s, "capital", 5);
    let view = s.maps_get(&id).unwrap();
    assert!(view.planned.len() >= 3, "a Capital has 3–5 districts");
    assert!(view.planned.iter().all(|p| !p.generated));
    // District names and IDs are the settlement's own.
    let first = &view.planned[0];
    assert_eq!(first.id.as_str(), "map-district-1");
    assert_eq!(first.label, "District 1");

    let view = s.maps_generate(&id, &only("map-district-2")).unwrap();
    let set = view.maps.clone().unwrap();
    assert_eq!(set.maps.len(), 1);
    assert_eq!(set.maps[0].id.as_str(), "map-district-2");
    let generated: Vec<bool> = view.planned.iter().map(|p| p.generated).collect();
    assert_eq!(generated[..2], [false, true]);
    assert_eq!(view.status, MapStatus::Current);

    // Another district later: the first is kept exactly as it was.
    let view = s.maps_generate(&id, &only("map-district-1")).unwrap();
    let set2 = view.maps.unwrap();
    assert_eq!(set2.maps.len(), 2);
    assert_eq!(set2.maps[0].id.as_str(), "map-district-1", "planned order");
    assert_eq!(set2.map(&MapId::new("map-district-2")), Some(&set.maps[0]));
    assert_eq!(set2.revision, 2);
}

#[test]
fn a_district_drawn_alone_matches_the_same_district_drawn_with_the_others() {
    let s = settlement("city", 8);
    let all = maps::generate_map_set(&BuiltinEngine, &s, None, &options(42)).unwrap();
    let one = maps::generate_map_set(&BuiltinEngine, &s, None, &only("map-district-2")).unwrap();
    let a = all.map(&MapId::new("map-district-2")).unwrap();
    assert_eq!(one.maps[0].seed, a.seed);
    assert_eq!(one.maps[0].geometry, a.geometry);
}

#[test]
fn only_a_planned_map_can_be_generated_alone() {
    let s = settlement("town", 1);
    let error =
        maps::generate_map_set(&BuiltinEngine, &s, None, &only("map-district-1")).unwrap_err();
    assert!(matches!(error, MapError::UnknownMap(_)));
}

#[test]
fn each_map_knows_when_the_settlement_has_changed_since() {
    let s = settlement("city", 3);
    let set = maps::generate_map_set(&BuiltinEngine, &s, None, &only("map-district-1")).unwrap();
    let mut edited = s.clone();
    edited.summary.name = "Renamed".into();
    assert_eq!(
        maps::stale_maps(&edited, &set),
        vec![MapId::new("map-district-1")]
    );
    // Drawing another district after the edit: only the older map is stale.
    let set = maps::generate_map_set(&BuiltinEngine, &edited, Some(&set), &only("map-district-2"))
        .unwrap();
    assert_eq!(
        maps::stale_maps(&edited, &set),
        vec![MapId::new("map-district-1")]
    );
    assert_eq!(maps::map_status(&edited, Some(&set)), MapStatus::Stale);
}
