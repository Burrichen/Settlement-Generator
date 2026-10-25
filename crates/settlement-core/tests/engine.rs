//! Generation engine tests. Fixtures are neutral test data, not game content.

use serde_json::json;
use settlement_core::content::ContentSet;
use settlement_core::dice::{Die, DieRoll, RandomFaces, ScriptedFaces};
use settlement_core::engine::{
    AttemptOutcome, GenerationError, GenerationEvent, GenerationLog, Generator, SkipReason,
    TableRollRecord,
};
use settlement_core::ids::TableId;

fn content(value: serde_json::Value) -> ContentSet {
    serde_json::from_value(value).expect("fixture deserialises")
}

fn run(c: &ContentSet, start: &[&str], faces: &[u32]) -> Result<GenerationLog, GenerationError> {
    let start: Vec<TableId> = start.iter().map(|&s| s.into()).collect();
    let mut faces = ScriptedFaces::new(faces.iter().copied());
    let log = Generator::new(c)?.run(&start, &mut faces);
    assert!(
        log.is_err() || faces.remaining() == 0,
        "test supplied more dice than were rolled"
    );
    log
}

fn rolls(log: &GenerationLog) -> Vec<&TableRollRecord> {
    log.table_rolls().collect()
}

fn results(log: &GenerationLog) -> Vec<&str> {
    log.table_rolls().map(|r| r.result.id.as_str()).collect()
}

/// A d12 table with four results, and a 2d6 table.
fn base() -> serde_json::Value {
    json!({
        "tables": [
            { "id": "t.d12", "name": "Economy", "dice": "1d12", "results": [
                { "id": "poor", "name": "Poor", "range": [1, 3] },
                { "id": "modest", "name": "Modest", "range": [4, 6] },
                { "id": "comfortable", "name": "Comfortable", "range": [7, 8] },
                { "id": "prosperous", "name": "Prosperous", "range": [9, 12], "tags": ["g.rich"] }
            ]},
            { "id": "t.2d6", "name": "Two", "dice": "2d6", "results": [
                { "id": "low", "name": "Low", "range": [2, 6] },
                { "id": "seven", "name": "Seven", "range": 7 },
                { "id": "high", "name": "High", "range": [8, 12] }
            ]}
        ],
        "tags": [ { "id": "g.rich", "name": "Rich" } ]
    })
}

#[test]
fn records_the_physical_roll_like_a_table() {
    let log = run(&content(base()), &["t.d12"], &[9]).unwrap();
    let record = rolls(&log)[0];
    assert_eq!(
        record.to_string(),
        "table: Economy\ndice: 1d12\nroll: 9\nresult: Prosperous"
    );
    assert_eq!(
        record.accepted().unwrap().roll.dice,
        vec![DieRoll {
            die: Die::D12,
            value: 9
        }]
    );
    assert_eq!(record.tags[0].id.as_str(), "g.rich");
    assert!(log.has_tag(&"g.rich".into()));
}

#[test]
fn multi_dice_tables_record_every_die() {
    let log = run(&content(base()), &["t.2d6"], &[3, 4]).unwrap();
    let attempt = rolls(&log)[0].accepted().unwrap();
    assert_eq!(attempt.roll.dice.len(), 2);
    assert_eq!(attempt.roll.total, 7);
    assert_eq!(attempt.roll.to_string(), "3 + 4 = 7");
    assert_eq!(results(&log), ["seven"]);
}

#[test]
fn every_supported_die_drives_a_table() {
    for die in Die::ALL {
        let sides = die.sides() as i32;
        let c = content(json!({ "tables": [{
            "id": "t", "name": "T", "dice": format!("1d{sides}"),
            "results": [
                { "id": "first", "name": "First", "range": 1 },
                { "id": "rest", "name": "Rest", "range": [2, sides] }
            ]
        }]}));
        let log = run(&c, &["t"], &[sides as u32]).unwrap();
        assert_eq!(results(&log), ["rest"], "{die}");
        let log = run(&c, &["t"], &[1]).unwrap();
        assert_eq!(results(&log), ["first"], "{die}");
    }
}

#[test]
fn random_generation_only_produces_faces_of_the_declared_dice() {
    let c = content(base());
    let generator = Generator::new(&c).unwrap();
    let mut faces = RandomFaces::seeded(3);
    for _ in 0..500 {
        let log = generator
            .run(&["t.d12".into(), "t.2d6".into()], &mut faces)
            .unwrap();
        for record in log.table_rolls() {
            for d in &record.accepted().unwrap().roll.dice {
                assert!((1..=d.die.sides()).contains(&d.value));
            }
        }
        let two = log.table_rolls().nth(1).unwrap().accepted().unwrap();
        assert!(two.roll.dice.iter().all(|d| d.die == Die::D6));
        assert!((2..=12).contains(&two.face));
    }
}

#[test]
fn routes_to_another_table_depth_first() {
    let mut c = base();
    c["tags"][0]["effects"] = json!([{ "kind": "route_to", "table": "t.2d6" }]);
    // d12 → prosperous → routes to 2d6 before the next start table.
    let log = run(&content(c), &["t.d12", "t.d12"], &[10, 1, 1, 2]).unwrap();
    assert_eq!(results(&log), ["prosperous", "low", "poor"]);
}

#[test]
fn route_can_force_a_variant() {
    let mut c = base();
    c["tables"][1]["variants"] = json!([{
        "id": "v.flat", "name": "Flat", "dice": "1d6",
        "ranges": [
            { "result": "low", "range": [1, 2] },
            { "result": "seven", "range": [3, 4] },
            { "result": "high", "range": [5, 6] }
        ]
    }]);
    c["tags"][0]["effects"] =
        json!([{ "kind": "route_to", "table": "t.2d6", "variant": "v.flat" }]);
    let log = run(&content(c), &["t.d12"], &[12, 4]).unwrap();
    let record = rolls(&log)[1];
    assert_eq!(record.variant.as_ref().unwrap().id.as_str(), "v.flat");
    assert_eq!(record.dice.as_deref(), Some("1d6"));
    assert_eq!(record.result.id.as_str(), "seven");
}

#[test]
fn variant_selected_by_tag_condition_for_later_tables() {
    let mut c = base();
    c["tables"][1]["variants"] = json!([{
        "id": "v.rich", "name": "Rich layout", "when": { "has_tag": "g.rich" },
        "ranges": [
            { "result": "low", "range": [2, 3] },
            { "result": "high", "range": [4, 12] }
        ]
    }]);
    let c = content(c);
    // Without the tag: base layout, 2+3 = 5 → low.
    let log = run(&c, &["t.d12", "t.2d6"], &[1, 2, 3]).unwrap();
    assert_eq!(results(&log), ["poor", "low"]);
    // With the tag from an earlier table: variant layout, 5 → high.
    let log = run(&c, &["t.d12", "t.2d6"], &[9, 2, 3]).unwrap();
    assert_eq!(results(&log), ["prosperous", "high"]);
    assert_eq!(rolls(&log)[1].variant.as_ref().unwrap().name, "Rich layout");
}

#[test]
fn use_variant_effect_selects_a_variant() {
    let mut c = base();
    c["tables"][1]["variants"] = json!([
        { "id": "v.never", "name": "Never", "when": { "has_tag": "g.unused" } },
        { "id": "v.chosen", "name": "Chosen", "dice": "2d6+2", "ranges": [
            { "result": "low", "range": [4, 6] },
            { "result": "seven", "range": 7 },
            { "result": "high", "range": [8, 14] }
        ]}
    ]);
    c["tags"] = json!([
        { "id": "g.rich", "name": "Rich",
          "effects": [{ "kind": "use_variant", "table": "t.2d6", "variant": "v.chosen" }] },
        { "id": "g.unused", "name": "Unused" }
    ]);
    let log = run(&content(c), &["t.d12", "t.2d6"], &[9, 6, 6]).unwrap();
    let record = rolls(&log)[1];
    assert_eq!(record.variant.as_ref().unwrap().id.as_str(), "v.chosen");
    assert_eq!(record.accepted().unwrap().roll.total, 14);
    assert_eq!(record.result.id.as_str(), "high");
}

#[test]
fn restricting_results_rerolls_like_at_the_table() {
    let mut c = base();
    c["tags"][0]["effects"] = json!([{
        "kind": "restrict_results", "table": "t.2d6", "only": ["low", "high"]
    }]);
    // 3+4 = 7 is restricted → reroll 6+6 = 12.
    let log = run(&content(c), &["t.d12", "t.2d6"], &[9, 3, 4, 6, 6]).unwrap();
    let record = rolls(&log)[1];
    assert_eq!(record.attempts.len(), 2);
    assert_eq!(record.attempts[0].outcome, AttemptOutcome::Restricted);
    assert_eq!(record.attempts[0].result.id.as_str(), "seven");
    assert_eq!(record.result.id.as_str(), "high");
    assert_eq!(
        record.to_string(),
        "table: Two\ndice: 2d6\nroll: 3 + 4 = 7 → Seven (restricted, rerolled)\nroll: 6 + 6 = 12\nresult: High"
    );
}

#[test]
fn exclusions_and_only_lists_combine() {
    let mut c = base();
    c["tags"] = json!([
        { "id": "g.rich", "name": "Rich", "effects": [
            { "kind": "restrict_results", "table": "t.2d6", "only": ["low", "seven"] },
            { "kind": "restrict_results", "table": "t.2d6", "exclude": ["low"] }
        ]}
    ]);
    let log = run(&content(c), &["t.d12", "t.2d6"], &[9, 1, 1, 6, 6, 3, 4]).unwrap();
    let outcomes: Vec<_> = rolls(&log)[1].attempts.iter().map(|a| a.outcome).collect();
    assert_eq!(
        outcomes,
        [
            AttemptOutcome::Restricted,
            AttemptOutcome::Restricted,
            AttemptOutcome::Accepted
        ]
    );
}

#[test]
fn result_eligibility_rules_reroll_ineligible_results() {
    let mut c = base();
    c["tables"][1]["results"][0]["eligibility"] = json!({ "has_tag": "g.rich" });
    let c = content(c);
    let log = run(&c, &["t.2d6"], &[1, 1, 4, 4]).unwrap();
    let record = rolls(&log)[0];
    assert_eq!(record.attempts[0].outcome, AttemptOutcome::Ineligible);
    assert_eq!(record.result.id.as_str(), "high");
    // Once the tag is active, the result is eligible.
    let log = run(&c, &["t.d12", "t.2d6"], &[9, 1, 1]).unwrap();
    assert_eq!(results(&log), ["prosperous", "low"]);
}

#[test]
fn disabled_results_are_rerolled() {
    let mut c = base();
    c["tables"][0]["results"][0]["enabled"] = json!(false);
    let log = run(&content(c), &["t.d12"], &[2, 5]).unwrap();
    let record = rolls(&log)[0];
    assert_eq!(record.attempts[0].outcome, AttemptOutcome::Disabled);
    assert_eq!(record.result.id.as_str(), "modest");
}

#[test]
fn modifiers_shift_the_roll_and_clamp_to_the_table() {
    let mut c = base();
    c["tags"][0]["effects"] = json!([
        { "kind": "modify_dice", "table": "t.2d6", "modifier": 3 },
        { "kind": "modify_dice", "table": "t.2d6", "modifier": 1 }
    ]);
    let log = run(&content(c), &["t.d12", "t.2d6"], &[9, 5, 6]).unwrap();
    let record = rolls(&log)[1];
    assert_eq!(record.dice.as_deref(), Some("2d6+4"));
    let attempt = record.accepted().unwrap();
    assert_eq!(attempt.roll.total, 15);
    assert_eq!(attempt.face, 12);
    assert_eq!(record.result.id.as_str(), "high");
    assert!(
        record
            .to_string()
            .contains("roll: 5 + 6 + 4 = 15 (read as 12)")
    );
}

#[test]
fn replacing_dice_changes_the_distribution_not_the_table() {
    let mut c = base();
    // A d12 table rolled on 2d6 instead: 1 can no longer come up.
    c["tags"][0]["effects"] = json!([{ "kind": "modify_dice", "table": "t.d12", "dice": "2d6" }]);
    let log = run(&content(c), &["t.d12", "t.d12"], &[9, 1, 1]).unwrap();
    let second = rolls(&log)[1];
    assert_eq!(second.dice.as_deref(), Some("2d6"));
    assert_eq!(second.accepted().unwrap().roll.dice.len(), 2);
    assert_eq!(second.result.id.as_str(), "poor");
}

#[test]
fn secondary_rolls_are_recorded() {
    let mut c = base();
    c["tags"][0]["effects"] =
        json!([{ "kind": "secondary_roll", "label": "Extra", "dice": "1d4+1d6" }]);
    let log = run(&content(c), &["t.d12"], &[11, 2, 5]).unwrap();
    let secondary = log
        .events
        .iter()
        .find_map(|e| match e {
            GenerationEvent::SecondaryRoll { label, roll, tag } => Some((label, roll, tag)),
            _ => None,
        })
        .unwrap();
    assert_eq!(secondary.0, "Extra");
    assert_eq!(secondary.1.total, 7);
    assert_eq!(secondary.1.dice[0].die, Die::D4);
    assert_eq!(secondary.2.name, "Rich");
}

#[test]
fn add_tag_chains_including_cycles_terminate() {
    let mut c = base();
    c["tags"] = json!([
        { "id": "g.rich", "name": "Rich", "effects": [{ "kind": "add_tag", "tag": "g.b" }] },
        { "id": "g.b", "name": "B", "effects": [{ "kind": "add_tag", "tag": "g.c" }] },
        { "id": "g.c", "name": "C", "effects": [{ "kind": "add_tag", "tag": "g.rich" }] }
    ]);
    let log = run(&content(c), &["t.d12"], &[9]).unwrap();
    let tags: Vec<_> = log.tags.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(tags, ["g.rich", "g.b", "g.c"]);
}

#[test]
fn planned_effects_are_never_executed() {
    let mut c = base();
    c["tags"][0]["effects"] = json!([
        { "kind": "route_to", "table": "t.future", "status": "planned" },
        { "kind": "modify_dice", "table": "t.2d6", "modifier": 5, "status": "planned" },
        { "kind": "add_tag", "tag": "g.future", "status": "planned" }
    ]);
    let log = run(&content(c), &["t.d12", "t.2d6"], &[9, 1, 1]).unwrap();
    assert_eq!(results(&log), ["prosperous", "low"]);
    assert_eq!(rolls(&log)[1].dice.as_deref(), Some("2d6"));
    assert_eq!(log.tags.len(), 1);
}

#[test]
fn disabled_tags_are_not_activated_and_have_no_effect() {
    let mut c = base();
    c["tags"][0]["enabled"] = json!(false);
    c["tags"][0]["effects"] = json!([{ "kind": "route_to", "table": "t.2d6" }]);
    let log = run(&content(c), &["t.d12"], &[9]).unwrap();
    assert!(log.tags.is_empty());
    assert!(rolls(&log)[0].tags.is_empty());
    assert_eq!(results(&log), ["prosperous"]);
}

#[test]
fn inactive_and_ineligible_tables_are_skipped() {
    let mut c = base();
    c["tables"][1]["active"] = json!(false);
    c["tables"].as_array_mut().unwrap().push(json!({
        "id": "t.gated", "name": "Gated", "dice": "1d4",
        "eligibility": { "has_tag": "g.rich" },
        "results": [{ "id": "r", "name": "R", "range": [1, 4] }]
    }));
    let log = run(&content(c), &["t.2d6", "t.gated"], &[]).unwrap();
    let skipped: Vec<_> = log
        .events
        .iter()
        .map(|e| match e {
            GenerationEvent::TableSkipped { table, reason } => (table.id.as_str(), *reason),
            _ => panic!("expected only skips"),
        })
        .collect();
    assert_eq!(
        skipped,
        [
            ("t.2d6", SkipReason::Inactive),
            ("t.gated", SkipReason::NotEligible)
        ]
    );
}

#[test]
fn no_eligible_result_is_an_error_not_a_hang() {
    let mut c = base();
    c["tags"][0]["effects"] = json!([{
        "kind": "restrict_results", "table": "t.2d6", "exclude": ["low", "seven", "high"]
    }]);
    let err = run(&content(c), &["t.d12", "t.2d6"], &[9]).unwrap_err();
    assert!(
        matches!(err, GenerationError::NoEligibleResult { table } if table.as_str() == "t.2d6")
    );
}

#[test]
fn eligible_results_out_of_reach_of_modified_dice_are_an_error() {
    let mut c = base();
    c["tags"][0]["effects"] = json!([
        { "kind": "modify_dice", "table": "t.2d6", "modifier": 10 },
        { "kind": "restrict_results", "table": "t.2d6", "only": ["low"] }
    ]);
    let err = run(&content(c), &["t.d12", "t.2d6"], &[9]).unwrap_err();
    assert!(matches!(err, GenerationError::NoEligibleResult { .. }));
}

#[test]
fn unbounded_routing_hits_the_step_limit() {
    // Validation only warns: "stop" exists but is never eligible.
    let c = content(json!({
        "tables": [{ "id": "a", "name": "A", "dice": "1d4", "results": [
            { "id": "again", "name": "Again", "range": [1, 3], "tags": ["loop"] },
            { "id": "stop", "name": "Stop", "range": 4, "eligibility": { "has_tag": "never" } }
        ]}],
        "tags": [
            { "id": "loop", "name": "Loop", "effects": [{ "kind": "route_to", "table": "a" }] },
            { "id": "never", "name": "Never" }
        ]
    }));
    let err = Generator::new(&c)
        .unwrap()
        .run(&["a".into()], &mut RandomFaces::seeded(1))
        .unwrap_err();
    assert!(matches!(err, GenerationError::StepLimit));
}

#[test]
fn invalid_content_is_rejected_before_generation() {
    let mut c = base();
    c["tables"][0]["results"][0]["range"] = json!([1, 2]);
    let c = content(c);
    let Err(GenerationError::InvalidContent(report)) = Generator::new(&c) else {
        panic!("expected invalid content");
    };
    assert!(report.has_errors());
}

#[test]
fn unknown_start_table_is_an_error() {
    let err = run(&content(base()), &["t.none"], &[]).unwrap_err();
    assert!(matches!(err, GenerationError::UnknownTable(_)));
}

#[test]
fn exhausted_physical_dice_are_reported() {
    let err = run(&content(base()), &["t.2d6"], &[3]).unwrap_err();
    assert!(matches!(err, GenerationError::Dice(_)));
}

#[test]
fn records_are_snapshots_independent_of_later_renames() {
    let mut c = content(base());
    let log = Generator::new(&c)
        .unwrap()
        .run(&["t.d12".into()], &mut ScriptedFaces::new([9]))
        .unwrap();
    c.tables[0].name = "Renamed".into();
    c.tables[0].results[3].name = "Renamed result".into();
    let record = log.table_rolls().next().unwrap();
    assert_eq!(record.table.name, "Economy");
    assert_eq!(record.result.name, "Prosperous");
    // And the log survives a JSON round trip unchanged.
    let json = serde_json::to_string(&log).unwrap();
    assert_eq!(serde_json::from_str::<GenerationLog>(&json).unwrap(), log);
}

// ----- God Mode and explanations ---------------------------------------------

use settlement_core::content::Effect;
use settlement_core::engine::GenerationOptions;

fn god_mode() -> GenerationOptions {
    GenerationOptions {
        god_mode: true,
        ..Default::default()
    }
}

#[test]
fn god_mode_ignores_tag_based_eligibility_but_keeps_emitted_tags() {
    let mut c = base();
    c["tables"][1]["results"][0]["eligibility"] = json!({ "has_tag": "g.missing" });
    c["tables"][1]["results"][0]["tags"] = json!(["g.low"]);
    c["tags"] = json!([
        { "id": "g.rich", "name": "Rich", "effects": [
            { "kind": "restrict_results", "table": "t.2d6", "only": ["seven"] }
        ]},
        { "id": "g.missing", "name": "Missing" },
        { "id": "g.low", "name": "Low tag" }
    ]);
    let c = content(c);

    // Normally "low" (2-6) is both restricted and ineligible: 1+1 is rerolled.
    let log = run(&c, &["t.d12", "t.2d6"], &[9, 1, 1, 3, 4]).unwrap();
    assert_eq!(results(&log), ["prosperous", "seven"]);

    // In God Mode the same first roll is accepted, and its tags are emitted.
    let generator = Generator::new(&c).unwrap().with_options(god_mode());
    let log = generator
        .run(
            &["t.d12".into(), "t.2d6".into()],
            &mut ScriptedFaces::new([9, 1, 1]),
        )
        .unwrap();
    assert_eq!(results(&log), ["prosperous", "low"]);
    assert!(log.has_tag(&"g.low".into()));
    assert!(log.options.god_mode);

    // The ignored restriction is still recorded to explain the result.
    let modifier = &rolls(&log)[1].modifiers[0];
    assert!(modifier.ignored);
    assert_eq!(modifier.tag.as_ref().unwrap().name, "Rich");
}

#[test]
fn god_mode_keeps_disabled_results_disabled() {
    let mut c = base();
    c["tables"][0]["results"][0]["enabled"] = json!(false);
    let c = content(c);
    let log = Generator::new(&c)
        .unwrap()
        .with_options(god_mode())
        .run(&["t.d12".into()], &mut ScriptedFaces::new([1, 5]))
        .unwrap();
    assert_eq!(results(&log), ["modest"]);
}

#[test]
fn records_explain_routing_and_applied_modifiers() {
    let mut c = base();
    c["tables"][1]["variants"] = json!([{ "id": "v", "name": "V", "dice": "1d12+1d6", "ranges": [
        { "result": "low", "range": [2, 6] },
        { "result": "seven", "range": 7 },
        { "result": "high", "range": [8, 18] }
    ]}]);
    c["tags"][0]["effects"] = json!([
        { "kind": "route_to", "table": "t.2d6" },
        { "kind": "use_variant", "table": "t.2d6", "variant": "v" },
        { "kind": "modify_dice", "table": "t.2d6", "modifier": 1 },
        { "kind": "modify_dice", "table": "t.d12", "modifier": 0 }
    ]);
    let log = run(&content(c), &["t.d12"], &[9, 2, 3]).unwrap();
    let record = rolls(&log)[1];
    assert_eq!(record.routed_by.as_ref().unwrap().name, "Rich");
    let kinds: Vec<_> = record
        .modifiers
        .iter()
        .map(|m| match m.effect {
            Effect::UseVariant { .. } => "variant",
            Effect::ModifyDice { .. } => "dice",
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, ["variant", "dice"]);
    assert_eq!(record.dice.as_deref(), Some("1d12+1d6+1"));
    assert!(
        record
            .to_string()
            .starts_with("table: Two\nrouted by: Rich\n")
    );
    // The first table was not routed and had no effective modifiers.
    assert!(rolls(&log)[0].routed_by.is_none());
    assert!(rolls(&log)[0].modifiers.is_empty());
}

// ----- redirects and choices ---------------------------------------------------

use settlement_core::engine::Selections;

fn choices(pairs: &[(&str, &str)]) -> Selections {
    pairs
        .iter()
        .map(|(t, r)| ((*t).into(), (*r).into()))
        .collect()
}

#[test]
fn redirects_replace_a_table_while_the_tag_is_active() {
    let mut c = base();
    c["tags"][0]["effects"] =
        json!([{ "kind": "redirect_table", "table": "t.2d6", "to": "t.d12" }]);
    let log = run(&content(c), &["t.d12", "t.2d6"], &[9, 2]).unwrap();
    let second = rolls(&log)[1];
    assert_eq!(second.table.id.as_str(), "t.d12");
    assert_eq!(
        second.redirected_from.as_ref().unwrap().id.as_str(),
        "t.2d6"
    );
    assert!(matches!(
        second.modifiers[0].effect,
        Effect::RedirectTable { .. }
    ));
    assert!(second.to_string().contains("instead of: Two"));
}

#[test]
fn choose_tables_require_a_choice() {
    let mut c = base();
    c["tables"].as_array_mut().unwrap().push(json!({
        "id": "t.pick", "name": "Pick", "selection": "choose",
        "results": [{ "id": "x", "name": "X", "tags": ["g.rich"] }, { "id": "y", "name": "Y" }]
    }));
    let c = content(c);
    let generator = Generator::new(&c).unwrap();
    let err = generator
        .run(&["t.pick".into()], &mut ScriptedFaces::new([]))
        .unwrap_err();
    assert!(matches!(err, GenerationError::SelectionRequired { .. }));

    let log = generator
        .run_with(
            &["t.pick".into()],
            &choices(&[("t.pick", "x")]),
            &mut ScriptedFaces::new([]),
        )
        .unwrap();
    let record = rolls(&log)[0];
    assert!(record.selected);
    assert!(record.dice.is_none());
    assert!(log.has_tag(&"g.rich".into()));
    assert_eq!(record.to_string(), "table: Pick\nchosen: X\nresult: X");

    let err = generator
        .run_with(
            &["t.pick".into()],
            &choices(&[("t.pick", "nope")]),
            &mut ScriptedFaces::new([]),
        )
        .unwrap_err();
    assert!(matches!(err, GenerationError::UnknownSelection { .. }));
}

#[test]
fn choosing_a_rolled_table_result_respects_eligibility_unless_god_mode() {
    let mut c = base();
    c["tables"][1]["results"][0]["eligibility"] = json!({ "has_tag": "g.rich" });
    let c = content(c);
    let pick = choices(&[("t.2d6", "low")]);
    let err = Generator::new(&c)
        .unwrap()
        .run_with(&["t.2d6".into()], &pick, &mut ScriptedFaces::new([]))
        .unwrap_err();
    assert!(matches!(
        err,
        GenerationError::SelectionNotAllowed {
            reason: AttemptOutcome::Ineligible,
            ..
        }
    ));
    let log = Generator::new(&c)
        .unwrap()
        .with_options(god_mode())
        .run_with(&["t.2d6".into()], &pick, &mut ScriptedFaces::new([]))
        .unwrap();
    assert_eq!(results(&log), ["low"]);
}
