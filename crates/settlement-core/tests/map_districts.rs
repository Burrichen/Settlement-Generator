//! District maps of Cities and Capitals: which settlements get which maps,
//! district names, deleting one district's map safely, points of interest,
//! saved views, lazy loading, and everything surviving a restart.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use settlement_core::db::Database;
use settlement_core::dice::RandomFaces;
use settlement_core::engine::Selections;
use settlement_core::ids::{MapId, ResultId};
use settlement_core::maps::geometry::Point;
use settlement_core::maps::{
    AssignOptions, MapGenerationOptions, MapLayout, MapScope, MarkerTarget, PlaceOn, SavedView,
    SeedChoice,
};
use settlement_core::service::{AppService, SettlementMapsView};
use settlement_core::settlements::SettlementId;

fn service() -> AppService {
    AppService::new(Database::open_in_memory().unwrap())
}

fn saved(s: &mut AppService, size: &str, seed: u64) -> SettlementId {
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
    s.save_new(&active, &format!("Test {size}")).unwrap().id
}

fn fixed() -> MapGenerationOptions {
    MapGenerationOptions {
        seed: SeedChoice::Fixed(3),
        ..Default::default()
    }
}

fn only(map: &MapId) -> MapGenerationOptions {
    MapGenerationOptions {
        only: Some(map.clone()),
        ..fixed()
    }
}

/// A Capital with at least three districts, all drawn.
fn capital(s: &mut AppService) -> (SettlementId, SettlementMapsView) {
    for seed in 1.. {
        let id = saved(s, "capital", seed);
        let view = s.maps_get(&id).unwrap();
        if view.planned.len() >= 3 {
            let view = s.maps_generate(&id, &fixed()).unwrap();
            return (id, view);
        }
    }
    unreachable!()
}

fn map_index(view: &SettlementMapsView, map: &MapId) -> usize {
    view.maps
        .as_ref()
        .unwrap()
        .maps
        .iter()
        .position(|m| &m.id == map)
        .unwrap()
}

#[test]
fn small_settlements_get_one_whole_map_and_cities_one_per_district() {
    let mut s = service();
    for (size, layout) in [
        ("tradingPost", MapLayout::Single),
        ("village", MapLayout::Single),
        ("town", MapLayout::Single),
        ("city", MapLayout::PerDistrict),
        ("capital", MapLayout::PerDistrict),
    ] {
        let id = saved(&mut s, size, 4);
        let view = s.maps_get(&id).unwrap();
        assert_eq!(view.layout, Some(layout), "{size}");
        match layout {
            MapLayout::Single => {
                assert_eq!(view.planned.len(), 1, "{size}");
                assert_eq!(view.planned[0].id.as_str(), "map-settlement");
                assert_eq!(view.planned[0].scope, MapScope::Settlement);
            }
            MapLayout::PerDistrict => {
                assert!(view.planned.len() >= 2, "{size}");
                for (i, p) in view.planned.iter().enumerate() {
                    let district = format!("district-{}", i + 1);
                    assert_eq!(p.id.as_str(), format!("map-{district}"));
                    let MapScope::District { district: d } = &p.scope else {
                        panic!("{size} maps are district maps");
                    };
                    assert_eq!(d.id, district);
                }
                assert!(
                    view.planned
                        .iter()
                        .all(|p| p.id.as_str() != "map-settlement")
                );
            }
        }
    }
}

#[test]
fn districts_are_drawn_one_at_a_time_and_never_reroll_the_settlement() {
    let mut s = service();
    let id = saved(&mut s, "capital", 2);
    let before = s.saved_get(&id).unwrap();
    let view = s.maps_get(&id).unwrap();
    // Opening a Capital draws nothing and generates no building details.
    assert!(view.maps.is_none() && view.buildings.is_empty());
    let first = view.planned[0].id.clone();
    let view = s.maps_generate(&id, &only(&first)).unwrap();
    let drawn: Vec<bool> = view.planned.iter().map(|p| p.generated).collect();
    assert!(drawn[0] && drawn[1..].iter().all(|d| !d));
    assert!(view.buildings.is_empty(), "no building details either");
    assert_eq!(
        s.saved_get(&id).unwrap(),
        before,
        "the settlement is unchanged"
    );
}

#[test]
fn renaming_a_district_changes_only_its_name() {
    let mut s = service();
    let (id, view) = capital(&mut s);
    let map = view.planned[1].clone();
    let MapScope::District { district } = &map.scope else {
        unreachable!()
    };
    let building = view.maps.as_ref().unwrap().maps[1].geometry.buildings[0]
        .id
        .clone();
    let details = s.maps_building(&id, &map.id, &building).unwrap();
    // Something of every kind in the district: a placed location and a point.
    let loc = view
        .locations
        .iter()
        .find(|l| l.district.as_ref() == Some(district))
        .unwrap()
        .id
        .clone();
    s.maps_location_assign(
        &id,
        &loc,
        &PlaceOn {
            map_id: map.id.clone(),
            building_id: building.clone(),
        },
        AssignOptions::default(),
    )
    .unwrap();
    let at = view.maps.as_ref().unwrap().maps[1].geometry.buildings[1].centroid;
    s.maps_poi_add(&id, &map.id, at, "Well".into(), String::new())
        .unwrap();

    let renamed = s
        .maps_district_rename(&id, &district.id, Some("  Harbour Ward ".into()))
        .unwrap();
    // Every view of the district uses the new name.
    let l = renamed.locations.iter().find(|l| l.id == loc).unwrap();
    assert_eq!(l.district.as_ref().unwrap().label, "Harbour Ward");
    let placed = l.placement.as_ref().unwrap();
    assert_eq!(placed.map_label, "Harbour Ward");
    assert_eq!(placed.district.as_ref().unwrap().label, "Harbour Ward");
    assert_eq!(renamed.pois[0].map_label, "Harbour Ward");
    let legend = renamed.legends.iter().find(|g| g.map_id == map.id).unwrap();
    assert!(
        legend
            .entries
            .iter()
            .all(|e| e.district.as_ref().unwrap().label == "Harbour Ward")
    );
    let p = &renamed.planned[1];
    assert_eq!(p.label, "Harbour Ward");
    assert_eq!(p.default_label.as_deref(), Some(district.label.as_str()));
    assert_eq!(p.id, map.id, "the map ID is unchanged");
    let shown = &renamed.maps.as_ref().unwrap().maps[1];
    assert_eq!(shown.label, "Harbour Ward");
    assert_eq!(shown.district().unwrap().id, district.id);
    // Saved building records still open, by the same IDs.
    let after = s.maps_building(&id, &map.id, &building).unwrap();
    assert_eq!(after.building_id, details.building_id);
    assert_eq!(after.version, details.version);
    assert_eq!(after.map_label, "Harbour Ward");
    assert_eq!(after.building_type, details.building_type);
    // Drawing another district keeps the name and this map as it was.
    let other = renamed.planned[0].id.clone();
    let redrawn = s.maps_generate(&id, &only(&other)).unwrap();
    assert_eq!(redrawn.planned[1].label, "Harbour Ward");
    assert_eq!(
        redrawn.maps.as_ref().unwrap().maps[1].geometry,
        shown.geometry
    );
    // Blank gives back the default; an unknown district is refused.
    let reset = s
        .maps_district_rename(&id, &district.id, Some(" ".into()))
        .unwrap();
    assert_eq!(reset.planned[1].label, district.label);
    assert!(reset.planned[1].default_label.is_none());
    assert!(
        s.maps_district_rename(&id, "district-99", Some("Nowhere".into()))
            .is_err()
    );
    let long = "x".repeat(61);
    assert!(
        s.maps_district_rename(&id, &district.id, Some(long))
            .is_err()
    );
}

#[test]
fn points_of_interest_share_the_marker_sequence_and_appear_in_the_directory() {
    let mut s = service();
    let (id, view) = capital(&mut s);
    let set = view.maps.as_ref().unwrap();
    let (m1, m2) = (&set.maps[0], &set.maps[1]);
    // A location placed first takes number 1.
    let loc = view
        .locations
        .iter()
        .find(|l| l.district.as_ref() == m1.district())
        .unwrap();
    s.maps_location_assign(
        &id,
        &loc.id,
        &PlaceOn {
            map_id: m1.id.clone(),
            building_id: m1.geometry.buildings[0].id.clone(),
        },
        AssignOptions::default(),
    )
    .unwrap();
    let at = m2.geometry.buildings[2].centroid;
    let view = s
        .maps_poi_add(&id, &m2.id, at, "Old well".into(), "Dry.".into())
        .unwrap();
    let poi = &view.pois[0];
    assert_eq!((poi.number, poi.label.as_str()), (2, "Old well"));
    assert_eq!(poi.district.as_ref(), m2.district());
    assert_eq!(poi.map_id, m2.id);
    let legend = &view.legends[map_index(&view, &m2.id)];
    assert_eq!(legend.entries.len(), 1);
    assert_eq!(legend.entries[0].number, 2);
    assert!(matches!(
        legend.entries[0].target,
        MarkerTarget::PointOfInterest { .. }
    ));
    assert_eq!(legend.markers[0].position, at);

    // Moving keeps its number; outside the map is refused.
    let moved = s
        .maps_poi_move(&id, poi.id.as_str(), m2.geometry.buildings[0].centroid)
        .unwrap();
    assert_eq!(moved.pois[0].number, 2);
    assert!(
        s.maps_poi_move(&id, poi.id.as_str(), Point(1.0e6, 0.0))
            .is_err()
    );
    assert!(
        s.maps_poi_add(&id, &m2.id, at, "   ".into(), String::new())
            .is_err()
    );
    // Renamed; then deleted, its number is not reused.
    let renamed = s
        .maps_poi_update(&id, poi.id.as_str(), "New well".into(), String::new())
        .unwrap();
    assert_eq!(renamed.pois[0].label, "New well");
    s.maps_poi_delete(&id, poi.id.as_str()).unwrap();
    let again = s
        .maps_poi_add(&id, &m2.id, at, "Shrine".into(), String::new())
        .unwrap();
    assert_eq!(again.pois[0].number, 3);
}

#[test]
fn deleting_a_district_map_says_what_goes_with_it_and_keeps_everything_else() {
    let mut s = service();
    let (id, view) = capital(&mut s);
    let set = view.maps.as_ref().unwrap();
    let (keep, gone) = (set.maps[0].clone(), set.maps[1].clone());
    let building = gone.geometry.buildings[0].id.clone();
    s.maps_building(&id, &gone.id, &building).unwrap();
    let loc = view
        .locations
        .iter()
        .find(|l| l.district.as_ref() == gone.district())
        .unwrap()
        .id
        .clone();
    s.maps_location_assign(
        &id,
        &loc,
        &PlaceOn {
            map_id: gone.id.clone(),
            building_id: building.clone(),
        },
        AssignOptions::default(),
    )
    .unwrap();
    s.maps_poi_add(
        &id,
        &gone.id,
        gone.geometry.buildings[1].centroid,
        "Well".into(),
        String::new(),
    )
    .unwrap();
    s.maps_save_view(
        &id,
        &SavedView {
            map_id: gone.id.clone(),
            centre: gone.geometry.buildings[0].centroid,
            zoom: 2.0,
        },
    )
    .unwrap();
    s.maps_district_rename(
        &id,
        &gone.district().unwrap().id,
        Some("Harbour Ward".into()),
    )
    .unwrap();
    let before = s.maps_get(&id).unwrap();
    let a = before.planned[1].annotations;
    assert_eq!((a.building_details, a.locations, a.pois), (1, 1, 1));

    let after = s.maps_delete_map(&id, &gone.id).unwrap();
    assert!(!after.planned[1].generated);
    assert_eq!(
        after.planned[1].label, "Harbour Ward",
        "the district is kept"
    );
    let remaining = &after.maps.as_ref().unwrap().maps;
    assert!(remaining.iter().all(|m| m.id != gone.id));
    let kept = remaining.iter().find(|m| m.id == keep.id).unwrap();
    assert_eq!(kept.geometry, keep.geometry, "other districts untouched");
    assert!(after.buildings.is_empty());
    assert!(after.pois.is_empty());
    assert!(after.views.is_empty());
    let l = after.locations.iter().find(|l| l.id == loc).unwrap();
    assert!(l.placement.is_none(), "taken off");
    assert_eq!(l.marker, Some(1), "and kept, with its number");

    // Drawn again: new building IDs, nothing reattached.
    let redrawn = s.maps_generate(&id, &only(&gone.id)).unwrap();
    let new = &redrawn.maps.as_ref().unwrap().maps[map_index(&redrawn, &gone.id)];
    assert!(new.geometry.buildings.iter().all(|b| b.id != building));
    assert!(redrawn.buildings.is_empty() && redrawn.pois.is_empty());
    assert!(
        s.maps_delete_map(&id, &MapId::from("map-district-99"))
            .is_err()
    );
}

#[test]
fn saved_views_are_checked_and_listed() {
    let mut s = service();
    let (id, view) = capital(&mut s);
    let map = &view.maps.as_ref().unwrap().maps[0];
    let good = SavedView {
        map_id: map.id.clone(),
        centre: map.geometry.buildings[0].centroid,
        zoom: 1.5,
    };
    s.maps_save_view(&id, &good).unwrap();
    s.maps_save_view(
        &id,
        &SavedView {
            zoom: 3.0,
            ..good.clone()
        },
    )
    .unwrap();
    assert_eq!(
        s.maps_get(&id).unwrap().views,
        [SavedView {
            zoom: 3.0,
            ..good.clone()
        }]
    );
    for bad in [
        SavedView {
            zoom: 0.0,
            ..good.clone()
        },
        SavedView {
            zoom: f64::NAN,
            ..good.clone()
        },
        SavedView {
            centre: Point(1.0e6, 0.0),
            ..good.clone()
        },
        SavedView {
            map_id: MapId::from("map-district-99"),
            ..good.clone()
        },
    ] {
        assert!(s.maps_save_view(&id, &bad).is_err(), "{bad:?}");
    }
}

struct TempDb(PathBuf);

impl TempDb {
    fn new() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        TempDb(std::env::temp_dir().join(format!(
            "settlement-districts-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )))
    }

    fn service(&self) -> AppService {
        AppService::new(Database::open(&self.0.join("data.sqlite3")).unwrap())
    }
}

impl Drop for TempDb {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn every_districts_maps_names_points_and_views_survive_a_restart() {
    let tmp = TempDb::new();
    let (id, before) = {
        let mut s = tmp.service();
        let (id, view) = capital(&mut s);
        let set = view.maps.as_ref().unwrap();
        for (i, map) in set.maps.iter().enumerate() {
            let district = map.district().unwrap().id.clone();
            s.maps_district_rename(&id, &district, Some(format!("Ward {i}")))
                .unwrap();
            s.maps_poi_add(
                &id,
                &map.id,
                map.geometry.buildings[0].centroid,
                format!("Mark {i}"),
                String::new(),
            )
            .unwrap();
            s.maps_save_view(
                &id,
                &SavedView {
                    map_id: map.id.clone(),
                    centre: map.geometry.buildings[1].centroid,
                    zoom: 1.0 + i as f64,
                },
            )
            .unwrap();
        }
        let view = s.maps_get(&id).unwrap();
        (id, view)
    };
    let after = tmp.service().maps_get(&id).unwrap();
    assert_eq!(after, before);
    let numbers: Vec<u32> = after.pois.iter().map(|p| p.number).collect();
    assert_eq!(
        numbers,
        (1..=after.planned.len() as u32).collect::<Vec<_>>()
    );
    assert_eq!(after.views.len(), after.planned.len());
    assert!(
        after
            .planned
            .iter()
            .enumerate()
            .all(|(i, p)| p.label == format!("Ward {i}"))
    );
}
