//! Regional naming packs: Halingo, Skgurr and Shinkira (provisional sample
//! data). Each region tag selects its own scheme; every table covers its dice
//! exactly; the approved examples come out of real dice rolls; and each
//! region's joining rule is its own.

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

/// Names of a table's results, in face order.
fn entries(c: &ContentSet, key: &str, suffix: &str) -> Vec<String> {
    table(c, key, suffix)
        .results
        .iter()
        .map(|r| r.name.clone())
        .collect()
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

#[test]
fn every_d12_and_d6_table_covers_its_dice_exactly() {
    let c = content();
    for region in &REGIONS {
        for (suffix, dice, sides) in [
            ("firstElement", "1d12", 12),
            ("secondElement", "1d12", 12),
            ("founder", "1d12", 12),
            ("founderEnding", "1d6", 6),
        ] {
            let t = table(&c, region.key, suffix);
            assert_eq!(t.dice.as_deref(), Some(dice), "{}", t.id);
            assert_eq!(t.results.len(), sides, "{}", t.id);
            for face in 1..=sides as i32 {
                let owners = t
                    .results
                    .iter()
                    .filter(|r| r.range.is_some_and(|range| range.contains(face)))
                    .count();
                assert_eq!(owners, 1, "{} face {face}", t.id);
            }
            // Equally likely: one face each.
            assert!(t.results.iter().all(|r| {
                let range = r.range.unwrap();
                range.min == range.max
            }));
        }
    }
}

#[test]
fn methods_are_weighted_70_30_on_a_d10() {
    let c = content();
    for region in &REGIONS {
        let method = table(&c, region.key, "method");
        assert_eq!(method.dice.as_deref(), Some("1d10"));
        let layout: Vec<(&str, &str, i32, i32)> = method
            .results
            .iter()
            .map(|r| {
                let range = r.range.unwrap();
                (r.id.as_str(), r.name.as_str(), range.min, range.max)
            })
            .collect();
        assert_eq!(
            layout,
            [
                ("geographic", "Geographic Compound", 1, 7),
                ("founder", "Historical Founder", 8, 10)
            ]
        );
    }
}

// ----- the approved lists --------------------------------------------------------------

#[test]
fn the_approved_entries_are_present_in_order() {
    let c = content();
    let expect = |key: &str, suffix: &str, list: &str| {
        let want: Vec<String> = list.split(", ").map(str::to_owned).collect();
        assert_eq!(entries(&c, key, suffix), want, "{key} {suffix}");
    };
    expect(
        "halingo",
        "firstElement",
        "Khar, Sere, Varr, Dun, Tal, Brine, Cinder, Ash, Red, Mara, Silt, Vek",
    );
    expect(
        "halingo",
        "secondElement",
        "Well, Rest, Pass, Hold, Wash, Reach, Ridge, Ford, Crossing, Gate, Station, Haven",
    );
    expect(
        "halingo",
        "founder",
        "Varesh, Tamar, Karven, Delan, Sevrin, Harad, Marella, Orvek, Saren, Jorek, Varin, Ilyra",
    );
    expect(
        "halingo",
        "founderEnding",
        "Well, Rest, Crossing, Station, Hold, Pass",
    );
    expect(
        "skgurr",
        "firstElement",
        "Dun, Glen, Brae, Grey, Black, Keld, Mor, Skel, Strath, Cairn, Rook, Thorn",
    );
    expect(
        "skgurr",
        "secondElement",
        "haven, wick, ness, firth, holm, head, port, muir, crag, shore, point, bay",
    );
    expect(
        "skgurr",
        "founder",
        "Ewan, Mairi, Duncan, Sorley, Fergus, Iona, Torin, Morven, Rona, Ivor, Eilidh, Alasdair",
    );
    expect(
        "skgurr",
        "founderEnding",
        "Point, Cove, Head, Haven, Watch, Bay",
    );
    expect(
        "shinkira",
        "firstElement",
        "Aki, Asa, Fuyu, Hana, Hira, Hoshi, Kiri, Kuro, Mizu, Sugi, Taki, Tsuki",
    );
    expect(
        "shinkira",
        "secondElement",
        "mura, sato, hara, sawa, kawa, tani, yama, oka, shima, mori, numa, zaki",
    );
    expect(
        "shinkira",
        "founder",
        "Arai, Endo, Hoshino, Ishida, Kagawa, Kitamura, Mizuno, Morita, Nakai, Sakurai, Takeda, Ueda",
    );
    expect(
        "shinkira",
        "founderEnding",
        "mura, sato, shinden, machi, yashiki, shiro",
    );
}

// ----- names from real dice ----------------------------------------------------------------

/// `[method face, first part face, second part face]`: method 1–7 is
/// Geographic, 8–10 Historical Founder.
fn check(region: u32, cases: &[([u32; 3], &str)]) {
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
            ([1, 1, 1], "Khar Well"),
            ([1, 7, 7], "Cinder Ridge"),
            ([1, 11, 9], "Silt Crossing"),
            ([1, 3, 2], "Varr Rest"),
            ([8, 1, 1], "Varesh's Well"),
            ([8, 3, 3], "Karven's Crossing"),
            ([8, 9, 2], "Saren's Rest"),
        ],
    );
}

#[test]
fn skgurr_names() {
    check(
        3,
        &[
            ([1, 1, 2], "Dunwick"),
            ([1, 4, 1], "Greyhaven"),
            ([1, 9, 8], "Strathmuir"),
            ([1, 8, 7], "Skelport"),
            ([8, 5, 1], "Fergus's Point"),
            ([8, 8, 2], "Morven's Cove"),
            ([8, 1, 5], "Ewan's Watch"),
        ],
    );
}

#[test]
fn shinkira_names() {
    check(
        4,
        &[
            ([1, 7, 4], "Kirisawa"),
            ([1, 9, 1], "Mizumura"),
            ([1, 8, 7], "Kuroyama"),
            ([1, 5, 8], "Hiraoka"),
            ([8, 11, 1], "Takedamura"),
            ([8, 7, 2], "Mizunosato"),
            ([8, 12, 3], "Uedashinden"),
        ],
    );
}

#[test]
fn founders_ending_in_s_follow_each_regions_own_rule() {
    // No approved Halingo or Skgurr founder ends in "s", so add one (as a
    // user could) and check the documented rules.
    let mut c = content();
    for key in ["halingo", "skgurr"] {
        let founders = c
            .tables
            .iter_mut()
            .find(|t| t.id.as_str() == format!("table.naming.{key}.founder"))
            .unwrap();
        founders.results[0].name = "Tamas".into();
    }
    // Halingo: an apostrophe only. Skgurr: always 's (as in Fergus's).
    assert_eq!(name(&c, 2, &[8, 1, 1]), "Tamas' Well");
    assert_eq!(name(&c, 3, &[8, 1, 1]), "Tamas's Point");
    // Shinkira never adds a possessive, and Ordon keeps its own rule.
    assert_eq!(name(&c, 4, &[8, 11, 1]), "Takedamura");
    assert_eq!(name(&c, 1, &[8, 95, 6]), "Nerysbury");
}

#[test]
fn regional_tables_are_customised_through_the_normal_override_system() {
    let mut service = AppService::new(Database::open_in_memory().unwrap());
    let id = TableId::new("table.naming.halingo.founder");
    let mut founders = service.table_detail(&id).unwrap().table;
    founders.results[0].name = "Vareshi".into();
    service.save_table(founders).unwrap();

    let content = service.content().unwrap();
    assert_eq!(name(&content, 2, &[8, 1, 1]), "Vareshi's Well");
    // Resetting restores the shipped entry.
    service.reset_table(&id).unwrap();
    let content = service.content().unwrap();
    assert_eq!(name(&content, 2, &[8, 1, 1]), "Varesh's Well");
}

#[test]
fn saved_regional_names_survive_later_content_changes() {
    let c = content();
    let mut s = at_region(&c, 2);
    s.roll(NAME, &mut dice(&[8, 1, 1])).unwrap(); // Varesh's Well
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
    assert_eq!(reopened.name().as_deref(), Some("Varesh's Well"));
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
    let mut ids: Vec<&str> = c.naming_schemes.iter().map(|s| s.id.as_str()).collect();
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
    // No generic scheme exists yet: other Setting Profiles are not named.
    assert!(
        c.naming_schemes
            .iter()
            .all(|s| s.id.as_str().starts_with("naming.kurovia."))
    );
}

#[test]
fn the_later_approved_entries_are_present_in_order() {
    let c = content();
    let expect = |key: &str, suffix: &str, list: &str| {
        let want: Vec<String> = list.split(", ").map(str::to_owned).collect();
        assert_eq!(entries(&c, key, suffix), want, "{key} {suffix}");
    };
    expect(
        "bratislor",
        "firstElement",
        "Bel, Bor, Drev, Kras, Luka, Mor, Novo, Staro, Vel, Ves, Vran, Zor",
    );
    expect(
        "bratislor",
        "secondElement",
        "grad, ovo, ine, ava, pol, nik, ic, gora, dol, ska, ova, ost",
    );
    expect(
        "bratislor",
        "founder",
        "Dragomir, Vesna, Radan, Ilena, Oskar, Vorel, Mira, Sava, Daria, Branik, Zora, Stanimir",
    );
    expect("bratislor", "founderEnding", "grad, ov, in, ova, dol, pol");
    expect(
        "hjartheland",
        "firstElement",
        "Nord, Hrafn, Bjorn, Eir, Skar, Kald, Sval, Hald, Stein, Vard, Hjell, Skog",
    );
    expect(
        "hjartheland",
        "secondElement",
        "vik, heim, dal, berg, nes, gard, havn, lund, fjord, set, holt, fell",
    );
    expect(
        "hjartheland",
        "founder",
        "Eirik, Signe, Astrid, Torvald, Hakon, Runa, Halvor, Yrsa, Sigrun, Alva, Jorund, Leif",
    );
    expect(
        "hjartheland",
        "founderEnding",
        "heim, gard, vik, set, dal, nes",
    );
    expect(
        "nareth",
        "firstElement",
        "Va, Saq, Naro, Tal, Iru, Kori, Mara, Isha, Eru, Nema, Varu, Luma",
    );
    expect(
        "nareth",
        "secondElement",
        "ra, neth, kai, ri, lo, sha, vai, mar, thil, sen, ru, ta",
    );
    expect(
        "nareth",
        "founder",
        "Talari, Ivara, Namar, Sorai, Vesha, Aruna, Kethra, Lurien, Maiva, Sarru, Tavira, Eshar",
    );
    expect("nareth", "founderEnding", "en, mar, tal, vai, ra, ket");
    expect(
        "targoth",
        "firstElement",
        "Kar, Tor, Varn, Drel, Or, Kel, Mor, Vath, Bren, Skarn, Dru, Thar",
    );
    expect(
        "targoth",
        "secondElement",
        "hold, gate, fell, watch, stone, deep, rest, reach, spire, dun, vale, cairn",
    );
    expect(
        "targoth",
        "founder",
        "Varok, Thren, Odrik, Maren, Dravar, Keled, Voran, Torek, Ravik, Elvar, Sorn, Kerin",
    );
    expect(
        "targoth",
        "founderEnding",
        "Hold, Gate, Watch, Rest, Cairn, Deep",
    );
}

#[test]
fn bratislor_names() {
    check(
        5,
        &[
            ([1, 3, 1], "Drevgrad"),
            ([1, 6, 4], "Morava"),
            ([1, 11, 11], "Vranova"),
            ([1, 9, 1], "Velgrad"),
            ([8, 6, 1], "Vorelgrad"),
            ([8, 3, 2], "Radanov"),
            ([8, 10, 5], "Branikdol"),
        ],
    );
}

#[test]
fn hjartheland_names() {
    check(
        6,
        &[
            ([1, 1, 1], "Nordvik"),
            ([1, 4, 2], "Eirheim"),
            ([1, 9, 3], "Steindal"),
            ([1, 12, 5], "Skognes"),
            ([8, 1, 1], "Eirikheim"),
            ([8, 2, 3], "Signevik"),
            ([8, 5, 2], "Hakongard"),
        ],
    );
}

#[test]
fn nareth_names() {
    check(
        7,
        &[
            ([1, 3, 2], "Naroneth"),
            ([1, 6, 7], "Korivai"),
            ([1, 2, 8], "Saqmar"),
            ([1, 12, 4], "Lumari"),
            ([8, 1, 1], "Talarien"),
            ([8, 2, 2], "Ivaramar"),
            ([8, 3, 4], "Namarvai"),
        ],
    );
}

#[test]
fn targoth_names() {
    check(
        8,
        &[
            ([1, 1, 1], "Karhold"),
            ([1, 10, 4], "Skarnwatch"),
            ([1, 8, 2], "Vathgate"),
            ([1, 4, 5], "Drelstone"),
            ([8, 1, 2], "Varok's Gate"),
            ([8, 2, 3], "Thren's Watch"),
            ([8, 7, 4], "Voran's Rest"),
        ],
    );
}

#[test]
fn non_english_conventions_are_kept() {
    let mut c = content();
    // Hjartheland and Bratislor never add an English possessive.
    assert_eq!(name(&c, 6, &[8, 1, 1]), "Eirikheim");
    assert_eq!(
        name(&c, 5, &[8, 2, 2]),
        "Vesnaov",
        "joined as written, no s"
    );
    // Letters beyond English join and capitalise correctly, if an editor
    // uses them (the shipped lists are plain ASCII).
    let first = c
        .tables
        .iter_mut()
        .find(|t| t.id.as_str() == "table.naming.hjartheland.firstElement")
        .unwrap();
    first.results[2].name = "bjørn".into();
    first.results[0].name = "ØST".into();
    assert_eq!(name(&c, 6, &[1, 3, 1]), "Bjørnvik");
    assert_eq!(name(&c, 6, &[1, 1, 3]), "Østdal");
    // Targoth founders always take 's, as in English place names.
    let founders = c
        .tables
        .iter_mut()
        .find(|t| t.id.as_str() == "table.naming.targoth.founder")
        .unwrap();
    founders.results[10].name = "Sorns".into();
    assert_eq!(name(&c, 8, &[8, 11, 2]), "Sorns's Gate");
}

#[test]
fn nareth_phonetic_rules_are_data_not_code() {
    let mut c = content();
    // Shipped with no transformations: parts are joined as they are.
    assert_eq!(name(&c, 7, &[1, 1, 1]), "Vara");
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
        serde_json::from_value(serde_json::json!({ "from": "ara", "to": "ra" })).unwrap();
    scheme.methods[0].join.replacements.push(rule);
    assert_eq!(name(&c, 7, &[1, 1, 1]), "Vra");
}
