//! Notable Location subtypes: District Count routing, canonical shared
//! subtype IDs, the Hospitality, Trade / Shop and Craft / Workshop subtype
//! tables, Magic Store Speciality, the Weaponsmith / Armoursmith split, the
//! Specialist Artisan reroll and the future Quality scale. Test names carry
//! the numbers of the approved test list.
//!
//! Scripted dice for one Trading Post location: `[1 (count: one location),
//! Type, subtype…]`. Type 1 is Hospitality, 4 Trade / Shop, 7 Craft /
//! Workshop and 9 Religious (no subtype).

use std::collections::HashSet;
use std::sync::Arc;

use settlement_core::bundled::bundled_content;
use settlement_core::content::ContentSet;
use settlement_core::db::Database;
use settlement_core::dice::{RandomFaces, ScriptedFaces};
use settlement_core::engine::{GenerationError, GenerationOptions, Generator, TableRollRecord};
use settlement_core::ids::{ResultId, TableId};
use settlement_core::locations::{
    LocationDetail, LocationSource, NotableLocation, NotableLocations, SubtypeResolution,
};
use settlement_core::quality::{Quality, QualityShift};
use settlement_core::service::AppService;
use settlement_core::session::{GenerationMode, GenerationSession, ViewOptions};
use settlement_core::settlements::SettlementSnapshot;
use settlement_core::validation::{IssueKind, Severity, validate};

const SIZE: usize = 5;
const LOCATIONS: usize = 18;
const HOSPITALITY: &str = "table.notableLocationSubtype.hospitality";
const TRADE: &str = "table.notableLocationSubtype.trade";
const CRAFT: &str = "table.notableLocationSubtype.craft";
const SPLIT: &str = "table.notableLocationSubtype.weaponsmithArmoursmith";
const SPECIALITY: &str = "table.magicStoreSpeciality";

fn content() -> ContentSet {
    bundled_content().unwrap()
}

/// Rolls `table` once on each face: `(face, result ID, result name)`.
fn every_face(table: &str, faces: i32) -> Vec<(i32, String, String)> {
    let c = content();
    let generator = Generator::new(&c).unwrap();
    let id = TableId::new(table);
    (1..=faces)
        .map(|face| {
            let mut dice = ScriptedFaces::new([face as u32]);
            let log = generator.run(std::slice::from_ref(&id), &mut dice).unwrap();
            assert_eq!(dice.remaining(), 0, "{table}: one die per roll");
            let r = log.record_for(&id).unwrap();
            (face, r.result.id.to_string(), r.result.name.clone())
        })
        .collect()
}

/// A complete settlement of `size`, its Notable Locations rolled on
/// exactly `faces` (every one is used).
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
    assert_eq!(dice.remaining(), 0, "{size} {faces:?}: every die is used");
    s
}

/// A complete settlement of `size`, every die (the Notable Locations' too)
/// from seeded software dice.
fn random_settlement(size: &str, bare_minimum_buildings: bool, seed: u64) -> GenerationSession {
    let mut s = GenerationSession::new(
        Arc::new(content()),
        GenerationOptions {
            god_mode: true,
            bare_minimum_buildings,
        },
        GenerationMode::OneTable,
    )
    .unwrap();
    let mut dice = RandomFaces::seeded(seed);
    s.choose(0, "kurovia".into(), &mut dice).unwrap();
    s.roll_remaining(&mut dice).unwrap();
    s.choose(SIZE, size.into(), &mut dice).unwrap();
    s.roll_remaining(&mut dice).unwrap();
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

/// The one rolled location of a Trading Post rolled on `[1, faces…]`.
fn one(faces: &[u32]) -> NotableLocation {
    let all: Vec<u32> = std::iter::once(1).chain(faces.iter().copied()).collect();
    let r = record(&settlement("tradingPost", false, &all));
    assert_eq!(r.locations.len(), 1);
    r.locations[0].clone()
}

fn subtype(l: &NotableLocation) -> (String, String) {
    let s = l.subtype.as_ref().expect("a subtype");
    (s.id.to_string(), s.name.clone())
}

fn faces_of(roll: &TableRollRecord) -> Vec<u32> {
    roll.attempts
        .iter()
        .flat_map(|a| a.roll.dice.iter().map(|d| d.value))
        .collect()
}

fn ids(c: &ContentSet, table: &str) -> Vec<String> {
    c.table(&table.into())
        .unwrap()
        .results
        .iter()
        .map(|r| r.id.to_string())
        .collect()
}

// ----- District Count (1–6) -------------------------------------------------------------

#[test]
fn d01_d02_city_rolls_1d6_for_two_to_four_districts() {
    let c = content();
    assert_eq!(
        c.table(&"table.districtCount.city".into())
            .unwrap()
            .dice
            .as_deref(),
        Some("1d6")
    );
    let values: Vec<_> = every_face("table.districtCount.city", 6)
        .into_iter()
        .map(|(_, id, _)| id)
        .collect();
    assert_eq!(values, ["two", "two", "three", "three", "four", "four"]);
    // Through generation: 1 → 2 districts, 6 → 4, each with a stable ID
    // apart from its label.
    for (face, n) in [(1, 2), (6, 4)] {
        let faces: Vec<u32> = std::iter::once(face)
            .chain((0..n).flat_map(|_| [1, 9, 9]))
            .collect();
        let r = record(&settlement("city", false, &faces));
        let districts = r.districts.as_ref().unwrap();
        assert_eq!(districts.roll.dice.as_deref(), Some("1d6"));
        assert_eq!(districts.count, n);
        let ids: Vec<_> = r
            .areas
            .iter()
            .map(|a| a.district.as_ref().unwrap().id.clone())
            .collect();
        let expected: Vec<_> = (1..=n).map(|i| format!("district-{i}")).collect();
        assert_eq!(ids, expected);
    }
    for seed in 0..12 {
        let n = record(&random_settlement("city", false, seed))
            .districts
            .unwrap()
            .count;
        assert!((2..=4).contains(&n), "seed {seed}: {n}");
    }
}

#[test]
fn d03_d04_capital_rolls_1d6_for_three_to_five_districts() {
    let c = content();
    assert_eq!(
        c.table(&"table.districtCount.capital".into())
            .unwrap()
            .dice
            .as_deref(),
        Some("1d6")
    );
    let values: Vec<_> = every_face("table.districtCount.capital", 6)
        .into_iter()
        .map(|(_, id, _)| id)
        .collect();
    assert_eq!(values, ["three", "three", "four", "four", "five", "five"]);
    for (face, n) in [(2, 3), (5, 5)] {
        let faces: Vec<u32> = std::iter::once(face)
            .chain((0..n).flat_map(|_| [1, 9, 9]))
            .collect();
        let r = record(&settlement("capital", false, &faces));
        assert_eq!(r.districts.as_ref().unwrap().count, n);
        assert_eq!(r.areas.len() as u32, n);
    }
    for seed in 0..12 {
        let n = record(&random_settlement("capital", false, seed))
            .districts
            .unwrap()
            .count;
        assert!((3..=5).contains(&n), "seed {seed}: {n}");
    }
}

#[test]
fn d05_trading_posts_villages_and_towns_have_no_districts() {
    for (size, faces) in [
        ("tradingPost", vec![1, 9]),
        ("village", vec![1, 9]),
        ("town", vec![1, 9, 9]),
    ] {
        let mut s = settlement(size, false, &faces);
        let r = record(&s);
        assert_eq!(r.districts, None, "{size}");
        assert_eq!(r.areas.len(), 1, "{size}");
        assert_eq!(r.areas[0].district, None, "{size}");
        assert!(
            !r.areas[0]
                .count
                .roll
                .table
                .id
                .as_str()
                .starts_with("table.districtCount"),
            "{size}"
        );
        assert!(matches!(
            s.roll_district_count(LOCATIONS, &mut RandomFaces::seeded(1)),
            Err(GenerationError::NoDistricts)
        ));
    }
}

#[test]
fn d06_bare_minimum_buildings_is_once_per_settlement_not_per_district() {
    for size in ["city", "capital"] {
        for seed in 0..8 {
            let r = record(&random_settlement(size, true, seed));
            let guaranteed: Vec<_> = r
                .locations
                .iter()
                .filter(|l| l.source == LocationSource::BareMinimumBuildings)
                .collect();
            assert_eq!(guaranteed.len(), 1, "{size} seed {seed}");
            assert_eq!(guaranteed[0].district, None);
            assert!(r.areas.len() >= 2);
            assert_eq!(r.locations.len() as u32, r.rolled_count() + 1);
        }
    }
}

// ----- shared subtype IDs (7–11) ----------------------------------------------------------

#[test]
fn s07_every_subtype_table_result_is_one_canonical_subtype() {
    let c = content();
    let catalogue: Vec<_> = c
        .location_subtypes
        .iter()
        .map(|s| s.id.to_string())
        .collect();
    let unique: HashSet<_> = catalogue.iter().collect();
    assert_eq!(unique.len(), catalogue.len(), "each subtype listed once");
    let names: HashSet<_> = c.location_subtypes.iter().map(|s| &s.name).collect();
    assert_eq!(names.len(), catalogue.len(), "no concept listed twice");
    for table in [HOSPITALITY, TRADE, CRAFT, SPLIT] {
        for id in ids(&c, table) {
            assert!(catalogue.contains(&id), "{table}: {id} is canonical");
            assert!(id.starts_with("locationSubtype."), "{id}");
        }
    }
    for id in &catalogue {
        assert!(!id.contains("shop.") && !id.contains("workshop."), "{id}");
    }
    // A subtype in two tables is still one catalogue entry.
    for shared in [
        "locationSubtype.generalSmithy",
        "locationSubtype.bowyerFletcher",
        "locationSubtype.alchemistApothecary",
        "locationSubtype.jeweller",
    ] {
        assert!(ids(&c, TRADE).contains(&shared.to_owned()));
        assert!(ids(&c, CRAFT).contains(&shared.to_owned()));
        assert_eq!(catalogue.iter().filter(|i| *i == shared).count(), 1);
    }
    // Which Type rolls which table.
    let links: Vec<_> = c
        .location_subtype_tables
        .iter()
        .map(|l| (l.location_type.to_string(), l.table.to_string()))
        .collect();
    assert_eq!(
        links,
        [
            (
                "locationType.hospitality".to_owned(),
                HOSPITALITY.to_owned()
            ),
            ("locationType.trade".to_owned(), TRADE.to_owned()),
            ("locationType.craft".to_owned(), CRAFT.to_owned()),
        ]
    );
}

/// The same subtype from Trade / Shop (Type 4) and Craft / Workshop
/// (Type 7): one ID, the parent Type kept apart.
fn shared(trade_face: u32, craft_face: u32, id: &str) -> (NotableLocation, NotableLocation) {
    let trade = one(&[4, trade_face]);
    let craft = one(&[7, craft_face]);
    assert_eq!(subtype(&trade).0, id);
    assert_eq!(subtype(&craft).0, id);
    assert_eq!(trade.location_type.id.as_str(), "locationType.trade");
    assert_eq!(craft.location_type.id.as_str(), "locationType.craft");
    (trade, craft)
}

#[test]
fn s08_general_smithy_is_one_subtype_from_trade_and_craft() {
    shared(1, 1, "locationSubtype.generalSmithy");
    shared(8, 4, "locationSubtype.generalSmithy");
}

#[test]
fn s09_bowyer_fletcher_is_one_subtype_from_trade_and_craft() {
    shared(17, 13, "locationSubtype.bowyerFletcher");
}

#[test]
fn s10_alchemist_apothecary_is_one_subtype_from_trade_and_craft() {
    shared(9, 15, "locationSubtype.alchemistApothecary");
}

#[test]
fn s11_both_jewellers_are_one_subtype_with_their_own_labels() {
    let (trade, craft) = shared(20, 19, "locationSubtype.jeweller");
    assert_eq!(subtype(&trade).1, "Jeweller / Precious Goods");
    assert_eq!(subtype(&craft).1, "Jeweller / Goldsmith");
}

// ----- Hospitality (12–16) -----------------------------------------------------------------

#[test]
fn h12_hospitality_uses_1d10() {
    let c = content();
    let t = c.table(&HOSPITALITY.into()).unwrap();
    assert_eq!(t.name, "Hospitality");
    assert_eq!(t.dice.as_deref(), Some("1d10"));
    let l = one(&[1, 10]);
    let roll = &l.subtype_rolls.as_ref().unwrap().roll;
    assert_eq!(roll.table.id.as_str(), HOSPITALITY);
    assert_eq!(roll.dice.as_deref(), Some("1d10"));
}

#[test]
fn h13_h14_h15_hospitality_faces() {
    for (face, id, name) in every_face(HOSPITALITY, 10) {
        let expected = match face {
            1..=6 => ("locationSubtype.tavernInn", "Tavern / Inn"),
            7..=8 => ("locationSubtype.eatingHouse", "Eating House"),
            _ => ("locationSubtype.bathhouse", "Bathhouse"),
        };
        assert_eq!((id.as_str(), name.as_str()), expected, "face {face}");
        let l = one(&[1, face as u32]);
        assert_eq!(subtype(&l).0, expected.0, "face {face}");
        assert_eq!(l.location_type.id.as_str(), "locationType.hospitality");
    }
    // Tavern and Inn are one subtype, not two.
    let c = content();
    assert_eq!(ids(&c, HOSPITALITY).len(), 3);
    assert!(
        c.location_subtypes
            .iter()
            .all(|s| s.name != "Tavern" && s.name != "Inn")
    );
}

#[test]
fn h16_the_guaranteed_tavern_inn_is_assigned_with_four_spare_rooms() {
    // Type 9 rolls no subtype: the two dice are the count and the Type, so
    // nothing was rolled for the Tavern / Inn.
    let r = record(&settlement("tradingPost", true, &[1, 9]));
    let tavern = r.guaranteed().unwrap();
    assert_eq!(tavern.location_type.id.as_str(), "locationType.hospitality");
    assert_eq!(
        tavern.subtype.as_ref().unwrap().id.as_str(),
        "locationSubtype.tavernInn"
    );
    assert_eq!(tavern.subtype.as_ref().unwrap().name, "Tavern / Inn");
    assert_eq!(tavern.spare_rooms, Some(4));
    assert_eq!(tavern.source, LocationSource::BareMinimumBuildings);
    assert_eq!(tavern.type_roll, None);
    assert_eq!(tavern.subtype_rolls, None);
    assert_eq!(tavern.speciality, None);
    assert_eq!(tavern.quality_shift, None);
}

// ----- Trade / Shop (17–25) -----------------------------------------------------------------

const TRADE_FACES: [(i32, i32, &str, &str); 6] = [
    (1, 8, "locationSubtype.generalSmithy", "General Smithy"),
    (
        9,
        13,
        "locationSubtype.alchemistApothecary",
        "Alchemist / Apothecary",
    ),
    (14, 16, "locationSubtype.magicStore", "Magic Store"),
    (
        17,
        18,
        "locationSubtype.bowyerFletcher",
        "Bowyer / Fletcher",
    ),
    (
        19,
        19,
        "locationSubtype.rareGoods",
        "Rare Goods / Curiosities",
    ),
    (
        20,
        20,
        "locationSubtype.jeweller",
        "Jeweller / Precious Goods",
    ),
];

#[test]
fn tr17_trade_uses_1d20() {
    let c = content();
    let t = c.table(&TRADE.into()).unwrap();
    assert_eq!(t.name, "Trade / Shop");
    assert_eq!(t.dice.as_deref(), Some("1d20"));
    let l = one(&[4, 20]);
    let roll = &l.subtype_rolls.as_ref().unwrap().roll;
    assert_eq!(roll.dice.as_deref(), Some("1d20"));
    assert_eq!(roll.table.name, "Trade / Shop");
}

#[test]
fn tr18_to_tr23_trade_faces() {
    for (face, id, name) in every_face(TRADE, 20) {
        let (_, _, eid, ename) = TRADE_FACES
            .iter()
            .find(|(a, b, _, _)| (*a..=*b).contains(&face))
            .unwrap();
        assert_eq!((id.as_str(), name.as_str()), (*eid, *ename), "face {face}");
    }
    for (min, max, id, name) in TRADE_FACES {
        for face in [min, max] {
            let mut faces = vec![4, face as u32];
            if id == "locationSubtype.magicStore" {
                faces.push(1);
            }
            let l = one(&faces);
            assert_eq!(subtype(&l), (id.to_owned(), name.to_owned()), "face {face}");
        }
    }
}

#[test]
fn tr24_tr25_no_general_store_or_market() {
    let c = content();
    for table in [HOSPITALITY, TRADE, CRAFT, SPLIT, SPECIALITY] {
        for r in &c.table(&table.into()).unwrap().results {
            let text = format!("{} {}", r.id, r.name).to_lowercase();
            assert!(!text.contains("general store"), "{text}");
            assert!(!text.contains("generalstore"), "{text}");
            assert!(!text.contains("market"), "{text}");
        }
    }
    for s in &c.location_subtypes {
        let text = format!("{} {}", s.id, s.name).to_lowercase();
        assert!(!text.contains("store") || s.id.as_str() == "locationSubtype.magicStore");
        assert!(!text.contains("market"), "{text}");
    }
}

// ----- Magic Store (26–28) ------------------------------------------------------------------

#[test]
fn m26_a_magic_store_rolls_exactly_one_1d6_speciality() {
    for face in 14..=16 {
        // Count, Type, Trade, then one d6: no die is left over or missing.
        let l = one(&[4, face, 5]);
        assert_eq!(subtype(&l).0, "locationSubtype.magicStore");
        let roll = l.speciality_roll.as_ref().unwrap();
        assert_eq!(roll.table.id.as_str(), SPECIALITY);
        assert_eq!(roll.dice.as_deref(), Some("1d6"));
        assert_eq!(roll.attempts.len(), 1);
        assert_eq!(faces_of(roll), [5]);
    }
    // Child data of the store, not a location of its own.
    let r = record(&settlement("tradingPost", false, &[1, 4, 14, 2]));
    assert_eq!(r.locations.len(), 1);
}

#[test]
fn m27_every_speciality_face() {
    let expected = [
        (
            "magicStoreSpeciality.armourProtection",
            "Armour & Protection",
        ),
        (
            "magicStoreSpeciality.booksScrollsLore",
            "Books, Scrolls & Lore",
        ),
        ("magicStoreSpeciality.clothing", "Clothing"),
        ("magicStoreSpeciality.jewellery", "Jewellery"),
        ("magicStoreSpeciality.weapons", "Weapons"),
        (
            "magicStoreSpeciality.curiositiesWondrous",
            "Curiosities & Wondrous Items",
        ),
    ];
    for (face, id, name) in every_face(SPECIALITY, 6) {
        assert_eq!(
            (id.as_str(), name.as_str()),
            expected[face as usize - 1],
            "face {face}"
        );
        let l = one(&[4, 15, face as u32]);
        let s = l.speciality.as_ref().unwrap();
        assert_eq!(
            (s.id.as_str(), s.name.as_str()),
            expected[face as usize - 1]
        );
    }
}

#[test]
fn m28_only_magic_stores_roll_a_speciality() {
    let mut cases: Vec<Vec<u32>> = (1..=20)
        .filter(|f| !(14..=16).contains(f))
        .map(|f| vec![4, f])
        .collect();
    cases.extend((1..=10).map(|f| vec![1, f]));
    cases.extend([1, 9, 13, 15, 17, 18, 19].map(|f| vec![7, f]));
    cases.push(vec![9]);
    for faces in cases {
        // `one` checks every die is used: a speciality roll would need one
        // more.
        let l = one(&faces);
        assert_eq!(l.speciality, None, "{faces:?}");
        assert_eq!(l.speciality_roll, None, "{faces:?}");
    }
}

// ----- Craft / Workshop (29–35) -------------------------------------------------------------

const CRAFT_FACES: [(i32, i32, &str, &str); 9] = [
    (1, 4, "locationSubtype.generalSmithy", "General Smithy"),
    (
        5,
        8,
        "locationSubtype.weaponsmithArmoursmith",
        "Weaponsmith / Armoursmith",
    ),
    (9, 12, "locationSubtype.builderMason", "Builder / Mason"),
    (
        13,
        14,
        "locationSubtype.bowyerFletcher",
        "Bowyer / Fletcher",
    ),
    (
        15,
        16,
        "locationSubtype.alchemistApothecary",
        "Alchemist / Apothecary",
    ),
    (
        17,
        17,
        "locationSubtype.engineerMechanist",
        "Engineer / Mechanist",
    ),
    (18, 18, "locationSubtype.tailorWeaver", "Tailor / Weaver"),
    (19, 19, "locationSubtype.jeweller", "Jeweller / Goldsmith"),
    (
        20,
        20,
        "locationSubtype.specialistArtisan",
        "Specialist Artisan",
    ),
];

#[test]
fn c29_craft_uses_1d20() {
    let c = content();
    let t = c.table(&CRAFT.into()).unwrap();
    assert_eq!(t.name, "Craft / Workshop");
    assert_eq!(t.dice.as_deref(), Some("1d20"));
    let l = one(&[7, 9]);
    let roll = &l.subtype_rolls.as_ref().unwrap().roll;
    assert_eq!(roll.dice.as_deref(), Some("1d20"));
    assert_eq!(roll.table.id.as_str(), CRAFT);
}

#[test]
fn c30_every_craft_face() {
    for (face, id, name) in every_face(CRAFT, 20) {
        let (_, _, eid, ename) = CRAFT_FACES
            .iter()
            .find(|(a, b, _, _)| (*a..=*b).contains(&face))
            .unwrap();
        assert_eq!((id.as_str(), name.as_str()), (*eid, *ename), "face {face}");
    }
    // Through generation the final results are the table's own, except the
    // two that need more dice.
    for (min, max, id, name) in CRAFT_FACES
        .iter()
        .filter(|(a, _, _, _)| *a != 5 && *a != 20)
    {
        for face in [*min, *max] {
            let l = one(&[7, face as u32]);
            assert_eq!(subtype(&l), ((*id).to_owned(), (*name).to_owned()));
            let rolls = l.subtype_rolls.as_ref().unwrap();
            assert!(rolls.rerolls.is_empty() && rolls.split.is_none());
        }
    }
}

#[test]
fn c31_c32_c33_weaponsmith_armoursmith_is_an_even_1d4_split() {
    let c = content();
    let t = c.table(&SPLIT.into()).unwrap();
    assert_eq!(t.name, "Weaponsmith / Armoursmith");
    assert_eq!(t.dice.as_deref(), Some("1d4"));
    for craft in 5..=8 {
        for d4 in 1..=4 {
            let l = one(&[7, craft, d4]);
            let rolls = l.subtype_rolls.as_ref().unwrap();
            // The original Craft roll and the d4 are both kept.
            assert_eq!(faces_of(&rolls.roll), [craft]);
            assert_eq!(
                rolls.roll.result.id.as_str(),
                "locationSubtype.weaponsmithArmoursmith"
            );
            let split = rolls.split.as_ref().unwrap();
            assert_eq!(split.table.id.as_str(), SPLIT);
            assert_eq!(split.dice.as_deref(), Some("1d4"));
            assert_eq!(faces_of(split), [d4]);
            let expected = if d4 <= 2 {
                ("locationSubtype.weaponsmith", "Weaponsmith")
            } else {
                ("locationSubtype.armoursmith", "Armoursmith")
            };
            assert_eq!(
                subtype(&l),
                (expected.0.to_owned(), expected.1.to_owned()),
                "craft {craft}, d4 {d4}"
            );
        }
    }
}

#[test]
fn c34_c35_builder_and_engineer_keep_their_future_keep_upgrade_metadata() {
    for (face, id, capability) in [
        (9, "locationSubtype.builderMason", "keepUpgrade.structural"),
        (
            17,
            "locationSubtype.engineerMechanist",
            "keepUpgrade.mechanical",
        ),
    ] {
        let l = one(&[7, face]);
        assert_eq!(subtype(&l).0, id);
        let caps: Vec<_> = l.capabilities.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(caps, [capability]);
        // And in a saved settlement.
        let json = serde_json::to_string(&l).unwrap();
        assert!(json.contains(capability), "{json}");
        let c = content();
        let s = c
            .location_subtypes
            .iter()
            .find(|s| s.id.as_str() == id)
            .unwrap();
        assert!(s.capabilities[0].name.starts_with("Keep upgrades"));
    }
    // No other subtype claims them.
    assert!(one(&[7, 1]).capabilities.is_empty());
}

// ----- Specialist Artisan (36–42) -----------------------------------------------------------

#[test]
fn a36_a37_specialist_artisan_rolls_craft_again_for_the_final_subtype() {
    let l = one(&[7, 20, 13]);
    let rolls = l.subtype_rolls.as_ref().unwrap();
    assert_eq!(
        rolls.roll.result.id.as_str(),
        "locationSubtype.specialistArtisan"
    );
    assert_eq!(faces_of(&rolls.roll), [20]);
    assert_eq!(rolls.rerolls.len(), 1);
    assert_eq!(rolls.rerolls[0].table.id.as_str(), CRAFT);
    assert_eq!(faces_of(&rolls.rerolls[0]), [13]);
    assert_eq!(
        subtype(&l),
        (
            "locationSubtype.bowyerFletcher".into(),
            "Bowyer / Fletcher".into()
        )
    );
}

#[test]
fn a38_a40_every_repeat_is_recorded_and_the_shift_never_stacks() {
    let l = one(&[7, 20, 20, 20, 13]);
    let rolls = l.subtype_rolls.as_ref().unwrap();
    let rerolled: Vec<_> = rolls.rerolls.iter().flat_map(faces_of).collect();
    assert_eq!(rerolled, [20, 20, 13]);
    assert!(
        rolls
            .rerolls
            .iter()
            .all(|r| r.dice.as_deref() == Some("1d20"))
    );
    assert_eq!(subtype(&l).0, "locationSubtype.bowyerFletcher");
    let shift = l.quality_shift.as_ref().unwrap();
    assert_eq!(shift.steps, 1, "one step, however many 20s");
    assert_eq!(
        shift.source.id.as_str(),
        "locationSubtype.specialistArtisan"
    );
}

#[test]
fn a39_specialist_artisan_gives_one_future_quality_step() {
    let l = one(&[7, 20, 1]);
    assert_eq!(
        l.quality_shift,
        Some(QualityShift {
            steps: 1,
            source: settlement_core::engine::Named {
                id: ResultId::new("locationSubtype.specialistArtisan"),
                name: "Specialist Artisan".into(),
            },
        })
    );
    // Nothing else shifts Quality.
    for faces in [vec![7, 1], vec![7, 5, 1], vec![4, 14, 1], vec![1, 1]] {
        assert_eq!(one(&faces).quality_shift, None, "{faces:?}");
    }
}

#[test]
fn a_specialist_artisan_can_resolve_into_a_split() {
    // 20 → reroll 6 (Weaponsmith / Armoursmith) → d4 3: an Armoursmith,
    // one step better.
    let l = one(&[7, 20, 6, 3]);
    let rolls = l.subtype_rolls.as_ref().unwrap();
    assert_eq!(faces_of(&rolls.rerolls[0]), [6]);
    assert_eq!(faces_of(rolls.split.as_ref().unwrap()), [3]);
    assert_eq!(subtype(&l).0, "locationSubtype.armoursmith");
    assert_eq!(l.quality_shift.as_ref().unwrap().steps, 1);
}

#[test]
fn a41_the_shift_caps_at_excellent() {
    let shift = one(&[7, 20, 1]).quality_shift.unwrap();
    assert_eq!(shift.apply(Quality::Average), Quality::AboveAverage);
    assert_eq!(shift.apply(Quality::AboveAverage), Quality::Excellent);
    assert_eq!(shift.apply(Quality::Excellent), Quality::Excellent);
    let big = QualityShift { steps: 9, ..shift };
    assert_eq!(big.apply(Quality::Poor), Quality::Excellent);
    assert_eq!(Quality::SCALE.last(), Some(&Quality::Excellent));
}

#[test]
fn a42_no_quality_distribution_exists() {
    let c = content();
    for t in &c.tables {
        let text = format!("{} {}", t.id, t.name).to_lowercase();
        assert!(!text.contains("quality"), "{text}: no Quality table yet");
    }
    // No location is given a Quality; only the pending shift is stored.
    for seed in 0..6 {
        let r = record(&random_settlement("capital", true, seed));
        let json = serde_json::to_value(&r).unwrap();
        for l in json["locations"].as_array().unwrap() {
            assert!(l.get("quality").is_none(), "{l}");
        }
    }
}

// ----- automatic generation ---------------------------------------------------------------

#[test]
fn automatic_generation_follows_the_same_tree() {
    let c = content();
    let resolver = |id: &ResultId| {
        c.location_subtypes
            .iter()
            .any(|s| &s.id == id && s.resolve.is_some())
    };
    for seed in 0..30 {
        let r = record(&random_settlement("capital", false, seed));
        for l in &r.locations {
            let rolls = l.subtype_rolls.as_ref();
            let t = l.location_type.id.as_str();
            let has_table = matches!(
                t,
                "locationType.hospitality" | "locationType.trade" | "locationType.craft"
            );
            assert_eq!(rolls.is_some(), has_table, "{t}");
            assert_eq!(l.subtype.is_some(), has_table, "{t}");
            let Some(rolls) = rolls else { continue };
            let s = &l.subtype.as_ref().unwrap().id;
            assert!(!resolver(s), "{s} is never final");
            assert_eq!(
                l.speciality.is_some(),
                s.as_str() == "locationSubtype.magicStore"
            );
            assert_eq!(
                l.quality_shift.is_some(),
                rolls.roll.result.id.as_str() == "locationSubtype.specialistArtisan"
            );
            assert_eq!(
                rolls.split.is_some(),
                rolls.resolved().result.id.as_str() == "locationSubtype.weaponsmithArmoursmith"
            );
        }
    }
}

/// Every die of a record, in the order generation rolls them.
fn recorded_faces(r: &NotableLocations) -> Vec<u32> {
    let mut out = Vec::new();
    if let Some(d) = &r.districts {
        out.extend(faces_of(&d.roll));
    }
    for area in &r.areas {
        out.extend(faces_of(&area.count.roll));
        let district = area.district.as_ref().map(|d| &d.id);
        for l in r
            .locations
            .iter()
            .filter(|l| l.source == LocationSource::Rolled && l.district.as_ref() == district)
        {
            out.extend(faces_of(l.type_roll.as_ref().unwrap()));
            if let Some(rolls) = &l.subtype_rolls {
                out.extend(faces_of(&rolls.roll));
                rolls.rerolls.iter().for_each(|x| out.extend(faces_of(x)));
                rolls.split.iter().for_each(|x| out.extend(faces_of(x)));
            }
            l.speciality_roll
                .iter()
                .for_each(|x| out.extend(faces_of(x)));
        }
    }
    out
}

#[test]
fn every_random_choice_is_physically_reproducible() {
    for (size, seed) in [("capital", 3), ("city", 7), ("town", 11), ("village", 5)] {
        let mut s = random_settlement(size, true, seed);
        let original = record(&s);
        let faces = recorded_faces(&original);
        // Rolling again with the recorded dice entered by hand gives the
        // same locations, every subtype roll included.
        let mut dice = ScriptedFaces::new(faces.iter().copied());
        s.roll(LOCATIONS, &mut dice).unwrap();
        assert_eq!(dice.remaining(), 0, "{size}: every die is recorded");
        assert_eq!(record(&s), original, "{size}");
    }
}

// ----- manual generation ---------------------------------------------------------------------

#[test]
fn manual_modes_offer_only_the_child_tables_that_apply() {
    // Trading Post 2: a Magic Store (Weapons) and a Specialist Artisan
    // Weaponsmith; then a Bare Minimum Buildings Tavern / Inn.
    let s = settlement("tradingPost", true, &[4, 4, 15, 5, 7, 20, 5, 1]);
    let view = s.view(LOCATIONS, ViewOptions::default()).unwrap();
    let locations = view.locations.unwrap();
    let details = &locations.options.details;
    let r = locations.record.unwrap();
    assert_eq!(details.len(), r.locations.len());
    // The Tavern / Inn: nothing to roll.
    assert_eq!(details[0], Default::default());
    // The Magic Store: Trade / Shop and its speciality only.
    let store = &details[1];
    assert_eq!(store.subtype.as_ref().unwrap().table.id.as_str(), TRADE);
    assert_eq!(
        store.speciality.as_ref().unwrap().table.id.as_str(),
        SPECIALITY
    );
    assert!(store.reroll.is_none() && store.split.is_none());
    // The Specialist Artisan: Craft, its reroll (never Specialist Artisan
    // again) and the split.
    let artisan = &details[2];
    assert_eq!(artisan.subtype.as_ref().unwrap().table.id.as_str(), CRAFT);
    let reroll = artisan.reroll.as_ref().unwrap();
    assert_eq!(reroll.table.id.as_str(), CRAFT);
    assert_eq!(reroll.options.len(), 8);
    assert!(
        reroll
            .options
            .iter()
            .all(|o| o.result.id.as_str() != "locationSubtype.specialistArtisan")
    );
    assert_eq!(artisan.split.as_ref().unwrap().table.id.as_str(), SPLIT);
    assert!(artisan.speciality.is_none());
    // A Religious location has no subtype table.
    let s = settlement("tradingPost", false, &[1, 9]);
    let view = s.view(LOCATIONS, ViewOptions::default()).unwrap();
    assert_eq!(
        view.locations.unwrap().options.details[0],
        Default::default()
    );
}

#[test]
fn choosing_a_subtype_rolls_what_follows_from_it() {
    let mut s = settlement("tradingPost", false, &[1, 4, 1]);
    // Magic Store: its speciality is rolled (5: Weapons).
    s.choose_location_detail(
        LOCATIONS,
        0,
        LocationDetail::Subtype,
        "locationSubtype.magicStore".into(),
        &mut ScriptedFaces::new([5]),
    )
    .unwrap();
    let l = record(&s).locations[0].clone();
    let rolls = l.subtype_rolls.as_ref().unwrap();
    assert!(rolls.roll.selected);
    assert_eq!(
        l.speciality.unwrap().id.as_str(),
        "magicStoreSpeciality.weapons"
    );
    // Then the speciality alone.
    s.choose_location_detail(
        LOCATIONS,
        0,
        LocationDetail::Speciality,
        "magicStoreSpeciality.clothing".into(),
        &mut ScriptedFaces::new([]),
    )
    .unwrap();
    let l = record(&s).locations[0].clone();
    assert_eq!(subtype(&l).0, "locationSubtype.magicStore");
    assert_eq!(l.speciality.unwrap().name, "Clothing");
    assert!(l.speciality_roll.unwrap().selected);
    // Back to a General Smithy: the speciality goes.
    s.roll_location_detail(
        LOCATIONS,
        0,
        LocationDetail::Subtype,
        &mut ScriptedFaces::new([3]),
    )
    .unwrap();
    let l = record(&s).locations[0].clone();
    assert_eq!(subtype(&l).0, "locationSubtype.generalSmithy");
    assert_eq!(l.speciality, None);
    assert_eq!(l.speciality_roll, None);
}

#[test]
fn a_chosen_specialist_artisan_rolls_again_and_its_reroll_can_be_chosen() {
    let mut s = settlement("tradingPost", false, &[1, 7, 1]);
    // Chosen Specialist Artisan: the reroll is still rolled (20, then 9).
    s.choose_location_detail(
        LOCATIONS,
        0,
        LocationDetail::Subtype,
        "locationSubtype.specialistArtisan".into(),
        &mut ScriptedFaces::new([20, 9]),
    )
    .unwrap();
    let l = record(&s).locations[0].clone();
    let rolls = l.subtype_rolls.as_ref().unwrap();
    assert_eq!(rolls.rerolls.len(), 2);
    assert_eq!(subtype(&l).0, "locationSubtype.builderMason");
    assert_eq!(l.quality_shift.as_ref().unwrap().steps, 1);
    // Choosing what the artisan makes replaces the rerolls.
    s.choose_location_detail(
        LOCATIONS,
        0,
        LocationDetail::Reroll,
        "locationSubtype.tailorWeaver".into(),
        &mut ScriptedFaces::new([]),
    )
    .unwrap();
    let l = record(&s).locations[0].clone();
    let rolls = l.subtype_rolls.as_ref().unwrap();
    assert_eq!(rolls.rerolls.len(), 1);
    assert!(rolls.rerolls[0].selected);
    assert_eq!(subtype(&l).0, "locationSubtype.tailorWeaver");
    assert_eq!(l.quality_shift.as_ref().unwrap().steps, 1);
    // The artisan cannot make itself.
    let err = s
        .choose_location_detail(
            LOCATIONS,
            0,
            LocationDetail::Reroll,
            "locationSubtype.specialistArtisan".into(),
            &mut ScriptedFaces::new([]),
        )
        .unwrap_err();
    assert!(matches!(err, GenerationError::RerollChoice { .. }), "{err}");
    // A chosen Weaponsmith / Armoursmith reroll rolls its d4 (4).
    s.choose_location_detail(
        LOCATIONS,
        0,
        LocationDetail::Reroll,
        "locationSubtype.weaponsmithArmoursmith".into(),
        &mut ScriptedFaces::new([4]),
    )
    .unwrap();
    assert_eq!(
        subtype(&record(&s).locations[0]).0,
        "locationSubtype.armoursmith"
    );
    // And the split can be chosen on its own.
    s.choose_location_detail(
        LOCATIONS,
        0,
        LocationDetail::Split,
        "locationSubtype.weaponsmith".into(),
        &mut ScriptedFaces::new([]),
    )
    .unwrap();
    let l = record(&s).locations[0].clone();
    assert_eq!(subtype(&l).0, "locationSubtype.weaponsmith");
    assert_eq!(l.quality_shift.as_ref().unwrap().steps, 1);
}

#[test]
fn details_that_do_not_apply_cannot_be_rolled() {
    // A General Smithy: no reroll, split or speciality.
    let mut s = settlement("tradingPost", true, &[1, 4, 1]);
    for detail in [
        LocationDetail::Reroll,
        LocationDetail::Split,
        LocationDetail::Speciality,
    ] {
        let err = s
            .roll_location_detail(LOCATIONS, 1, detail, &mut RandomFaces::seeded(1))
            .unwrap_err();
        assert!(
            matches!(err, GenerationError::NoLocationDetail { index: 1, .. }),
            "{err}"
        );
    }
    // The Tavern / Inn's subtype is fixed.
    assert!(matches!(
        s.roll_location_detail(
            LOCATIONS,
            0,
            LocationDetail::Subtype,
            &mut RandomFaces::seeded(1)
        ),
        Err(GenerationError::GuaranteedLocationNotRolled)
    ));
    // A Religious location has no subtype.
    let mut s = settlement("tradingPost", false, &[1, 9]);
    assert!(
        s.roll_location_detail(
            LOCATIONS,
            0,
            LocationDetail::Subtype,
            &mut RandomFaces::seeded(1)
        )
        .is_err()
    );
    // Choosing Craft for it rolls a Craft subtype (1: General Smithy).
    s.choose_location_type(
        LOCATIONS,
        0,
        "locationType.craft".into(),
        &mut ScriptedFaces::new([1]),
    )
    .unwrap();
    let l = record(&s).locations[0].clone();
    assert_eq!(l.location_type.id.as_str(), "locationType.craft");
    assert_eq!(subtype(&l).0, "locationSubtype.generalSmithy");
}

// ----- saved settlements ---------------------------------------------------------------------

#[test]
fn snapshots_keep_every_subtype_roll() {
    // A Magic Store, an artisan Armoursmith, in two City districts.
    let s = settlement("city", true, &[1, 1, 4, 14, 6, 9, 1, 7, 20, 20, 7, 3, 9]);
    let snapshot = s.to_snapshot();
    let stored = snapshot.notable_locations().unwrap().clone();
    assert_eq!(stored, record(&s));
    assert_eq!(stored.app_version, settlement_core::VERSION);
    let json = serde_json::to_string(&snapshot).unwrap();
    for field in [
        "\"subtype_rolls\"",
        "\"rerolls\"",
        "\"split\"",
        "\"speciality\"",
        "\"speciality_roll\"",
        "\"quality_shift\"",
        "\"district-2\"",
        "locationSubtype.magicStore",
        "Curiosities & Wondrous Items",
        "locationSubtype.armoursmith",
    ] {
        assert!(json.contains(field), "{field}");
    }
    let back: SettlementSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(back, snapshot);

    // Later edits to the subtype tables and catalogue change nothing saved.
    let mut changed = content();
    for t in changed
        .tables
        .iter_mut()
        .filter(|t| [HOSPITALITY, TRADE, CRAFT, SPLIT, SPECIALITY].contains(&t.id.as_str()))
    {
        for r in &mut t.results {
            r.name = format!("{} (edited)", r.name);
        }
    }
    for st in &mut changed.location_subtypes {
        st.capabilities.clear();
        st.resolve = None;
    }
    let reopened = GenerationSession::from_snapshot(
        Arc::new(changed),
        GenerationOptions::default(),
        GenerationMode::OneTable,
        &back,
    )
    .unwrap();
    assert_eq!(reopened.to_snapshot().stages, snapshot.stages);
}

#[test]
fn locations_saved_before_subtypes_still_load() {
    // A v0.1.5 location: the old Tavern subtype, no subtype rolls.
    let saved = serde_json::json!({
        "id": "notable-location-guaranteed-tavern",
        "source": "bareMinimumBuildings",
        "type": { "id": "locationType.hospitality", "name": "Hospitality" },
        "subtype": { "id": "locationSubtype.tavern", "name": "Tavern" },
        "spare_rooms": 4
    });
    let l: NotableLocation = serde_json::from_value(saved).unwrap();
    assert_eq!(l.subtype.unwrap().name, "Tavern");
    assert_eq!(l.subtype_rolls, None);
    assert!(l.capabilities.is_empty());
}

// ----- table editor ----------------------------------------------------------------------------

#[test]
fn the_new_tables_are_editable_and_keep_their_canonical_ids() {
    let mut s = AppService::new(Database::open_in_memory().unwrap());
    // Trade / Shop shows where its subtypes are shared.
    let detail = s.table_detail(&TRADE.into()).unwrap();
    let smithy = detail
        .subtypes
        .iter()
        .find(|u| u.result.as_str() == "locationSubtype.generalSmithy")
        .unwrap();
    assert_eq!(smithy.subtype.name, "General Smithy");
    assert_eq!(smithy.also_in[0].table.id.as_str(), CRAFT);
    let store = detail
        .subtypes
        .iter()
        .find(|u| u.result.as_str() == "locationSubtype.magicStore")
        .unwrap();
    assert!(
        store.notes[0].contains("Magic Store Speciality"),
        "{:?}",
        store.notes
    );
    // Relabelling a shared subtype keeps its ID, and does not touch Craft.
    let mut draft = detail.table.clone();
    draft.results[0].name = "Smith".into();
    let saved = s.save_table(draft).unwrap();
    assert_eq!(
        saved.table.results[0].id.as_str(),
        "locationSubtype.generalSmithy"
    );
    assert_eq!(saved.table.results[0].name, "Smith");
    let craft = s.table_detail(&CRAFT.into()).unwrap();
    assert_eq!(craft.table.results[0].name, "General Smithy");
    let smithy = craft
        .subtypes
        .iter()
        .find(|u| u.result.as_str() == "locationSubtype.generalSmithy")
        .unwrap();
    assert_eq!(smithy.also_in[0].label, "Smith");
    s.reset_table(&TRADE.into()).unwrap();
    // Every new table can be edited and reset.
    for table in [
        HOSPITALITY,
        CRAFT,
        SPLIT,
        SPECIALITY,
        "table.districtCount.city",
    ] {
        let id = TableId::new(table);
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

#[test]
fn an_added_copy_of_a_canonical_subtype_is_flagged() {
    let mut c = content();
    let t = c
        .tables
        .iter_mut()
        .find(|t| t.id.as_str() == HOSPITALITY)
        .unwrap();
    let mut copy = t.results[0].clone();
    copy.id = ResultId::new("user.result.0001");
    copy.name = "General Smithy".into();
    copy.range = None;
    t.results.push(copy);
    let report = validate(&c);
    assert!(
        report.issues.iter().any(|i| i.severity == Severity::Warning
            && i.kind
                == IssueKind::LooksLikeLocationSubtype {
                    subtype: "locationSubtype.generalSmithy".into()
                }),
        "{report}"
    );
}

#[test]
fn subtype_content_errors_are_caught() {
    let has = |c: &ContentSet, f: &dyn Fn(&IssueKind) -> bool| {
        let report = validate(c);
        report.errors().any(|i| f(&i.kind))
    };
    // A result that is not a canonical subtype.
    let mut c = content();
    let t = c
        .tables
        .iter_mut()
        .find(|t| t.id.as_str() == CRAFT)
        .unwrap();
    t.results[0].id = ResultId::new("workshop.generalSmithy");
    assert!(has(&c, &|k| matches!(
        k,
        IssueKind::UnknownLocationSubtype { .. }
    )));
    // The same concept listed twice.
    let mut c = content();
    let mut twin = c.location_subtypes[3].clone();
    twin.id = ResultId::new("locationSubtype.smithy");
    c.location_subtypes.push(twin);
    assert!(has(&c, &|k| matches!(
        k,
        IssueKind::DuplicateSubtypeName { .. }
    )));
    // A split that splits again.
    let mut c = content();
    let t = c
        .tables
        .iter_mut()
        .find(|t| t.id.as_str() == SPLIT)
        .unwrap();
    t.results[0].id = ResultId::new("locationSubtype.specialistArtisan");
    assert!(has(&c, &|k| *k == IssueKind::SplitResultNotFinal));
    // Every Craft result Specialist Artisan: a reroll could never end.
    let mut c = content();
    let t = c
        .tables
        .iter_mut()
        .find(|t| t.id.as_str() == CRAFT)
        .unwrap();
    for r in t.results.iter_mut().filter(|r| r.range.unwrap().max < 20) {
        r.enabled = false;
    }
    assert!(has(&c, &|k| *k == IssueKind::RerollNeverEnds));
    // The guaranteed Tavern / Inn needs its subtype.
    let mut c = content();
    c.location_subtypes
        .retain(|s| s.id.as_str() != "locationSubtype.tavernInn");
    assert!(has(&c, &|k| matches!(
        k,
        IssueKind::UnknownLocationSubtype { subtype } if subtype.as_str() == "locationSubtype.tavernInn"
    )));
    // A split table must exist.
    let mut c = content();
    for s in &mut c.location_subtypes {
        if let Some(SubtypeResolution::Split { table }) = &mut s.resolve {
            *table = TableId::new("table.missing");
        }
    }
    assert!(has(&c, &|k| matches!(k, IssueKind::MissingTable { .. })));
}

#[test]
fn no_executable_effect_touches_the_subtype_tables() {
    let c = content();
    for tag in &c.tags {
        for def in tag.effects.iter().filter(|d| d.is_active()) {
            if let Some(t) = def.effect.target_table() {
                assert!(
                    ![HOSPITALITY, TRADE, CRAFT, SPLIT, SPECIALITY].contains(&t.as_str()),
                    "{}",
                    tag.id
                );
            }
        }
    }
    for table in [HOSPITALITY, TRADE, CRAFT, SPLIT, SPECIALITY] {
        let t = c.table(&table.into()).unwrap();
        assert!(t.results.iter().all(|r| r.tags.is_empty()), "{table}");
    }
}
