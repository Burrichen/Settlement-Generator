//! The bundled v0.1.0 content, checked against the approved specification.

use std::collections::BTreeMap;

use settlement_core::bundled::{bundled_content, bundled_file_names};
use settlement_core::content::{ContentSet, ContentStatus, Effect, SelectionMode, Table};
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
    assert_eq!(incomplete, ["table.environment", "table.size"]);
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
    ] {
        assert!(
            !c.tables.iter().any(|t| t.name.contains(name)),
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
            "table.age"
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
fn size_is_explicitly_incomplete() {
    let c = content();
    let t = table(&c, "table.size");
    assert_eq!(t.status, ContentStatus::Incomplete);
    assert!(t.dice.is_none(), "no distribution may be invented");
    assert!(t.results.iter().all(|r| r.range.is_none()));
    let names: Vec<_> = t.results.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(
        names,
        ["Trading Post", "Village", "Town", "City", "Capital"]
    );
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
                    results, concept, ..
                } => Some(concept.clone().unwrap_or_else(|| {
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
        ["trade+transportWaystation"]
    );
    assert_eq!(
        intents("connection.isolated"),
        ["trade+transportWaystation"]
    );
    assert_eq!(intents("origin.estateSeat"), ["noble"]);
    assert_eq!(intents("age.new").len(), 3);
}

// ----- generation over bundled content ---------------------------------------

fn run(profile: &str, extra: &[(&str, &str)], faces: &[u32]) -> GenerationLog {
    let c = content();
    let mut selections = Selections::new();
    selections.insert("table.settingProfile".into(), profile.into());
    // Size has no approved distribution, so it must be chosen.
    selections.insert("table.size".into(), "village".into());
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

fn result_of(log: &GenerationLog, table: &str) -> String {
    log.record_for(&TableId::from(table))
        .map(|r| r.result.name.clone())
        .unwrap_or_default()
}

#[test]
fn kurovia_uses_the_kurovia_region_and_region_environment() {
    // Region 1 = Ordon; Environment 19 = Coast; Connections 16 = Major Route
    // (Ordon variant); Size chosen (incomplete); Origin 20; Purpose 20; Age 8.
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
fn incomplete_size_stops_automatic_generation_with_a_readable_error() {
    let c = content();
    let mut selections = Selections::new();
    selections.insert("table.settingProfile".into(), "kurovia".into());
    let err = Generator::new(&c)
        .unwrap()
        .run_sequence(&selections, &mut ScriptedFaces::new([1, 1, 1]))
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Size cannot be generated yet: its content is incomplete (The dice distribution for Size has not been finalised.)"
    );
}

#[test]
fn incomplete_size_can_still_be_chosen_manually() {
    let log = run("kurovia", &[("table.size", "town")], &[1, 1, 1, 1, 1, 1]);
    let size = log.record_for(&"table.size".into()).unwrap();
    assert!(size.selected);
    assert_eq!(size.result.name, "Town");
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
            "Age"
        ]
    );
}
