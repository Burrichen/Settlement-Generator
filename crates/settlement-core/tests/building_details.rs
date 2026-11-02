//! Building details: generated lazily when a building is first opened, once
//! only; edited by the user and protected from regeneration; stored one
//! building at a time and kept across restarts.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use settlement_core::db::Database;
use settlement_core::dice::{RandomFaces, ScriptedFaces};
use settlement_core::engine::Selections;
use settlement_core::ids::{BuildingId, MapId, ResultId};
use settlement_core::maps::details::{
    FieldOrigin, GeneratedSource, MAX_NAME_CHARS, NotRolledReason, OCCUPANTS_TABLE, TYPE_TABLE,
};
use settlement_core::maps::{
    BuildingDetailsView, BuildingEdits, DetailsState, MapGenerationOptions, SeedChoice, UserValue,
};
use settlement_core::service::AppService;
use settlement_core::settlements::SettlementId;

fn service() -> AppService {
    AppService::new(Database::open_in_memory().unwrap())
}

/// A saved settlement of `size` with its maps drawn.
fn mapped(s: &mut AppService, size: &str, seed: u64) -> SettlementId {
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
    let id = s.save_new(&active, &format!("Test {size}")).unwrap().id;
    s.maps_generate(&id, &fixed(1)).unwrap();
    id
}

fn fixed(seed: u64) -> MapGenerationOptions {
    MapGenerationOptions {
        seed: SeedChoice::Fixed(seed),
        ..Default::default()
    }
}

/// The `n`th building of map `map`.
fn building(s: &AppService, id: &SettlementId, map: &str, n: usize) -> (MapId, BuildingId) {
    let view = s.maps_get(id).unwrap();
    let map = view
        .maps
        .unwrap()
        .maps
        .into_iter()
        .find(|m| m.id.as_str() == map)
        .unwrap();
    (map.id.clone(), map.geometry.buildings[n].id.clone())
}

fn open(
    s: &mut AppService,
    id: &SettlementId,
    (map, b): &(MapId, BuildingId),
) -> BuildingDetailsView {
    s.maps_building_with(id, map, b, &mut RandomFaces::seeded(7))
        .unwrap()
}

fn text(v: &str) -> Option<UserValue<String>> {
    Some(UserValue::new(Some(v.to_owned())))
}

fn summaries(s: &AppService, id: &SettlementId) -> Vec<BuildingId> {
    s.maps_get(id)
        .unwrap()
        .buildings
        .into_iter()
        .map(|b| b.building_id)
        .collect()
}

// ----- lazy generation ------------------------------------------------------------

#[test]
fn drawing_a_map_generates_no_building_details() {
    let mut s = service();
    let id = mapped(&mut s, "town", 3);
    assert!(
        summaries(&s, &id).is_empty(),
        "every building starts ungenerated"
    );
}

#[test]
fn the_first_opening_generates_details_once_and_later_ones_read_them_back() {
    let mut s = service();
    let id = mapped(&mut s, "town", 3);
    let target = building(&s, &id, "map-settlement", 0);

    let first = open(&mut s, &id, &target);
    assert_eq!(first.state, DetailsState::Generated);
    assert_eq!((first.version, first.generation), (1, 1));
    assert_eq!(first.building_id, target.1);
    assert_eq!(summaries(&s, &id), std::slice::from_ref(&target.1));

    // No dice at all: opening again must not roll anything.
    let again = s
        .maps_building_with(&id, &target.0, &target.1, &mut ScriptedFaces::new([]))
        .unwrap();
    assert_eq!(again, first);
    assert_eq!(summaries(&s, &id).len(), 1);
}

#[test]
fn generated_values_record_where_they_came_from() {
    let mut s = service();
    let id = mapped(&mut s, "town", 3);
    let target = building(&s, &id, "map-settlement", 0);
    let v = open(&mut s, &id, &target);

    // Rolled facts keep their whole roll, on the editable content table.
    assert_eq!(v.building_type.origin, FieldOrigin::Table);
    let GeneratedSource::Table { roll } = &v.building_type.generated.source else {
        panic!("the type is rolled");
    };
    assert_eq!(roll.table.id.as_str(), TYPE_TABLE);
    assert_eq!(
        v.building_type.value.as_deref(),
        Some(roll.result.name.as_str())
    );
    assert!(roll.accepted().is_some(), "every die is kept");
    for field in [&v.condition, &v.occupancy, &v.description] {
        assert_eq!(field.origin, FieldOrigin::Table);
        assert!(field.value.is_some());
    }
    // The address is inferred from the map, not rolled.
    assert_eq!(v.name.origin, FieldOrigin::Inferred);
    let number = target
        .1
        .as_str()
        .rsplit('-')
        .next()
        .unwrap()
        .parse::<u32>()
        .unwrap();
    assert!(
        v.name
            .value
            .as_deref()
            .unwrap()
            .starts_with(&format!("No. {number}, ")),
        "{:?}",
        v.name.value
    );
    // Suggestions come from the content tables.
    assert!(v.choices.building_type.contains(&"Storehouse".to_owned()));
}

#[test]
fn tables_not_rolled_leave_their_field_blank_and_say_why() {
    let mut s = service();
    let id = mapped(&mut s, "town", 3);
    let target = building(&s, &id, "map-settlement", 1);
    // A Town has no Building Type variant: 17 is a Storehouse, which has no
    // occupants. Then condition, occupancy ("occupied" on 1 in every
    // layout) and the feature.
    let v = s
        .maps_building_with(
            &id,
            &target.0,
            &target.1,
            &mut ScriptedFaces::new([17, 5, 1, 3]),
        )
        .unwrap();
    assert_eq!(v.building_type.value.as_deref(), Some("Storehouse"));
    assert_eq!(v.occupancy.value.as_deref(), Some("Occupied"));
    assert_eq!(v.occupants.value, None);
    assert_eq!(v.occupants.origin, FieldOrigin::NotRolled);
    assert_eq!(
        v.occupants.generated.source,
        GeneratedSource::NotRolled {
            table: OCCUPANTS_TABLE.into(),
            reason: NotRolledReason::NotEligible
        }
    );
    assert!(v.description.value.is_some());
}

#[test]
fn details_suit_the_settlement() {
    // A Village never has tenements; a City never has farmhouses.
    for (size, never) in [("village", "Tenement"), ("city", "Farmhouse")] {
        let mut s = service();
        let id = mapped(&mut s, size, 5);
        let view = s.maps_get(&id).unwrap();
        let map = view.maps.unwrap().maps.into_iter().next().unwrap();
        for (i, b) in map.geometry.buildings.iter().take(30).enumerate() {
            let v = s
                .maps_building_with(&id, &map.id, &b.id, &mut RandomFaces::seeded(i as u64))
                .unwrap();
            assert_ne!(v.building_type.value.as_deref(), Some(never), "{size}");
        }
    }
}

#[test]
fn opening_a_building_in_one_district_generates_nothing_in_the_others() {
    let mut s = service();
    let id = mapped(&mut s, "city", 4);
    let view = s.maps_get(&id).unwrap();
    let set = view.maps.unwrap();
    assert!(set.maps.len() >= 2, "a City has several districts");
    let first = &set.maps[0];
    let target = (first.id.clone(), first.geometry.buildings[3].id.clone());
    open(&mut s, &id, &target);

    let listed = s.maps_get(&id).unwrap().buildings;
    assert_eq!(listed.len(), 1, "only the opened building");
    assert_eq!(listed[0].map_id, first.id);
    for other in &set.maps[1..] {
        for b in &other.geometry.buildings {
            assert!(listed.iter().all(|l| l.building_id != b.id));
        }
    }
}

// ----- editing --------------------------------------------------------------------

#[test]
fn editing_customises_the_building_and_keeps_its_generated_values() {
    let mut s = service();
    let id = mapped(&mut s, "town", 3);
    let (map, b) = building(&s, &id, "map-settlement", 0);
    let generated = open(&mut s, &id, &(map.clone(), b.clone()));

    let edits = BuildingEdits {
        name: text("The Crooked Lantern"),
        notes: "Owner owes the guild money.".into(),
        ..Default::default()
    };
    let v = s.maps_building_edit(&id, &map, &b, 1, edits).unwrap();
    assert_eq!(v.state, DetailsState::Customised);
    assert_eq!(v.version, 2);
    assert_eq!(v.name.value.as_deref(), Some("The Crooked Lantern"));
    assert_eq!(v.name.origin, FieldOrigin::User);
    // The generated address is kept alongside, for "use generated".
    assert_eq!(v.name.generated, generated.name.generated);
    // Fields not edited still show what was generated.
    assert_eq!(v.building_type, generated.building_type);
    assert_eq!(v.notes, "Owner owes the guild money.");
    assert!(v.edited_at.is_some());
    assert_eq!(
        s.maps_get(&id).unwrap().buildings[0].state,
        DetailsState::Customised
    );
    assert_eq!(
        s.maps_get(&id).unwrap().buildings[0].name.as_deref(),
        Some("The Crooked Lantern")
    );
}

#[test]
fn blank_and_optional_fields() {
    let mut s = service();
    let id = mapped(&mut s, "town", 3);
    let (map, b) = building(&s, &id, "map-settlement", 0);
    let generated = open(&mut s, &id, &(map.clone(), b.clone()));

    let edits = BuildingEdits {
        // Left blank on purpose: whitespace, or no value.
        name: text("   "),
        occupants: Some(UserValue::new(None)),
        ..Default::default()
    };
    let v = s.maps_building_edit(&id, &map, &b, 1, edits).unwrap();
    assert_eq!(
        (v.name.value.as_deref(), v.name.origin),
        (None, FieldOrigin::User)
    );
    assert_eq!(
        (v.occupants.value, v.occupants.origin),
        (None, FieldOrigin::User)
    );
    assert_eq!(
        v.description, generated.description,
        "absent means generated"
    );
    assert_eq!(v.notes, "");
    assert_eq!(
        v.state,
        DetailsState::Customised,
        "a blank field is a choice"
    );

    // Saving no edits at all gives every generated value back.
    let v = s
        .maps_building_edit(&id, &map, &b, 2, BuildingEdits::default())
        .unwrap();
    assert_eq!(v.state, DetailsState::Generated);
    assert_eq!(v.name, generated.name);

    // Bad values are refused and nothing is written.
    let long = BuildingEdits {
        name: text(&"x".repeat(MAX_NAME_CHARS + 1)),
        ..Default::default()
    };
    assert!(s.maps_building_edit(&id, &map, &b, 3, long).is_err());
    let crowded = BuildingEdits {
        occupants: Some(UserValue::new(Some(1_000_000))),
        ..Default::default()
    };
    assert!(s.maps_building_edit(&id, &map, &b, 3, crowded).is_err());
    assert_eq!(open(&mut s, &id, &(map, b)).version, 3);
}

#[test]
fn an_edit_from_a_stale_copy_is_refused() {
    let mut s = service();
    let id = mapped(&mut s, "town", 3);
    let (map, b) = building(&s, &id, "map-settlement", 0);
    open(&mut s, &id, &(map.clone(), b.clone()));
    let first = BuildingEdits {
        notes: "first".into(),
        ..Default::default()
    };
    s.maps_building_edit(&id, &map, &b, 1, first).unwrap();
    // Another copy still at version 1 must not overwrite version 2.
    let stale = BuildingEdits {
        notes: "stale".into(),
        ..Default::default()
    };
    let error = s.maps_building_edit(&id, &map, &b, 1, stale).unwrap_err();
    assert!(error.contains("changed elsewhere"), "{error}");
    assert_eq!(open(&mut s, &id, &(map, b)).notes, "first");
}

#[test]
fn an_ungenerated_or_unknown_building_cannot_be_edited() {
    let mut s = service();
    let id = mapped(&mut s, "town", 3);
    let (map, b) = building(&s, &id, "map-settlement", 0);
    let edits = BuildingEdits {
        notes: "x".into(),
        ..Default::default()
    };
    assert!(
        s.maps_building_edit(&id, &map, &b, 1, edits.clone())
            .is_err()
    );
    assert!(summaries(&s, &id).is_empty(), "editing never generates");
    let nowhere = BuildingId::from("bld-settlement-r1-9999");
    assert!(
        s.maps_building_with(&id, &map, &nowhere, &mut RandomFaces::seeded(1))
            .is_err()
    );
    let other_map = MapId::from("map-district-1");
    assert!(
        s.maps_building_with(&id, &other_map, &b, &mut RandomFaces::seeded(1))
            .is_err()
    );
}

#[test]
fn two_buildings_never_share_a_record() {
    let mut s = service();
    let id = mapped(&mut s, "town", 3);
    let a = building(&s, &id, "map-settlement", 0);
    let b = building(&s, &id, "map-settlement", 1);
    open(&mut s, &id, &a);
    let edits = BuildingEdits {
        name: text("Only A"),
        ..Default::default()
    };
    s.maps_building_edit(&id, &a.0, &a.1, 1, edits).unwrap();

    // B is still ungenerated, then gets its own record.
    assert_eq!(summaries(&s, &id), std::slice::from_ref(&a.1));
    let vb = open(&mut s, &id, &b);
    assert_eq!(vb.building_id, b.1);
    assert_eq!(vb.state, DetailsState::Generated);
    assert_ne!(vb.name.value.as_deref(), Some("Only A"));
    let va = open(&mut s, &id, &a);
    assert_eq!(va.name.value.as_deref(), Some("Only A"));
    assert_eq!(summaries(&s, &id).len(), 2);
}

// ----- regenerate and reset ---------------------------------------------------------

#[test]
fn regenerating_rerolls_generated_fields_and_protects_the_users() {
    let mut s = service();
    let id = mapped(&mut s, "town", 3);
    let (map, b) = building(&s, &id, "map-settlement", 2);
    // Town type 1 is a House; 17 a Storehouse.
    s.maps_building_with(&id, &map, &b, &mut ScriptedFaces::new([1, 7, 1, 2, 1]))
        .unwrap();
    let edits = BuildingEdits {
        condition: text("Haunted"),
        notes: "Keep this.".into(),
        ..Default::default()
    };
    s.maps_building_edit(&id, &map, &b, 1, edits).unwrap();

    let v = s
        .maps_building_regenerate_with(
            &id,
            &map,
            &b,
            2,
            false,
            &mut ScriptedFaces::new([17, 7, 1, 1]),
        )
        .unwrap();
    assert_eq!(
        v.building_type.value.as_deref(),
        Some("Storehouse"),
        "rerolled"
    );
    assert_eq!(v.condition.value.as_deref(), Some("Haunted"), "protected");
    assert_eq!(v.condition.origin, FieldOrigin::User);
    assert_eq!(v.notes, "Keep this.");
    assert_eq!(v.state, DetailsState::Customised);
    assert_eq!((v.generation, v.version), (2, 3));

    // Only an explicit choice discards the user's values; notes stay.
    let v = s
        .maps_building_regenerate_with(
            &id,
            &map,
            &b,
            3,
            true,
            &mut ScriptedFaces::new([1, 7, 1, 2, 1]),
        )
        .unwrap();
    assert_eq!(v.building_type.value.as_deref(), Some("House"));
    assert_eq!(v.condition.origin, FieldOrigin::Table);
    assert_eq!(v.notes, "Keep this.");
    assert_eq!(v.generation, 3);
}

#[test]
fn reset_gives_back_the_generated_values_and_keeps_notes() {
    let mut s = service();
    let id = mapped(&mut s, "town", 3);
    let (map, b) = building(&s, &id, "map-settlement", 0);
    let generated = open(&mut s, &id, &(map.clone(), b.clone()));
    let edits = BuildingEdits {
        building_type: text("Bakery"),
        description: text("Smells of bread."),
        ..Default::default()
    };
    s.maps_building_edit(&id, &map, &b, 1, edits).unwrap();
    let v = s.maps_building_reset(&id, &map, &b, 2).unwrap();
    assert_eq!(v.state, DetailsState::Generated);
    assert_eq!(v.building_type, generated.building_type);
    assert_eq!(v.description, generated.description);
    assert_eq!(v.generation, 1, "reset does not reroll");

    let notes = BuildingEdits {
        building_type: text("Bakery"),
        notes: "mine".into(),
        ..Default::default()
    };
    s.maps_building_edit(&id, &map, &b, 3, notes).unwrap();
    let v = s.maps_building_reset(&id, &map, &b, 4).unwrap();
    assert_eq!(v.building_type, generated.building_type);
    assert_eq!(v.notes, "mine");
    assert_eq!(
        v.state,
        DetailsState::Customised,
        "notes are the user's own"
    );
}

// ----- persistence --------------------------------------------------------------------

struct TempDb(PathBuf);

impl TempDb {
    fn new() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        TempDb(std::env::temp_dir().join(format!(
            "settlement-building-test-{}-{}",
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
fn details_and_edits_survive_a_restart_with_stable_ids() {
    let tmp = TempDb::new();
    let (id, target, before) = {
        let mut s = tmp.service();
        let id = mapped(&mut s, "town", 3);
        let target = building(&s, &id, "map-settlement", 4);
        open(&mut s, &id, &target);
        let edits = BuildingEdits {
            name: text("Mill House"),
            occupants: Some(UserValue::new(None)),
            notes: "Two lines\nof notes".into(),
            ..Default::default()
        };
        let before = s
            .maps_building_edit(&id, &target.0, &target.1, 1, edits)
            .unwrap();
        (id, target, before)
    };
    let mut s = tmp.service();
    // The same building, by the same ID, with nothing rolled again.
    assert_eq!(building(&s, &id, "map-settlement", 4), target);
    let after = s
        .maps_building_with(&id, &target.0, &target.1, &mut ScriptedFaces::new([]))
        .unwrap();
    assert_eq!(after, before);
    assert_eq!(after.name.value.as_deref(), Some("Mill House"));
    assert_eq!(after.occupants.value, None);
    assert_eq!(after.notes, "Two lines\nof notes");
}

#[test]
fn redrawn_maps_leave_old_details_behind_and_deleting_maps_removes_them() {
    let mut s = service();
    let id = mapped(&mut s, "village", 2);
    let target = building(&s, &id, "map-settlement", 0);
    open(&mut s, &id, &target);
    assert_eq!(summaries(&s, &id).len(), 1);

    // New building IDs: the old details can never attach to a new building.
    s.maps_generate(&id, &fixed(1)).unwrap();
    assert!(summaries(&s, &id).is_empty());
    let (_, fresh) = building(&s, &id, "map-settlement", 0);
    assert_ne!(fresh, target.1);
    assert!(
        s.maps_building_with(&id, &target.0, &target.1, &mut RandomFaces::seeded(1))
            .is_err()
    );

    let again = building(&s, &id, "map-settlement", 0);
    open(&mut s, &id, &again);
    s.maps_delete(&id).unwrap();
    s.maps_generate(&id, &fixed(1)).unwrap();
    assert!(summaries(&s, &id).is_empty());
}

#[test]
fn the_gui_map_types_declare_every_building_field() {
    /// Every key, except inside roll records (`RollRecord` in api.ts).
    fn keys(v: &serde_json::Value, out: &mut std::collections::BTreeSet<String>) {
        match v {
            serde_json::Value::Object(m) => {
                for (k, v) in m {
                    out.insert(k.clone());
                    if k != "roll" {
                        keys(v, out);
                    }
                }
            }
            serde_json::Value::Array(a) => a.iter().for_each(|v| keys(v, out)),
            _ => {}
        }
    }
    let mut s = service();
    let id = mapped(&mut s, "town", 3);
    let (map, b) = building(&s, &id, "map-settlement", 1);
    // A storehouse: its occupants are not rolled.
    s.maps_building_with(&id, &map, &b, &mut ScriptedFaces::new([17, 5, 1, 3]))
        .unwrap();
    let edits = BuildingEdits {
        name: text("x"),
        notes: "y".into(),
        ..Default::default()
    };
    let view = s.maps_building_edit(&id, &map, &b, 1, edits).unwrap();
    let mut found = std::collections::BTreeSet::new();
    keys(&serde_json::to_value(&view).unwrap(), &mut found);
    keys(
        &serde_json::to_value(s.maps_get(&id).unwrap().buildings).unwrap(),
        &mut found,
    );
    // `Named` and `District` are declared in api.ts.
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
    // The limits the GUI enforces are the core's.
    use settlement_core::maps::details::{
        MAX_DESCRIPTION_CHARS, MAX_LABEL_CHARS, MAX_NOTES_CHARS, MAX_OCCUPANTS,
    };
    for (name, value) in [
        ("MAX_NAME_CHARS", MAX_NAME_CHARS),
        ("MAX_LABEL_CHARS", MAX_LABEL_CHARS),
        ("MAX_DESCRIPTION_CHARS", MAX_DESCRIPTION_CHARS),
        ("MAX_NOTES_CHARS", MAX_NOTES_CHARS),
        ("MAX_OCCUPANTS", MAX_OCCUPANTS as usize),
    ] {
        let line = format!("export const {name} = {value};");
        assert!(ts.contains(&line), "types.ts must declare `{line}`");
    }
}
