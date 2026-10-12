//! Notable Locations: the count by Size (per district for Cities and
//! Capitals), the 1d20 Type, and the guaranteed Tavern added by the Bare
//! Minimum Buildings setting.

use std::sync::Arc;

use settlement_core::bundled::bundled_content;
use settlement_core::content::{ContentSet, Effect, IntentChange};
use settlement_core::db::Database;
use settlement_core::dice::{RandomFaces, ScriptedFaces};
use settlement_core::engine::{GenerationEvent, GenerationOptions, Generator, SkipReason};
use settlement_core::ids::{ResultId, TableId};
use settlement_core::layers::{ContentLayers, UserOverrides};
use settlement_core::locations::{LocationSource, NotableLocations};
use settlement_core::service::AppService;
use settlement_core::session::{GenerationMode, GenerationSession, ViewOptions};
use settlement_core::settings::Settings;
use settlement_core::settlements::SettlementSnapshot;
use settlement_core::validation::{IssueKind, validate};

const SIZE: usize = 5;
const LOCATIONS: usize = 18;
const TYPE: &str = "table.notableLocationType";

fn content() -> ContentSet {
    bundled_content().unwrap()
}

/// `(count table, dice, [(min, max, count)])` for each Size, as approved.
/// `(table, dice, [(min, max, count)])`.
type CountSpec = (&'static str, &'static str, Vec<(i32, i32, i64)>);

fn counts() -> Vec<CountSpec> {
    vec![
        (
            "table.notableLocationCount.tradingPost",
            "1d4",
            vec![(1, 2, 1), (3, 4, 2)],
        ),
        (
            "table.notableLocationCount.village",
            "1d6",
            vec![(1, 2, 1), (3, 4, 2), (5, 6, 3)],
        ),
        (
            "table.notableLocationCount.town",
            "1d6",
            vec![(1, 2, 2), (3, 4, 3), (5, 6, 4)],
        ),
        (
            "table.notableLocationCount.cityDistrict",
            "1d8",
            vec![(1, 2, 2), (3, 4, 3), (5, 6, 4), (7, 8, 5)],
        ),
        (
            "table.notableLocationCount.capitalDistrict",
            "1d10",
            vec![(1, 2, 2), (3, 4, 3), (5, 6, 4), (7, 8, 5), (9, 10, 6)],
        ),
        (
            "table.districtCount.city",
            "1d6",
            vec![(1, 2, 2), (3, 4, 3), (5, 6, 4)],
        ),
        (
            "table.districtCount.capital",
            "1d6",
            vec![(1, 2, 3), (3, 4, 4), (5, 6, 5)],
        ),
    ]
}

/// `(min, max, result ID, name)` of the Type table, as approved.
const TYPES: [(i32, i32, &str, &str); 12] = [
    (1, 3, "locationType.hospitality", "Hospitality"),
    (4, 6, "locationType.trade", "Trade / Shop"),
    (7, 8, "locationType.craft", "Craft / Workshop"),
    (9, 10, "locationType.religious", "Religious"),
    (11, 11, "locationType.civic", "Civic / Government"),
    (12, 12, "locationType.military", "Military / Security"),
    (13, 13, "locationType.organisation", "Guild / Organisation"),
    (14, 14, "locationType.knowledge", "Knowledge / Education"),
    (15, 15, "locationType.entertainment", "Entertainment"),
    (16, 16, "locationType.criminal", "Criminal"),
    (17, 18, "locationType.industry", "Industry / Resource"),
    (19, 20, "locationType.unusual", "Unusual / Special"),
];

/// Rolls `table` once on each face; returns `(face, result name, value)`.
fn every_face(table: &str, faces: i32) -> Vec<(i32, String, Option<i64>)> {
    let c = content();
    let generator = Generator::new(&c).unwrap();
    let id = TableId::new(table);
    (1..=faces)
        .map(|face| {
            let mut dice = ScriptedFaces::new([face as u32]);
            let log = generator.run(std::slice::from_ref(&id), &mut dice).unwrap();
            assert_eq!(dice.remaining(), 0, "{table}: one die per roll");
            let record = log.record_for(&id).unwrap();
            assert_eq!(record.attempts[0].roll.total, face);
            (face, record.result.name.clone(), record.value)
        })
        .collect()
}

/// A complete settlement of `size` (chosen, so god mode makes it legal
/// whatever came before), its Notable Locations then rolled on `faces`.
fn settlement(size: &str, bare_minimum_buildings: bool, faces: &[u32]) -> GenerationSession {
    let mut s = GenerationSession::new(
        Arc::new(content()),
        GenerationOptions {
            god_mode: true,
            bare_minimum_buildings,
        },
        GenerationMode::OneTable,
    )
    .unwrap();
    s.choose(0, "kurovia".into(), &mut RandomFaces::seeded(1))
        .unwrap();
    s.roll_remaining(&mut RandomFaces::seeded(2)).unwrap();
    s.choose(SIZE, size.into(), &mut RandomFaces::seeded(3))
        .unwrap();
    s.roll_remaining(&mut RandomFaces::seeded(4)).unwrap();
    let mut dice = ScriptedFaces::new(faces.iter().copied());
    s.roll(LOCATIONS, &mut dice).unwrap();
    assert_eq!(dice.remaining(), 0, "{size}: every scripted die is used");
    assert!(s.is_complete());
    s
}

fn record(s: &GenerationSession) -> NotableLocations {
    s.view(LOCATIONS, ViewOptions::default())
        .unwrap()
        .locations
        .unwrap()
        .record
        .unwrap()
}

fn guaranteed(r: &NotableLocations) -> usize {
    r.locations
        .iter()
        .filter(|l| l.source == LocationSource::BareMinimumBuildings)
        .count()
}

fn types(r: &NotableLocations) -> Vec<String> {
    r.locations
        .iter()
        .map(|l| l.location_type.id.to_string())
        .collect()
}

// ----- count tables (1–7) -----------------------------------------------------------------

#[test]
fn t01_trading_post_uses_1d4() {
    let c = content();
    let t = c
        .table(&"table.notableLocationCount.tradingPost".into())
        .unwrap();
    assert_eq!(t.dice.as_deref(), Some("1d4"));
}

#[test]
fn t02_trading_post_produces_only_one_or_two() {
    let rolled = every_face("table.notableLocationCount.tradingPost", 4);
    let values: Vec<_> = rolled.iter().map(|(_, _, v)| v.unwrap()).collect();
    assert_eq!(values, [1, 1, 2, 2]);
    // And through generation: 1 → one location, 4 → two.
    assert_eq!(
        record(&settlement("tradingPost", false, &[1, 5]))
            .locations
            .len(),
        1
    );
    assert_eq!(
        record(&settlement("tradingPost", false, &[4, 5, 5]))
            .locations
            .len(),
        2
    );
}

#[test]
fn t03_to_t06_each_size_uses_its_die_and_range() {
    for (table, dice, rows) in counts() {
        let t = content().table(&table.into()).unwrap().clone();
        assert_eq!(t.dice.as_deref(), Some(dice), "{table}");
        let faces: i32 = dice[2..].parse().unwrap();
        for (face, _, value) in every_face(table, faces) {
            let expected = rows
                .iter()
                .find(|(a, b, _)| (*a..=*b).contains(&face))
                .unwrap()
                .2;
            assert_eq!(value, Some(expected), "{table} face {face}");
        }
    }
}

#[test]
fn t07_every_face_is_covered_exactly_once() {
    let c = content();
    let mut tables: Vec<(String, i32)> = counts()
        .into_iter()
        .map(|(t, d, _)| (t.to_owned(), d[2..].parse().unwrap()))
        .collect();
    tables.push((TYPE.to_owned(), 20));
    for (table, faces) in tables {
        let t = c.table(&table.as_str().into()).unwrap();
        for face in 1..=faces {
            let claims = t
                .results
                .iter()
                .filter(|r| r.range.unwrap().contains(face))
                .count();
            assert_eq!(claims, 1, "{table}: face {face}");
        }
        assert!(
            t.results.iter().all(|r| r.range.unwrap().max <= faces),
            "{table}: no face beyond the die"
        );
    }
}

#[test]
fn villages_and_towns_roll_one_count_for_the_whole_settlement() {
    // Village 5 → 3 locations; Town 1 → 2.
    let village = record(&settlement("village", false, &[5, 1, 4, 7]));
    assert_eq!(village.districts, None);
    assert_eq!(village.areas.len(), 1);
    assert_eq!(village.rolled_count(), 3);
    assert!(village.locations.iter().all(|l| l.district.is_none()));
    let town = record(&settlement("town", false, &[1, 9, 11]));
    assert_eq!(town.rolled_count(), 2);
    assert_eq!(
        types(&town),
        ["locationType.religious", "locationType.civic"]
    );
}

#[test]
fn cities_and_capitals_roll_a_count_per_district() {
    // City: 5 → 4 districts, then each district's count (1d8) and types.
    let city = record(&settlement(
        "city",
        false,
        &[5, 1, 1, 1, 3, 1, 1, 1, 1, 1, 1, 7, 1, 1, 1, 1, 1],
    ));
    assert_eq!(city.districts.as_ref().unwrap().count, 4);
    let per: Vec<u32> = city.areas.iter().map(|a| a.count.count).collect();
    assert_eq!(per, [2, 3, 2, 5]);
    assert_eq!(city.rolled_count(), 12);
    for (i, area) in city.areas.iter().enumerate() {
        let d = area.district.as_ref().unwrap();
        assert_eq!(d.label, format!("District {}", i + 1));
        let mine = city
            .locations
            .iter()
            .filter(|l| l.district.as_ref() == Some(&d.id))
            .count();
        assert_eq!(mine as u32, area.count.count);
    }
    // Capital: 1 → 3 districts, each at least 2.
    let capital = record(&settlement(
        "capital",
        false,
        &[1, 1, 1, 1, 1, 1, 1, 10, 1, 1, 1, 1, 1, 1],
    ));
    assert_eq!(capital.districts.as_ref().unwrap().count, 3);
    let per: Vec<u32> = capital.areas.iter().map(|a| a.count.count).collect();
    assert_eq!(per, [2, 2, 6]);
}

#[test]
fn changing_the_district_count_keeps_the_districts_that_remain() {
    let mut s = settlement("city", false, &[5, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1]);
    let before = record(&s);
    assert_eq!(before.areas.len(), 4);
    s.choose_district_count(LOCATIONS, "two".into(), &mut RandomFaces::seeded(1))
        .unwrap();
    let after = record(&s);
    assert_eq!(after.areas, before.areas[..2]);
    assert!(
        after
            .locations
            .iter()
            .all(|l| { matches!(l.district.as_deref(), Some("district-1" | "district-2")) })
    );
    // Back up to four: two new districts are rolled; the kept ones stay.
    s.choose_district_count(LOCATIONS, "four".into(), &mut RandomFaces::seeded(2))
        .unwrap();
    let again = record(&s);
    assert_eq!(again.areas[..2], before.areas[..2]);
    assert_eq!(again.areas.len(), 4);
    // Towns are not mapped by district.
    let mut town = settlement("town", false, &[1, 1, 1]);
    assert!(
        town.roll_district_count(LOCATIONS, &mut RandomFaces::seeded(1))
            .is_err()
    );
}

#[test]
fn changing_one_area_count_keeps_its_other_locations() {
    let mut s = settlement("village", false, &[5, 1, 4, 7]);
    let before = record(&s);
    s.choose_location_count(LOCATIONS, 0, "one".into(), &mut RandomFaces::seeded(1))
        .unwrap();
    let after = record(&s);
    assert_eq!(after.locations, before.locations[..1]);
    s.choose_location_count(LOCATIONS, 0, "three".into(), &mut RandomFaces::seeded(2))
        .unwrap();
    let again = record(&s);
    assert_eq!(again.locations[0], before.locations[0]);
    // New locations get new IDs; removed ones are never reused.
    let ids: Vec<_> = again.locations.iter().map(|l| l.id.clone()).collect();
    assert_eq!(ids[0], "notable-location-01");
    assert!(
        !ids[1..].contains(&"notable-location-02".to_owned()),
        "{ids:?}"
    );
}

// ----- Bare Minimum Buildings (8–17) -----------------------------------------------------------

#[test]
fn t08_the_setting_persists_between_sessions() {
    let mut service = AppService::new(Database::open_in_memory().unwrap());
    assert!(!service.settings().unwrap().bare_minimum_buildings);
    service
        .save_settings(&Settings {
            bare_minimum_buildings: true,
            ..Settings::default()
        })
        .unwrap();
    assert!(service.settings().unwrap().bare_minimum_buildings);
    let active = service.start_generation().unwrap();
    assert!(active.session.options().bare_minimum_buildings);
    // Saved before the setting existed: off.
    let old: Settings = serde_json::from_value(serde_json::json!({
        "generation_method": "automatic",
        "generation_presentation": "incremental",
        "god_mode": false,
        "tag_view": false,
        "theme": "modern_dark"
    }))
    .unwrap();
    assert!(!old.bare_minimum_buildings);
}

#[test]
fn t09_disabled_creates_no_guaranteed_tavern() {
    let r = record(&settlement("tradingPost", false, &[1, 1]));
    assert_eq!(guaranteed(&r), 0);
    assert_eq!(r.locations.len(), 1);
}

#[test]
fn t10_t11_enabled_creates_exactly_one_tavern_with_four_spare_rooms() {
    for (size, faces) in [
        ("tradingPost", vec![1, 5]),
        ("village", vec![1, 5]),
        ("town", vec![1, 5, 5]),
    ] {
        let r = record(&settlement(size, true, &faces));
        assert_eq!(guaranteed(&r), 1, "{size}");
        let tavern = r.guaranteed().unwrap();
        assert_eq!(r.locations[0], *tavern, "{size}: listed first");
        assert_eq!(tavern.location_type.id.as_str(), "locationType.hospitality");
        assert_eq!(tavern.location_type.name, "Hospitality");
        assert_eq!(
            tavern.subtype.as_ref().unwrap().id.as_str(),
            "locationSubtype.tavern"
        );
        assert_eq!(tavern.subtype.as_ref().unwrap().name, "Tavern");
        assert_eq!(tavern.spare_rooms, Some(4));
        assert_eq!(tavern.district, None);
    }
}

#[test]
fn t12_t13_the_tavern_is_added_to_the_rolled_count() {
    // A Trading Post rolling 1 has one rolled location, plus the Tavern.
    let r = record(&settlement("tradingPost", true, &[1, 5]));
    assert_eq!(r.rolled_count(), 1);
    assert_eq!(r.locations.len(), 2);
    assert_eq!(
        r.locations
            .iter()
            .filter(|l| l.source == LocationSource::Rolled)
            .count(),
        1
    );
    assert_eq!(types(&r)[1], "locationType.trade");
}

#[test]
fn t14_the_tavern_does_not_roll_the_type_table() {
    let mut s = settlement("tradingPost", true, &[1, 5]);
    let r = record(&s);
    assert_eq!(r.guaranteed().unwrap().type_roll, None);
    // Exactly one Type roll (the rolled location's), none for the Tavern.
    let rolled = r.locations.iter().filter(|l| l.type_roll.is_some()).count();
    assert_eq!(rolled, 1);
    // It cannot be re-rolled or chosen either.
    assert!(
        s.roll_location_type(LOCATIONS, 0, &mut RandomFaces::seeded(1))
            .is_err()
    );
    assert!(
        s.choose_location_type(
            LOCATIONS,
            0,
            "locationType.trade".into(),
            &mut RandomFaces::seeded(1)
        )
        .is_err()
    );
    assert_eq!(record(&s), r);
}

#[test]
fn t15_disabling_removes_only_the_setting_tavern() {
    let mut s = settlement("village", true, &[5, 4, 9, 19]);
    let on = record(&s);
    assert_eq!(on.locations.len(), 4);
    s.set_bare_minimum_buildings(false).unwrap();
    let off = record(&s);
    assert_eq!(guaranteed(&off), 0);
    assert_eq!(off.locations, on.locations[1..]);
    assert_eq!(off.areas, on.areas);
    // And back: the Tavern returns, the rolled locations are untouched.
    s.set_bare_minimum_buildings(true).unwrap();
    assert_eq!(record(&s), on);
    assert!(s.options().bare_minimum_buildings);
}

#[test]
fn t16_a_rolled_hospitality_is_not_the_guaranteed_tavern() {
    // The rolled location is Hospitality (face 1) too.
    let mut s = settlement("tradingPost", true, &[1, 1]);
    let r = record(&s);
    assert_eq!(
        types(&r),
        ["locationType.hospitality", "locationType.hospitality"]
    );
    assert_eq!(guaranteed(&r), 1);
    let rolled = &r.locations[1];
    assert_eq!(rolled.source, LocationSource::Rolled);
    assert_eq!(rolled.subtype, None);
    assert_eq!(rolled.spare_rooms, None);
    assert_ne!(rolled.id, r.guaranteed().unwrap().id);
    s.set_bare_minimum_buildings(false).unwrap();
    let off = record(&s);
    assert_eq!(off.locations, std::slice::from_ref(rolled));
    // Without the setting, a rolled Hospitality stays rolled.
    let plain = record(&settlement("tradingPost", false, &[1, 1]));
    assert_eq!(guaranteed(&plain), 0);
    assert_eq!(plain.guaranteed(), None);
}

#[test]
fn t17_cities_and_capitals_get_one_tavern_not_one_per_district() {
    let city = record(&settlement(
        "city",
        true,
        &[5, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
    ));
    assert_eq!(city.areas.len(), 4);
    assert_eq!(guaranteed(&city), 1);
    assert_eq!(city.locations.len() as u32, city.rolled_count() + 1);
    let capital = record(&settlement(
        "capital",
        true,
        &[5, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
    ));
    assert_eq!(capital.areas.len(), 5);
    assert_eq!(guaranteed(&capital), 1);
    // Changing the districts never adds another.
    let mut s = settlement("city", true, &[1, 1, 1, 1, 1, 1, 1]);
    s.choose_district_count(LOCATIONS, "four".into(), &mut RandomFaces::seeded(1))
        .unwrap();
    s.roll_location_count(LOCATIONS, 3, &mut RandomFaces::seeded(2))
        .unwrap();
    assert_eq!(guaranteed(&record(&s)), 1);
}

// ----- Type (18–33) ---------------------------------------------------------------------

#[test]
fn t18_type_uses_1d20() {
    let c = content();
    let t = c.table(&TYPE.into()).unwrap();
    assert_eq!(t.dice.as_deref(), Some("1d20"));
    assert_eq!(t.name, "Notable Location Type");
    let r = record(&settlement("tradingPost", false, &[1, 20]));
    let roll = r.locations[0].type_roll.as_ref().unwrap();
    assert_eq!(roll.dice.as_deref(), Some("1d20"));
    assert_eq!(roll.attempts[0].roll.total, 20);
}

#[test]
fn t19_to_t30_every_face_gives_its_type() {
    let rolled = every_face(TYPE, 20);
    for (face, name, _) in rolled {
        let (_, _, _, expected) = TYPES
            .iter()
            .find(|(a, b, _, _)| (*a..=*b).contains(&face))
            .unwrap();
        assert_eq!(name, *expected, "face {face}");
    }
    // And through generation, with the type's stable ID.
    for (min, max, id, _) in TYPES {
        for face in [min, max] {
            let r = record(&settlement("tradingPost", false, &[1, face as u32]));
            assert_eq!(types(&r), [id], "face {face}");
        }
    }
}

#[test]
fn t31_residence_and_estate_do_not_exist() {
    let c = content();
    let t = c.table(&TYPE.into()).unwrap();
    for r in &t.results {
        for word in ["Residence", "Estate", "Noble"] {
            assert!(!r.name.contains(word), "{}", r.name);
            assert!(
                !r.id.as_str().to_lowercase().contains(&word.to_lowercase()),
                "{}",
                r.id
            );
        }
    }
    // Criminal took its place.
    assert!(t.result(&"locationType.criminal".into()).is_some());
    // No Noble system: no table or scheme of its own.
    assert!(
        c.tables
            .iter()
            .all(|t| !t.id.as_str().starts_with("table.noble"))
    );
    assert!(c.location_schemes.iter().all(|s| !s.name.contains("Noble")));
}

#[test]
fn t32_type_results_have_stable_ids() {
    let c = content();
    let t = c.table(&TYPE.into()).unwrap();
    let actual: Vec<_> = t
        .results
        .iter()
        .map(|r| {
            let range = r.range.unwrap();
            (range.min, range.max, r.id.to_string(), r.name.clone())
        })
        .collect();
    let expected: Vec<_> = TYPES
        .iter()
        .map(|(a, b, id, n)| (*a, *b, (*id).to_owned(), (*n).to_owned()))
        .collect();
    assert_eq!(actual, expected);
    assert!(
        t.results
            .iter()
            .all(|r| r.id.as_str().starts_with("locationType."))
    );
}

/// The planned Type intents on a tag: `(result, change)`.
fn type_intents(c: &ContentSet, tag: &str) -> Vec<(String, IntentChange)> {
    c.tag(&tag.into())
        .unwrap_or_else(|| panic!("{tag}"))
        .effects
        .iter()
        .filter_map(|d| match &d.effect {
            Effect::Intent {
                table: Some(table),
                results,
                change,
                ..
            } if table.as_str() == TYPE => {
                assert!(!d.is_active(), "{tag}: intents are never executed");
                Some(
                    results
                        .iter()
                        .map(|r| (r.to_string(), *change))
                        .collect::<Vec<_>>(),
                )
            }
            _ => None,
        })
        .flatten()
        .collect()
}

#[test]
fn t33_modifier_intentions_are_direction_only() {
    use IntentChange::{Increase, StrongIncrease};
    let c = content();
    let has = |tag: &str, result: &str, change: IntentChange| {
        assert!(
            type_intents(&c, tag).contains(&(format!("locationType.{result}"), change)),
            "{tag} → {result}: {:?}",
            type_intents(&c, tag)
        );
    };
    has("purpose.trade", "trade", Increase);
    has("purpose.craftIndustry", "craft", Increase);
    has("purpose.craftIndustry", "industry", Increase);
    has("purpose.resourceExtraction", "industry", Increase);
    has("purpose.religious", "religious", Increase);
    has("purpose.military", "military", Increase);
    has("purpose.administrative", "civic", Increase);
    has("purpose.scholarly", "knowledge", Increase);
    has("authority.guild", "organisation", Increase);
    has("authority.religious", "religious", Increase);
    has("authority.military", "military", Increase);
    has("economy.prosperous", "trade", Increase);
    has("economy.wealthy", "trade", Increase);
    has("security.wellDefended", "military", Increase);
    has("security.fortified", "military", Increase);
    has("crime.high", "criminal", Increase);
    has("crime.rampant", "criminal", StrongIncrease);
    // No executable effect touches the new tables: nothing changes a roll.
    let new = |t: &str| {
        t == TYPE
            || t.starts_with("table.notableLocationCount")
            || t.starts_with("table.districtCount")
    };
    for tag in &c.tags {
        for def in &tag.effects {
            if let Some(t) = def.effect.target_table() {
                if new(t.as_str()) {
                    assert!(
                        matches!(def.effect, Effect::Intent { .. }) && !def.is_active(),
                        "{}: only planned intents",
                        tag.id
                    );
                }
            }
        }
    }
    // The Type table has no weights or modifiers of its own.
    let t = c.table(&TYPE.into()).unwrap();
    assert!(t.results.iter().all(|r| r.tags.is_empty()));
}

// ----- persistence and parity (34–37) ------------------------------------------------------

#[test]
fn t34_locations_persist_in_snapshots() {
    let s = settlement("city", true, &[1, 1, 3, 4, 1, 2, 9]);
    let snapshot = s.to_snapshot();
    let stored = snapshot.notable_locations().unwrap();
    assert_eq!(*stored, record(&s));
    assert_eq!(guaranteed(stored), 1);
    assert_eq!(stored.areas.len(), 2);
    let json = serde_json::to_string(&snapshot).unwrap();
    assert!(json.contains("\"bareMinimumBuildings\""));
    assert!(json.contains("\"spare_rooms\":4"));
    let back: SettlementSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(back, snapshot);
    let stage = back
        .display_stages()
        .into_iter()
        .find(|st| st.label == "Notable Locations")
        .unwrap();
    assert_eq!(stage.result.unwrap().name, "5 Notable Locations");
    // Reopening with the setting off keeps the saved Tavern as saved.
    let reopened = GenerationSession::from_snapshot(
        Arc::new(content()),
        GenerationOptions::default(),
        GenerationMode::OneTable,
        &back,
    )
    .unwrap();
    // The saved stages stand as saved (only the session's own options,
    // recorded in the log, differ).
    assert_eq!(reopened.to_snapshot().stages, snapshot.stages);
    assert_eq!(
        reopened.to_snapshot().notable_locations(),
        snapshot.notable_locations()
    );
}

#[test]
fn t35_snapshots_ignore_later_table_edits() {
    let snapshot = settlement("town", true, &[3, 4, 12, 16]).to_snapshot();
    let mut changed = content();
    for t in changed.tables.iter_mut().filter(|t| {
        t.id.as_str() == TYPE || t.id.as_str().starts_with("table.notableLocationCount")
    }) {
        for r in &mut t.results {
            r.name = format!("{} (edited)", r.name);
            if let Some(v) = &mut r.value {
                *v += 1;
            }
        }
    }
    let reopened = GenerationSession::from_snapshot(
        Arc::new(changed),
        GenerationOptions::default(),
        GenerationMode::OneTable,
        &snapshot,
    )
    .unwrap();
    assert_eq!(reopened.to_snapshot().stages, snapshot.stages);
    assert_eq!(
        types(snapshot.notable_locations().unwrap()),
        [
            "locationType.hospitality",
            "locationType.trade",
            "locationType.military",
            "locationType.criminal"
        ]
    );
}

#[test]
fn settlements_saved_before_notable_locations_load_with_it_pending() {
    let s = settlement("village", false, &[1, 1]);
    let mut old = s.to_snapshot();
    old.stages.retain(|st| st.locations.is_none());
    old.log.events.retain(|e| match e {
        GenerationEvent::TableRolled(r) => !r.table.id.as_str().contains("otableLocation"),
        _ => true,
    });
    let reopened = GenerationSession::from_snapshot(
        Arc::new(content()),
        GenerationOptions::default(),
        GenerationMode::OneTable,
        &old,
    )
    .unwrap();
    // Nothing is rolled on load: like the other later stages, it is marked
    // as not recorded, and can still be rolled.
    let mut reopened = reopened;
    let view = reopened.view(LOCATIONS, ViewOptions::default()).unwrap();
    assert_eq!(view.skipped, Some(SkipReason::NotRecorded));
    assert!(view.locations.unwrap().record.is_none());
    reopened
        .roll(LOCATIONS, &mut RandomFaces::seeded(1))
        .unwrap();
    assert!(record(&reopened).rolled_count() >= 1);
}

#[test]
fn changing_the_size_redoes_the_locations() {
    let mut s = settlement("village", true, &[1, 1]);
    let before = record(&s);
    s.choose(SIZE, "capital".into(), &mut RandomFaces::seeded(9))
        .unwrap();
    let view = s.view(LOCATIONS, ViewOptions::default()).unwrap();
    assert!(view.locations.unwrap().record.is_none());
    s.roll(LOCATIONS, &mut RandomFaces::seeded(10)).unwrap();
    let after = record(&s);
    assert_ne!(after.scheme, before.scheme);
    assert_eq!(after.scheme.id.as_str(), "locations.capital");
    assert!(after.districts.is_some());
    assert_eq!(guaranteed(&after), 1);
}

#[test]
fn t36_both_interfaces_share_the_service_rules() {
    // The GUI and the TUI both generate through the service: its settings
    // decide the Tavern for either.
    let mut service = AppService::new(Database::open_in_memory().unwrap());
    for enabled in [false, true] {
        service
            .save_settings(&Settings {
                bare_minimum_buildings: enabled,
                ..Settings::default()
            })
            .unwrap();
        let mut active = service.start_generation().unwrap();
        active
            .session
            .choose(0, "kurovia".into(), &mut RandomFaces::seeded(1))
            .unwrap();
        active
            .session
            .roll_remaining(&mut RandomFaces::seeded(2))
            .unwrap();
        let view = active.view().unwrap();
        let locations = view.stages[LOCATIONS].locations.as_ref().unwrap();
        assert_eq!(locations.bare_minimum_buildings, enabled);
        let r = locations.record.as_ref().unwrap();
        assert_eq!(guaranteed(r), usize::from(enabled));
        assert_eq!(
            r.locations.len() as u32,
            r.rolled_count() + u32::from(enabled)
        );
        assert!(locations.options.types.is_some());
        assert!(locations.options.count.is_some());
    }
}

#[test]
fn t37_overrides_survive_an_update_of_the_defaults() {
    let installed = content();
    let mut layers = ContentLayers::new(installed.clone(), UserOverrides::default());
    let mut table = installed.table(&TYPE.into()).unwrap().clone();
    table.results[9].name = "Den of Thieves".into();
    layers.put_table(table).unwrap();
    let overrides = layers.overrides().clone();

    let mut updated = installed;
    let t = updated
        .tables
        .iter_mut()
        .find(|t| t.id.as_str() == TYPE)
        .unwrap();
    t.description = "Updated.".into();
    t.results[0].name = "Inns & Hospitality".into();
    let merged = ContentLayers::new(updated, overrides).merged();
    let table = merged.content.table(&TYPE.into()).unwrap();
    assert_eq!(table.results[9].name, "Den of Thieves");
    assert_eq!(table.results[0].name, "Inns & Hospitality");
    assert!(!merged.report.has_errors(), "{}", merged.report);
}

#[test]
fn the_tables_are_editable_and_resettable() {
    let mut s = AppService::new(Database::open_in_memory().unwrap());
    let mut ids: Vec<String> = counts().into_iter().map(|(t, _, _)| t.to_owned()).collect();
    ids.push(TYPE.to_owned());
    for table in ids {
        let id = TableId::new(table.as_str());
        let detail = s.table_detail(&id).unwrap();
        assert!(detail.issues.is_empty(), "{table}: {:?}", detail.issues);
        let mut draft = detail.table.clone();
        draft.name = "Renamed".into();
        s.save_table(draft).unwrap();
        s.reset_table(&id).unwrap();
        assert_eq!(
            &s.table_detail(&id).unwrap().table,
            content().table(&id).unwrap()
        );
    }
}

// ----- validation ---------------------------------------------------------------------------

#[test]
fn bundled_locations_content_is_valid() {
    let report = validate(&content());
    assert!(!report.has_errors(), "{report}");
    assert_eq!(content().location_schemes.len(), 5);
}

#[test]
fn a_count_result_without_a_value_is_an_error() {
    let mut c = content();
    let t = c
        .tables
        .iter_mut()
        .find(|t| t.id.as_str() == "table.notableLocationCount.town")
        .unwrap();
    t.results[0].value = None;
    let report = validate(&c);
    assert!(
        report
            .issues
            .iter()
            .any(|i| i.kind == IssueKind::MissingValue),
        "{report}"
    );
}

#[test]
fn the_type_table_needs_hospitality_for_the_tavern() {
    let mut c = content();
    let t = c.tables.iter_mut().find(|t| t.id.as_str() == TYPE).unwrap();
    t.results[0].id = ResultId::new("locationType.inn");
    let report = validate(&c);
    assert!(report.has_errors(), "{report}");
}
