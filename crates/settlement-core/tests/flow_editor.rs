//! The flow editor's core: the newer step kinds (For Each, Remove Tag,
//! Apply Modifier, Read Setting, Stop, guarded recursion), the generic
//! patterns a flow must be able to express, flow validation, the edit
//! operations both editors use, and the plain-language pseudocode. Content
//! here is generic test data, not settlement content.

use std::sync::Arc;

use serde_json::{Value, json};
use settlement_core::content::ContentSet;
use settlement_core::db::Database;
use settlement_core::dice::ScriptedFaces;
use settlement_core::engine::GenerationOptions;
use settlement_core::framework::describe::Describer;
use settlement_core::framework::edit::{FlowEdit, Position, Slot, StepKind, apply};
use settlement_core::framework::{
    Flow, SettingValue, SettingValues, StepAction, StepOutcome, StepRecord, StepSkip,
};
use settlement_core::ids::{FlowId, SettingId, StepId};
use settlement_core::service::AppService;
use settlement_core::session::{GenerationMode, GenerationSession, ViewOptions};
use settlement_core::validation::{IssueKind, Severity, ValidationReport, validate};

// ----- helpers ---------------------------------------------------------------------

fn content(extra: Value) -> ContentSet {
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
                { "id": "red", "name": "Red", "range": [1, 2] },
                { "id": "blue", "name": "Blue", "range": [3, 4] },
                { "id": "green", "name": "Green", "range": [5, 6] }
              ] },
            { "id": "table.count", "name": "Count", "dice": "1d4",
              "results": [
                { "id": "one", "name": "One", "range": 1, "value": 1 },
                { "id": "two", "name": "Two", "range": 2, "value": 2 },
                { "id": "three", "name": "Three", "range": [3, 4], "value": 3 }
              ] }
        ],
        "tags": [{ "id": "tag.lucky", "name": "Lucky" }],
        "objects": [
            { "id": "object.district", "name": "District", "id_prefix": "district" },
            { "id": "object.location", "name": "Location", "id_prefix": "location" },
            { "id": "object.figure", "name": "Figure", "label": "Figure {n}", "id_prefix": "figure" }
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
    serde_json::from_value(base).expect("content parses")
}

/// `extra` content plus a start flow `flow.main` with `steps`.
fn with_main(extra: Value, steps: Value) -> ContentSet {
    let mut extra = extra;
    extra["start"] = json!("flow.main");
    let mut all = vec![json!({ "id": "flow.main", "name": "Main", "steps": steps })];
    all.extend(
        extra
            .get("flows")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
    );
    extra["flows"] = Value::Array(all);
    content(extra)
}

fn errors(report: &ValidationReport) -> Vec<IssueKind> {
    report
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error)
        .map(|i| i.kind.clone())
        .collect()
}

fn warnings(report: &ValidationReport) -> Vec<IssueKind> {
    report
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Warning)
        .map(|i| i.kind.clone())
        .collect()
}

fn session_with(c: ContentSet, options: GenerationOptions) -> GenerationSession {
    let report = validate(&c);
    assert!(!report.has_errors(), "{report}");
    GenerationSession::new(Arc::new(c), options, GenerationMode::OneTable).expect("session")
}

fn session(extra: Value, steps: Value) -> GenerationSession {
    session_with(with_main(extra, steps), GenerationOptions::default())
}

fn dice(values: &[u32]) -> ScriptedFaces {
    ScriptedFaces::new(values.iter().copied())
}

fn record(s: &GenerationSession, stage: usize) -> StepRecord {
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

fn ids_with_prefix(record: &StepRecord, prefix: &str) -> Vec<String> {
    record
        .objects()
        .iter()
        .filter(|o| o.id.starts_with(prefix))
        .map(|o| o.id.clone())
        .collect()
}

// ----- generic patterns --------------------------------------------------------------

#[test]
fn n_districts_each_with_n_locations() {
    let steps = json!([
        { "id": "districts", "kind": "roll", "table": "table.count", "name": "Districts" },
        { "id": "each-district", "kind": "repeat", "count": { "value_of": { "step": "districts" } }, "steps": [
            { "id": "district", "kind": "create_object", "object": "object.district", "steps": [
                { "id": "locations", "kind": "roll", "table": "table.count" },
                { "id": "each-location", "kind": "repeat",
                  "count": { "value_of": { "step": "locations" } }, "steps": [
                    { "id": "location", "kind": "create_object", "object": "object.location", "steps": [
                        { "id": "grade", "kind": "roll", "table": "table.grade" } ] } ] } ] } ] }
    ]);
    let mut s = session(json!({}), steps);
    s.roll(0, &mut dice(&[2])).unwrap(); // two districts
    // District 1: one location; district 2: three.
    s.roll(1, &mut dice(&[1, 1, 3, 2, 3, 4])).unwrap();
    let r = record(&s, 1);
    assert_eq!(
        ids_with_prefix(&r, "district"),
        ["district-01", "district-02"]
    );
    assert_eq!(
        ids_with_prefix(&r, "location"),
        ["location-01", "location-02", "location-03", "location-04"]
    );
    assert_eq!(rolls(&r), ["One", "Low", "Three", "Mid", "High", "Top"]);
}

#[test]
fn n_figures_and_one_relationship_per_unique_pair() {
    let steps = json!([
        { "id": "count", "kind": "roll", "table": "table.count" },
        { "id": "figures", "kind": "repeat", "count": { "value_of": { "step": "count" } }, "steps": [
            { "id": "figure", "kind": "create_object", "object": "object.figure", "steps": [] } ] },
        { "id": "relationships", "kind": "for_each", "object": "object.figure", "pairs": true, "steps": [
            { "id": "bond", "kind": "roll", "table": "table.colour" } ] }
    ]);
    let mut s = session(json!({}), steps);
    s.roll(0, &mut dice(&[3])).unwrap(); // three figures
    s.roll(1, &mut dice(&[])).unwrap();
    s.roll(2, &mut dice(&[1, 3, 5])).unwrap();
    assert_eq!(record(&s, 1).objects().len(), 3);
    let r = record(&s, 2);
    let StepOutcome::ForEach { pairs, iterations } = &r.outcome else {
        panic!("{:?}", r.outcome)
    };
    assert!(pairs);
    let names: Vec<String> = iterations
        .iter()
        .map(|i| {
            i.objects
                .iter()
                .map(|o| o.name.as_str())
                .collect::<Vec<_>>()
                .join(" & ")
        })
        .collect();
    assert_eq!(
        names,
        [
            "Figure 1 & Figure 2",
            "Figure 1 & Figure 3",
            "Figure 2 & Figure 3"
        ]
    );
    assert_eq!(rolls(&r), ["Red", "Blue", "Green"]);
}

#[test]
fn for_each_object_runs_once_per_object() {
    let steps = json!([
        { "id": "figures", "kind": "repeat", "count": { "times": 2 }, "steps": [
            { "id": "figure", "kind": "create_object", "object": "object.figure", "steps": [] } ] },
        { "id": "each", "kind": "for_each", "object": "object.figure", "steps": [
            { "id": "mood", "kind": "roll", "table": "table.grade" } ] }
    ]);
    let mut s = session(json!({}), steps);
    s.roll(0, &mut dice(&[])).unwrap();
    s.roll(1, &mut dice(&[1, 4])).unwrap();
    let r = record(&s, 1);
    let StepOutcome::ForEach { iterations, .. } = &r.outcome else {
        panic!()
    };
    assert_eq!(iterations.len(), 2);
    assert_eq!(iterations[1].objects[0].id, "figure-02");
    assert_eq!(rolls(&r), ["Low", "Top"]);
}

#[test]
fn a_setting_inserts_extra_generation_only_when_on() {
    let extra = json!({ "flows": [{ "id": "flow.extra", "name": "Guaranteed Building", "steps": [
        { "id": "building", "kind": "create_object", "object": "object.location", "steps": [] } ] }] });
    let steps = json!([
        { "id": "rolled", "kind": "repeat", "count": { "times": 2 }, "steps": [
            { "id": "location", "kind": "create_object", "object": "object.location", "steps": [] } ] },
        { "id": "minimum", "kind": "branch", "cases": [
            { "when": { "setting_on": "app.bareMinimumBuildings" }, "steps": [
                { "id": "insert", "kind": "run_flow", "flow": "flow.extra" } ] } ] }
    ]);
    for (on, expected) in [(false, 2), (true, 3)] {
        let options = GenerationOptions {
            bare_minimum_buildings: on,
            ..Default::default()
        };
        let mut s = session_with(with_main(extra.clone(), steps.clone()), options);
        s.roll(0, &mut dice(&[])).unwrap();
        s.roll(1, &mut dice(&[])).unwrap();
        let made = record(&s, 0).objects().len() + record(&s, 1).objects().len();
        assert_eq!(made, expected, "setting on: {on}");
    }
}

// ----- the newer step kinds ----------------------------------------------------------

#[test]
fn deliberate_recursion_stops_at_its_maximum_depth() {
    let extra = json!({ "flows": [{ "id": "flow.deeper", "name": "Deeper", "steps": [
        { "id": "roll", "kind": "roll", "table": "table.grade" },
        { "id": "again", "kind": "run_flow", "flow": "flow.deeper", "max_depth": 2 } ] }] });
    let steps = json!([{ "id": "start", "kind": "run_flow", "flow": "flow.deeper" }]);
    let c = with_main(extra.clone(), steps.clone());
    let report = validate(&c);
    assert!(!report.has_errors(), "{report}");
    assert!(warnings(&report).contains(&IssueKind::LimitedRecursion {
        flows: vec!["flow.deeper".into()],
        depth: 2
    }));
    let mut s = session(extra, steps);
    s.roll(0, &mut dice(&[1, 2, 3])).unwrap();
    let r = record(&s, 0);
    assert_eq!(rolls(&r), ["Low", "Mid", "High"]);
    fn deepest(r: &StepRecord) -> &StepRecord {
        r.children().last().map_or(r, |c| deepest(c))
    }
    assert!(matches!(
        deepest(&r).outcome,
        StepOutcome::Skipped {
            reason: StepSkip::RecursionLimit
        }
    ));
}

#[test]
fn unguarded_recursion_is_an_error_and_depth_must_be_sensible() {
    let c = with_main(
        json!({ "flows": [{ "id": "flow.loop", "name": "Loop", "steps": [
            { "id": "again", "kind": "run_flow", "flow": "flow.loop" } ] }] }),
        json!([]),
    );
    assert!(errors(&validate(&c)).contains(&IssueKind::FlowCycle {
        flows: vec!["flow.loop".into()]
    }));
    let c = with_main(
        json!({ "flows": [{ "id": "flow.loop", "name": "Loop", "steps": [
            { "id": "again", "kind": "run_flow", "flow": "flow.loop", "max_depth": 0 } ] }] }),
        json!([]),
    );
    assert!(errors(&validate(&c)).contains(&IssueKind::InvalidRecursionDepth));
}

#[test]
fn stop_ends_the_flow_it_is_in() {
    let extra = json!({ "flows": [{ "id": "flow.short", "name": "Short", "steps": [
        { "id": "a", "kind": "roll", "table": "table.grade" },
        { "id": "end", "kind": "stop" },
        { "id": "b", "kind": "roll", "table": "table.grade" } ] }] });
    let steps = json!([
        { "id": "run", "kind": "repeat", "count": { "times": 1 }, "steps": [
            { "id": "sub", "kind": "run_flow", "flow": "flow.short" },
            { "id": "after", "kind": "roll", "table": "table.colour" } ] }
    ]);
    let c = with_main(extra.clone(), steps.clone());
    assert!(
        warnings(&validate(&c))
            .iter()
            .any(|k| matches!(k, IssueKind::UnreachableStep { .. }))
    );
    let mut s = session(extra, steps);
    s.roll(0, &mut dice(&[4, 5])).unwrap();
    // The subflow stops after its first roll; the caller carries on.
    assert_eq!(rolls(&record(&s, 0)), ["Top", "Green"]);
}

#[test]
fn remove_tag_apply_modifier_and_read_setting() {
    let extra = json!({ "settings": [{ "id": "setting.size", "name": "Size", "kind": "choice",
        "options": [{ "id": "small", "name": "Small" }, { "id": "large", "name": "Large" }],
        "default": "small" }] });
    let steps = json!([
        { "id": "add", "kind": "add_tag", "tag": "tag.lucky" },
        { "id": "remove", "kind": "remove_tag", "tag": "tag.lucky" },
        { "id": "boost", "kind": "apply_modifier", "table": "table.grade", "modifier": { "shift": 1 } },
        { "id": "grade", "kind": "roll", "table": "table.grade" },
        { "id": "size", "kind": "read_setting", "setting": "setting.size" },
        { "id": "by-size", "kind": "branch", "cases": [
            { "when": { "result_is": { "step": "size", "results": ["large"] } }, "steps": [
                { "id": "big", "kind": "roll", "table": "table.colour" } ] } ] }
    ]);
    let c = with_main(extra.clone(), steps.clone());
    let mut values = SettingValues::default();
    values.insert(
        SettingId::new("setting.size"),
        SettingValue::Choice("large".into()),
    );
    let mut s = session_with(c, GenerationOptions::default()).with_settings(values);
    for stage in 0..6 {
        s.roll(
            stage,
            &mut dice(match stage {
                3 => &[1],
                5 => &[6],
                _ => &[],
            }),
        )
        .unwrap();
    }
    assert!(s.active_tags().is_empty());
    // A later top-level roll is shifted up from Low.
    let grade = s.view(3, ViewOptions::default()).unwrap().result.unwrap();
    assert_eq!(grade.name, "Mid");
    let StepOutcome::ReadSetting { value, .. } = &record(&s, 4).outcome else {
        panic!()
    };
    assert_eq!(*value, SettingValue::Choice("large".into()));
    assert_eq!(rolls(&record(&s, 5)), ["Green"]);
}

// ----- validation --------------------------------------------------------------------

#[test]
fn loops_and_conditions_that_can_never_work_are_reported() {
    let check = |steps: Value| validate(&with_main(json!({}), steps));
    let empty = check(
        json!([{ "id": "r", "kind": "repeat", "count": { "times": 0 }, "steps": [
        { "id": "x", "kind": "roll", "table": "table.grade" } ] }]),
    );
    assert!(
        warnings(&empty)
            .iter()
            .any(|k| matches!(k, IssueKind::EmptyLoop { .. }))
    );
    let never_made = check(
        json!([{ "id": "e", "kind": "for_each", "object": "object.figure",
        "steps": [{ "id": "x", "kind": "roll", "table": "table.grade" }] }]),
    );
    assert!(
        warnings(&never_made)
            .iter()
            .any(|k| matches!(k, IssueKind::EmptyLoop { .. }))
    );
    let impossible = check(json!([
        { "id": "g", "kind": "roll", "table": "table.grade" },
        { "id": "b", "kind": "branch", "cases": [
            { "when": { "result_is": { "step": "g", "results": ["red"] } }, "steps": [] } ] }
    ]));
    assert!(
        errors(&impossible)
            .iter()
            .any(|k| matches!(k, IssueKind::ImpossibleCondition { .. }))
    );
    let contradiction = check(json!([
        { "id": "b", "kind": "branch", "cases": [
            { "when": { "all": [ { "has_tag": "tag.lucky" }, { "not": { "has_tag": "tag.lucky" } } ] },
              "steps": [] } ] }
    ]));
    assert!(
        errors(&contradiction)
            .iter()
            .any(|k| matches!(k, IssueKind::ImpossibleCondition { .. }))
    );
    let unreachable_case = check(json!([
        { "id": "b", "kind": "branch", "cases": [
            { "when": { "all": [] }, "steps": [] },
            { "when": { "has_tag": "tag.lucky" }, "steps": [] } ] }
    ]));
    assert!(
        warnings(&unreachable_case)
            .iter()
            .any(|k| matches!(k, IssueKind::UnreachableStep { .. }))
    );
}

#[test]
fn detached_steps_never_run_and_are_reported() {
    let mut c = with_main(
        json!({}),
        json!([{ "id": "a", "kind": "roll", "table": "table.grade" }]),
    );
    let flow = c.flows[0].clone();
    c.flows[0] = apply(
        &flow,
        FlowEdit::Detach {
            step: StepId::new("a"),
        },
    )
    .unwrap();
    assert!(c.flows[0].steps.is_empty());
    assert!(
        warnings(&validate(&c))
            .iter()
            .any(|k| matches!(k, IssueKind::UnreachableStep { .. }))
    );
}

// ----- edit operations ---------------------------------------------------------------

fn top(index: usize) -> Position {
    Position {
        slot: Slot::Top,
        index,
    }
}

fn ids(steps: &[settlement_core::framework::FlowStep]) -> Vec<String> {
    steps.iter().map(|s| s.id.to_string()).collect()
}

#[test]
fn steps_are_added_moved_nested_duplicated_and_removed() {
    let flow = Flow {
        id: FlowId::new("user.flow.test"),
        name: "Test".into(),
        description: String::new(),
        steps: Vec::new(),
        editor: Default::default(),
    };
    let flow = apply(
        &flow,
        FlowEdit::Add {
            kind: StepKind::RollTable,
            at: top(0),
        },
    )
    .unwrap();
    let flow = apply(
        &flow,
        FlowEdit::Add {
            kind: StepKind::RepeatTimes,
            at: top(9),
        },
    )
    .unwrap();
    let flow = apply(
        &flow,
        FlowEdit::Add {
            kind: StepKind::RollTable,
            at: top(9),
        },
    )
    .unwrap();
    assert_eq!(ids(&flow.steps), ["roll", "repeat", "roll-2"]);

    // Move the first roll into the repeat.
    let into_repeat = Position {
        slot: Slot::Body {
            step: StepId::new("repeat"),
        },
        index: 0,
    };
    let flow = apply(
        &flow,
        FlowEdit::Move {
            step: StepId::new("roll"),
            to: into_repeat.clone(),
        },
    )
    .unwrap();
    assert_eq!(ids(&flow.steps), ["repeat", "roll-2"]);
    assert_eq!(
        ids(&flow.steps[0]
            .children()
            .into_iter()
            .cloned()
            .collect::<Vec<_>>()),
        ["roll"]
    );

    // A step cannot go inside itself.
    let err = apply(
        &flow,
        FlowEdit::Move {
            step: StepId::new("repeat"),
            to: into_repeat,
        },
    )
    .unwrap_err();
    assert!(err.contains("inside itself"), "{err}");

    // Duplicating copies the children with fresh IDs.
    let flow = apply(
        &flow,
        FlowEdit::Duplicate {
            step: StepId::new("repeat"),
        },
    )
    .unwrap();
    assert_eq!(ids(&flow.steps), ["repeat", "repeat-2", "roll-2"]);
    let all: Vec<String> = flow.all_steps().iter().map(|s| s.id.to_string()).collect();
    let mut unique = all.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(all.len(), unique.len(), "{all:?}");

    // Updating a repeat's count keeps what is inside it.
    let mut changed = flow.steps[0].clone();
    if let StepAction::Repeat { count, steps } = &mut changed.action {
        *count = settlement_core::framework::Count::Times(3);
        steps.clear();
    }
    let flow = apply(&flow, FlowEdit::Update { step: changed }).unwrap();
    assert_eq!(flow.steps[0].children().len(), 1);

    // Detach and reattach; then remove with its children.
    let flow = apply(
        &flow,
        FlowEdit::Detach {
            step: StepId::new("roll-2"),
        },
    )
    .unwrap();
    assert_eq!(flow.editor.detached.len(), 1);
    let flow = apply(
        &flow,
        FlowEdit::Attach {
            step: StepId::new("roll-2"),
            to: top(0),
        },
    )
    .unwrap();
    assert_eq!(ids(&flow.steps), ["roll-2", "repeat", "repeat-2"]);
    let flow = apply(
        &flow,
        FlowEdit::Place {
            step: StepId::new("repeat"),
            at: [10.0, 20.0],
        },
    )
    .unwrap();
    let flow = apply(
        &flow,
        FlowEdit::Remove {
            step: StepId::new("repeat"),
        },
    )
    .unwrap();
    assert_eq!(ids(&flow.steps), ["roll-2", "repeat-2"]);
    assert!(flow.editor.positions.is_empty());

    // A missing step fails without changing anything.
    assert!(
        apply(
            &flow,
            FlowEdit::Remove {
                step: StepId::new("ghost")
            }
        )
        .is_err()
    );
}

#[test]
fn branch_cases_can_be_added_and_removed_but_not_all() {
    let flow = Flow {
        id: FlowId::new("user.flow.test"),
        name: "Test".into(),
        description: String::new(),
        steps: Vec::new(),
        editor: Default::default(),
    };
    let flow = apply(
        &flow,
        FlowEdit::Add {
            kind: StepKind::Condition,
            at: top(0),
        },
    )
    .unwrap();
    let id = StepId::new("if");
    let flow = apply(&flow, FlowEdit::AddCase { step: id.clone() }).unwrap();
    let flow = apply(
        &flow,
        FlowEdit::Add {
            kind: StepKind::Stop,
            at: Position {
                slot: Slot::Case {
                    step: id.clone(),
                    case: 1,
                },
                index: 0,
            },
        },
    )
    .unwrap();
    assert_eq!(flow.all_steps().len(), 2);
    let flow = apply(
        &flow,
        FlowEdit::RemoveCase {
            step: id.clone(),
            case: 1,
        },
    )
    .unwrap();
    assert_eq!(flow.all_steps().len(), 1);
    assert!(apply(&flow, FlowEdit::RemoveCase { step: id, case: 0 }).is_err());
}

// ----- pseudocode --------------------------------------------------------------------

#[test]
fn pseudocode_reads_like_the_rule() {
    let extra = json!({
        "tables": [
            { "id": "table.authority", "name": "Authority", "dice": "1d4", "results": [
                { "id": "military", "name": "Military Authority", "range": [1, 2] },
                { "id": "civil", "name": "Civil Authority", "range": [3, 4] } ] },
            { "id": "table.representation", "name": "Representation", "dice": "1d4", "results": [
                { "id": "single", "name": "Single", "range": [1, 2] },
                { "id": "group", "name": "Group", "range": [3, 4] } ] }
        ],
        "flows": [{ "id": "flow.size", "name": "Authority Size", "steps": [] }]
    });
    let steps = json!([
        { "id": "authority", "kind": "roll", "table": "table.authority" },
        { "id": "representation", "kind": "roll", "table": "table.representation" },
        { "id": "size", "kind": "branch", "cases": [
            { "when": { "all": [
                { "result_is": { "step": "authority", "results": ["military"] } },
                { "result_is": { "step": "representation", "results": ["group"] } } ] },
              "steps": [{ "id": "run-size", "kind": "run_flow", "flow": "flow.size" }] } ] },
        { "id": "count", "kind": "repeat", "count": { "times": 2 }, "steps": [
            { "id": "stop", "kind": "stop" } ] }
    ]);
    let c = with_main(extra, steps);
    let flow = c.flow(&FlowId::new("flow.main")).unwrap().clone();
    let lines: Vec<String> = Describer::new(&c, Some(&flow))
        .pseudocode(&flow)
        .iter()
        .map(|l| format!("{}{}", "  ".repeat(l.depth), l.text))
        .collect();
    assert_eq!(
        lines,
        [
            "Roll Authority.",
            "Roll Representation.",
            "IF Authority is Military Authority",
            "AND Representation is Group",
            "THEN run Authority Size.",
            "Repeat 2 times:",
            "  Stop.",
        ]
    );
}

// ----- service ------------------------------------------------------------------------

#[test]
fn flows_are_checked_drafted_saved_and_duplicated_through_the_service() {
    let mut service = AppService::new(Database::open_in_memory().unwrap());
    let flow = AppService::new_flow();
    let flow = AppService::edit_flow(
        &flow,
        FlowEdit::Add {
            kind: StepKind::Stop,
            at: top(0),
        },
    )
    .unwrap();
    let check = service.check_flow(&flow).unwrap();
    assert!(check.is_acceptable(), "{:?}", check.issues);
    assert_eq!(check.pseudocode[0].text, "Stop.");

    // A roll with no table yet cannot be saved.
    let broken = AppService::edit_flow(
        &flow,
        FlowEdit::Add {
            kind: StepKind::RollTable,
            at: top(0),
        },
    )
    .unwrap();
    assert!(!service.check_flow(&broken).unwrap().is_acceptable());

    service.save_flow_draft(&broken).unwrap();
    assert_eq!(
        service.flow_draft(&broken.id).unwrap(),
        Some(broken.clone())
    );
    let saved = service.save_flow(flow.clone()).unwrap();
    assert_eq!(saved.steps.len(), 1);
    assert_eq!(service.flow_draft(&flow.id).unwrap(), None);

    let copy = service.duplicate_flow(&flow.id).unwrap();
    assert_ne!(copy.id, flow.id);
    assert_eq!(copy.name, "Copy of New flow");

    let context = service.flow_context(&saved).unwrap();
    assert_eq!(context.kinds.len(), StepKind::ALL.len());
    assert!(
        context
            .settings
            .iter()
            .any(|s| s.name == "Bare Minimum Buildings")
    );
    assert!(context.flows.iter().any(|f| f.id == flow.id.to_string()));
    assert!(
        !context.subjects.is_empty(),
        "pipeline stages can be tested"
    );
}
