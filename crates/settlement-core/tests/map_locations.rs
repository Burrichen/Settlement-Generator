//! The Notable Location directory on the maps: placing locations on
//! buildings, persistent marker numbers, the legend, moving, replacing and
//! unplacing, renaming, custom locations, districts and persistence.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use settlement_core::db::Database;
use settlement_core::dice::RandomFaces;
use settlement_core::engine::Selections;
use settlement_core::ids::{BuildingId, MapId, ResultId};
use settlement_core::maps::directory::{LocationAssignment, MAX_LOCATION_NAME_CHARS};
use settlement_core::maps::{
    AssignOptions, BuildingEdits, LocationEdit, LocationEntry, LocationOrigin, LocationRecord,
    MapGenerationOptions, MarkerTarget, PlaceOn, SeedChoice, UserValue,
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

fn draw(s: &mut AppService, id: &SettlementId) -> SettlementMapsView {
    s.maps_generate(
        id,
        &MapGenerationOptions {
            seed: SeedChoice::Fixed(1),
            ..Default::default()
        },
    )
    .unwrap()
}

/// A mapped Town with at least two Notable Locations.
fn town(s: &mut AppService) -> SettlementId {
    for seed in 1.. {
        let id = saved(s, "town", seed);
        if draw(s, &id).locations.len() >= 2 {
            return id;
        }
    }
    unreachable!()
}

/// Building `n` of the map `map` (index into the saved maps).
fn on(view: &SettlementMapsView, map: usize, n: usize) -> PlaceOn {
    let m = &view.maps.as_ref().unwrap().maps[map];
    PlaceOn {
        map_id: m.id.clone(),
        building_id: m.geometry.buildings[n].id.clone(),
    }
}

fn entry<'v>(view: &'v SettlementMapsView, id: &str) -> &'v LocationEntry {
    view.locations.iter().find(|l| l.id == id).unwrap()
}

const PLAIN: AssignOptions = AssignOptions {
    move_location: false,
    replace_occupant: false,
};

#[test]
fn the_directory_lists_the_settlements_own_locations_unplaced() {
    let mut s = service();
    let id = town(&mut s);
    let view = s.maps_get(&id).unwrap();
    for l in &view.locations {
        assert_eq!(l.origin, LocationOrigin::Settlement);
        assert!(l.id.starts_with("notable-location-"), "{}", l.id);
        assert!(l.marker.is_none() && l.placement.is_none());
        assert_eq!(l.version, 0, "nothing is stored until the user acts");
        assert!(!l.name.is_empty() && !l.location_type.is_empty());
    }
    assert!(view.legends.iter().all(|l| l.markers.is_empty()));
}

#[test]
fn placing_a_location_numbers_it_and_shows_it_on_the_map_and_legend() {
    let mut s = service();
    let id = town(&mut s);
    let view = s.maps_get(&id).unwrap();
    let loc = view.locations[0].id.clone();
    let target = on(&view, 0, 3);
    let view = s.maps_location_assign(&id, &loc, &target, PLAIN).unwrap();

    let e = entry(&view, &loc);
    assert_eq!(e.marker, Some(1));
    let p = e.placement.as_ref().unwrap();
    assert_eq!(
        (&p.map_id, &p.building_id),
        (&target.map_id, &target.building_id)
    );
    assert!(!p.missing);

    let legend = &view.legends[0];
    assert_eq!(legend.markers.len(), 1);
    assert_eq!(legend.entries.len(), 1);
    let (marker, line) = (&legend.markers[0], &legend.entries[0]);
    assert_eq!((marker.number, line.number), (1, 1));
    assert_eq!(line.label, e.name);
    assert_eq!(
        line.location_type.as_deref(),
        Some(e.location_type.as_str())
    );
    let building = view.maps.as_ref().unwrap().maps[0]
        .building(&target.building_id)
        .unwrap();
    assert_eq!(marker.position, building.centroid);
    assert_eq!(
        marker.target,
        MarkerTarget::NotableLocation {
            location_id: loc.clone(),
            building_id: target.building_id.clone()
        }
    );
}

#[test]
fn marker_numbers_are_stable_and_leave_gaps() {
    let mut s = service();
    let id = town(&mut s);
    let view = s.maps_get(&id).unwrap();
    let (a, b) = (view.locations[0].id.clone(), view.locations[1].id.clone());
    s.maps_location_assign(&id, &a, &on(&view, 0, 0), PLAIN)
        .unwrap();
    s.maps_location_assign(&id, &b, &on(&view, 0, 1), PLAIN)
        .unwrap();

    // Unplacing A keeps B's number, and A's own when it is placed again.
    let after = s.maps_location_unassign(&id, &a).unwrap();
    assert_eq!(entry(&after, &b).marker, Some(2));
    assert_eq!(entry(&after, &a).marker, Some(1), "kept while unplaced");
    assert_eq!(after.legends[0].entries.len(), 1);
    let after = s
        .maps_location_assign(&id, &a, &on(&view, 0, 5), PLAIN)
        .unwrap();
    assert_eq!(entry(&after, &a).marker, Some(1));

    // A deleted custom location's number is never reused.
    let custom = LocationEdit {
        name: Some("Old Mill".into()),
        location_type: Some("Mill".into()),
        ..Default::default()
    };
    let with_c = s
        .maps_location_create(&id, custom.clone(), Some(&on(&view, 0, 2)), PLAIN)
        .unwrap();
    let c = with_c.locations.last().unwrap().clone();
    assert_eq!((c.origin, c.marker), (LocationOrigin::Custom, Some(3)));
    s.maps_location_delete(&id, &c.id).unwrap();
    let with_d = s
        .maps_location_create(&id, custom, Some(&on(&view, 0, 2)), PLAIN)
        .unwrap();
    let d = with_d.locations.last().unwrap();
    assert_eq!(d.marker, Some(4));
    assert_ne!(d.id, c.id, "custom IDs are not reused either");

    let numbers: Vec<u32> = with_d.legends[0].entries.iter().map(|e| e.number).collect();
    assert_eq!(numbers, [1, 2, 4]);
    let markers: Vec<u32> = with_d.legends[0].markers.iter().map(|m| m.number).collect();
    assert_eq!(markers, numbers, "markers and legend agree");
}

#[test]
fn a_location_and_a_building_are_never_doubled_up_by_accident() {
    let mut s = service();
    let id = town(&mut s);
    let view = s.maps_get(&id).unwrap();
    let (a, b) = (view.locations[0].id.clone(), view.locations[1].id.clone());
    let (one, two) = (on(&view, 0, 0), on(&view, 0, 1));
    s.maps_location_assign(&id, &a, &one, PLAIN).unwrap();

    // The same location on a second building: only as an explicit move.
    let error = s.maps_location_assign(&id, &a, &two, PLAIN).unwrap_err();
    assert!(error.contains("already on Building 1"), "{error}");
    let moved = s
        .maps_location_assign(
            &id,
            &a,
            &two,
            AssignOptions {
                move_location: true,
                ..PLAIN
            },
        )
        .unwrap();
    assert_eq!(
        entry(&moved, &a).placement.as_ref().unwrap().building_id,
        two.building_id
    );
    assert_eq!(entry(&moved, &a).marker, Some(1));
    assert_eq!(moved.legends[0].markers.len(), 1, "one marker, moved");

    // A second location on a taken building: only as an explicit replace.
    let error = s.maps_location_assign(&id, &b, &two, PLAIN).unwrap_err();
    assert!(error.contains("already holds"), "{error}");
    let replaced = s
        .maps_location_assign(
            &id,
            &b,
            &two,
            AssignOptions {
                replace_occupant: true,
                ..PLAIN
            },
        )
        .unwrap();
    assert!(entry(&replaced, &a).placement.is_none(), "taken off, kept");
    assert_eq!(entry(&replaced, &a).marker, Some(1));
    assert_eq!(entry(&replaced, &b).marker, Some(2));
    // Placing again where it already is changes nothing.
    let again = s.maps_location_assign(&id, &b, &two, PLAIN).unwrap();
    assert_eq!(entry(&again, &b).version, entry(&replaced, &b).version);
}

#[test]
fn the_database_itself_refuses_two_locations_on_one_building() {
    let snapshot = {
        let mut active = service().start_generation().unwrap();
        active
            .session
            .choose(0, ResultId::new("kurovia"), &mut RandomFaces::seeded(1))
            .unwrap();
        active.session.to_snapshot()
    };
    let mut db = Database::open_in_memory().unwrap();
    let settlement = db.create_settlement("Guard", snapshot).unwrap();
    let placed = |n: u32| {
        let mut r: LocationRecord = serde_json::from_value(serde_json::json!({
            "id": format!("custom-location-{n}"), "origin": "custom", "version": 0
        }))
        .unwrap();
        r.assignment = Some(LocationAssignment {
            map_id: MapId::from("map-settlement"),
            building_id: BuildingId::from("bld-settlement-r1-0001"),
            district: None,
            assigned_at: time::OffsetDateTime::UNIX_EPOCH,
        });
        r
    };
    let result = db.update_location_book(&settlement.summary.id, |book| {
        for n in [1, 2] {
            book.records
                .insert(format!("custom-location-{n}"), placed(n));
        }
        Ok::<_, ()>(())
    });
    assert!(result.is_err(), "the unique index catches it");
    assert!(
        db.load_location_book(&settlement.summary.id)
            .unwrap()
            .records
            .is_empty(),
        "and nothing was written"
    );
}

#[test]
fn unplacing_keeps_the_location_and_the_buildings_details() {
    let mut s = service();
    let id = town(&mut s);
    let view = s.maps_get(&id).unwrap();
    let loc = view.locations[0].id.clone();
    let target = on(&view, 0, 4);
    s.maps_building(&id, &target.map_id, &target.building_id)
        .unwrap();
    let details = s
        .maps_building_edit(
            &id,
            &target.map_id,
            &target.building_id,
            1,
            BuildingEdits {
                name: Some(UserValue::new(Some("The Lamb".into()))),
                ..Default::default()
            },
        )
        .unwrap();
    s.maps_location_update(
        &id,
        &loc,
        0,
        LocationEdit {
            name: Some("The Lamb and Flag".into()),
            notes: "Meeting place.".into(),
            ..Default::default()
        },
    )
    .unwrap();
    s.maps_location_assign(&id, &loc, &target, PLAIN).unwrap();
    let view = s.maps_location_unassign(&id, &loc).unwrap();

    let e = entry(&view, &loc);
    assert!(e.placement.is_none());
    assert_eq!(e.name, "The Lamb and Flag");
    assert_eq!(e.notes, "Meeting place.");
    assert_eq!(e.marker, Some(1));
    let after = s
        .maps_building(&id, &target.map_id, &target.building_id)
        .unwrap();
    assert_eq!(after, details, "the building's details are untouched");
    assert!(
        s.maps_location_unassign(&id, &loc).is_err(),
        "not placed now"
    );
}

#[test]
fn renaming_and_notes_check_the_version_and_the_kind_of_location() {
    let mut s = service();
    let id = town(&mut s);
    let view = s.maps_get(&id).unwrap();
    let loc = view.locations[0].id.clone();
    let default = view.locations[0].default_name.clone();
    let rename = |name: &str| LocationEdit {
        name: Some(name.into()),
        ..Default::default()
    };
    let view = s
        .maps_location_update(&id, &loc, 0, rename("The Bell"))
        .unwrap();
    assert_eq!(entry(&view, &loc).name, "The Bell");
    assert!(entry(&view, &loc).renamed);
    assert_eq!(entry(&view, &loc).version, 1);
    // From a stale copy: refused.
    assert!(
        s.maps_location_update(&id, &loc, 0, rename("Stale"))
            .is_err()
    );
    // Blank: back to the default name.
    let view = s.maps_location_update(&id, &loc, 1, rename("  ")).unwrap();
    assert_eq!(entry(&view, &loc).name, default);
    // A settlement location's type is the settlement's.
    let retype = LocationEdit {
        location_type: Some("Castle".into()),
        ..Default::default()
    };
    assert!(s.maps_location_update(&id, &loc, 2, retype).is_err());
    let long = rename(&"x".repeat(MAX_LOCATION_NAME_CHARS + 1));
    assert!(s.maps_location_update(&id, &loc, 2, long).is_err());

    // A custom one's type is the user's.
    let view = s
        .maps_location_create(
            &id,
            LocationEdit {
                location_type: Some("Castle".into()),
                ..Default::default()
            },
            None,
            PLAIN,
        )
        .unwrap();
    let c = view.locations.last().unwrap();
    assert_eq!(
        (c.name.as_str(), c.location_type.as_str()),
        ("Castle", "Castle")
    );
    assert!(c.placement.is_none() && c.marker.is_none());
    let view = s
        .maps_location_update(
            &id,
            &c.id.clone(),
            c.version,
            LocationEdit {
                name: Some("Blackrock Keep".into()),
                location_type: Some("Castle".into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(view.locations.last().unwrap().name, "Blackrock Keep");
    // The settlement's own cannot be deleted.
    assert!(s.maps_location_delete(&id, &loc).is_err());
}

/// A mapped City with Notable Locations in at least two districts.
fn city(s: &mut AppService) -> (SettlementId, SettlementMapsView) {
    for seed in 1.. {
        let id = saved(s, "city", seed);
        let view = draw(s, &id);
        let districts: std::collections::BTreeSet<_> = view
            .locations
            .iter()
            .filter_map(|l| l.district.as_ref().map(|d| d.id.clone()))
            .collect();
        if districts.len() >= 2 {
            return (id, view);
        }
    }
    unreachable!()
}

#[test]
fn district_locations_stand_in_their_district_and_numbers_are_settlement_wide() {
    let mut s = service();
    let (id, view) = city(&mut s);
    let set = view.maps.as_ref().unwrap();
    let index_of = |district: &str| {
        set.maps
            .iter()
            .position(|m| m.district().is_some_and(|d| d.id == district))
            .unwrap()
    };
    let first = view
        .locations
        .iter()
        .find(|l| l.district.is_some())
        .unwrap();
    let home = first.district.clone().unwrap();
    let other = view
        .locations
        .iter()
        .find(|l| l.district.as_ref().is_some_and(|d| d.id != home.id))
        .unwrap();
    let away = other.district.clone().unwrap();

    // Not on another district's map.
    let error = s
        .maps_location_assign(&id, &first.id, &on(&view, index_of(&away.id), 0), PLAIN)
        .unwrap_err();
    assert!(error.contains(&home.label), "{error}");

    s.maps_location_assign(&id, &first.id, &on(&view, index_of(&home.id), 0), PLAIN)
        .unwrap();
    let view2 = s
        .maps_location_assign(&id, &other.id, &on(&view, index_of(&away.id), 0), PLAIN)
        .unwrap();
    // One sequence for the settlement: no two districts share a number.
    assert_eq!(entry(&view2, &first.id).marker, Some(1));
    assert_eq!(entry(&view2, &other.id).marker, Some(2));
    let legend = |d: &str| {
        let map = &set.maps[index_of(d)].id;
        view2.legends.iter().find(|l| &l.map_id == map).unwrap()
    };
    assert_eq!(legend(&home.id).entries[0].number, 1);
    assert_eq!(legend(&home.id).entries[0].district.as_ref(), Some(&home));
    assert_eq!(legend(&away.id).entries[0].number, 2);
    // The settlement-wide directory has every district's locations.
    let p = entry(&view2, &other.id).placement.as_ref().unwrap();
    assert_eq!(p.district.as_ref(), Some(&away));
}

#[test]
fn deleting_or_redrawing_maps_never_attaches_a_location_to_another_building() {
    let mut s = service();
    let id = town(&mut s);
    let view = s.maps_get(&id).unwrap();
    let loc = view.locations[0].id.clone();
    s.maps_location_assign(&id, &loc, &on(&view, 0, 0), PLAIN)
        .unwrap();

    // Redrawn: new building IDs, so the placement is shown as missing.
    let redrawn = draw(&mut s, &id);
    let p = entry(&redrawn, &loc).placement.as_ref().unwrap();
    assert!(p.missing);
    assert!(redrawn.legends[0].markers.is_empty());
    // It can be placed again, without "move", keeping its number.
    let again = s
        .maps_location_assign(&id, &loc, &on(&redrawn, 0, 0), PLAIN)
        .unwrap();
    assert_eq!(entry(&again, &loc).marker, Some(1));

    // Deleted: placements are taken off (a later drawing reuses IDs).
    let gone = s.maps_delete(&id).unwrap();
    assert!(entry(&gone, &loc).placement.is_none());
    assert_eq!(entry(&gone, &loc).marker, Some(1));
    let fresh = draw(&mut s, &id);
    assert!(fresh.legends[0].markers.is_empty());
}

struct TempDb(PathBuf);

impl TempDb {
    fn new() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        TempDb(std::env::temp_dir().join(format!(
            "settlement-locations-test-{}-{}",
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
fn placements_names_and_numbers_survive_a_restart() {
    let tmp = TempDb::new();
    let (id, before) = {
        let mut s = tmp.service();
        let id = town(&mut s);
        let view = s.maps_get(&id).unwrap();
        let (a, b) = (view.locations[0].id.clone(), view.locations[1].id.clone());
        s.maps_location_assign(&id, &a, &on(&view, 0, 2), PLAIN)
            .unwrap();
        s.maps_location_assign(&id, &b, &on(&view, 0, 6), PLAIN)
            .unwrap();
        s.maps_location_unassign(&id, &a).unwrap();
        let before = s
            .maps_location_update(
                &id,
                &b,
                1,
                LocationEdit {
                    name: Some("Guildhall".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        (id, before)
    };
    let s = tmp.service();
    let after = s.maps_get(&id).unwrap();
    assert_eq!(after.locations, before.locations);
    assert_eq!(after.legends, before.legends);
    assert_eq!(after.legends[0].entries[0].number, 2);
    assert_eq!(after.legends[0].entries[0].label, "Guildhall");
}

#[test]
fn the_gui_map_types_declare_every_location_field() {
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
    let (id, view) = city(&mut s);
    let loc = view
        .locations
        .iter()
        .find(|l| l.district.is_some())
        .unwrap();
    let set = view.maps.as_ref().unwrap();
    let map = set
        .maps
        .iter()
        .position(|m| m.district() == loc.district.as_ref())
        .unwrap();
    s.maps_location_update(
        &id,
        &loc.id,
        0,
        LocationEdit {
            name: Some("x".into()),
            notes: "y".into(),
            ..Default::default()
        },
    )
    .unwrap();
    let view = s
        .maps_location_assign(&id, &loc.id, &on(&view, map, 0), PLAIN)
        .unwrap();
    let mut found = std::collections::BTreeSet::new();
    keys(&serde_json::to_value(&view.locations).unwrap(), &mut found);
    keys(&serde_json::to_value(&view.legends).unwrap(), &mut found);
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

#[test]
fn location_subtypes_and_specialities_name_the_map_entries() {
    let mut s = service();
    let (mut subtypes, mut specialities) = (0, 0);
    for seed in 1..12 {
        let id = saved(&mut s, "town", seed);
        let view = s.maps_get(&id).unwrap();
        let snapshot = s.saved_get(&id).unwrap();
        let recorded = snapshot
            .stages
            .iter()
            .find_map(|st| st.locations.clone())
            .unwrap();
        for l in &recorded.locations {
            let e = entry(&view, &l.id);
            assert!(e.location_type.starts_with(&l.location_type.name));
            if let Some(sub) = &l.subtype {
                subtypes += 1;
                assert_eq!(e.default_name, sub.name, "named by its subtype");
                assert!(e.location_type.contains(&sub.name), "{}", e.location_type);
            }
            if let Some(spec) = &l.speciality {
                specialities += 1;
                assert!(e.location_type.contains(&spec.name), "{}", e.location_type);
            }
        }
    }
    assert!(subtypes > 0, "some locations have subtypes");
    let _ = specialities; // rare (Magic Store only); checked when it occurs
}
