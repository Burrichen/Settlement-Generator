//! Regional naming packs: Halingo, Skgurr, Shinkira, Bratislor, Hjartheland,
//! Nareth and Targoth (Ordon has its own tests). Each region tag selects its
//! own scheme; every table covers its dice exactly; every method result is
//! wired to a method; examples come out of real dice rolls; and each region's
//! joining rule is its own. Skgurr has its own d100 element tables and only
//! the Geographic Compound method.

use std::sync::Arc;

use settlement_core::bundled::bundled_content;
use settlement_core::content::ContentSet;
use settlement_core::db::Database;
use settlement_core::dice::ScriptedFaces;
use settlement_core::engine::GenerationOptions;
use settlement_core::ids::{ResultId, TableId};
use settlement_core::service::AppService;
use settlement_core::session::{GenerationMode, GenerationSession, ViewOptions};

const NAME: usize = 2;

struct Region {
    key: &'static str,
    /// Face of the Kurovia Region d8.
    face: u32,
    name: &'static str,
}

/// The regional packs added after Ordon (Ordon has its own tests).
const REGIONS: [Region; 7] = [
    Region {
        key: "halingo",
        face: 2,
        name: "Halingo",
    },
    Region {
        key: "skgurr",
        face: 3,
        name: "Skgurr",
    },
    Region {
        key: "shinkira",
        face: 4,
        name: "Shinkira",
    },
    Region {
        key: "bratislor",
        face: 5,
        name: "Bratislor",
    },
    Region {
        key: "hjartheland",
        face: 6,
        name: "Hjartheland",
    },
    Region {
        key: "nareth",
        face: 7,
        name: "Nareth",
    },
    Region {
        key: "targoth",
        face: 8,
        name: "Targoth",
    },
];

fn content() -> ContentSet {
    bundled_content().unwrap()
}

fn dice(values: &[u32]) -> ScriptedFaces {
    ScriptedFaces::new(values.iter().copied())
}

fn at_region(c: &ContentSet, face: u32) -> GenerationSession {
    let mut s = GenerationSession::new(
        Arc::new(c.clone()),
        GenerationOptions::default(),
        GenerationMode::OneTable,
    )
    .unwrap();
    s.choose(0, "kurovia".into(), &mut dice(&[])).unwrap();
    s.roll(1, &mut dice(&[face])).unwrap();
    s
}

/// The name rolled at a region with the given method and part faces.
fn name(c: &ContentSet, region: u32, faces: &[u32]) -> String {
    let mut s = at_region(c, region);
    s.roll(NAME, &mut dice(faces)).unwrap();
    s.name().unwrap()
}

fn table<'c>(c: &'c ContentSet, key: &str, suffix: &str) -> &'c settlement_core::content::Table {
    c.table(&TableId::new(format!("table.naming.{key}.{suffix}")))
        .unwrap_or_else(|| panic!("table.naming.{key}.{suffix}"))
}

// ----- routing -------------------------------------------------------------------------

#[test]
fn each_region_tag_selects_its_own_scheme() {
    let c = content();
    for region in REGIONS.iter().chain([&Region {
        key: "ordon",
        face: 1,
        name: "Ordon",
    }]) {
        let s = at_region(&c, region.face);
        let view = s
            .view(
                NAME,
                ViewOptions {
                    show_tags: true,
                    debug: false,
                },
            )
            .unwrap();
        let naming = view.naming.unwrap();
        assert_eq!(
            naming.scheme.as_ref().unwrap().id.as_str(),
            format!("naming.kurovia.{}", region.key)
        );
        let routing: Vec<String> = naming.routing.iter().map(|t| t.id.to_string()).collect();
        assert_eq!(
            routing,
            ["world.kurovia".to_owned(), format!("region.{}", region.key)]
        );
        assert_eq!(view.table.name, format!("{} Naming Method", region.name));
        assert!(view.can_roll);
    }
}

#[test]
fn every_scheme_routes_on_kurovia_and_its_own_region_tag() {
    let c = content();
    for region in &REGIONS {
        let scheme = c
            .naming_schemes
            .iter()
            .find(|s| s.id.as_str() == format!("naming.kurovia.{}", region.key))
            .unwrap();
        let tags: Vec<&str> = scheme.when.tags().iter().map(|t| t.as_str()).collect();
        assert_eq!(tags, ["world.kurovia", &format!("region.{}", region.key)]);
        assert_eq!(
            scheme.method_table.as_str(),
            format!("table.naming.{}.method", region.key)
        );
    }
}

// ----- dice coverage -------------------------------------------------------------------

fn scheme<'c>(c: &'c ContentSet, key: &str) -> &'c settlement_core::naming::NamingScheme {
    c.naming_schemes
        .iter()
        .find(|s| s.id.as_str() == format!("naming.kurovia.{key}"))
        .unwrap()
}

#[test]
fn every_regional_table_covers_its_dice_exactly() {
    let c = content();
    for region in &REGIONS {
        let scheme = scheme(&c, region.key);
        let tables = std::iter::once(&scheme.method_table)
            .chain(scheme.methods.iter().flat_map(|m| m.components.iter()));
        for id in tables {
            let t = c.table(id).unwrap_or_else(|| panic!("{id}"));
            // Skgurr's single method is selected, not rolled.
            let Some(dice) = t.dice.as_deref() else {
                assert_eq!(t.results.len(), 1, "{id}");
                continue;
            };
            let sides: i32 = dice
                .strip_prefix("1d")
                .and_then(|n| n.parse().ok())
                .unwrap_or_else(|| panic!("{id}: {dice}"));
            for face in 1..=sides {
                let owners = t
                    .results
                    .iter()
                    .filter(|r| r.range.is_some_and(|range| range.contains(face)))
                    .count();
                assert_eq!(owners, 1, "{id} face {face}");
            }
        }
    }
}

#[test]
fn skgurr_element_tables_cover_a_d100_exactly() {
    let c = content();
    for suffix in ["firstElement", "secondElement"] {
        let t = table(&c, "skgurr", suffix);
        assert_eq!(t.dice.as_deref(), Some("1d100"), "{}", t.id);
        assert_eq!(t.results.len(), 30, "{}", t.id);
        for face in 1..=100 {
            let owners = t
                .results
                .iter()
                .filter(|r| r.range.is_some_and(|range| range.contains(face)))
                .count();
            assert_eq!(owners, 1, "{} face {face}", t.id);
        }
        // 4 faces each for the first ten entries, 3 for the other twenty.
        let widths: Vec<i32> = t
            .results
            .iter()
            .map(|r| {
                let range = r.range.unwrap();
                range.max - range.min + 1
            })
            .collect();
        assert_eq!(widths[..10], [4; 10], "{}", t.id);
        assert_eq!(widths[10..], [3; 20], "{}", t.id);
    }
}

#[test]
fn skgurr_always_uses_geographic_compound() {
    let c = content();
    // One method and no dice: nothing is rolled to pick it.
    let method = table(&c, "skgurr", "method");
    assert_eq!(method.dice, None);
    assert_eq!(method.results.len(), 1);
    let only = &method.results[0];
    assert_eq!(
        (only.id.as_str(), only.name.as_str()),
        ("geographic", "Geographic Compound")
    );
    let scheme = c
        .naming_schemes
        .iter()
        .find(|s| s.id.as_str() == "naming.kurovia.skgurr")
        .unwrap();
    assert_eq!(scheme.methods.len(), 1);
    // No founder tables ship for Skgurr.
    for suffix in ["founder", "founderEnding"] {
        let id = TableId::new(format!("table.naming.skgurr.{suffix}"));
        assert!(c.table(&id).is_none(), "{id}");
    }
}

#[test]
fn every_method_result_has_its_own_method() {
    let c = content();
    for region in &REGIONS {
        let scheme = scheme(&c, region.key);
        let results: Vec<&str> = c
            .table(&scheme.method_table)
            .unwrap()
            .results
            .iter()
            .map(|r| r.id.as_str())
            .collect();
        let methods: Vec<&str> = scheme.methods.iter().map(|m| m.result.as_str()).collect();
        assert_eq!(results, methods, "{}", region.key);
    }
}

#[test]
fn halingo_never_generates_heresy() {
    // Heresy is an established Halingo settlement: it is reserved.
    let c = content();
    let scheme = scheme(&c, "halingo");
    for id in scheme.methods.iter().flat_map(|m| m.components.iter()) {
        let t = c.table(id).unwrap();
        assert!(
            t.results
                .iter()
                .all(|r| !r.name.eq_ignore_ascii_case("heresy")),
            "{id}"
        );
    }
}

// ----- names from real dice ----------------------------------------------------------------

/// `[method face, part faces…]`: one face per component of the method.
fn check(region: u32, cases: &[(&[u32], &str)]) {
    let c = content();
    for (faces, expected) in cases {
        assert_eq!(name(&c, region, faces), *expected, "dice {faces:?}");
    }
}

#[test]
fn halingo_names() {
    check(
        2,
        &[
            (&[1, 1, 1], "Khar Well"),
            (&[1, 7, 7], "Dun Rest"),
            (&[16, 1, 1], "Last Well"),
            (&[28, 5, 3], "Silver Lode"),
            (&[38, 10, 4], "Black Depot"),
            (&[44, 1], "Cinderford"),
            (&[70, 1, 1], "Varesh Well"),
            (&[73, 1], "Kharaven"),
            (&[97, 1, 1], "Broken Beacon"),
            (&[99, 1], "Dissent"),
        ],
    );
}

#[test]
fn skgurr_names() {
    let c = content();
    for (faces, expected) in [
        ([1, 1], "Dunwick"),
        ([17, 5], "Inverness"),
        ([68, 29], "Greyhaven"),
        ([9, 21], "Strathmuir"),
        ([64, 36], "Skelport"),
        ([92, 77], "Sronvoe"),
        ([100, 71], "Muckleskerry"),
        ([5, 100], "Glensound"),
    ] {
        assert_eq!(name(&c, 3, &faces), expected, "dice {faces:?}");
    }
    // The method is recorded as selected, not rolled.
    let mut s = at_region(&c, 3);
    s.roll(NAME, &mut dice(&[1, 1])).unwrap();
    let record = s
        .to_snapshot()
        .stages
        .iter()
        .find_map(|st| st.name.clone())
        .unwrap();
    assert_eq!(record.rolls[0].result.id, ResultId::new("geographic"));
    assert!(record.rolls[0].selected);
    assert_eq!(record.rolls[0].dice, None);
}

#[test]
fn shinkira_names() {
    check(
        4,
        &[
            (&[1, 7, 4], "Hanasawa"),
            (&[1, 50, 20], "Kitashinden"),
            (&[8, 1, 1], "Araimura"),
            (&[8, 100, 20], "Yukimurazaki"),
        ],
    );
}

#[test]
fn founders_ending_in_s_follow_each_regions_own_rule() {
    let mut c = content();
    let founders = c
        .tables
        .iter_mut()
        .find(|t| t.id.as_str() == "table.naming.halingo.founder")
        .unwrap();
    founders.results[0].name = "Tamas".into();
    // Halingo joins a landholder and a site with a space, no possessive.
    assert_eq!(name(&c, 2, &[70, 1, 1]), "Tamas Well");
    // Shinkira never adds a possessive, and Ordon keeps its own rule.
    assert_eq!(name(&c, 4, &[8, 1, 1]), "Araimura");
    // Ordon's approved founder form carries its own possessive: Nerys, A.
    assert_eq!(name(&c, 1, &[43, 95, 1]), "Nerysbury");
}

#[test]
fn regional_tables_are_customised_through_the_normal_override_system() {
    let mut service = AppService::new(Database::open_in_memory().unwrap());
    let id = TableId::new("table.naming.halingo.founder");
    let mut founders = service.table_detail(&id).unwrap().table;
    founders.results[0].name = "Vareshi".into();
    service.save_table(founders).unwrap();

    let content = service.content().unwrap();
    assert_eq!(name(&content, 2, &[70, 1, 1]), "Vareshi Well");
    // Resetting restores the shipped entry.
    service.reset_table(&id).unwrap();
    let content = service.content().unwrap();
    assert_eq!(name(&content, 2, &[70, 1, 1]), "Varesh Well");
}

#[test]
fn saved_regional_names_survive_later_content_changes() {
    let c = content();
    let mut s = at_region(&c, 2);
    s.roll(NAME, &mut dice(&[70, 1, 1])).unwrap(); // Varesh Well
    let snapshot = s.to_snapshot();

    let mut changed = c.clone();
    let founders = changed
        .tables
        .iter_mut()
        .find(|t| t.id.as_str() == "table.naming.halingo.founder")
        .unwrap();
    founders.results[0].name = "Someone Else".into();
    let reopened = GenerationSession::from_snapshot(
        Arc::new(changed),
        GenerationOptions::default(),
        GenerationMode::OneTable,
        &snapshot,
    )
    .unwrap();
    assert_eq!(reopened.name().as_deref(), Some("Varesh Well"));
    let record = snapshot
        .stages
        .iter()
        .find_map(|st| st.name.as_ref())
        .unwrap();
    let parts: Vec<&ResultId> = record.rolls.iter().skip(1).map(|r| &r.result.id).collect();
    assert_eq!(parts, [&ResultId::new("varesh"), &ResultId::new("well")]);
}

// ----- Bratislor, Hjartheland, Nareth, Targoth -------------------------------------------

#[test]
fn exactly_eight_kurovian_schemes_cover_every_kurovian_region() {
    let c = content();
    // Settlement schemes only: person schemes name Authority Figures.
    let mut ids: Vec<&str> = c
        .naming_schemes
        .iter()
        .filter(|s| s.subject == settlement_core::naming::NamingSubject::Settlement)
        .map(|s| s.id.as_str())
        .collect();
    ids.sort();
    assert_eq!(
        ids,
        [
            "naming.kurovia.bratislor",
            "naming.kurovia.halingo",
            "naming.kurovia.hjartheland",
            "naming.kurovia.nareth",
            "naming.kurovia.ordon",
            "naming.kurovia.shinkira",
            "naming.kurovia.skgurr",
            "naming.kurovia.targoth",
        ]
    );
    // Every result of the Kurovia Region table is named by its own scheme.
    let regions = c.table(&TableId::new("table.kuroviaRegion")).unwrap();
    assert_eq!(regions.results.len(), 8);
    for (i, region) in regions.results.iter().enumerate() {
        let s = at_region(&c, i as u32 + 1);
        let naming = s
            .view(NAME, ViewOptions::default())
            .unwrap()
            .naming
            .unwrap();
        assert_eq!(
            naming.scheme.unwrap().id.to_string(),
            format!("naming.kurovia.{}", region.id),
            "{}",
            region.name
        );
    }
    // No generic settlement scheme exists yet: other Setting Profiles are
    // not named.
    assert!(
        c.naming_schemes
            .iter()
            .filter(|s| s.subject == settlement_core::naming::NamingSubject::Settlement)
            .all(|s| s.id.as_str().starts_with("naming.kurovia."))
    );
}

#[test]
fn bratislor_names() {
    check(
        5,
        &[
            (&[1, 1, 1], "Belgrad"),
            (&[1, 9, 4], "Lukagrad"),
            (&[9, 1, 1], "Dragomirgrad"),
            (&[14, 1, 1], "Mlinov"),
            (&[17, 1, 1], "Ardanesti"),
            (&[19, 1, 1], "Sivov"),
        ],
    );
}

#[test]
fn hjartheland_names() {
    check(
        6,
        &[
            (&[1, 1, 1], "Nordvik"),
            (&[23, 1, 1], "Steindal"),
            (&[43, 1, 1], "Snøbre"),
            (&[94, 1, 1], "Eiriksheim"),
            (&[98, 1], "Eldrun"),
        ],
    );
}

#[test]
fn nareth_names() {
    check(
        7,
        &[
            (&[1, 1, 1], "Yanamayu"),
            (&[26, 1, 1], "Pukallaqta"),
            (&[65, 1, 1], "Rumillaqta"),
            (&[93, 1, 1], "Talarien"),
            (&[99, 1], "Yuranei"),
        ],
    );
}

#[test]
fn targoth_names() {
    check(
        8,
        &[
            (&[1, 1, 1], "Karla"),
            (&[1, 40, 10], "Surengang"),
            (&[9, 1, 1], "Varok's Ling"),
            (&[9, 11, 2], "Sorn's Ling"),
        ],
    );
}

/// The name a regional table gives on a face.
fn at(c: &ContentSet, key: &str, suffix: &str, face: i32) -> String {
    table(c, key, suffix)
        .results
        .iter()
        .find(|r| r.range.is_some_and(|range| range.contains(face)))
        .unwrap()
        .name
        .clone()
}

#[test]
fn non_english_conventions_are_kept() {
    let mut c = content();
    // Hjartheland and Bratislor never add an English possessive.
    assert_eq!(name(&c, 6, &[94, 1, 1]), "Eiriksheim");
    assert_eq!(name(&c, 5, &[9, 1, 1]), "Dragomirgrad");
    // Letters beyond English join and capitalise correctly, if an editor
    // uses them.
    let first = c
        .tables
        .iter_mut()
        .find(|t| t.id.as_str() == "table.naming.hjartheland.firstElement")
        .unwrap();
    first.results[2].name = "bjørn".into(); // faces 5–6
    first.results[0].name = "ØST".into(); // faces 1–2
    let second = |face| at(&c, "hjartheland", "secondElement", face);
    assert_eq!(name(&c, 6, &[1, 5, 1]), format!("Bjørn{}", second(1)));
    assert_eq!(name(&c, 6, &[1, 1, 3]), format!("Øst{}", second(3)));
    // Targoth founders always take 's, as in English place names.
    let founders = c
        .tables
        .iter_mut()
        .find(|t| t.id.as_str() == "table.naming.targoth.founder")
        .unwrap();
    founders.results[10].name = "Sorns".into();
    assert_eq!(name(&c, 8, &[9, 11, 2]), "Sorns's Ling");
}

#[test]
fn nareth_phonetic_rules_are_data_not_code() {
    let mut c = content();
    // Shipped with no transformations: parts are joined as they are.
    assert_eq!(name(&c, 7, &[1, 1, 1]), "Yanamayu");
    let scheme = c
        .naming_schemes
        .iter_mut()
        .find(|s| s.id.as_str() == "naming.kurovia.nareth")
        .unwrap();
    assert!(
        scheme
            .methods
            .iter()
            .all(|m| m.join.replacements.is_empty())
    );
    // An editor can add a phonetic rule as data.
    let rule: settlement_core::naming::Replacement =
        serde_json::from_value(serde_json::json!({ "from": "nama", "to": "na" })).unwrap();
    scheme.methods[0].join.replacements.push(rule);
    assert_eq!(name(&c, 7, &[1, 1, 1]), "Yanayu");
}
