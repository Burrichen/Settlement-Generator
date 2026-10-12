//! Settlement naming (v0.1.1): the Ordon naming content, the deterministic
//! joining rules, scheme routing, the name stage in every generation mode,
//! manual names, and snapshot preservation.

use std::sync::Arc;

use serde_json::json;
use settlement_core::bundled::bundled_content;
use settlement_core::content::{Condition, ContentSet};
use settlement_core::dice::{RandomFaces, ScriptedFaces};
use settlement_core::engine::{GenerationError, GenerationOptions, SkipReason};
use settlement_core::ids::{ResultId, TableId, TagId};
use settlement_core::naming::{NAME_STAGE_ID, NameSource};
use settlement_core::session::{GenerationMode, GenerationSession, StageStatus, ViewOptions};
use settlement_core::settlements::SettlementSnapshot;
use settlement_core::validation::{IssueKind, validate};

const PROFILE: usize = 0;
const REGION: usize = 1;
const NAME: usize = 2;

const METHOD: &str = "table.naming.ordon.method";
const FIRST: &str = "table.naming.ordon.firstElement";
const SECOND: &str = "table.naming.ordon.secondElement";
const FOUNDER: &str = "table.naming.ordon.founder";
const ENDING: &str = "table.naming.ordon.founderEnding";

fn content() -> ContentSet {
    bundled_content().unwrap()
}

fn dice(values: &[u32]) -> ScriptedFaces {
    ScriptedFaces::new(values.iter().copied())
}

fn session(c: &ContentSet, mode: GenerationMode, god_mode: bool) -> GenerationSession {
    GenerationSession::new(
        Arc::new(c.clone()),
        GenerationOptions {
            god_mode,
            ..Default::default()
        },
        mode,
    )
    .unwrap()
}

/// A One-table session at Kurovia › Ordon (or another region by face).
fn kurovia(c: &ContentSet, region_face: u32) -> GenerationSession {
    let mut s = session(c, GenerationMode::OneTable, false);
    s.choose(PROFILE, "kurovia".into(), &mut dice(&[])).unwrap();
    s.roll(REGION, &mut dice(&[region_face])).unwrap();
    s
}

fn name_of(s: &GenerationSession) -> String {
    s.view(NAME, ViewOptions::default())
        .unwrap()
        .naming
        .unwrap()
        .name
        .unwrap()
}

/// `(result id, name, first face, last face)` for every result of `table`.
fn layout(c: &ContentSet, table: &str) -> Vec<(String, String, i32, i32)> {
    c.table(&TableId::new(table))
        .unwrap()
        .results
        .iter()
        .map(|r| {
            let range = r.range.unwrap();
            (r.id.to_string(), r.name.clone(), range.min, range.max)
        })
        .collect()
}

/// Every face from 1 to `faces` belongs to exactly one result.
fn assert_exact_cover(c: &ContentSet, table: &str, faces: i32) {
    for face in 1..=faces {
        let owners = layout(c, table)
            .iter()
            .filter(|(_, _, min, max)| (*min..=*max).contains(&face))
            .count();
        assert_eq!(owners, 1, "{table}: face {face} has {owners} results");
    }
    let last = layout(c, table).iter().map(|r| r.3).max().unwrap();
    assert_eq!(last, faces, "{table}: no faces beyond {faces}");
}

// ----- the Ordon content ---------------------------------------------------------

#[test]
fn geographic_tables_cover_every_d100_face_exactly_once() {
    let c = content();
    assert_exact_cover(&c, FIRST, 100);
    assert_exact_cover(&c, SECOND, 100);
    for table in [FIRST, SECOND, FOUNDER] {
        assert_eq!(
            c.table(&TableId::new(table)).unwrap().dice.as_deref(),
            Some("1d100")
        );
    }
}

#[test]
fn first_elements_keep_the_approved_order_and_weights() {
    let c = content();
    let first = layout(&c, FIRST);
    let names: Vec<&str> = first.iter().map(|r| r.1.as_str()).collect();
    assert_eq!(
        names,
        [
            "Alder", "Ash", "Burn", "Black", "Bram", "Bracken", "Crow", "Dun", "Elm", "Farn",
            "Green", "Hart", "Hazel", "Hol", "Lang", "Oak", "Red", "Stan", "Thorn", "Wren", "Beck",
            "Birch", "Broad", "Clay", "East", "Fen", "High", "Mere", "North", "West",
        ]
    );
    let four = [
        "Alder", "Ash", "Bracken", "Elm", "Hart", "Hazel", "Oak", "Thorn", "Beck", "Birch",
    ];
    for (_, name, min, max) in &first {
        let expected = if four.contains(&name.as_str()) { 4 } else { 3 };
        assert_eq!(max - min + 1, expected, "{name}");
    }
}

#[test]
fn second_elements_are_the_22_endings_with_the_approved_weights() {
    let c = content();
    let weights: Vec<(String, i32)> = layout(&c, SECOND)
        .into_iter()
        .map(|(_, name, min, max)| (name, max - min + 1))
        .collect();
    let expected = [
        ("ton", 14),
        ("ham", 13),
        ("ley", 13),
        ("ford", 8),
        ("wick", 8),
        ("bury", 3),
        ("field", 3),
        ("den", 3),
        ("combe", 3),
        ("hurst", 3),
        ("well", 3),
        ("worth", 3),
        ("brook", 3),
        ("bridge", 3),
        ("dale", 3),
        ("cliff", 2),
        ("croft", 2),
        ("gate", 2),
        ("holt", 2),
        ("moor", 2),
        ("stead", 2),
        ("wood", 2),
    ]
    .map(|(n, f)| (n.to_owned(), f));
    assert_eq!(weights, expected);
}

#[test]
fn founders_are_100_names_one_face_each_in_order() {
    let c = content();
    let founders = layout(&c, FOUNDER);
    assert_eq!(founders.len(), 100);
    for (i, (_, name, min, max)) in founders.iter().enumerate() {
        let face = i as i32 + 1;
        assert_eq!((*min, *max), (face, face), "{name}");
    }
    let names: Vec<&str> = founders.iter().map(|r| r.1.as_str()).collect();
    assert_eq!(names[..3], ["Aldric", "Alden", "Alaric"]);
    assert_eq!(names[73], "Godwin");
    assert_eq!(names[94], "Nerys");
    assert_eq!(names[98..], ["Godric", "Hereward"]);
    let mut unique = names.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), 100, "founder names are distinct");
}

#[test]
fn founder_endings_cover_the_d12() {
    let c = content();
    assert_exact_cover(&c, ENDING, 12);
    let endings: Vec<(String, i32, i32)> = layout(&c, ENDING)
        .into_iter()
        .map(|(_, n, a, b)| (n, a, b))
        .collect();
    assert_eq!(
        endings,
        [
            ("ton", 1, 3),
            ("ham", 4, 5),
            ("bury", 6, 7),
            ("stead", 8, 9),
            ("wick", 10, 10),
            ("worth", 11, 11),
            ("ley", 12, 12),
        ]
        .map(|(n, a, b)| (n.to_owned(), a, b))
    );
}

#[test]
fn methods_are_weighted_70_30_on_a_d10() {
    let c = content();
    let method = c.table(&TableId::new(METHOD)).unwrap();
    assert_eq!(method.dice.as_deref(), Some("1d10"));
    assert_eq!(
        layout(&c, METHOD),
        [
            ("geographic".into(), "Geographic Compound".into(), 1, 7),
            ("founder".into(), "Historical Founder".into(), 8, 10),
        ]
    );
    // Rolled with real dice: faces 1–7 pick Geographic, 8–10 Founder.
    for (face, expected) in [
        (1, "geographic"),
        (7, "geographic"),
        (8, "founder"),
        (10, "founder"),
    ] {
        let mut s = kurovia(&c, 1);
        s.roll(NAME, &mut dice(&[face, 50, 5])).unwrap();
        let method = s
            .view(NAME, ViewOptions::default())
            .unwrap()
            .naming
            .unwrap()
            .method;
        assert_eq!(method.unwrap().id.as_str(), expected, "face {face}");
    }
}

// ----- acceptance cases, rolled with physical dice ---------------------------------

#[test]
fn acceptance_names() {
    let c = content();
    let cases: [(&[u32], &str); 7] = [
        (&[1, 1, 49], "Alderwick"),   // Geographic: Alder + wick
        (&[8, 74, 6], "Godwinsbury"), // Founder: Godwin + bury
        (&[8, 95, 6], "Nerysbury"),   // Founder: Nerys + bury (no second s)
        (&[1, 5, 15], "Asham"),       // exception: Ash + ham
        (&[1, 5, 69], "Ashurst"),     // exception: Ash + hurst
        (&[8, 25, 8], "Edransstead"), // Edran + stead
        (&[8, 13, 10], "Bericwick"),  // Beric + wick (no s)
    ];
    for (faces, expected) in cases {
        let mut s = kurovia(&c, 1);
        s.roll(NAME, &mut dice(faces)).unwrap();
        assert_eq!(name_of(&s), expected, "dice {faces:?}");
    }
    let mut s = kurovia(&c, 1);
    s.roll(NAME, &mut dice(&[9, 28, 11])).unwrap();
    assert_eq!(name_of(&s), "Oswaldsworth");
    s.roll(NAME, &mut dice(&[2, 18, 59])).unwrap();
    assert_eq!(name_of(&s), "Brackenbury");
    s.roll(NAME, &mut dice(&[3, 62, 41])).unwrap();
    assert_eq!(name_of(&s), "Thornford");
}

#[test]
fn every_roll_is_recorded_and_inspectable_with_tag_view() {
    let c = content();
    let mut s = kurovia(&c, 1);
    s.roll(NAME, &mut dice(&[8, 74, 6])).unwrap();
    let plain = s.view(NAME, ViewOptions::default()).unwrap();
    let naming = plain.naming.as_ref().unwrap();
    assert!(
        naming.parts.is_empty() && naming.routing.is_empty(),
        "details only with Tag View"
    );
    assert_eq!(plain.result.unwrap().name, "Godwinsbury");
    assert_eq!(plain.outcome.unwrap().roll.as_deref(), Some("8"));

    let tagged = s
        .view(
            NAME,
            ViewOptions {
                show_tags: true,
                debug: false,
            },
        )
        .unwrap();
    let naming = tagged.naming.unwrap();
    assert_eq!(naming.scheme.unwrap().name, "Ordon");
    assert_eq!(naming.method.unwrap().name, "Historical Founder");
    let routing: Vec<&str> = naming.routing.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(routing, ["world.kurovia", "region.ordon"]);
    let parts: Vec<(&str, &str, &str)> = naming
        .parts
        .iter()
        .map(|p| {
            (
                p.result.name.as_str(),
                p.dice.as_deref().unwrap(),
                p.roll.as_deref().unwrap(),
            )
        })
        .collect();
    assert_eq!(parts, [("Godwin", "1d100", "74"), ("bury", "1d12", "6")]);
}

#[test]
fn naming_is_deterministic_for_the_same_dice() {
    let c = content();
    let names: Vec<String> = (0..2)
        .map(|_| {
            let mut s = kurovia(&c, 1);
            s.roll(NAME, &mut RandomFaces::seeded(42)).unwrap();
            name_of(&s)
        })
        .collect();
    assert_eq!(names[0], names[1]);
}

// ----- routing and fallback ----------------------------------------------------------

#[test]
fn kurovia_and_ordon_select_the_ordon_scheme() {
    let c = content();
    let s = kurovia(&c, 1); // Ordon
    let view = s.view(NAME, ViewOptions::default()).unwrap();
    assert!(view.can_roll);
    assert_eq!(view.table.id.as_str(), METHOD);
    assert_eq!(
        view.naming.unwrap().scheme.unwrap().id.as_str(),
        "naming.kurovia.ordon"
    );
    let methods: Vec<&str> = view
        .options
        .iter()
        .map(|o| o.result.name.as_str())
        .collect();
    assert_eq!(methods, ["Geographic Compound", "Historical Founder"]);
}

#[test]
fn regions_without_a_scheme_skip_the_name_and_allow_a_typed_one() {
    let c = content();
    // Every Kurovian region has naming data now; other Setting Profiles have
    // none yet (no generic scheme exists).
    let mut s = session(&c, GenerationMode::OneTable, false);
    s.choose(PROFILE, "lowFantasy".into(), &mut dice(&[]))
        .unwrap();
    s.roll(REGION, &mut dice(&[1])).unwrap();
    let view = s.view(NAME, ViewOptions::default()).unwrap();
    assert_eq!(view.skipped, Some(SkipReason::NoNamingScheme));
    assert!(!view.can_roll && view.options.is_empty());
    // Nothing is fabricated: choosing a method is refused...
    assert!(matches!(
        s.choose(NAME, "founder".into(), &mut dice(&[])),
        Err(GenerationError::NoNamingScheme)
    ));
    // ...but a typed name always works.
    s.set_custom_name(NAME, "  Saltmarsh  ").unwrap();
    let view = s.view(NAME, ViewOptions::default()).unwrap();
    assert_eq!(view.status, StageStatus::Resolved);
    assert_eq!(view.naming.unwrap().source, Some(NameSource::Custom));
    assert_eq!(s.name().as_deref(), Some("Saltmarsh"));
}

#[test]
fn typed_names_are_checked() {
    let c = content();
    let mut s = kurovia(&c, 1);
    assert!(matches!(
        s.set_custom_name(NAME, "   "),
        Err(GenerationError::InvalidName(_))
    ));
    assert!(s.set_custom_name(NAME, &"x".repeat(81)).is_err());
    assert!(
        s.set_custom_name(NAME, &"😀".repeat(80)).is_ok(),
        "characters, not bytes"
    );
    assert!(matches!(
        s.set_custom_name(REGION, "Not a name stage"),
        Err(GenerationError::NotNamingStage { .. })
    ));
}

#[test]
fn choosing_a_method_rolls_only_that_methods_components() {
    let c = content();
    let mut s = kurovia(&c, 1);
    s.choose(NAME, "founder".into(), &mut dice(&[74, 6]))
        .unwrap();
    assert_eq!(name_of(&s), "Godwinsbury");
    let outcome = s
        .view(NAME, ViewOptions::default())
        .unwrap()
        .outcome
        .unwrap();
    assert!(outcome.chosen);
    // The name can be rerolled without touching the rest of the settlement.
    let region_before = s.view(REGION, ViewOptions::default()).unwrap().result;
    s.roll(NAME, &mut dice(&[1, 1, 49])).unwrap();
    assert_eq!(name_of(&s), "Alderwick");
    assert_eq!(
        s.view(REGION, ViewOptions::default()).unwrap().result,
        region_before
    );
}

#[test]
fn changing_the_region_invalidates_a_generated_name_but_not_a_typed_one() {
    let c = content();
    let mut s = kurovia(&c, 1);
    s.roll(NAME, &mut dice(&[1, 1, 49])).unwrap(); // Alderwick
    // Bratislor names by its own scheme: the Ordon name is invalidated and
    // stays visible until a new name is rolled.
    let change = s.roll(REGION, &mut dice(&[5])).unwrap(); // Bratislor
    assert!(change.invalidated.contains(&NAME));
    let view = s.view(NAME, ViewOptions::default()).unwrap();
    assert_eq!(view.previous.unwrap().name, "Alderwick");
    assert_eq!(view.status, StageStatus::Pending);
    assert_eq!(
        view.naming.unwrap().scheme.unwrap().id.as_str(),
        "naming.kurovia.bratislor"
    );
    // A Setting Profile with no naming data skips the stage instead.
    s.choose(PROFILE, "lowFantasy".into(), &mut dice(&[]))
        .unwrap();
    s.roll(REGION, &mut dice(&[1])).unwrap();
    let view = s.view(NAME, ViewOptions::default()).unwrap();
    assert_eq!(view.skipped, Some(SkipReason::NoNamingScheme));

    let mut s = kurovia(&c, 1);
    s.set_custom_name(NAME, "Riverside").unwrap();
    let change = s.roll(REGION, &mut dice(&[5])).unwrap();
    assert!(!change.invalidated.contains(&NAME));
    assert_eq!(s.name().as_deref(), Some("Riverside"));
}

#[test]
fn the_name_stage_works_in_every_mode() {
    let c = content();
    // Incremental: Name comes straight after Region.
    let mut s = session(&c, GenerationMode::Incremental, false);
    s.choose(PROFILE, "kurovia".into(), &mut dice(&[])).unwrap();
    s.advance().unwrap();
    s.roll(REGION, &mut dice(&[1])).unwrap();
    s.advance().unwrap();
    assert_eq!(s.cursor(), NAME);
    s.roll(NAME, &mut dice(&[1, 1, 49])).unwrap();
    assert_eq!(name_of(&s), "Alderwick");
    s.advance().unwrap();

    // Automatic: Ordon settlements are named; others are skipped. A typed
    // name survives a full re-roll.
    let mut selections = settlement_core::engine::Selections::new();
    selections.insert("table.settingProfile".into(), "kurovia".into());
    let mut named = 0;
    for seed in 0..40 {
        let s = GenerationSession::automatic(
            Arc::new(c.clone()),
            GenerationOptions::default(),
            &selections,
            &mut RandomFaces::seeded(seed),
        )
        .unwrap();
        let region = s
            .view(REGION, ViewOptions::default())
            .unwrap()
            .result
            .unwrap();
        let name = s.view(NAME, ViewOptions::default()).unwrap();
        // Every Kurovian region has naming data, so Automatic always
        // names a Kurovia settlement.
        assert_eq!(name.status, StageStatus::Resolved, "{}", region.name);
        assert!(s.name().is_some());
        named += 1;
    }
    assert_eq!(named, 40);

    let mut s = kurovia(&c, 1);
    s.set_custom_name(NAME, "Riverside").unwrap();
    s.choose(5, "town".into(), &mut dice(&[])).ok(); // Size, if reached
    s.reroll_all(&mut RandomFaces::seeded(3)).ok();
    assert_eq!(s.name().as_deref(), Some("Riverside"));
}

#[test]
fn god_mode_unlocks_restricted_methods_without_touching_region_tags() {
    let mut c = content();
    let method = c
        .tables
        .iter_mut()
        .find(|t| t.id.as_str() == METHOD)
        .unwrap();
    // Suppose Historical Founder needed a tag this settlement lacks.
    method.results[1].eligibility = Some(Condition::HasTag(TagId::new("age.old")));
    for god_mode in [false, true] {
        let mut s = session(&c, GenerationMode::OneTable, god_mode);
        s.choose(PROFILE, "kurovia".into(), &mut dice(&[])).unwrap();
        s.roll(REGION, &mut dice(&[1])).unwrap();
        let tags_before = s.active_tags();
        let chosen = s.choose(NAME, "founder".into(), &mut dice(&[74, 6]));
        assert_eq!(chosen.is_ok(), god_mode);
        assert_eq!(s.active_tags(), tags_before, "naming never changes tags");
    }
}

// ----- snapshots ----------------------------------------------------------------------

fn finished(c: &ContentSet) -> GenerationSession {
    let mut s = kurovia(c, 1);
    s.roll(NAME, &mut dice(&[8, 74, 6])).unwrap(); // Godwinsbury
    s.roll_remaining(&mut RandomFaces::seeded(5)).ok();
    s.choose(5, "town".into(), &mut dice(&[])).unwrap(); // Size
    s.roll_remaining(&mut RandomFaces::seeded(6)).unwrap();
    assert!(s.is_complete());
    s
}

#[test]
fn the_snapshot_records_the_name_and_how_it_was_made() {
    let c = content();
    let snapshot = finished(&c).to_snapshot();
    let stage = snapshot
        .stages
        .iter()
        .find(|s| s.stage_table.as_str() == NAME_STAGE_ID)
        .unwrap();
    assert_eq!(stage.label, "Name");
    assert_eq!(stage.result.as_ref().unwrap().name, "Godwinsbury");
    let record = stage.name.as_ref().unwrap();
    assert_eq!(record.name, "Godwinsbury");
    assert_eq!(record.source, NameSource::Generated);
    assert_eq!(
        record.scheme.as_ref().unwrap().id.as_str(),
        "naming.kurovia.ordon"
    );
    assert_eq!(record.method.as_ref().unwrap().id.as_str(), "founder");
    assert_eq!(record.app_version, settlement_core::VERSION);
    let rolls: Vec<(&str, &str, i32)> = record
        .rolls
        .iter()
        .map(|r| {
            (
                r.result.id.as_str(),
                r.dice.as_deref().unwrap(),
                r.accepted().unwrap().roll.total,
            )
        })
        .collect();
    assert_eq!(
        rolls,
        [
            ("founder", "1d10", 8),
            ("godwin", "1d100", 74),
            ("bury", "1d12", 6)
        ]
    );
    // Naming rolls are not mixed into the settlement's own log or tags.
    assert!(
        snapshot
            .log
            .table_rolls()
            .all(|r| !r.table.id.as_str().starts_with("table.naming"))
    );
}

#[test]
fn saved_names_never_change_when_naming_content_changes() {
    let c = content();
    let snapshot = finished(&c).to_snapshot();
    let json = serde_json::to_string(&snapshot).unwrap();

    // Rename the founder and change the joining rules afterwards.
    let mut changed = c.clone();
    let founder = changed
        .tables
        .iter_mut()
        .find(|t| t.id.as_str() == FOUNDER)
        .unwrap();
    founder.results[73].name = "Godwine".into();
    let ordon = changed
        .naming_schemes
        .iter_mut()
        .find(|s| s.id.as_str() == "naming.kurovia.ordon")
        .unwrap();
    ordon.methods[1].join = Default::default();

    let reopened = GenerationSession::from_snapshot(
        Arc::new(changed),
        GenerationOptions::default(),
        GenerationMode::OneTable,
        &snapshot,
    )
    .unwrap();
    assert_eq!(reopened.name().as_deref(), Some("Godwinsbury"));
    assert_eq!(
        serde_json::to_string(&reopened.to_snapshot()).unwrap(),
        json
    );
}

#[test]
fn settlements_saved_before_naming_open_and_save_unchanged() {
    let c = content();
    // A v0.1.0 snapshot: no Name stage at all.
    let mut snapshot: SettlementSnapshot = finished(&c).to_snapshot();
    snapshot
        .stages
        .retain(|s| s.stage_table.as_str() != NAME_STAGE_ID);
    let reopened = GenerationSession::from_snapshot(
        Arc::new(c.clone()),
        GenerationOptions::default(),
        GenerationMode::OneTable,
        &snapshot,
    )
    .unwrap();
    let view = reopened.view(NAME, ViewOptions::default()).unwrap();
    assert_eq!(view.skipped, Some(SkipReason::NotRecorded));
    assert!(reopened.is_complete(), "can be saved without naming");
    assert_eq!(
        reopened.to_snapshot(),
        snapshot,
        "nothing regenerated or added"
    );
}

// ----- validation ---------------------------------------------------------------------

#[test]
fn naming_schemes_are_validated() {
    let mut c = content();
    let scheme = c
        .naming_schemes
        .iter_mut()
        .find(|s| s.id.as_str() == "naming.kurovia.ordon")
        .unwrap();
    scheme.methods.remove(1); // Historical Founder has no method now
    scheme.methods[0].exceptions[0].results = vec![ResultId::new("nope"), ResultId::new("ham")];
    scheme.methods[0]
        .components
        .push(TableId::new("table.missing"));
    let report = validate(&c);
    let kinds: Vec<&IssueKind> = report.errors().map(|i| &i.kind).collect();
    assert!(
        kinds
            .iter()
            .any(|k| matches!(k, IssueKind::NamingMethodMissing))
    );
    assert!(kinds.iter().any(
        |k| matches!(k, IssueKind::MissingTable { table } if table.as_str() == "table.missing")
    ));
    assert!(
        kinds
            .iter()
            .any(|k| matches!(k, IssueKind::NamingExceptionLength { .. }))
    );
    // A broken scheme stops generation with a readable report.
    assert!(
        GenerationSession::new(
            Arc::new(c),
            GenerationOptions::default(),
            GenerationMode::OneTable
        )
        .is_err()
    );
}

#[test]
fn content_files_can_define_schemes_and_naming_steps() {
    let c: ContentSet = serde_json::from_value(json!({
        "sequence": [ "t.a", { "naming": true, "name": "Name" } ],
        "tables": [
            { "id": "t.a", "name": "A", "dice": "1d4", "results": [
                { "id": "x", "name": "X", "range": [1, 4], "tags": ["g.x"] } ] },
            { "id": "t.m", "name": "Methods", "dice": "1d4", "results": [
                { "id": "m", "name": "Only", "range": [1, 4] } ] },
            { "id": "t.p", "name": "Parts", "dice": "1d4", "results": [
                { "id": "p", "name": "Brook", "range": [1, 4] } ] }
        ],
        "tags": [ { "id": "g.x", "name": "X" } ],
        "naming_schemes": [ {
            "id": "n.x", "name": "X names", "when": { "has_tag": "g.x" },
            "method_table": "t.m",
            "methods": [ { "result": "m", "components": ["t.p", "t.p"] } ]
        } ]
    }))
    .unwrap();
    assert!(!validate(&c).has_errors(), "{:?}", validate(&c));
    let mut s = GenerationSession::new(
        Arc::new(c),
        GenerationOptions::default(),
        GenerationMode::Automatic,
    )
    .unwrap();
    s.run_automatic(&Default::default(), &mut dice(&[1, 1, 1, 1]))
        .unwrap();
    assert_eq!(s.name().as_deref(), Some("Brookbrook"));
}
