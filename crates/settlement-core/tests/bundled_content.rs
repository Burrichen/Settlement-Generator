//! The bundled v0.1.0 content, checked against the approved specification.

use std::collections::BTreeMap;

use settlement_core::authority::{AuthorityRepresentatives, Representation};
use settlement_core::bundled::{bundled_content, bundled_file_names};
use settlement_core::content::{
    ContentSet, ContentStatus, Effect, IntentChange, SelectionMode, Table,
};
use settlement_core::dice::{DiceExpr, ScriptedFaces};
use settlement_core::engine::{GenerationError, GenerationLog, Generator, Selections};
use settlement_core::ids::TableId;
use settlement_core::validation::{IssueKind, validate};

fn content() -> ContentSet {
    bundled_content().expect("bundled content loads")
}

fn table<'a>(c: &'a ContentSet, id: &str) -> &'a Table {
    c.table(&id.into())
        .unwrap_or_else(|| panic!("missing {id}"))
}

/// `(result name, chance in percent)` for a table's base layout.
fn percentages(t: &Table) -> Vec<(String, u32)> {
    let expr = DiceExpr::parse(t.dice.as_deref().unwrap()).unwrap();
    assert_eq!(expr.dice_count(), 1, "{} uses a single die", t.id);
    let sides = (expr.max_total() - expr.min_total() + 1) as u32;
    t.results
        .iter()
        .map(|r| {
            let range = r.range.unwrap();
            let width = (range.max - range.min + 1) as u32;
            (r.name.clone(), width * 100 / sides)
        })
        .collect()
}

fn expect(t: &Table, dice: &str, expected: &[(&str, u32)]) {
    assert_eq!(t.dice.as_deref(), Some(dice), "{}", t.id);
    let actual = percentages(t);
    let expected: Vec<(String, u32)> = expected.iter().map(|(n, p)| (n.to_string(), *p)).collect();
    assert_eq!(actual, expected, "{}", t.id);
}

/// Faces of each result in a variant, `name -> (min, max)`.
fn variant_ranges(t: &Table, variant: &str) -> BTreeMap<String, (i32, i32)> {
    let v = t.variant(&variant.into()).unwrap();
    v.ranges
        .as_ref()
        .unwrap()
        .iter()
        .map(|a| {
            let name = t.result(&a.result).unwrap().name.clone();
            (name, (a.range.min, a.range.max))
        })
        .collect()
}

fn ranges(pairs: &[(&str, i32, i32)]) -> BTreeMap<String, (i32, i32)> {
    pairs
        .iter()
        .map(|(n, a, b)| (n.to_string(), (*a, *b)))
        .collect()
}

// ----- integrity -------------------------------------------------------------

#[test]
fn bundled_files_are_discovered() {
    let names: Vec<_> = bundled_file_names().collect();
    assert!(names.len() >= 9, "{names:?}");
    assert!(names.iter().all(|n| n.ends_with(".json")));
}

#[test]
fn every_content_file_holds_at_most_one_table() {
    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "json") {
                out.push(path);
            }
        }
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let mut files = Vec::new();
    walk(&root, &mut files);
    assert!(files.len() > 100, "{}", files.len());
    for file in files {
        let content: ContentSet =
            serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert!(
            content.tables.len() <= 1,
            "{} has {} tables: every table has its own file",
            file.display(),
            content.tables.len()
        );
    }
}

#[test]
fn bundled_content_has_no_errors_and_only_the_known_incomplete_items() {
    let report = validate(&content());
    assert!(!report.has_errors(), "{report}");
    let incomplete: Vec<_> = report
        .warnings()
        .map(|w| {
            assert!(
                matches!(w.kind, IssueKind::IncompleteContent { .. }),
                "unexpected warning: {w}"
            );
            w.location.table.as_ref().unwrap().to_string()
        })
        .collect();
    // Release readiness: this list must be empty before shipping v0.1.0.
    assert_eq!(
        incomplete,
        ["table.environment", "table.authorityFigure.species"]
    );
}

#[test]
fn bundled_content_never_uses_the_user_namespace() {
    let c = content();
    for t in &c.tables {
        assert!(!t.id.is_user(), "{}", t.id);
        assert!(t.results.iter().all(|r| !r.id.is_user()), "{}", t.id);
    }
    assert!(c.tags.iter().all(|t| !t.id.is_user()));
}

#[test]
fn no_unapproved_numeric_modifiers_are_executable() {
    for tag in &content().tags {
        for def in &tag.effects {
            match &def.effect {
                Effect::ModifyDice { .. } | Effect::RestrictResults { .. } => {
                    panic!("{} has an executable numeric/eligibility modifier", tag.id)
                }
                Effect::Intent { .. } => assert!(!def.is_active(), "{} intent is active", tag.id),
                _ => {}
            }
        }
    }
}

#[test]
fn out_of_scope_tables_are_absent() {
    let c = content();
    for name in [
        "Location",
        "Development",
        "Materials",
        "Broad Character",
        "Fortress",
        "NPC",
        "Residence / Estate",
        "Residence",
        "Estate",
    ] {
        // Notable Locations are in scope; the old Location table is not.
        assert!(
            !c.tables
                .iter()
                .any(|t| t.name.contains(name) && !t.name.contains("Notable Location")),
            "{name} must not be a table"
        );
        assert!(
            !c.tables
                .iter()
                .flat_map(|t| &t.results)
                .any(|r| r.name == name),
            "{name} must not be a result"
        );
    }
}

// ----- tables match the specification ----------------------------------------

#[test]
fn sequence_is_the_approved_pipeline() {
    let seq: Vec<_> = content()
        .sequence_tables()
        .iter()
        .map(|t| t.to_string())
        .collect();
    assert_eq!(
        seq,
        [
            "table.settingProfile",
            "table.genericRegion",
            "table.environment",
            "table.connections",
            "table.size",
            "table.origin",
            "table.purpose",
            "table.age",
            "table.economy",
            "table.authority",
            "table.authorityRepresentation",
            "table.authoritySize",
            "table.security",
            "table.crime",
            "table.stability"
        ]
    );
}

#[test]
fn setting_profile_keeps_every_profile() {
    let c = content();
    let t = table(&c, "table.settingProfile");
    assert_eq!(t.name, "Setting Profile");
    assert_eq!(t.selection, SelectionMode::Choose);
    let entries: Vec<_> = t
        .results
        .iter()
        .map(|r| (r.name.as_str(), r.tags[0].as_str()))
        .collect();
    assert_eq!(
        entries,
        [
            ("Kurovia", "world.kurovia"),
            ("Low Fantasy", "world.lowFantasy"),
            ("Medium Fantasy", "world.mediumFantasy"),
            ("High Fantasy", "world.highFantasy"),
            ("Forgotten Realms", "world.forgottenRealms"),
            ("Horror", "world.horror"),
        ]
    );
}

#[test]
fn region_tables() {
    let c = content();
    expect(
        table(&c, "table.genericRegion"),
        "1d10",
        &[
            ("Coldlands", 20),
            ("Warmlands", 20),
            ("Heartlands", 20),
            ("Mountains", 20),
            ("Coastlands", 20),
        ],
    );
    let kurovia = table(&c, "table.kuroviaRegion");
    assert_eq!(kurovia.dice.as_deref(), Some("1d8"));
    let names: Vec<_> = kurovia.results.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "Ordon",
            "Halingo",
            "Skgurr",
            "Shinkira",
            "Bratislor",
            "Hjartheland",
            "Nareth",
            "Targoth"
        ]
    );
    for r in &kurovia.results {
        assert_eq!(r.tags.len(), 1);
        assert_eq!(r.tags[0].as_str(), format!("region.{}", r.id));
    }
}

#[test]
fn kurovia_environment_tables() {
    let c = content();
    let env = |region: &str| table(&c, &format!("table.kuroviaEnvironment.{region}"));
    expect(
        env("ordon"),
        "1d20",
        &[
            ("Farmland & Open Country", 25),
            ("Grassland / Plains", 15),
            ("Woodland", 15),
            ("Moorland / Heath", 5),
            ("Hills", 10),
            ("Mountains", 10),
            ("Wetlands", 10),
            ("Coast", 10),
        ],
    );
    expect(
        env("halingo"),
        "1d20",
        &[
            ("Arid / Desert", 50),
            ("Badlands", 35),
            ("Ichor Fields", 15),
        ],
    );
    expect(
        env("skgurr"),
        "1d20",
        &[
            ("Coast", 35),
            ("Hills", 30),
            ("Moorland / Heath", 20),
            ("Wetlands", 15),
        ],
    );
    expect(
        env("shinkira"),
        "1d20",
        &[
            ("Farmland & Open Country", 5),
            ("Grassland / Plains", 5),
            ("Woodland", 25),
            ("Mountains", 25),
            ("Hills", 10),
            ("Coast", 15),
            ("Crystalline Wilds", 15),
        ],
    );
    expect(
        env("bratislor"),
        "1d20",
        &[
            ("Wasteland / Blighted Land", 70),
            ("Woodland", 15),
            ("Farmland & Open Country", 15),
        ],
    );
    expect(
        env("hjartheland"),
        "1d20",
        &[
            ("Cold / Tundra", 35),
            ("Glacial", 35),
            ("Mountains", 15),
            ("Forever Ice", 15),
        ],
    );
    expect(
        env("nareth"),
        "1d20",
        &[("Tropical / Jungle", 65), ("Wetlands", 35)],
    );
    expect(
        env("targoth"),
        "1d20",
        &[
            ("Mountains", 55),
            ("Cold / Tundra", 20),
            ("Hills", 10),
            ("Subterranean", 15),
        ],
    );
    let crystalline = c.tag(&"env.crystallineWilds".into()).unwrap();
    assert!(crystalline.effects.is_empty(), "no Purpose modifier");
}

#[test]
fn connections_table_and_region_variants() {
    let c = content();
    let t = table(&c, "table.connections");
    expect(
        t,
        "1d20",
        &[
            ("Isolated", 25),
            ("Local Road", 25),
            ("Regional Road", 25),
            ("Major Route", 25),
        ],
    );
    assert!(!t.results.iter().any(|r| r.name.contains("Track")));
    assert_eq!(
        variant_ranges(t, "ordon"),
        ranges(&[
            ("Isolated", 1, 6),
            ("Local Road", 7, 9),
            ("Regional Road", 10, 15),
            ("Major Route", 16, 20)
        ])
    );
    assert_eq!(
        variant_ranges(t, "hjartheland"),
        ranges(&[
            ("Isolated", 1, 6),
            ("Local Road", 7, 12),
            ("Regional Road", 13, 17),
            ("Major Route", 18, 20)
        ])
    );
    assert_eq!(
        variant_ranges(t, "nareth"),
        ranges(&[
            ("Isolated", 1, 7),
            ("Local Road", 8, 10),
            ("Regional Road", 11, 13),
            ("Major Route", 14, 20)
        ])
    );
    assert_eq!(
        variant_ranges(t, "targoth"),
        ranges(&[
            ("Isolated", 1, 8),
            ("Local Road", 9, 16),
            ("Regional Road", 17, 20)
        ])
    );
    assert_eq!(
        variant_ranges(t, "shinkira"),
        ranges(&[
            ("Isolated", 1, 5),
            ("Local Road", 6, 13),
            ("Regional Road", 14, 20)
        ])
    );
    // Bratislor, Halingo and Skgurr use Standard: no variant for them.
    for region in ["bratislor", "halingo", "skgurr"] {
        assert!(t.variant(&region.into()).is_none());
    }
}

#[test]
fn size_table_is_standard_1d10() {
    let c = content();
    let t = table(&c, "table.size");
    assert_eq!(t.status, ContentStatus::Final);
    assert_eq!(t.selection, SelectionMode::Roll);
    expect(
        t,
        "1d10",
        &[
            ("Trading Post", 20),
            ("Village", 20),
            ("Town", 20),
            ("City", 20),
            ("Capital", 20),
        ],
    );
    // Every face 1-10 selects exactly one result.
    for face in 1..=10 {
        let claims = t
            .results
            .iter()
            .filter(|r| r.range.unwrap().contains(face))
            .count();
        assert_eq!(claims, 1, "face {face}");
    }
    let faces_and_tags: Vec<_> = t
        .results
        .iter()
        .map(|r| {
            let range = r.range.unwrap();
            let tags: Vec<_> = r.tags.iter().map(|g| g.as_str()).collect();
            (r.id.as_str(), range.min, range.max, tags)
        })
        .collect();
    assert_eq!(
        faces_and_tags,
        [
            ("tradingPost", 1, 2, vec!["size.tradingPost"]),
            ("village", 3, 4, vec!["size.village"]),
            ("town", 5, 6, vec!["size.town"]),
            ("city", 7, 8, vec!["size.city"]),
            ("capital", 9, 10, vec!["size.capital"]),
        ]
    );
    assert!(t.variants.is_empty() && t.follow_ups.is_empty());
    for tag in [
        "size.tradingPost",
        "size.village",
        "size.town",
        "size.city",
        "size.capital",
    ] {
        assert!(c.tag(&tag.into()).is_some(), "{tag} is defined");
    }
}

#[test]
fn size_is_an_ordered_scale_from_trading_post_to_capital() {
    let c = content();
    let t = table(&c, "table.size");
    assert!(t.ordered);
    let names: Vec<_> = t.results.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(
        names,
        ["Trading Post", "Village", "Town", "City", "Capital"]
    );
    // The faces ascend with the scale, so a future dice modifier moves the
    // result towards a smaller or larger settlement.
    let mins: Vec<_> = t.results.iter().map(|r| r.range.unwrap().min).collect();
    assert!(mins.windows(2).all(|w| w[0] < w[1]), "{mins:?}");
    // Only the five core sizes: no Fortress, Hamlet or Small/Large variants.
    assert_eq!(t.results.len(), 5);
}

#[test]
fn size_modifiers_stay_planned_intents() {
    let c = content();
    // Authority intents are checked with the Authority table.
    let intent = |tag: &str| -> Vec<(Option<String>, Vec<String>, IntentChange)> {
        c.tag(&tag.into())
            .unwrap()
            .effects
            .iter()
            .filter(|d| {
                d.effect
                    .target_table()
                    .is_none_or(|t| t.as_str() != "table.authority")
            })
            .map(|d| {
                assert!(!d.is_active(), "{tag} has no approved numbers yet");
                match &d.effect {
                    Effect::Intent {
                        table,
                        results,
                        change,
                        ..
                    } => (
                        table.as_ref().map(|t| t.to_string()),
                        results.iter().map(|r| r.to_string()).collect(),
                        *change,
                    ),
                    other => panic!("{tag}: unexpected {other:?}"),
                }
            })
            .collect()
    };
    let purpose = Some("table.purpose".to_string());
    assert_eq!(
        intent("size.tradingPost"),
        [
            (
                purpose.clone(),
                vec!["trade".into()],
                IntentChange::StrongIncrease
            ),
            (
                purpose.clone(),
                vec!["transportWaystation".into()],
                IntentChange::Increase
            ),
        ]
    );
    assert_eq!(
        intent("size.capital"),
        [(
            purpose,
            vec!["administrative".into()],
            IntentChange::StrongIncrease
        )]
    );
    for tag in ["size.village", "size.town", "size.city"] {
        assert!(intent(tag).is_empty(), "{tag}");
    }
    // Age moves Size along its scale; the exact dice change is not approved.
    let size = Some("table.size".to_string());
    for (tag, change) in [
        ("age.new", IntentChange::Decrease),
        ("age.old", IntentChange::Increase),
        ("age.ancient", IntentChange::Increase),
    ] {
        let size_intents: Vec<_> = intent(tag)
            .into_iter()
            .filter(|(table, ..)| *table == size)
            .collect();
        assert_eq!(size_intents, [(size.clone(), vec![], change)], "{tag}");
    }
}

const ECONOMY: [&str; 5] = ["Failing", "Struggling", "Stable", "Prosperous", "Wealthy"];

#[test]
fn economy_table_is_standard_1d12() {
    let c = content();
    let t = table(&c, "table.economy");
    assert_eq!(t.status, ContentStatus::Final);
    assert_eq!(t.selection, SelectionMode::Roll);
    assert_eq!(t.dice.as_deref(), Some("1d12"));
    // Every face 1-12 selects exactly one result: full, non-overlapping
    // coverage.
    for face in 1..=12 {
        let claims = t
            .results
            .iter()
            .filter(|r| r.range.unwrap().contains(face))
            .count();
        assert_eq!(claims, 1, "face {face}");
    }
    let faces_and_tags: Vec<_> = t
        .results
        .iter()
        .map(|r| {
            let range = r.range.unwrap();
            let tags: Vec<_> = r.tags.iter().map(|g| g.as_str()).collect();
            (r.id.as_str(), range.min, range.max, tags)
        })
        .collect();
    assert_eq!(
        faces_and_tags,
        [
            ("failing", 1, 1, vec!["economy.failing"]),
            ("struggling", 2, 3, vec!["economy.struggling"]),
            ("stable", 4, 7, vec!["economy.stable"]),
            ("prosperous", 8, 10, vec!["economy.prosperous"]),
            ("wealthy", 11, 12, vec!["economy.wealthy"]),
        ]
    );
    assert!(t.variants.is_empty() && t.follow_ups.is_empty());
    for result in &t.results {
        assert!(
            c.tag(&result.tags[0]).is_some(),
            "{} is defined",
            result.tags[0]
        );
    }
}

#[test]
fn economy_is_an_ordered_scale_from_failing_to_wealthy() {
    let c = content();
    let t = table(&c, "table.economy");
    assert!(t.ordered);
    let names: Vec<_> = t.results.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ECONOMY);
    let mins: Vec<_> = t.results.iter().map(|r| r.range.unwrap().min).collect();
    assert!(mins.windows(2).all(|w| w[0] < w[1]), "{mins:?}");
}

#[test]
fn economy_modifiers_stay_planned_intents() {
    let c = content();
    // (table or concept, change) of each intent on a tag.
    let intents = |tag: &str| -> Vec<(String, IntentChange)> {
        c.tag(&tag.into())
            .unwrap()
            .effects
            .iter()
            .filter_map(|d| match &d.effect {
                Effect::Intent {
                    table,
                    results,
                    concept,
                    change,
                } => {
                    assert!(!d.is_active(), "{tag}: no approved numbers yet");
                    let target = concept.clone().or_else(|| {
                        results
                            .is_empty()
                            .then(|| table.as_ref().unwrap().to_string())
                    })?;
                    Some((target, *change))
                }
                _ => None,
            })
            .collect()
    };
    let economy = |change| ("table.economy".to_string(), change);
    // Upstream: shifts along the Economy scale, without a magnitude.
    assert_eq!(
        intents("connection.isolated"),
        [economy(IntentChange::Decrease)]
    );
    assert_eq!(
        intents("connection.majorRoute"),
        [economy(IntentChange::Increase)]
    );
    assert_eq!(intents("purpose.trade"), [economy(IntentChange::Increase)]);
    // Downstream: destination tables do not exist yet.
    let concept = |c: &str, change| (c.to_string(), change);
    use IntentChange::*;
    assert_eq!(
        intents("economy.failing"),
        [
            concept("opportunities", StrongIncrease),
            concept("building quality", StrongDecrease),
            concept("economic pressures", Increase),
            concept("table.crime", StrongIncrease),
            concept("table.stability", StrongDecrease),
        ]
    );
    assert_eq!(
        intents("economy.struggling"),
        [
            concept("opportunities", Increase),
            concept("building quality", Decrease),
            concept("table.crime", Increase),
            concept("table.stability", Decrease),
        ]
    );
    assert!(intents("economy.stable").is_empty());
    assert_eq!(
        intents("economy.prosperous"),
        [
            concept("shops / services", Increase),
            concept("table.crime", Increase),
            concept("table.stability", Increase),
        ]
    );
    assert_eq!(
        intents("economy.wealthy"),
        [
            concept("shops / services", StrongIncrease),
            concept("high-quality / notable buildings", Increase),
            concept("table.crime", StrongIncrease),
            concept("table.stability", Increase),
        ]
    );
}

#[test]
fn automatic_generation_rolls_economy_on_1d12() {
    let c = content();
    let generator = Generator::new(&c).unwrap();
    let mut selections = Selections::new();
    selections.insert("table.settingProfile".into(), "kurovia".into());
    choose_settlement_state(&mut selections);
    let expected = |face: u32| match face {
        1 => "Failing",
        2..=3 => "Struggling",
        4..=7 => "Stable",
        8..=10 => "Prosperous",
        _ => "Wealthy",
    };
    for face in 1..=12 {
        // Region, Environment, Connections, Size, Origin, Purpose, Age, then
        // Economy, then Authority.
        // Authority 6 = Appointed Official: single, so no sub-rolls.
        let mut dice = ScriptedFaces::new([1, 1, 1, 1, 1, 1, 1, face, 6]);
        let log = generator.run_sequence(&selections, &mut dice).unwrap();
        assert_eq!(dice.remaining(), 0, "unused dice");
        let economy = log.record_for(&"table.economy".into()).unwrap();
        assert!(!economy.selected);
        assert_eq!(economy.dice.as_deref(), Some("1d12"));
        assert_eq!(economy.attempts.len(), 1);
        assert_eq!(economy.attempts[0].roll.total, face as i32);
        let name = expected(face);
        assert_eq!(economy.result.name, name, "face {face}");
        let tag = format!("economy.{}", name.to_lowercase());
        let tags: Vec<_> = economy.tags.iter().map(|g| g.id.as_str()).collect();
        assert_eq!(tags, [tag.as_str()], "face {face}");
        // Economy comes straight after Age.
        let order: Vec<_> = log.table_rolls().map(|r| r.table.id.as_str()).collect();
        let age = order.iter().position(|t| *t == "table.age").unwrap();
        assert_eq!(order[age + 1], "table.economy");
    }
}

#[test]
fn economy_can_be_chosen_manually() {
    let log = run(
        "kurovia",
        &[("table.economy", "wealthy")],
        &[1, 1, 1, 1, 1, 1],
    );
    let economy = log.record_for(&"table.economy".into()).unwrap();
    assert!(economy.selected);
    assert_eq!(economy.result.name, "Wealthy");
    assert!(log.has_tag(&"economy.wealthy".into()));
}

/// `(face range, result ID, name, tag)` of the approved Authority table.
const AUTHORITY: [((i32, i32), &str, &str, &str); 8] = [
    ((1, 3), "council", "Local Council", "authority.council"),
    ((4, 5), "noble", "Noble / Landed Ruler", "authority.noble"),
    (
        (6, 6),
        "appointed",
        "Appointed Official",
        "authority.appointed",
    ),
    ((7, 8), "guild", "Merchant / Guild Rule", "authority.guild"),
    (
        (9, 9),
        "religious",
        "Religious Authority",
        "authority.religious",
    ),
    (
        (10, 10),
        "military",
        "Military Authority",
        "authority.military",
    ),
    (
        (11, 11),
        "elders",
        "Elders / Community Leaders",
        "authority.elders",
    ),
    (
        (12, 12),
        "assembly",
        "Popular Assembly",
        "authority.assembly",
    ),
];

#[test]
fn authority_table_is_standard_1d12() {
    let c = content();
    let t = table(&c, "table.authority");
    assert_eq!(t.status, ContentStatus::Final);
    assert_eq!(t.selection, SelectionMode::Roll);
    assert_eq!(t.dice.as_deref(), Some("1d12"));
    assert!(!t.ordered, "Authority is not a scale");
    for face in 1..=12 {
        let claims = t
            .results
            .iter()
            .filter(|r| r.range.unwrap().contains(face))
            .count();
        assert_eq!(claims, 1, "face {face}");
    }
    let actual: Vec<_> = t
        .results
        .iter()
        .map(|r| {
            let range = r.range.unwrap();
            assert_eq!(r.tags.len(), 1, "{}", r.id);
            (
                (range.min, range.max),
                r.id.as_str(),
                r.name.as_str(),
                r.tags[0].as_str(),
            )
        })
        .collect();
    assert_eq!(actual, AUTHORITY);
    for r in &t.results {
        assert!(!r.description.is_empty(), "{} has a description", r.id);
        assert!(c.tag(&r.tags[0]).is_some(), "{} is defined", r.tags[0]);
    }
    assert!(t.variants.is_empty() && t.follow_ups.is_empty());
}

#[test]
fn authority_results_reserve_a_future_npc_generator() {
    let c = content();
    let t = table(&c, "table.authority");
    for r in &t.results {
        let generator = r.generator.as_ref().unwrap_or_else(|| panic!("{}", r.id));
        assert_eq!(generator.as_str(), format!("authorityNpcs.{}", r.id));
    }
    // No other table uses the hook yet, and no NPC content exists.
    for other in c
        .tables
        .iter()
        .filter(|t| t.id.as_str() != "table.authority")
    {
        assert!(
            other.results.iter().all(|r| r.generator.is_none()),
            "{}",
            other.id
        );
    }
    for t in &c.tables {
        let text = format!("{} {}", t.id, t.name).to_lowercase();
        assert!(!text.contains("npc"), "{} must not be an NPC table", t.id);
    }
}

#[test]
fn authority_modifiers_stay_planned_intents() {
    let c = content();
    // `(result, change)` of each Authority intent on a tag.
    let authority = |tag: &str| -> Vec<(String, IntentChange)> {
        c.tag(&tag.into())
            .unwrap()
            .effects
            .iter()
            .filter_map(|d| match &d.effect {
                Effect::Intent {
                    table: Some(table),
                    results,
                    change,
                    ..
                } if table.as_str() == "table.authority" => {
                    assert!(!d.is_active(), "{tag}: no approved numbers yet");
                    assert_eq!(results.len(), 1, "{tag}");
                    Some((results[0].to_string(), *change))
                }
                _ => None,
            })
            .collect()
    };
    use IntentChange::{Decrease, Increase};
    let more = |r: &str| (r.to_string(), Increase);
    assert_eq!(authority("origin.militaryOutpost"), [more("military")]);
    assert_eq!(authority("origin.religiousFoundation"), [more("religious")]);
    assert_eq!(authority("origin.estateSeat"), [more("noble")]);
    assert_eq!(authority("origin.tradeStop"), [more("guild")]);
    assert_eq!(authority("purpose.military"), [more("military")]);
    assert_eq!(authority("purpose.religious"), [more("religious")]);
    assert_eq!(authority("purpose.trade"), [more("guild")]);
    assert_eq!(
        authority("purpose.administrative"),
        [more("council"), more("appointed")]
    );
    assert_eq!(authority("size.tradingPost"), [more("guild")]);
    assert_eq!(
        authority("size.capital"),
        [
            more("appointed"),
            more("noble"),
            more("council"),
            ("elders".to_string(), Decrease)
        ]
    );
    // Economy is independent of Authority.
    for tag in c
        .tags
        .iter()
        .filter(|t| t.id.as_str().starts_with("economy."))
    {
        assert!(authority(tag.id.as_str()).is_empty(), "{}", tag.id);
    }
    // Nothing else targets Authority.
    let sources: Vec<_> = c
        .tags
        .iter()
        .filter(|t| !authority(t.id.as_str()).is_empty())
        .map(|t| t.id.as_str())
        .collect();
    assert_eq!(sources.len(), 10, "{sources:?}");
}

#[test]
fn automatic_generation_rolls_authority_on_1d12() {
    let c = content();
    let generator = Generator::new(&c).unwrap();
    let mut selections = Selections::new();
    selections.insert("table.settingProfile".into(), "kurovia".into());
    choose_settlement_state(&mut selections);
    for face in 1..=12 {
        // Region, Environment, Connections, Size, Origin, Purpose, Age,
        // Economy, then Authority, then one sub-roll for some results: Size
        // for a group (1 = 2 representatives) or Representation for Guild
        // and Military (1 = single).
        let mut faces = vec![1, 1, 1, 1, 1, 1, 1, 1, face];
        if !matches!(face, 4..=6 | 9) {
            faces.push(1);
        }
        let mut dice = ScriptedFaces::new(faces);
        let log = generator.run_sequence(&selections, &mut dice).unwrap();
        assert_eq!(dice.remaining(), 0, "unused dice");
        let record = log.record_for(&"table.authority".into()).unwrap();
        assert!(!record.selected);
        assert_eq!(record.dice.as_deref(), Some("1d12"));
        assert_eq!(record.attempts.len(), 1);
        assert_eq!(record.attempts[0].roll.total, face as i32);
        let (_, id, name, tag) = AUTHORITY
            .iter()
            .find(|((min, max), ..)| (*min..=*max).contains(&(face as i32)))
            .unwrap();
        assert_eq!(record.result.id.as_str(), *id, "face {face}");
        assert_eq!(record.result.name, *name, "face {face}");
        let tags: Vec<_> = record.tags.iter().map(|g| g.id.as_str()).collect();
        assert_eq!(tags, [*tag], "face {face}");
        // Authority follows Economy.
        let order: Vec<_> = log.table_rolls().map(|r| r.table.id.as_str()).collect();
        let economy = order.iter().position(|t| *t == "table.economy").unwrap();
        assert_eq!(order[economy + 1], "table.authority");
    }
}

#[test]
fn authority_can_be_chosen_manually() {
    let log = run(
        "kurovia",
        &[("table.authority", "assembly")],
        &[1, 1, 1, 1, 1, 1],
    );
    let record = log.record_for(&"table.authority".into()).unwrap();
    assert!(record.selected);
    assert_eq!(record.result.name, "Popular Assembly");
    assert!(log.has_tag(&"authority.assembly".into()));
}

// ----- Authority representation and size ---------------------------------------

#[test]
fn representation_is_a_conditional_1d4_split_evenly() {
    let c = content();
    let t = table(&c, "table.authorityRepresentation");
    assert_eq!(t.dice.as_deref(), Some("1d4"));
    assert!(t.eligibility.is_some(), "conditional");
    let rows: Vec<_> = t
        .results
        .iter()
        .map(|r| {
            let range = r.range.unwrap();
            (r.id.as_str(), range.min, range.max, r.tags[0].as_str())
        })
        .collect();
    assert_eq!(
        rows,
        [
            ("single", 1, 2, "authorityRepresentation.single"),
            ("group", 3, 4, "authorityRepresentation.group"),
        ]
    );
}

#[test]
fn authority_size_is_a_conditional_1d10_from_2_to_6() {
    let c = content();
    let t = table(&c, "table.authoritySize");
    assert_eq!(t.dice.as_deref(), Some("1d10"));
    assert!(t.eligibility.is_some(), "conditional");
    // Every face 1-10 is claimed once, two faces (20%) per size.
    for face in 1..=10 {
        let claims = t
            .results
            .iter()
            .filter(|r| r.range.unwrap().contains(face))
            .count();
        assert_eq!(claims, 1, "face {face}");
    }
    let rows: Vec<_> = t
        .results
        .iter()
        .map(|r| {
            let range = r.range.unwrap();
            assert_eq!(range.max - range.min + 1, 2, "{} has two faces", r.id);
            assert!(r.tags.is_empty(), "Authority Size is data, not a tag");
            (range.min, range.max, r.value.unwrap(), r.name.as_str())
        })
        .collect();
    assert_eq!(
        rows,
        [
            (1, 2, 2, "2 Representatives"),
            (3, 4, 3, "3 Representatives"),
            (5, 6, 4, "4 Representatives"),
            (7, 8, 5, "5 Representatives"),
            (9, 10, 6, "6 Representatives"),
        ]
    );
    let sequence: Vec<_> = c
        .sequence
        .iter()
        .filter(|s| s.sub_stage)
        .filter_map(|s| s.table().map(|t| t.as_str()))
        .collect();
    assert_eq!(
        sequence,
        ["table.authorityRepresentation", "table.authoritySize"]
    );
}

/// Generates with every stage before Authority on face 1, then the given
/// Authority face and sub-rolls. All dice must be used.
fn generate_authority(faces_after_economy: &[u32]) -> GenerationLog {
    let c = content();
    let mut selections = Selections::new();
    selections.insert("table.settingProfile".into(), "kurovia".into());
    choose_settlement_state(&mut selections);
    let mut faces = vec![1; 8];
    faces.extend_from_slice(faces_after_economy);
    let mut dice = ScriptedFaces::new(faces);
    let log = Generator::new(&c)
        .unwrap()
        .run_sequence(&selections, &mut dice)
        .unwrap();
    assert_eq!(
        dice.remaining(),
        0,
        "unused dice for {faces_after_economy:?}"
    );
    log
}

fn representatives(log: &GenerationLog) -> (Representation, u32) {
    let r = AuthorityRepresentatives::from_log(log).unwrap();
    (r.representation, r.size)
}

#[test]
fn always_group_authorities_go_straight_to_authority_size() {
    // Local Council (1-3), Elders (11) and Popular Assembly (12).
    for authority in [1, 2, 3, 11, 12] {
        let log = generate_authority(&[authority, 5]); // Size 5 = 4
        assert!(
            log.record_for(&"table.authorityRepresentation".into())
                .is_none(),
            "no representation roll for {authority}"
        );
        let size = log.record_for(&"table.authoritySize".into()).unwrap();
        assert_eq!(size.dice.as_deref(), Some("1d10"));
        assert_eq!(size.attempts[0].roll.total, 5);
        assert_eq!(size.value, Some(4));
        assert_eq!(representatives(&log), (Representation::Group, 4));
        assert!(log.has_tag(&"authorityRepresentation.group".into()));
    }
}

#[test]
fn always_single_authorities_roll_nothing_more() {
    // Noble (4-5), Appointed Official (6) and Religious Authority (9).
    for authority in [4, 5, 6, 9] {
        let log = generate_authority(&[authority]);
        for table in ["table.authorityRepresentation", "table.authoritySize"] {
            assert!(log.record_for(&table.into()).is_none(), "{table}");
        }
        assert_eq!(representatives(&log), (Representation::Single, 1));
        assert!(log.has_tag(&"authorityRepresentation.single".into()));
        assert!(!log.has_tag(&"authorityRepresentation.group".into()));
    }
}

#[test]
fn guild_and_military_roll_1d4_for_representation() {
    // Merchant / Guild Rule (7-8) and Military Authority (10).
    for authority in [7, 8, 10] {
        for d4 in 1..=2 {
            let log = generate_authority(&[authority, d4]);
            let rep = log
                .record_for(&"table.authorityRepresentation".into())
                .unwrap();
            assert_eq!(rep.dice.as_deref(), Some("1d4"));
            assert_eq!(rep.attempts[0].roll.total, d4 as i32);
            assert_eq!(rep.result.name, "Single Representative");
            assert!(log.record_for(&"table.authoritySize".into()).is_none());
            assert_eq!(representatives(&log), (Representation::Single, 1));
        }
        for d4 in 3..=4 {
            let log = generate_authority(&[authority, d4, 10]); // Size 10 = 6
            let rep = log
                .record_for(&"table.authorityRepresentation".into())
                .unwrap();
            assert_eq!(rep.attempts[0].roll.total, d4 as i32);
            assert_eq!(rep.result.name, "Group Representatives");
            let size = log.record_for(&"table.authoritySize".into()).unwrap();
            assert_eq!(size.attempts[0].roll.total, 10);
            assert_eq!(representatives(&log), (Representation::Group, 6));
        }
    }
}

#[test]
fn every_authority_size_face_gives_its_count() {
    for (face, count) in [
        (1, 2),
        (2, 2),
        (3, 3),
        (4, 3),
        (5, 4),
        (6, 4),
        (7, 5),
        (8, 5),
        (9, 6),
        (10, 6),
    ] {
        let log = generate_authority(&[1, face]); // Local Council
        let size = log.record_for(&"table.authoritySize".into()).unwrap();
        assert_eq!(size.value, Some(count), "face {face}");
        assert_eq!(size.result.name, format!("{count} Representatives"));
        assert_eq!(representatives(&log).1, count as u32, "face {face}");
    }
}

#[test]
fn the_npc_contract_carries_authority_representation_and_size() {
    let log = generate_authority(&[7, 4, 7]); // Guild, group, 5
    let r = AuthorityRepresentatives::from_log(&log).unwrap();
    assert_eq!(r.authority.id.as_str(), "guild");
    assert_eq!(r.authority.name, "Merchant / Guild Rule");
    let tags: Vec<_> = r.tags.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(tags, ["authority.guild"]);
    assert_eq!((r.representation, r.size), (Representation::Group, 5));
    let json = serde_json::to_value(&r).unwrap();
    assert_eq!(json["representation"], "group");
    assert_eq!(json["size"], 5);
    // No NPCs are generated: nothing but the Authority stages was rolled.
    let tables: Vec<_> = log.table_rolls().map(|r| r.table.id.as_str()).collect();
    assert!(tables.iter().all(|t| !t.to_lowercase().contains("npc")));
}

#[test]
fn origin_purpose_and_age_tables() {
    let c = content();
    expect(
        table(&c, "table.origin"),
        "1d20",
        &[
            ("Purpose-Built", 35),
            ("Strategic Position", 15),
            ("Trade Stop", 10),
            ("Resource Discovery", 10),
            ("Refuge / Resettlement", 10),
            ("Religious Foundation", 5),
            ("Military Outpost", 5),
            ("Estate / Seat", 5),
            ("Organic Growth", 5),
        ],
    );
    expect(
        table(&c, "table.purpose"),
        "1d20",
        &[
            ("Agriculture", 20),
            ("Trade", 15),
            ("Resource Extraction", 15),
            ("Craft / Industry", 10),
            ("Fishing", 10),
            ("Forestry", 5),
            ("Religious", 5),
            ("Administrative", 5),
            ("Transport / Waystation", 5),
            ("Military", 5),
            ("Scholarly", 5),
        ],
    );
    let age = table(&c, "table.age");
    assert_eq!(age.dice.as_deref(), Some("1d8"));
    let faces: Vec<_> = age
        .results
        .iter()
        .map(|r| (r.name.as_str(), r.range.unwrap().min, r.range.unwrap().max))
        .collect();
    assert_eq!(
        faces,
        [
            ("New", 1, 1),
            ("Established", 2, 4),
            ("Old", 5, 7),
            ("Ancient", 8, 8)
        ]
    );
}

#[test]
fn known_modifier_intentions_are_recorded_as_planned_intents() {
    let c = content();
    let intents = |tag: &str| -> Vec<String> {
        c.tag(&tag.into())
            .unwrap()
            .effects
            .iter()
            .filter(|d| !d.is_active())
            .filter_map(|d| match &d.effect {
                Effect::Intent {
                    table,
                    results,
                    concept,
                    ..
                } => Some(concept.clone().unwrap_or_else(|| {
                    // A whole-table intent (a shift along an ordered scale)
                    // is shown by its table.
                    if results.is_empty() {
                        return table.as_ref().map(|t| t.to_string()).unwrap_or_default();
                    }
                    results
                        .iter()
                        .map(|r| r.to_string())
                        .collect::<Vec<_>>()
                        .join("+")
                })),
                _ => None,
            })
            .collect()
    };
    assert_eq!(intents("env.farmlandOpenCountry"), ["agriculture"]);
    assert_eq!(intents("env.woodland"), ["forestry"]);
    assert_eq!(
        intents("env.coast"),
        ["fishing", "trade", "transportWaystation"]
    );
    assert_eq!(intents("env.ichorFields"), ["resourceExtraction"]);
    assert_eq!(
        intents("connection.majorRoute"),
        ["trade+transportWaystation", "table.economy"]
    );
    assert_eq!(
        intents("connection.isolated"),
        ["trade+transportWaystation", "table.economy"]
    );
    assert_eq!(
        intents("purpose.trade"),
        ["table.economy", "guild", "locationType.trade"]
    );
    // The "noble" concept (a guaranteed noble) and the Noble Authority result.
    assert_eq!(intents("origin.estateSeat"), ["noble", "noble"]);
    assert_eq!(intents("age.new").len(), 3);
}

// ----- generation over bundled content ---------------------------------------

fn run(profile: &str, extra: &[(&str, &str)], faces: &[u32]) -> GenerationLog {
    let c = content();
    let mut selections = Selections::new();
    selections.insert("table.settingProfile".into(), profile.into());
    choose_settlement_state(&mut selections);
    // Size, Economy and Authority are chosen so these tests' dice do not
    // depend on them; rolling them is covered by their own tests below.
    selections.insert("table.size".into(), "village".into());
    selections.insert("table.economy".into(), "stable".into());
    selections.insert("table.authority".into(), "council".into());
    selections.insert("table.authoritySize".into(), "four".into());
    for (t, r) in extra {
        selections.insert((*t).into(), (*r).into());
    }
    let mut dice = ScriptedFaces::new(faces.iter().copied());
    let log = Generator::new(&c)
        .unwrap()
        .run_sequence(&selections, &mut dice)
        .unwrap();
    assert_eq!(dice.remaining(), 0, "unused dice");
    log
}

/// Chooses Security, Crime and Stability so a test's dice do not depend on
/// them; rolling them is covered by `settlement_state.rs`.
fn choose_settlement_state(selections: &mut Selections) {
    selections.insert("table.security".into(), "guarded".into());
    selections.insert("table.crime".into(), "moderate".into());
    selections.insert("table.stability".into(), "stable".into());
}

fn result_of(log: &GenerationLog, table: &str) -> String {
    log.record_for(&TableId::from(table))
        .map(|r| r.result.name.clone())
        .unwrap_or_default()
}

#[test]
fn kurovia_uses_the_kurovia_region_and_region_environment() {
    // Region 1 = Ordon; Environment 19 = Coast; Connections 16 = Major Route
    // (Ordon variant); Size chosen; Origin 20; Purpose 20; Age 8.
    let log = run("kurovia", &[], &[1, 19, 16, 20, 20, 8]);
    assert_eq!(result_of(&log, "table.kuroviaRegion"), "Ordon");
    let env = log
        .record_for(&"table.kuroviaEnvironment.ordon".into())
        .unwrap();
    assert_eq!(env.result.name, "Coast");
    assert_eq!(
        env.redirected_from.as_ref().unwrap().id.as_str(),
        "table.environment"
    );
    let conn = log.record_for(&"table.connections".into()).unwrap();
    assert_eq!(conn.variant.as_ref().unwrap().name, "Ordon");
    assert_eq!(conn.result.name, "Major Route");
    assert!(log.record_for(&"table.size".into()).unwrap().selected);
    assert_eq!(result_of(&log, "table.age"), "Ancient");
}

#[test]
fn shinkira_with_coast_can_upgrade_to_major_route() {
    // Region 4 = Shinkira; Environment 15 = Coast; Connections 6 = Local Road;
    // follow-up 1d4 = 4 → Major Route.
    let log = run("kurovia", &[], &[4, 15, 6, 4, 1, 1, 1]);
    let conn = log.record_for(&"table.connections".into()).unwrap();
    assert_eq!(conn.variant.as_ref().unwrap().name, "Shinkira");
    assert_eq!(conn.attempts[0].result.name, "Local Road");
    assert_eq!(conn.follow_ups.len(), 1);
    assert_eq!(conn.follow_ups[0].roll.total, 4);
    assert_eq!(conn.result.name, "Major Route");
    assert!(log.has_tag(&"connection.majorRoute".into()));
    assert!(!log.has_tag(&"connection.localRoad".into()));
    assert!(
        conn.to_string()
            .contains("Shinkira coastal upgrade: roll 4 (1d4) → Major Route")
    );

    // 1-3 keep the original result.
    let log = run("kurovia", &[], &[4, 15, 14, 3, 1, 1, 1]);
    let conn = log.record_for(&"table.connections".into()).unwrap();
    assert_eq!(conn.result.name, "Regional Road");
    assert_eq!(conn.follow_ups[0].becomes, None);
}

#[test]
fn shinkira_without_coast_never_reaches_major_route() {
    // Environment 3 = Woodland: no follow-up roll; 20 = Regional Road.
    let log = run("kurovia", &[], &[4, 3, 20, 1, 1, 1]);
    let conn = log.record_for(&"table.connections".into()).unwrap();
    assert!(conn.follow_ups.is_empty());
    assert_eq!(conn.result.name, "Regional Road");
}

#[test]
fn shinkira_isolated_is_not_upgraded_even_with_coast() {
    let log = run("kurovia", &[], &[4, 16, 2, 1, 1, 1]);
    let conn = log.record_for(&"table.connections".into()).unwrap();
    assert!(conn.follow_ups.is_empty());
    assert_eq!(conn.result.name, "Isolated");
}

#[test]
fn targoth_cannot_roll_major_route() {
    let log = run("kurovia", &[], &[8, 1, 20, 1, 1, 1]);
    assert_eq!(result_of(&log, "table.connections"), "Regional Road");
}

#[test]
fn other_profiles_use_generic_region_and_stop_at_undesigned_environment() {
    let c = content();
    for profile in [
        "lowFantasy",
        "mediumFantasy",
        "highFantasy",
        "forgottenRealms",
        "horror",
    ] {
        let mut selections = Selections::new();
        selections.insert("table.settingProfile".into(), profile.into());
        choose_settlement_state(&mut selections);
        selections.insert("table.size".into(), "village".into());
        let generator = Generator::new(&c).unwrap();
        // Region 9 = Coastlands from Generic Region (Horror's own table is
        // only planned).
        let log = generator
            .run_with(
                &["table.settingProfile".into(), "table.genericRegion".into()],
                &selections,
                &mut ScriptedFaces::new([9]),
            )
            .unwrap();
        assert_eq!(
            result_of(&log, "table.genericRegion"),
            "Coastlands",
            "{profile}"
        );
        // The full pipeline stops at Environment rather than inventing one.
        let err = generator
            .run_sequence(&selections, &mut ScriptedFaces::new([9]))
            .unwrap_err();
        assert!(
            matches!(&err, GenerationError::IncompleteContent { table, .. } if table.as_str() == "table.environment"),
            "{profile}: {err}"
        );
        assert_eq!(
            err.to_string(),
            "Environment cannot be generated yet: its content is incomplete (Environment content exists only for Kurovia regions. Environment for other regions has not been designed.)"
        );
    }
}

#[test]
fn automatic_generation_rolls_size_on_1d10() {
    let c = content();
    let generator = Generator::new(&c).unwrap();
    let mut selections = Selections::new();
    selections.insert("table.settingProfile".into(), "kurovia".into());
    choose_settlement_state(&mut selections);
    // Region 1 = Ordon; Environment 1; Connections 1; then Size, Origin 1,
    // Purpose 1, Age 1, Economy 1, Authority 6 (single: no sub-rolls).
    for (face, name, tag) in [
        (1, "Trading Post", "size.tradingPost"),
        (2, "Trading Post", "size.tradingPost"),
        (3, "Village", "size.village"),
        (4, "Village", "size.village"),
        (5, "Town", "size.town"),
        (6, "Town", "size.town"),
        (7, "City", "size.city"),
        (8, "City", "size.city"),
        (9, "Capital", "size.capital"),
        (10, "Capital", "size.capital"),
    ] {
        let mut dice = ScriptedFaces::new([1, 1, 1, face, 1, 1, 1, 1, 6]);
        let log = generator.run_sequence(&selections, &mut dice).unwrap();
        assert_eq!(dice.remaining(), 0, "unused dice");
        let size = log.record_for(&"table.size".into()).unwrap();
        assert!(!size.selected);
        assert_eq!(size.dice.as_deref(), Some("1d10"));
        assert_eq!(size.attempts.len(), 1);
        assert_eq!(size.attempts[0].roll.total, face as i32);
        assert_eq!(size.result.name, name, "face {face}");
        let size_tags: Vec<_> = size.tags.iter().map(|g| g.id.as_str()).collect();
        assert_eq!(size_tags, [tag], "face {face}");
        assert!(log.has_tag(&tag.into()));
    }
}

#[test]
fn size_can_still_be_chosen_manually() {
    let log = run("kurovia", &[("table.size", "capital")], &[1, 1, 1, 1, 1, 1]);
    let size = log.record_for(&"table.size".into()).unwrap();
    assert!(size.selected);
    assert_eq!(size.result.name, "Capital");
    assert!(log.has_tag(&"size.capital".into()));
}

#[test]
fn every_stage_has_its_pipeline_name() {
    let names: Vec<_> = content()
        .sequence
        .iter()
        .map(|s| s.name.clone().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "Setting Profile",
            "Region",
            "Name",
            "Environment",
            "Connections",
            "Size",
            "Origin",
            "Purpose",
            "Age",
            "Economy",
            "Authority",
            "Representation",
            "Authority Size",
            "Roles",
            "Authority Figures",
            "Security",
            "Crime",
            "Stability",
            "Notable Locations"
        ]
    );
}
