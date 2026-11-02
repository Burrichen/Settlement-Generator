//! The generation framework: modifiers, scales, shared results, settings,
//! conditions, flows (branches, repeats, subflows, objects), unique results,
//! planned rules, validation and persistence. Content here is generic test
//! data, not settlement content.

use std::sync::Arc;

use serde_json::{Value, json};
use settlement_core::content::{ContentSet, Modifier};
use settlement_core::db::Database;
use settlement_core::dice::ScriptedFaces;
use settlement_core::engine::{GenerationEvent, GenerationOptions, Generator, TableRollRecord};
use settlement_core::framework::layers::ItemKind;
use settlement_core::framework::{Flow, SettingValue, SharedResult, StepOutcome, StepRecord};
use settlement_core::layers::{ContentLayers, UserOverrides};
use settlement_core::service::AppService;
use settlement_core::session::{GenerationMode, GenerationSession, ViewOptions};
use settlement_core::validation::{IssueKind, Severity, ValidationReport, validate};

// ----- helpers ---------------------------------------------------------------------

/// Generic content: an ordered 1d4 scale table, an unordered 1d6 table, a
/// count table and a few tags. `extra` is merged in at the top level.
fn content_json(extra: Value) -> Value {
    let mut base = json!({
        "tables": [
            { "id": "table.grade", "name": "Grade", "dice": "1d4", "ordered": true,
              "results": [
                { "id": "low", "name": "Low", "range": 1, "value": 1 },
                { "id": "mid", "name": "Mid", "range": 2, "value": 2 },
                { "id": "high", "name": "High", "range": 3, "value": 3 },
                { "id": "top", "name": "Top", "range": 4, "value": 4 }
              ] },
            { "id": "table.colour", "name": "Colour", "dice": "1d6",
              "results": [
                { "id": "red", "name": "Red", "range": [1, 2], "tags": ["tag.warm"] },
                { "id": "blue", "name": "Blue", "range": [3, 4] },
                { "id": "green", "name": "Green", "range": [5, 6] }
              ] },
            { "id": "table.count", "name": "Count", "dice": "1d4",
              "results": [
                { "id": "one", "name": "One", "range": 1, "value": 1 },
                { "id": "two", "name": "Two", "range": 2, "value": 2 },
                { "id": "three", "name": "Three", "range": [3, 4], "value": 3 }
              ] },
            { "id": "table.title", "name": "Title", "dice": "1d4",
              "results": [
                { "id": "chief", "name": "Chief", "range": [1, 2], "unique": true, "fallback": "aide" },
                { "id": "aide", "name": "Aide", "range": [3, 4] }
              ] },
            { "id": "table.pick", "name": "Pick", "selection": "choose",
              "results": [ { "id": "a", "name": "A" }, { "id": "b", "name": "B" } ] }
        ],
        "tags": [
            { "id": "tag.warm", "name": "Warm" },
            { "id": "tag.lucky", "name": "Lucky" }
        ]
    });
    if let (Value::Object(base), Value::Object(extra)) = (&mut base, extra) {
        for (k, v) in extra {
            match (base.get_mut(&k), v) {
                (Some(Value::Array(a)), Value::Array(b)) => a.extend(b),
                (_, v) => {
                    base.insert(k, v);
                }
            }
        }
    }
    base
}

fn content(extra: Value) -> ContentSet {
    serde_json::from_value(content_json(extra)).expect("content parses")
}

fn dice(values: &[u32]) -> ScriptedFaces {
    ScriptedFaces::new(values.iter().copied())
}

fn kinds(report: &ValidationReport) -> Vec<IssueKind> {
    report
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error)
        .map(|i| i.kind.clone())
        .collect()
}

fn assert_valid(c: &ContentSet) {
    let report = validate(c);
    assert!(!report.has_errors(), "{report}");
}

fn assert_error(c: &ContentSet, kind: IssueKind) {
    let errors = kinds(&validate(c));
    assert!(errors.contains(&kind), "expected {kind:?} in {errors:?}");
}

/// Rolls `table` once with a tag active that carries `effects`.
fn roll_with_effects(effects: Value, table: &str, faces: &[u32]) -> TableRollRecord {
    let c = content(json!({
        "tables": [{ "id": "table.start", "name": "Start", "dice": "1d4",
                     "results": [{ "id": "go", "name": "Go", "range": [1, 4], "tags": ["tag.mod"] }] }],
        "tags": [{ "id": "tag.mod", "name": "Mod", "effects": effects }]
    }));
    assert_valid(&c);
    let mut f = dice(
        &[1].into_iter()
            .chain(faces.iter().copied())
            .collect::<Vec<_>>(),
    );
    let log = Generator::new(&c)
        .unwrap()
        .run(&["table.start".into(), table.into()], &mut f)
        .unwrap();
    assert_eq!(f.remaining(), 0, "unused dice");
    log.record_for(&table.into()).unwrap().clone()
}

/// A session over content whose start flow has `steps`.
fn session(extra: Value, steps: Value) -> GenerationSession {
    let mut extra = extra;
    extra["start"] = json!("flow.main");
    let flows = extra
        .get("flows")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut all = vec![json!({ "id": "flow.main", "name": "Main", "steps": steps })];
    all.extend(flows);
    extra["flows"] = Value::Array(all);
    let c = content(extra);
    assert_valid(&c);
    GenerationSession::new(
        Arc::new(c),
        GenerationOptions::default(),
        GenerationMode::OneTable,
    )
    .expect("session starts")
}

fn flow_record(s: &GenerationSession, stage: usize) -> StepRecord {
    s.view(
        stage,
        ViewOptions {
            show_tags: true,
            debug: false,
        },
    )
    .unwrap()
    .flow
    .and_then(|f| f.record)
    .expect("a flow record")
}

fn rolls(record: &StepRecord) -> Vec<String> {
    record
        .table_rolls()
        .iter()
        .map(|r| r.result.name.clone())
        .collect()
}

// ----- modifiers -------------------------------------------------------------------

#[test]
fn shift_moves_along_the_ordered_scale() {
    for (steps, face, expected) in [
        (1, 1, "Mid"),
        (2, 1, "High"),
        (-1, 4, "High"),
        (-2, 4, "Mid"),
        (3, 1, "Top"),
        // Configurable X: past either end stops at the end.
        (7, 2, "Top"),
        (-9, 3, "Low"),
    ] {
        let r = roll_with_effects(
            json!([{ "kind": "shift", "table": "table.grade", "steps": steps }]),
            "table.grade",
            &[face],
        );
        assert_eq!(r.result.name, expected, "shift {steps} from face {face}");
        // The roll itself is unchanged; the move is recorded.
        assert_eq!(r.accepted().unwrap().roll.total, face as i32);
        let moved = r.adjusted_from.as_ref().map(|f| f.name.as_str());
        assert_eq!(
            moved.is_some(),
            expected != r.accepted().unwrap().result.name
        );
    }
}

#[test]
fn shifts_add_up_and_skip_results_that_cannot_be_had() {
    // Two Shift +1 effects make +2; Mid is disabled for this test, so the
    // shift passes over it.
    let c = content(json!({
        "tables": [{ "id": "table.start", "name": "Start", "dice": "1d4",
                     "results": [{ "id": "go", "name": "Go", "range": [1, 4], "tags": ["tag.a", "tag.b"] }] }],
        "tags": [
            { "id": "tag.a", "name": "A", "effects": [{ "kind": "shift", "table": "table.grade", "steps": 1 }] },
            { "id": "tag.b", "name": "B", "effects": [
                { "kind": "shift", "table": "table.grade", "steps": 1 },
                { "kind": "restrict_results", "table": "table.grade", "exclude": ["mid"] } ] }
        ]
    }));
    assert_valid(&c);
    let log = Generator::new(&c)
        .unwrap()
        .run(
            &["table.start".into(), "table.grade".into()],
            &mut dice(&[1, 1]),
        )
        .unwrap();
    // Low +2, passing over the excluded Mid: High, then Top.
    assert_eq!(
        log.record_for(&"table.grade".into()).unwrap().result.name,
        "Top"
    );
}

#[test]
fn plus_and_minus_change_the_rolled_total() {
    let r = roll_with_effects(
        json!([{ "kind": "plus", "table": "table.colour", "amount": 2 }]),
        "table.colour",
        &[2],
    );
    assert_eq!(r.dice.as_deref(), Some("1d6+2"));
    assert_eq!(r.accepted().unwrap().roll.total, 4);
    assert_eq!(r.result.name, "Blue");
    let r = roll_with_effects(
        json!([{ "kind": "minus", "table": "table.colour", "amount": 3 }]),
        "table.colour",
        &[2],
    );
    // 2 - 3 = -1 is read as the lowest face.
    assert_eq!(r.accepted().unwrap().face, 1);
    assert_eq!(r.result.name, "Red");
}

#[test]
fn caps_give_the_ends_of_the_scale() {
    let r = roll_with_effects(
        json!([{ "kind": "cap_at_highest", "table": "table.grade" }]),
        "table.grade",
        &[1],
    );
    assert_eq!(r.result.name, "Top");
    assert_eq!(r.adjusted_from.unwrap().name, "Low");
    let r = roll_with_effects(
        json!([{ "kind": "cap_at_lowest", "table": "table.grade" }]),
        "table.grade",
        &[4],
    );
    assert_eq!(r.result.name, "Low");
}

#[test]
fn a_chosen_result_is_not_moved_by_shift() {
    let c = content(json!({
        "tables": [{ "id": "table.start", "name": "Start", "dice": "1d4",
                     "results": [{ "id": "go", "name": "Go", "range": [1, 4], "tags": ["tag.mod"] }] }],
        "tags": [{ "id": "tag.mod", "name": "Mod", "effects": [{ "kind": "shift", "table": "table.grade", "steps": 2 }] }]
    }));
    let mut selections = settlement_core::engine::Selections::new();
    selections.insert("table.grade".into(), "low".into());
    let log = Generator::new(&c)
        .unwrap()
        .run_with(
            &["table.start".into(), "table.grade".into()],
            &selections,
            &mut dice(&[1]),
        )
        .unwrap();
    assert_eq!(
        log.record_for(&"table.grade".into()).unwrap().result.name,
        "Low"
    );
}

#[test]
fn a_named_scale_serves_a_table_through_shared_results() {
    let c = content(json!({
        "shared_results": [
            { "id": "size.small", "name": "Small" },
            { "id": "size.large", "name": "Large" }
        ],
        "scales": [{ "id": "scale.size", "name": "Size", "steps": ["size.small", "size.large"] }],
        "tables": [
            { "id": "table.start", "name": "Start", "dice": "1d4",
              "results": [{ "id": "go", "name": "Go", "range": [1, 4], "tags": ["tag.mod"] }] },
            { "id": "table.box", "name": "Box", "dice": "1d4", "scale": "scale.size",
              "results": [
                { "id": "s", "shared": "size.small", "range": [1, 2] },
                { "id": "l", "shared": "size.large", "range": [3, 4] }
              ] }
        ],
        "tags": [{ "id": "tag.mod", "name": "Mod", "effects": [{ "kind": "shift", "table": "table.box", "steps": 1 }] }]
    }));
    let mut c = c;
    settlement_core::framework::link_shared_results(&mut c);
    assert_valid(&c);
    let log = Generator::new(&c)
        .unwrap()
        .run(
            &["table.start".into(), "table.box".into()],
            &mut dice(&[1, 1]),
        )
        .unwrap();
    let r = log.record_for(&"table.box".into()).unwrap();
    assert_eq!(r.result.name, "Large");
    assert_eq!(r.adjusted_from.as_ref().unwrap().name, "Small");
}

#[test]
fn modifiers_are_validated() {
    let tag = |effect: Value| {
        content(json!({ "tags": [{ "id": "tag.mod", "name": "Mod", "effects": [effect] }] }))
    };
    assert_error(
        &tag(json!({ "kind": "shift", "table": "table.grade", "steps": 0 })),
        IssueKind::InvalidModifier {
            reason: "a shift must move at least one step".into(),
        },
    );
    assert_error(
        &tag(json!({ "kind": "plus", "table": "table.grade", "amount": 0 })),
        IssueKind::InvalidModifier {
            reason: "the amount must be a positive whole number".into(),
        },
    );
    assert_error(
        &tag(json!({ "kind": "shift", "table": "table.colour", "steps": 1 })),
        IssueKind::ModifierNeedsScale,
    );
    assert_error(
        &tag(json!({ "kind": "cap_at_highest", "table": "table.colour" })),
        IssueKind::ModifierNeedsScale,
    );
    assert_error(
        &tag(json!({ "kind": "minus", "table": "table.pick", "amount": 1 })),
        IssueKind::ModifierNeedsDice,
    );
    assert_error(
        &tag(json!({ "kind": "shift", "table": "table.nothing", "steps": 1 })),
        IssueKind::MissingTable {
            table: "table.nothing".into(),
        },
    );
    // A planned modifier is a design note: not checked.
    assert_valid(&content(
        json!({ "tags": [{ "id": "tag.mod", "name": "Mod", "effects": [
        { "kind": "shift", "table": "table.notYet", "steps": 2, "status": "planned" }
    ] }] }),
    ));
}

#[test]
fn modifiers_display_by_name() {
    assert_eq!(Modifier::Shift(1).to_string(), "Shift +1");
    assert_eq!(Modifier::Shift(-2).to_string(), "Shift -2");
    assert_eq!(Modifier::Plus(3).to_string(), "Plus 3");
    assert_eq!(Modifier::Minus(1).to_string(), "Minus 1");
    assert_eq!(Modifier::CapAtHighest.to_string(), "Cap at Highest");
    assert_eq!(Modifier::CapAtLowest.to_string(), "Cap at Lowest");
    let m: Modifier = serde_json::from_value(json!({ "shift": -2 })).unwrap();
    assert_eq!(m, Modifier::Shift(-2));
    let m: Modifier = serde_json::from_value(json!("cap_at_lowest")).unwrap();
    assert_eq!(m, Modifier::CapAtLowest);
}

// ----- conditions and settings --------------------------------------------------------

#[test]
fn settings_drive_branches_and_table_eligibility() {
    let extra = json!({
        "settings": [
            { "id": "setting.extras", "name": "Extras", "kind": "toggle" },
            { "id": "setting.mode", "name": "Mode", "kind": "choice", "default": "calm",
              "options": [{ "id": "calm", "name": "Calm" }, { "id": "wild", "name": "Wild" }] }
        ]
    });
    let steps = json!([
        { "id": "pick", "kind": "branch", "cases": [
            { "when": { "all": [ { "setting_on": "setting.extras" },
                                 { "setting_is": { "setting": "setting.mode", "value": "wild" } } ] },
              "steps": [{ "id": "c1", "kind": "roll", "table": "table.colour" }] },
            { "when": { "any": [ { "setting_on": "setting.extras" }, { "setting_on": "app.godMode" } ] },
              "steps": [{ "id": "g1", "kind": "roll", "table": "table.grade" }] }
          ],
          "otherwise": [{ "id": "n1", "kind": "roll", "table": "table.count" }] }
    ]);
    let run = |values: &[(&str, SettingValue)]| {
        let s = session(extra.clone(), steps.clone());
        let mut s = s.with_settings(
            values
                .iter()
                .map(|(k, v)| ((*k).into(), v.clone()))
                .collect(),
        );
        s.roll(0, &mut dice(&[1])).unwrap();
        let record = flow_record(&s, 0);
        let StepOutcome::Branch { case, .. } = &record.outcome else {
            panic!("a branch")
        };
        (*case, rolls(&record))
    };
    assert_eq!(run(&[]), (None, vec!["One".into()]));
    assert_eq!(
        run(&[("setting.extras", SettingValue::Toggle(true))]),
        (Some(1), vec!["Low".into()])
    );
    assert_eq!(
        run(&[
            ("setting.extras", SettingValue::Toggle(true)),
            ("setting.mode", SettingValue::Choice("wild".into()))
        ]),
        (Some(0), vec!["Red".into()])
    );
}

#[test]
fn settings_and_conditions_are_validated() {
    assert_error(
        &content(json!({ "settings": [{ "id": "app.mine", "name": "Mine", "kind": "toggle" }] })),
        IssueKind::ReservedSettingId,
    );
    assert_error(
        &content(
            json!({ "settings": [{ "id": "setting.m", "name": "M", "kind": "choice",
            "default": "nope", "options": [{ "id": "a", "name": "A" }] }] }),
        ),
        IssueKind::InvalidSettingDefault,
    );
    // A table condition reading a missing setting.
    let mut c = content(json!({}));
    c.tables[0].eligibility = serde_json::from_value(json!({ "setting_on": "setting.none" })).ok();
    assert_error(
        &c,
        IssueKind::MissingSetting {
            setting: "setting.none".into(),
        },
    );
    // Result tests only work in flows.
    let mut c = content(json!({}));
    c.tables[0].eligibility =
        serde_json::from_value(json!({ "result_is": { "step": "x", "results": ["a"] } })).ok();
    assert_error(&c, IssueKind::ConditionNeedsFlow);
}

#[test]
fn routing_by_an_earlier_result_and_value() {
    let steps = json!([
        { "id": "grade", "kind": "roll", "table": "table.grade" },
        { "id": "after", "kind": "branch", "cases": [
            { "when": { "result_is": { "step": "grade", "results": ["top"] } },
              "steps": [{ "id": "a", "kind": "add_tag", "tag": "tag.lucky" }] },
            { "when": { "value_in": { "step": "grade", "min": 2, "max": 3 } },
              "steps": [{ "id": "b", "kind": "roll", "table": "table.colour" }] }
        ] }
    ]);
    for (face, case, tagged) in [(4, Some(0), true), (3, Some(1), false), (1, None, false)] {
        let mut s = session(json!({}), steps.clone());
        s.roll(0, &mut dice(&[face])).unwrap();
        let faces: &[u32] = if case == Some(1) { &[5] } else { &[] };
        s.roll(1, &mut dice(faces)).unwrap();
        let StepOutcome::Branch { case: taken, .. } = flow_record(&s, 1).outcome else {
            panic!()
        };
        assert_eq!(taken, case, "face {face}");
        assert_eq!(
            s.active_tags().iter().any(|t| t.id.as_str() == "tag.lucky"),
            tagged
        );
    }
}

// ----- flows -------------------------------------------------------------------------

#[test]
fn repeat_runs_fixed_counted_and_rolled_numbers_of_times() {
    let steps = json!([
        { "id": "count", "kind": "roll", "table": "table.count" },
        { "id": "fixed", "kind": "repeat", "count": { "times": 2 },
          "steps": [{ "id": "f", "kind": "roll", "table": "table.colour" }] },
        { "id": "counted", "kind": "repeat", "count": { "value_of": { "step": "count", "add": -1 } },
          "steps": [{ "id": "c", "kind": "roll", "table": "table.colour" }] },
        { "id": "rolled", "kind": "repeat", "count": { "dice": "1d4" },
          "steps": [{ "id": "r", "kind": "roll", "table": "table.colour" }] }
    ]);
    let mut s = session(json!({}), steps);
    s.roll(0, &mut dice(&[3])).unwrap(); // Three
    s.roll(1, &mut dice(&[1, 3])).unwrap();
    s.roll(2, &mut dice(&[5, 6])).unwrap(); // 3 - 1 = 2 times
    s.roll(3, &mut dice(&[1, 2])).unwrap(); // rolled 1: once
    assert_eq!(rolls(&flow_record(&s, 1)), ["Red", "Blue"]);
    assert_eq!(rolls(&flow_record(&s, 2)), ["Green", "Green"]);
    let StepOutcome::Repeat {
        count, count_roll, ..
    } = flow_record(&s, 3).outcome
    else {
        panic!()
    };
    assert_eq!(count, 1);
    assert_eq!(count_roll.unwrap().total, 1);
}

#[test]
fn objects_get_ids_fields_and_their_own_tags() {
    let extra = json!({
        "objects": [{ "id": "object.thing", "name": "Thing", "label": "Thing {n}", "id_prefix": "thing",
                      "fields": [{ "id": "colour", "name": "Colour" }, { "id": "weight", "name": "Weight" }] }]
    });
    let steps = json!([
        { "id": "things", "kind": "repeat", "count": { "times": 2 }, "steps": [
            { "id": "make", "kind": "create_object", "object": "object.thing", "steps": [
                { "id": "colour", "kind": "roll", "table": "table.colour", "field": "colour" },
                { "id": "weight", "kind": "set_value", "field": "weight", "value": 7 }
            ] }
        ] }
    ]);
    let mut s = session(extra, steps);
    s.roll(0, &mut dice(&[1, 3])).unwrap();
    let record = flow_record(&s, 0);
    let objects = record.objects();
    assert_eq!(objects.len(), 2);
    assert_eq!(objects[0].id, "thing-01");
    assert_eq!(objects[1].id, "thing-02");
    assert_eq!(objects[1].label, "Thing 2");
    assert_eq!(objects[0].fields[0].result.as_ref().unwrap().name, "Red");
    assert_eq!(objects[0].fields[1].value, Some(7));
    // Red's Warm tag is the first object's own, not the generation's.
    assert_eq!(objects[0].tags.len(), 1);
    assert!(objects[1].tags.is_empty());
    assert!(s.active_tags().is_empty());
    assert_eq!(record.summary(), "2 × Thing");
}

#[test]
fn shared_object_tags_become_generation_tags() {
    let extra = json!({ "objects": [{ "id": "object.thing", "name": "Thing" }] });
    let steps = json!([
        { "id": "make", "kind": "create_object", "object": "object.thing", "share_tags": true,
          "steps": [{ "id": "colour", "kind": "roll", "table": "table.colour" }] }
    ]);
    let mut s = session(extra, steps);
    s.roll(0, &mut dice(&[2])).unwrap();
    assert_eq!(s.active_tags()[0].id.as_str(), "tag.warm");
}

#[test]
fn unique_results_fall_back_instead_of_rerolling() {
    let steps = json!([
        { "id": "titles", "kind": "repeat", "count": { "times": 3 },
          "steps": [{ "id": "t", "kind": "roll", "table": "table.title" }] }
    ]);
    let mut s = session(json!({}), steps);
    // Chief, then Chief again (held: Aide), then Aide.
    s.roll(0, &mut dice(&[1, 2, 4])).unwrap();
    let record = flow_record(&s, 0);
    let given: Vec<String> = record.children().iter().map(|r| r.summary()).collect();
    assert_eq!(given, ["Chief", "Aide", "Aide"]);
    let StepOutcome::Roll { fallback, .. } = &record.children()[1].outcome else {
        panic!()
    };
    assert_eq!(fallback.as_ref().unwrap().name, "Aide");
}

#[test]
fn subflows_run_inline_and_are_reusable() {
    let extra = json!({
        "flows": [{ "id": "flow.pair", "name": "Pair", "steps": [
            { "id": "p1", "kind": "roll", "table": "table.colour" },
            { "id": "p2", "kind": "roll", "table": "table.grade" }
        ] }]
    });
    let steps = json!([
        { "id": "first", "kind": "run_flow", "flow": "flow.pair" },
        { "id": "again", "kind": "run_flow", "flow": "flow.pair" }
    ]);
    let mut s = session(extra, steps);
    s.roll(0, &mut dice(&[1, 4])).unwrap();
    s.roll(1, &mut dice(&[6, 1])).unwrap();
    assert_eq!(rolls(&flow_record(&s, 0)), ["Red", "Top"]);
    assert_eq!(rolls(&flow_record(&s, 1)), ["Green", "Low"]);
}

#[test]
fn step_modifiers_apply_only_while_their_condition_holds() {
    let steps = json!([
        { "id": "colour", "kind": "roll", "table": "table.colour" },
        { "id": "grade", "kind": "roll", "table": "table.grade", "modifiers": [
            { "modifier": { "shift": 2 }, "when": { "has_tag": "tag.warm" } },
            { "modifier": { "plus": 1 } }
        ] }
    ]);
    let mut s = session(json!({}), steps.clone());
    s.roll(0, &mut dice(&[1])).unwrap(); // Red: Warm
    s.roll(1, &mut dice(&[1])).unwrap(); // 1 + 1 = Mid, shifted +2 = Top
    assert_eq!(
        s.view(1, ViewOptions::default())
            .unwrap()
            .result
            .unwrap()
            .name,
        "Top"
    );
    // Rerolling the colour away from Warm invalidates the grade: its rules
    // changed.
    let change = s.roll(0, &mut dice(&[3])).unwrap();
    assert_eq!(change.invalidated, [1]);
    s.roll(1, &mut dice(&[1])).unwrap();
    assert_eq!(
        s.view(1, ViewOptions::default())
            .unwrap()
            .result
            .unwrap()
            .name,
        "Mid"
    );
}

#[test]
fn a_step_condition_skips_and_reopens_a_stage() {
    let steps = json!([
        { "id": "colour", "kind": "roll", "table": "table.colour" },
        { "id": "grade", "kind": "roll", "table": "table.grade", "when": { "has_tag": "tag.warm" } },
        { "id": "count", "kind": "repeat", "count": { "times": 1 }, "when": { "not": { "has_tag": "tag.warm" } },
          "steps": [{ "id": "c", "kind": "roll", "table": "table.count" }] }
    ]);
    let mut s = session(json!({}), steps);
    s.roll(0, &mut dice(&[3])).unwrap(); // Blue: not warm
    let views = s.views(ViewOptions::default()).unwrap();
    assert_eq!(
        views[1].skipped,
        Some(settlement_core::engine::SkipReason::NotEligible)
    );
    s.roll(2, &mut dice(&[1])).unwrap();
    // Red makes Warm: grade reopens, the repeat no longer applies.
    let change = s.roll(0, &mut dice(&[1])).unwrap();
    assert_eq!(change.reopened, [1]);
    assert!(change.invalidated.contains(&2));
    // Once the grade is settled, the repeat is skipped.
    s.roll(1, &mut dice(&[2])).unwrap();
    let views = s.views(ViewOptions::default()).unwrap();
    assert!(views[2].skipped.is_some());
}

#[test]
fn planned_and_inactive_steps_never_run() {
    let steps = json!([
        { "id": "later", "kind": "roll", "table": "table.notDesignedYet", "status": "planned",
          "note": "Waiting for the table" },
        { "id": "off", "kind": "roll", "table": "table.grade", "active": false },
        { "id": "on", "kind": "roll", "table": "table.colour" }
    ]);
    let s = session(json!({}), steps);
    // Only the active step is a stage; the planned one may name a table
    // that does not exist yet.
    assert_eq!(s.stage_count(), 1);
}

#[test]
fn automatic_generation_runs_flow_stages_and_saves_them() {
    let extra = json!({ "objects": [{ "id": "object.thing", "name": "Thing" }] });
    let steps = json!([
        { "id": "count", "kind": "roll", "table": "table.count" },
        { "id": "things", "kind": "repeat", "count": { "value_of": { "step": "count" } }, "steps": [
            { "id": "make", "kind": "create_object", "object": "object.thing",
              "steps": [{ "id": "colour", "kind": "roll", "table": "table.colour" }] }
        ] }
    ]);
    let mut s = session(extra, steps);
    s.run_automatic(&Default::default(), &mut dice(&[2, 1, 5]))
        .unwrap();
    assert!(s.is_complete());
    let snapshot = s.to_snapshot();
    let stage = &snapshot.stages[1];
    assert_eq!(stage.stage_table.as_str(), "step.things");
    assert_eq!(stage.result.as_ref().unwrap().name, "2 × Thing");
    assert_eq!(rolls(stage.flow.as_ref().unwrap()), ["Red", "Green"]);
    // Reopening restores the record exactly.
    let reopened = GenerationSession::from_snapshot(
        Arc::new(content(json!({
            "objects": [{ "id": "object.thing", "name": "Thing" }],
            "start": "flow.main",
            "flows": [{ "id": "flow.main", "name": "Main", "steps": [
                { "id": "count", "kind": "roll", "table": "table.count" },
                { "id": "things", "kind": "repeat", "count": { "value_of": { "step": "count" } }, "steps": [
                    { "id": "make", "kind": "create_object", "object": "object.thing",
                      "steps": [{ "id": "colour", "kind": "roll", "table": "table.colour" }] }
                ] }
            ] }]
        }))),
        GenerationOptions::default(),
        GenerationMode::OneTable,
        &snapshot,
    )
    .unwrap();
    assert_eq!(reopened.to_snapshot(), snapshot);
    // Changing the count invalidates the repeat that read it.
    let mut reopened = reopened;
    let change = reopened.roll(0, &mut dice(&[1])).unwrap();
    assert_eq!(change.invalidated, [1]);
}

// ----- validation of flows -----------------------------------------------------------

fn flows(flows: Value) -> ContentSet {
    content(
        json!({ "flows": flows, "objects": [{ "id": "object.thing", "name": "Thing",
        "fields": [{ "id": "colour", "name": "Colour" }] }] }),
    )
}

#[test]
fn flow_references_are_validated() {
    let one = |steps: Value| flows(json!([{ "id": "flow.x", "name": "X", "steps": steps }]));
    assert_error(
        &one(json!([{ "id": "a", "kind": "roll", "table": "table.none" }])),
        IssueKind::MissingTable {
            table: "table.none".into(),
        },
    );
    assert_error(
        &one(json!([{ "id": "a", "kind": "add_tag", "tag": "tag.none" }])),
        IssueKind::MissingTag {
            tag: "tag.none".into(),
        },
    );
    assert_error(
        &one(json!([{ "id": "a", "kind": "run_flow", "flow": "flow.none" }])),
        IssueKind::MissingFlow {
            flow: "flow.none".into(),
        },
    );
    assert_error(
        &one(json!([{ "id": "a", "kind": "create_object", "object": "object.none" }])),
        IssueKind::MissingObjectType {
            object: "object.none".into(),
        },
    );
    assert_error(
        &one(
            json!([{ "id": "a", "kind": "set_result", "table": "table.grade", "result": "nope" }]),
        ),
        IssueKind::BrokenResultReference {
            table: "table.grade".into(),
            result: "nope".into(),
        },
    );
    assert_error(
        &one(
            json!([{ "id": "a", "kind": "repeat", "count": { "value_of": { "step": "ghost" } }, "steps": [] }]),
        ),
        IssueKind::MissingStep {
            step: "ghost".into(),
        },
    );
    assert_error(
        &one(json!([{ "id": "a", "kind": "branch", "cases": [
            { "when": { "result_is": { "step": "ghost", "results": ["x"] } }, "steps": [] } ] }])),
        IssueKind::MissingStep {
            step: "ghost".into(),
        },
    );
    assert_error(
        &one(json!([{ "id": "a", "kind": "roll", "table": "table.grade", "field": "colour" }])),
        IssueKind::FieldOutsideObject,
    );
    assert_error(
        &one(
            json!([{ "id": "a", "kind": "create_object", "object": "object.thing", "steps": [
            { "id": "b", "kind": "roll", "table": "table.grade", "field": "size" } ] }]),
        ),
        IssueKind::MissingField {
            field: "size".into(),
        },
    );
    assert_error(
        &one(json!([{ "id": "a", "kind": "repeat", "count": { "times": 1000 }, "steps": [] }])),
        IssueKind::InvalidCount {
            reason: "a repeat can run at most 100 times".into(),
        },
    );
    assert_error(
        &one(json!([{ "id": "a", "kind": "roll", "table": "table.colour",
            "modifiers": [{ "modifier": { "shift": 1 } }] }])),
        IssueKind::ModifierNeedsScale,
    );
    // Missing IDs: duplicates and malformed IDs.
    assert_error(
        &one(
            json!([{ "id": "a", "kind": "roll", "table": "table.grade" },
                    { "id": "a", "kind": "roll", "table": "table.colour" }]),
        ),
        IssueKind::DuplicateId {
            scope: settlement_core::validation::IdScope::Step,
            id: "a".into(),
        },
    );
    assert_error(
        &one(json!([{ "id": "not valid", "kind": "roll", "table": "table.grade" }])),
        IssueKind::InvalidId {
            id: "not valid".into(),
        },
    );
    // A start flow that does not exist.
    let mut c = flows(json!([]));
    c.start = Some("flow.missing".into());
    assert_error(
        &c,
        IssueKind::MissingFlow {
            flow: "flow.missing".into(),
        },
    );
}

#[test]
fn flows_that_run_each_other_in_a_loop_are_rejected() {
    let c = flows(json!([
        { "id": "flow.a", "name": "A", "steps": [{ "id": "x", "kind": "run_flow", "flow": "flow.b" }] },
        { "id": "flow.b", "name": "B", "steps": [{ "id": "y", "kind": "run_flow", "flow": "flow.a" }] },
        { "id": "flow.self", "name": "Self", "steps": [{ "id": "z", "kind": "run_flow", "flow": "flow.self" }] }
    ]));
    let errors = kinds(&validate(&c));
    assert!(errors.contains(&IssueKind::FlowCycle {
        flows: vec!["flow.a".into(), "flow.b".into()]
    }));
    assert!(errors.contains(&IssueKind::FlowCycle {
        flows: vec!["flow.self".into()]
    }));
    // A planned loop is a design note and is allowed.
    let c = flows(json!([
        { "id": "flow.self", "name": "Self", "steps": [
            { "id": "z", "kind": "run_flow", "flow": "flow.self", "status": "planned" } ] }
    ]));
    assert_valid(&c);
}

#[test]
fn builtin_steps_are_validated() {
    let mut c = flows(json!([{ "id": "flow.main", "name": "Main", "steps": [
        { "id": "a", "kind": "builtin", "handler": "teleport" },
        { "id": "b", "kind": "builtin", "handler": "notable_locations", "config": { "size_table": 3 } },
        { "id": "c", "kind": "repeat", "count": { "times": 1 }, "steps": [
            { "id": "d", "kind": "builtin", "handler": "authority_roles", "config": { "authority": {
                "table": "table.grade", "size_table": "table.count",
                "single_tag": "tag.warm", "group_tag": "tag.lucky" } } } ] },
        { "id": "e", "kind": "builtin", "handler": "notable_locations", "config": {
            "size_table": "table.grade", "type_table": "table.missing" } }
    ] }]));
    c.start = Some("flow.main".into());
    let errors = kinds(&validate(&c));
    assert!(errors.contains(&IssueKind::UnknownBuiltin {
        handler: "teleport".into()
    }));
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, IssueKind::InvalidBuiltinConfig { .. }))
    );
    assert!(errors.contains(&IssueKind::BuiltinNotTopLevel));
    assert!(errors.contains(&IssueKind::MissingTable {
        table: "table.missing".into()
    }));
}

// ----- shared results and layering ---------------------------------------------------

#[test]
fn shared_results_are_reused_and_a_table_may_keep_its_own_field() {
    let mut c = content(json!({
        "shared_results": [{ "id": "env.wood", "name": "Woodland", "tags": ["tag.warm"] }],
        "tables": [
            { "id": "table.one", "name": "One", "dice": "1d4", "results": [
                { "id": "wood", "shared": "env.wood", "range": [1, 2] },
                { "id": "x", "name": "X", "range": [3, 4] } ] },
            { "id": "table.two", "name": "Two", "dice": "1d4", "results": [
                { "id": "wood", "shared": "env.wood", "name": "Old Woods", "range": [1, 4] } ] }
        ]
    }));
    settlement_core::framework::link_shared_results(&mut c);
    assert_valid(&c);
    let one = &c.table(&"table.one".into()).unwrap().results[0];
    let two = &c.table(&"table.two".into()).unwrap().results[0];
    assert_eq!(one.name, "Woodland");
    assert_eq!(two.name, "Old Woods");
    assert_eq!(one.tags, two.tags);
    // A missing shared result is reported.
    let mut c = content(
        json!({ "tables": [{ "id": "table.one", "name": "One", "dice": "1d4",
        "results": [{ "id": "w", "shared": "env.none", "range": [1, 4] }] }] }),
    );
    settlement_core::framework::link_shared_results(&mut c);
    assert_error(
        &c,
        IssueKind::MissingSharedResult {
            shared: "env.none".into(),
        },
    );
}

fn layered() -> ContentLayers {
    let mut bundled = content(json!({
        "shared_results": [{ "id": "env.wood", "name": "Woodland" }],
        "tables": [
            { "id": "table.one", "name": "One", "dice": "1d4", "results": [
                { "id": "wood", "shared": "env.wood", "range": [1, 2] },
                { "id": "x", "name": "X", "range": [3, 4] } ] },
            { "id": "table.two", "name": "Two", "dice": "1d4", "results": [
                { "id": "wood", "shared": "env.wood", "name": "Old Woods", "range": [1, 4] } ] }
        ],
        "start": "flow.main",
        "flows": [{ "id": "flow.main", "name": "Main", "steps": [
            { "id": "one", "kind": "roll", "table": "table.one" } ] }]
    }));
    settlement_core::framework::link_shared_results(&mut bundled);
    ContentLayers::new(bundled, UserOverrides::default())
}

#[test]
fn customising_a_shared_result_reaches_every_table_that_uses_it() {
    let mut l = layered();
    l.put_item(SharedResult {
        name: "Forest".into(),
        ..l.bundled().shared_results[0].clone()
    })
    .unwrap();
    let merged = l.merged();
    assert!(!merged.report.has_errors(), "{}", merged.report);
    let c = merged.content;
    assert_eq!(
        c.table(&"table.one".into()).unwrap().results[0].name,
        "Forest"
    );
    // The table that named it itself keeps its own name.
    assert_eq!(
        c.table(&"table.two".into()).unwrap().results[0].name,
        "Old Woods"
    );
    // The bundled content is untouched; reset restores the default.
    assert_eq!(l.bundled().shared_results[0].name, "Woodland");
    l.reset_item::<SharedResult>(&"env.wood".into()).unwrap();
    assert!(l.overrides().is_empty());
}

#[test]
fn flows_can_be_customised_deleted_and_restored() {
    let mut l = layered();
    let mut flow: Flow = l.bundled().flows[0].clone();
    flow.steps.push(
        serde_json::from_value(json!({ "id": "two", "kind": "roll", "table": "table.two" }))
            .unwrap(),
    );
    l.put_item(flow).unwrap();
    assert_eq!(l.merged().content.pipeline().len(), 2);
    // A user flow needs a user ID.
    let mut other: Flow = l.bundled().flows[0].clone();
    other.id = "flow.other".into();
    assert!(l.put_item(other.clone()).is_err());
    other.id = "user.flow.other".into();
    l.put_item(other).unwrap();
    // Deleting the start flow is refused by a checked edit: it is in use.
    let refused = l.apply_checked(|l| l.delete_item::<Flow>(&"flow.main".into()));
    assert!(refused.is_err());
    l.reset_item::<Flow>(&"flow.main".into()).unwrap();
    assert_eq!(l.merged().content.pipeline().len(), 1);
}

#[test]
fn user_overrides_of_framework_items_round_trip_through_the_database() {
    let mut l = layered();
    l.put_item(SharedResult {
        name: "Forest".into(),
        ..l.bundled().shared_results[0].clone()
    })
    .unwrap();
    let mut db = Database::open_in_memory().unwrap();
    db.save_overrides(l.overrides()).unwrap();
    let loaded = db.load_overrides().unwrap();
    assert_eq!(&loaded, l.overrides());
    let json = serde_json::to_value(&loaded).unwrap();
    assert_eq!(json["shared_results"]["env.wood"]["action"], "replace");
}

#[test]
fn content_setting_values_persist_and_reach_generation() {
    let mut service = AppService::new(Database::open_in_memory().unwrap());
    // Bundled content declares Bare Minimum Buildings as an ordinary
    // setting; an unknown setting is refused.
    let ids: Vec<String> = service
        .content_settings()
        .unwrap()
        .iter()
        .map(|v| v.setting.id.to_string())
        .collect();
    assert_eq!(ids, ["setting.bareMinimumBuildings"]);
    assert!(
        service
            .set_content_setting(&"setting.none".into(), SettingValue::Toggle(true))
            .is_err()
    );
    let mut db = Database::open_in_memory().unwrap();
    let values = [("setting.x".into(), SettingValue::Toggle(true))]
        .into_iter()
        .collect();
    db.save_content_settings(&values).unwrap();
    assert_eq!(db.load_content_settings().unwrap(), values);
}

#[test]
fn framework_items_can_be_listed_and_edited_through_the_service() {
    let mut service = AppService::new(Database::open_in_memory().unwrap());
    let items = service.framework_items().unwrap();
    assert!(
        items
            .iter()
            .any(|i| i.kind == ItemKind::Flow && i.id == "flow.settlement")
    );
    assert!(items.iter().any(|i| i.kind == ItemKind::SharedResult));
    // A user shared result, then a refused one that breaks the content.
    service
        .save_framework_item(
            ItemKind::SharedResult,
            json!({ "id": "user.result.a", "name": "A" }),
        )
        .unwrap();
    let mut flow = service
        .framework_item(ItemKind::Flow, "flow.settlement")
        .unwrap();
    flow["steps"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "id": "broken", "kind": "roll", "table": "table.none" }));
    let refused = service.save_framework_item(ItemKind::Flow, flow);
    assert!(refused.unwrap_err().contains("table.none"));
    service
        .delete_framework_item(ItemKind::SharedResult, "user.result.a")
        .unwrap();
}

#[test]
fn saved_flow_stages_never_change_with_content() {
    let steps = json!([
        { "id": "colours", "kind": "repeat", "count": { "times": 2 },
          "steps": [{ "id": "c", "kind": "roll", "table": "table.colour" }] }
    ]);
    let mut s = session(json!({}), steps);
    s.roll(0, &mut dice(&[1, 6])).unwrap();
    let snapshot = s.to_snapshot();
    let mut db = Database::open_in_memory().unwrap();
    let saved = db.create_settlement("Test", snapshot.clone()).unwrap();
    // Later content renames the results; the saved record keeps its labels.
    let stored = db.get_settlement(&saved.summary.id).unwrap();
    assert_eq!(stored.snapshot, snapshot);
    assert_eq!(
        rolls(stored.snapshot.stages[0].flow.as_ref().unwrap()),
        ["Red", "Green"]
    );
}

#[test]
fn a_flow_run_has_recursion_and_size_limits() {
    // Validation stops loops; the runner's own limits stop runaway repeats.
    let steps = json!([
        { "id": "outer", "kind": "repeat", "count": { "times": 100 }, "steps": [
            { "id": "inner", "kind": "repeat", "count": { "times": 100 }, "steps": [
                { "id": "more", "kind": "repeat", "count": { "times": 2 }, "steps": [
                    { "id": "t", "kind": "add_tag", "tag": "tag.lucky" } ] } ] } ] }
    ]);
    let mut s = session(json!({}), steps);
    let err = s.roll(0, &mut dice(&[])).unwrap_err();
    assert!(err.to_string().contains("more than 10000 steps"), "{err}");
}

#[test]
fn bundled_pipeline_is_a_flow_with_configured_builtins() {
    let c = settlement_core::bundled::bundled_content().unwrap();
    assert_eq!(
        c.start.as_ref().map(|s| s.as_str()),
        Some("flow.settlement")
    );
    assert!(c.sequence.is_empty());
    let handlers: Vec<String> = c
        .pipeline()
        .into_iter()
        .filter_map(|s| match s.action {
            settlement_core::framework::StepAction::Builtin { handler, .. } => Some(handler),
            _ => None,
        })
        .collect();
    assert_eq!(
        handlers,
        ["authority_roles", "authority_figures", "notable_locations"]
    );
    // Shared results are linked: the Kurovian environments share one
    // definition.
    let ordon = c.table(&"table.kuroviaEnvironment.ordon".into()).unwrap();
    let woodland = ordon.result(&"woodland".into()).unwrap();
    assert_eq!(
        woodland.shared.as_ref().map(|s| s.as_str()),
        Some("environment.woodland")
    );
    assert_eq!(woodland.name, "Woodland");
    // The events of a generation never include a flow stage's rolls.
    let _ = GenerationEvent::TableSkipped {
        table: settlement_core::engine::Named {
            id: "t".into(),
            name: String::new(),
        },
        reason: settlement_core::engine::SkipReason::Inactive,
    };
}
