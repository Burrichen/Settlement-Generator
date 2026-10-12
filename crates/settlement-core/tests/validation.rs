//! Content validation tests. Fixtures use neutral placeholder IDs; they are
//! test data, not game content.

use serde_json::json;
use settlement_core::content::{ContentSet, FaceRange};
use settlement_core::dice::DiceParseError;
use settlement_core::validation::{IdScope, IssueKind, Severity, ValidationReport, validate};

fn content(value: serde_json::Value) -> ContentSet {
    serde_json::from_value(value).expect("fixture deserialises")
}

fn kinds(report: &ValidationReport) -> Vec<&IssueKind> {
    report.issues.iter().map(|i| &i.kind).collect()
}

fn assert_has(report: &ValidationReport, severity: Severity, kind: IssueKind) {
    assert!(
        report
            .issues
            .iter()
            .any(|i| i.severity == severity && i.kind == kind),
        "expected {severity:?} {kind:?}, got:\n{report}"
    );
}

/// One d6 table with three results covering 1-6, and two tags.
fn base() -> serde_json::Value {
    json!({
        "tables": [{
            "id": "table.a", "name": "A", "dice": "1d6",
            "results": [
                { "id": "low", "name": "Low", "range": [1, 2], "tags": ["tag.x"] },
                { "id": "mid", "name": "Mid", "range": [3, 4] },
                { "id": "high", "name": "High", "range": [5, 6], "tags": ["tag.y"] }
            ]
        }],
        "tags": [
            { "id": "tag.x", "name": "X" },
            { "id": "tag.y", "name": "Y" }
        ]
    })
}

#[test]
fn well_formed_content_has_no_issues() {
    let report = validate(&content(base()));
    assert!(report.issues.is_empty(), "{report}");
}

#[test]
fn empty_content_is_valid() {
    assert!(validate(&ContentSet::default()).issues.is_empty());
}

#[test]
fn detects_overlapping_ranges() {
    let mut c = base();
    c["tables"][0]["results"][1]["range"] = json!([2, 5]);
    let report = validate(&content(c));
    assert_has(
        &report,
        Severity::Error,
        IssueKind::OverlappingRanges {
            first: "low".into(),
            second: "mid".into(),
            overlap: FaceRange::single(2),
        },
    );
    assert_has(
        &report,
        Severity::Error,
        IssueKind::OverlappingRanges {
            first: "mid".into(),
            second: "high".into(),
            overlap: FaceRange::single(5),
        },
    );
}

#[test]
fn detects_uncovered_outcomes_in_gaps_and_ends() {
    let mut c = base();
    c["tables"][0]["dice"] = json!("1d8");
    c["tables"][0]["results"][1]["range"] = json!(4);
    let report = validate(&content(c));
    assert_has(
        &report,
        Severity::Error,
        IssueKind::UncoveredOutcomes {
            dice: "1d8".into(),
            outcomes: vec![FaceRange::single(3), FaceRange::new(7, 8)],
        },
    );
}

#[test]
fn table_without_results_is_entirely_uncovered() {
    let c = json!({ "tables": [{ "id": "t", "name": "T", "dice": "2d6" }] });
    let report = validate(&content(c));
    assert_has(
        &report,
        Severity::Error,
        IssueKind::UncoveredOutcomes {
            dice: "2d6".into(),
            outcomes: vec![FaceRange::new(2, 12)],
        },
    );
}

#[test]
fn detects_impossible_ranges() {
    let mut c = base();
    c["tables"][0]["results"][0]["range"] = json!([2, 1]);
    c["tables"][0]["results"][2]["range"] = json!([5, 7]);
    let report = validate(&content(c));
    assert_has(
        &report,
        Severity::Error,
        IssueKind::ImpossibleRange {
            range: FaceRange { min: 2, max: 1 },
            possible: None,
        },
    );
    assert_has(
        &report,
        Severity::Error,
        IssueKind::ImpossibleRange {
            range: FaceRange::new(5, 7),
            possible: Some(FaceRange::new(1, 6)),
        },
    );
}

#[test]
fn two_d6_range_starting_at_one_is_impossible() {
    let c = json!({ "tables": [{ "id": "t", "name": "T", "dice": "2d6", "results": [
        { "id": "a", "name": "A", "range": [1, 6] },
        { "id": "b", "name": "B", "range": [7, 12] }
    ]}]});
    let report = validate(&content(c));
    assert_has(
        &report,
        Severity::Error,
        IssueKind::ImpossibleRange {
            range: FaceRange::new(1, 6),
            possible: Some(FaceRange::new(2, 12)),
        },
    );
}

#[test]
fn detects_missing_tags_everywhere_they_are_referenced() {
    let mut c = base();
    c["tables"][0]["results"][0]["tags"] = json!(["tag.gone"]);
    c["tables"][0]["eligibility"] = json!({ "has_tag": "tag.cond" });
    c["tables"][0]["results"][1]["eligibility"] = json!({ "not": { "has_tag": "tag.res" } });
    c["tags"][0]["effects"] = json!([{ "kind": "add_tag", "tag": "tag.added" }]);
    let report = validate(&content(c));
    for tag in ["tag.gone", "tag.cond", "tag.res", "tag.added"] {
        assert_has(
            &report,
            Severity::Error,
            IssueKind::MissingTag { tag: tag.into() },
        );
    }
}

#[test]
fn detects_missing_tables_and_variants() {
    let mut c = base();
    c["tags"][0]["effects"] = json!([
        { "kind": "route_to", "table": "table.nowhere" },
        { "kind": "modify_dice", "table": "table.gone", "modifier": 1 },
        { "kind": "use_variant", "table": "table.a", "variant": "v.none" },
        { "kind": "route_to", "table": "table.a", "variant": "v.none2" }
    ]);
    let report = validate(&content(c));
    assert_has(
        &report,
        Severity::Error,
        IssueKind::MissingTable {
            table: "table.nowhere".into(),
        },
    );
    assert_has(
        &report,
        Severity::Error,
        IssueKind::MissingTable {
            table: "table.gone".into(),
        },
    );
    for variant in ["v.none", "v.none2"] {
        assert_has(
            &report,
            Severity::Error,
            IssueKind::MissingVariant {
                table: "table.a".into(),
                variant: variant.into(),
            },
        );
    }
    let missing = report
        .issues
        .iter()
        .find(|i| matches!(i.kind, IssueKind::MissingTable { .. }))
        .unwrap();
    assert_eq!(missing.location.tag.as_ref().unwrap().as_str(), "tag.x");
    assert_eq!(missing.location.effect, Some(0));
}

#[test]
fn detects_invalid_dice_in_tables_variants_and_effects() {
    let mut c = base();
    c["tables"][0]["dice"] = json!("1d7");
    c["tables"][0]["variants"] = json!([
        { "id": "v1", "name": "V1" },
        { "id": "v2", "name": "V2", "dice": "zz" }
    ]);
    c["tags"][0]["effects"] = json!([
        { "kind": "secondary_roll", "label": "Extra", "dice": "0d6" },
        { "kind": "modify_dice", "table": "table.a", "dice": "" }
    ]);
    let report = validate(&content(c));
    let invalid: Vec<_> = report
        .issues
        .iter()
        .filter_map(|i| match &i.kind {
            IssueKind::InvalidDice { expression, error } => Some((expression.as_str(), error)),
            _ => None,
        })
        .collect();
    // v1 inherits the table's dice, so the bad base expression is reported once.
    assert_eq!(
        invalid,
        vec![
            ("1d7", &DiceParseError::UnsupportedDie { sides: 7 }),
            ("zz", &DiceParseError::InvalidToken { token: "zz".into() }),
            ("0d6", &DiceParseError::ZeroCount),
            ("", &DiceParseError::Empty),
        ]
    );
}

#[test]
fn detects_broken_result_references() {
    let mut c = base();
    c["tables"][0]["variants"] = json!([{
        "id": "v", "name": "V",
        "ranges": [
            { "result": "low", "range": [1, 3] },
            { "result": "ghost", "range": [4, 6] }
        ]
    }]);
    c["tags"][0]["effects"] = json!([{
        "kind": "restrict_results", "table": "table.a",
        "only": ["low", "phantom"], "exclude": ["spectre"]
    }]);
    let report = validate(&content(c));
    for result in ["ghost", "phantom", "spectre"] {
        assert_has(
            &report,
            Severity::Error,
            IssueKind::BrokenResultReference {
                table: "table.a".into(),
                result: result.into(),
            },
        );
    }
}

#[test]
fn detects_duplicate_ids_in_every_scope() {
    let c = json!({
        "tables": [
            { "id": "t", "name": "T", "dice": "1d4",
              "results": [
                  { "id": "r", "name": "R", "range": [1, 2] },
                  { "id": "r", "name": "R again", "range": [3, 4] }
              ],
              "variants": [
                  { "id": "v", "name": "V", "ranges": [
                      { "result": "r", "range": [1, 2] },
                      { "result": "r", "range": [3, 4] }
                  ]},
                  { "id": "v", "name": "V again" }
              ]
            },
            { "id": "t", "name": "T again", "dice": "1d4",
              "results": [{ "id": "r", "name": "R", "range": [1, 4] }] }
        ],
        "tags": [ { "id": "g", "name": "G" }, { "id": "g", "name": "G again" } ]
    });
    let report = validate(&content(c));
    for (scope, id) in [
        (IdScope::Table, "t"),
        (IdScope::Tag, "g"),
        (IdScope::Result, "r"),
        (IdScope::Variant, "v"),
        (IdScope::VariantRange, "r"),
    ] {
        assert_has(
            &report,
            Severity::Error,
            IssueKind::DuplicateId {
                scope,
                id: id.into(),
            },
        );
    }
}

#[test]
fn detects_malformed_ids() {
    let mut c = base();
    c["tables"][0]["id"] = json!("Table A");
    c["tags"][1]["id"] = json!("");
    let report = validate(&content(c));
    assert_has(
        &report,
        Severity::Error,
        IssueKind::InvalidId {
            id: "Table A".into(),
        },
    );
    assert_has(
        &report,
        Severity::Error,
        IssueKind::InvalidId { id: "".into() },
    );
}

#[test]
fn guaranteed_routing_cycle_is_an_error() {
    // Every result of a routes to b and every result of b routes to a.
    let c = json!({
        "tables": [
            { "id": "a", "name": "A", "dice": "1d4", "results": [
                { "id": "1", "name": "1", "range": [1, 2], "tags": ["to-b"] },
                { "id": "2", "name": "2", "range": [3, 4], "tags": ["to-b"] }
            ]},
            { "id": "b", "name": "B", "dice": "1d4", "results": [
                { "id": "1", "name": "1", "range": [1, 4], "tags": ["via"] }
            ]}
        ],
        "tags": [
            { "id": "to-b", "name": "to b", "effects": [{ "kind": "route_to", "table": "b" }] },
            { "id": "via", "name": "via", "effects": [{ "kind": "add_tag", "tag": "to-a" }] },
            { "id": "to-a", "name": "to a", "effects": [{ "kind": "route_to", "table": "a" }] }
        ]
    });
    let report = validate(&content(c));
    assert_has(
        &report,
        Severity::Error,
        IssueKind::RoutingCycle {
            tables: vec!["a".into(), "b".into()],
            guaranteed: true,
        },
    );
}

#[test]
fn possible_routing_cycle_is_a_warning() {
    let c = json!({
        "tables": [
            { "id": "a", "name": "A", "dice": "1d4", "results": [
                { "id": "again", "name": "Again", "range": [1, 2], "tags": ["loop"] },
                { "id": "stop", "name": "Stop", "range": [3, 4] }
            ]}
        ],
        "tags": [
            { "id": "loop", "name": "Loop", "effects": [{ "kind": "route_to", "table": "a" }] }
        ]
    });
    let report = validate(&content(c));
    assert!(!report.has_errors(), "{report}");
    assert_has(
        &report,
        Severity::Warning,
        IssueKind::RoutingCycle {
            tables: vec!["a".into()],
            guaranteed: false,
        },
    );
}

#[test]
fn self_route_on_every_result_is_an_error() {
    let c = json!({
        "tables": [{ "id": "a", "name": "A", "dice": "1d4", "results": [
            { "id": "r", "name": "R", "range": [1, 4], "tags": ["loop"] }
        ]}],
        "tags": [
            { "id": "loop", "name": "Loop", "effects": [{ "kind": "route_to", "table": "a" }] }
        ]
    });
    assert_has(
        &validate(&content(c)),
        Severity::Error,
        IssueKind::RoutingCycle {
            tables: vec!["a".into()],
            guaranteed: true,
        },
    );
}

#[test]
fn routes_through_disabled_or_planned_effects_do_not_form_cycles() {
    let c = json!({
        "tables": [{ "id": "a", "name": "A", "dice": "1d4", "results": [
            { "id": "r", "name": "R", "range": [1, 4], "tags": ["off", "planned"] }
        ]}],
        "tags": [
            { "id": "off", "name": "Off", "enabled": false,
              "effects": [{ "kind": "route_to", "table": "a" }] },
            { "id": "planned", "name": "Planned",
              "effects": [{ "kind": "route_to", "table": "a", "status": "planned" }] }
        ]
    });
    let report = validate(&content(c));
    assert!(report.issues.is_empty(), "{report}");
}

#[test]
fn planned_effects_may_reference_content_that_does_not_exist_yet() {
    let mut c = base();
    c["tags"][0]["effects"] = json!([
        { "kind": "restrict_results", "table": "table.future", "only": ["someday"],
          "status": "planned", "note": "for a table added later" },
        { "kind": "use_variant", "table": "table.a", "variant": "v.future", "status": "planned" },
        { "kind": "add_tag", "tag": "tag.future", "status": "planned" }
    ]);
    let report = validate(&content(c));
    assert!(report.issues.is_empty(), "{report}");
}

#[test]
fn rejects_unsupported_schema_version() {
    let mut c = base();
    c["schema_version"] = json!(99);
    assert_has(
        &validate(&content(c)),
        Severity::Error,
        IssueKind::UnsupportedSchemaVersion {
            found: 99,
            supported: 1,
        },
    );
}

#[test]
fn warns_about_results_with_no_faces() {
    let mut c = base();
    c["tables"][0]["results"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "id": "orphan", "name": "Orphan" }));
    let report = validate(&content(c));
    assert!(!report.has_errors(), "{report}");
    assert_eq!(kinds(&report), vec![&IssueKind::UnreachableResult]);
    assert_eq!(
        report.issues[0].location.result.as_ref().unwrap().as_str(),
        "orphan"
    );
}

#[test]
fn variant_only_results_are_reachable() {
    let mut c = base();
    c["tables"][0]["results"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "id": "special", "name": "Special" }));
    c["tables"][0]["variants"] = json!([{
        "id": "v", "name": "V", "dice": "1d4",
        "ranges": [
            { "result": "low", "range": [1, 3] },
            { "result": "special", "range": 4 }
        ]
    }]);
    let report = validate(&content(c));
    assert!(report.issues.is_empty(), "{report}");
}

#[test]
fn variant_layouts_are_checked_against_their_own_dice() {
    let mut c = base();
    c["tables"][0]["variants"] = json!([{ "id": "v", "name": "V", "dice": "1d8" }]);
    let report = validate(&content(c));
    let issue = report
        .issues
        .iter()
        .find(|i| matches!(i.kind, IssueKind::UncoveredOutcomes { .. }))
        .expect("uncovered 7-8 under the variant's d8");
    assert_eq!(issue.location.variant.as_ref().unwrap().as_str(), "v");
}

#[test]
fn issues_serialise_with_code_location_and_message() {
    let mut c = base();
    c["tables"][0]["results"][0]["tags"] = json!(["tag.gone"]);
    let report = validate(&content(c));
    let json = serde_json::to_value(&report.issues[0]).unwrap();
    assert_eq!(json["severity"], "error");
    assert_eq!(json["code"], "missing_tag");
    assert_eq!(json["tag"], "tag.gone");
    assert_eq!(json["location"]["table"], "table.a");
    assert_eq!(json["location"]["result"], "low");
    assert_eq!(
        json["message"],
        "table `table.a` › result `low`: references missing tag `tag.gone`"
    );
    let back: settlement_core::validation::ValidationIssue = serde_json::from_value(json).unwrap();
    assert_eq!(back, report.issues[0]);
}

#[test]
fn variants_without_overrides_do_not_repeat_base_issues() {
    let mut c = base();
    c["tables"][0]["results"][0]["range"] = json!(1);
    c["tables"][0]["variants"] =
        json!([{ "id": "v", "name": "V", "when": { "has_tag": "tag.x" } }]);
    let report = validate(&content(c));
    assert_eq!(report.issues.len(), 1, "{report}");
    assert!(report.issues[0].location.variant.is_none());
}

// ----- ordered scales ----------------------------------------------------------

#[test]
fn ordered_table_with_ascending_faces_is_valid() {
    let mut c = base();
    c["tables"][0]["ordered"] = json!(true);
    let parsed = content(c);
    assert!(parsed.tables[0].ordered);
    let report = validate(&parsed);
    assert!(report.issues.is_empty(), "{report}");
}

#[test]
fn ordered_table_warns_when_faces_leave_scale_order() {
    let mut c = base();
    c["tables"][0]["ordered"] = json!(true);
    c["tables"][0]["results"][0]["range"] = json!([3, 4]);
    c["tables"][0]["results"][1]["range"] = json!([1, 2]);
    let report = validate(&content(c));
    assert!(!report.has_errors(), "{report}");
    let warnings: Vec<_> = report.warnings().collect();
    assert_eq!(warnings.len(), 1, "{report}");
    assert_eq!(warnings[0].kind, IssueKind::ScaleOutOfOrder);
    assert_eq!(
        warnings[0].location.result.as_ref().unwrap().as_str(),
        "mid"
    );
}

#[test]
fn unordered_table_may_put_faces_in_any_order() {
    let mut c = base();
    c["tables"][0]["results"][0]["range"] = json!([3, 4]);
    c["tables"][0]["results"][1]["range"] = json!([1, 2]);
    assert!(validate(&content(c)).issues.is_empty());
}

#[test]
fn ordered_flag_is_omitted_when_false() {
    let json = content(base()).to_json_pretty();
    assert!(!json.contains("ordered"), "{json}");
}

// ----- choose-only, incomplete, intents, redirects, follow-ups, sequence -----

#[test]
fn rolled_tables_need_dice_but_choose_and_incomplete_tables_do_not() {
    let c = json!({ "tables": [
        { "id": "rolled", "name": "R", "results": [{ "id": "a", "name": "A" }] },
        { "id": "chosen", "name": "C", "selection": "choose", "results": [{ "id": "a", "name": "A" }] },
        { "id": "draft", "name": "D", "status": "incomplete", "status_note": "not decided",
          "results": [{ "id": "a", "name": "A" }] }
    ]});
    let report = validate(&content(c));
    assert_has(&report, Severity::Error, IssueKind::MissingDice);
    assert_has(
        &report,
        Severity::Warning,
        IssueKind::IncompleteContent {
            note: "not decided".into(),
        },
    );
    assert_eq!(report.errors().count(), 1, "{report}");
    assert!(
        report
            .issues
            .iter()
            .all(|i| i.location.table.as_ref().unwrap().as_str() != "chosen"),
        "{report}"
    );
}

#[test]
fn active_intents_are_rejected() {
    let mut c = base();
    c["tags"][0]["effects"] = json!([
        { "kind": "intent", "table": "table.a", "results": ["low"], "change": "increase" }
    ]);
    assert_has(
        &validate(&content(c)),
        Severity::Error,
        IssueKind::IntentNotPlanned,
    );
}

#[test]
fn redirects_are_checked_for_targets_and_cycles() {
    let mut c = base();
    c["tables"].as_array_mut().unwrap().push(json!({
        "id": "table.b", "name": "B", "dice": "1d4",
        "results": [{ "id": "r", "name": "R", "range": [1, 4] }]
    }));
    c["tags"][0]["effects"] = json!([
        { "kind": "redirect_table", "table": "table.a", "to": "table.b" },
        { "kind": "redirect_table", "table": "table.a", "to": "table.none" }
    ]);
    c["tags"][1]["effects"] = json!([
        { "kind": "redirect_table", "table": "table.b", "to": "table.a" }
    ]);
    let report = validate(&content(c));
    assert_has(
        &report,
        Severity::Error,
        IssueKind::MissingTable {
            table: "table.none".into(),
        },
    );
    assert_has(
        &report,
        Severity::Error,
        IssueKind::RedirectCycle {
            tables: vec!["table.a".into(), "table.b".into()],
        },
    );
}

#[test]
fn follow_ups_are_fully_checked() {
    let mut c = base();
    c["tables"][0]["follow_ups"] = json!([
        { "id": "up", "name": "Up", "when": { "has_tag": "tag.gone" },
          "on_results": ["low", "ghost"], "dice": "1d4",
          "outcomes": [ { "range": [1, 2] }, { "range": 2, "becomes": "phantom" } ] },
        { "id": "up", "name": "Again", "on_results": ["low"], "dice": "1d3", "outcomes": [] }
    ]);
    let report = validate(&content(c));
    assert_has(
        &report,
        Severity::Error,
        IssueKind::MissingTag {
            tag: "tag.gone".into(),
        },
    );
    for result in ["ghost", "phantom"] {
        assert_has(
            &report,
            Severity::Error,
            IssueKind::BrokenResultReference {
                table: "table.a".into(),
                result: result.into(),
            },
        );
    }
    assert_has(
        &report,
        Severity::Error,
        IssueKind::OverlappingRanges {
            first: "(keep)".into(),
            second: "phantom".into(),
            overlap: FaceRange::single(2),
        },
    );
    assert_has(
        &report,
        Severity::Error,
        IssueKind::UncoveredOutcomes {
            dice: "1d4".into(),
            outcomes: vec![FaceRange::new(3, 4)],
        },
    );
    assert_has(
        &report,
        Severity::Error,
        IssueKind::DuplicateId {
            scope: IdScope::FollowUp,
            id: "up".into(),
        },
    );
    assert!(
        report
            .issues
            .iter()
            .any(|i| i.location.follow_up.as_deref() == Some("up"))
    );
}

#[test]
fn sequence_steps_must_exist() {
    let mut c = base();
    c["sequence"] = json!(["table.a", "table.missing"]);
    let report = validate(&content(c));
    let issue = report.errors().next().expect("an error");
    assert_eq!(
        issue.kind,
        IssueKind::MissingTable {
            table: "table.missing".into()
        }
    );
    assert_eq!(issue.location.sequence_step, Some(1));
    assert!(issue.message.starts_with("generation sequence step 2"));
}

#[test]
fn mixed_case_ids_are_valid() {
    let mut c = base();
    c["tags"][0]["id"] = json!("tag.lowFantasy");
    c["tables"][0]["results"][0]["tags"] = json!(["tag.lowFantasy"]);
    assert!(validate(&content(c)).issues.is_empty());
}

#[test]
fn sequence_steps_may_be_bare_ids_or_labelled() {
    let mut c = base();
    c["sequence"] = json!(["table.a", { "table": "table.a", "name": "Again" }]);
    let set = content(c);
    assert!(validate(&set).issues.is_empty());
    assert_eq!(set.sequence[1].name.as_deref(), Some("Again"));
    let json = serde_json::to_value(&set).unwrap();
    assert_eq!(json["sequence"][0], "table.a");
    assert_eq!(json["sequence"][1]["name"], "Again");
}

// ----- subordinate generator references ------------------------------------------

#[test]
fn unresolved_generator_references_are_valid_but_must_be_well_formed() {
    let mut c = base();
    c["tables"][0]["results"][0]["generator"] = json!("someGenerators.low");
    let parsed = content(c.clone());
    assert_eq!(
        parsed.tables[0].results[0]
            .generator
            .as_ref()
            .unwrap()
            .as_str(),
        "someGenerators.low"
    );
    assert!(parsed.tables[0].results[1].generator.is_none());
    assert!(validate(&parsed).issues.is_empty());

    c["tables"][0]["results"][0]["generator"] = json!("not valid");
    let report = validate(&content(c));
    assert_has(
        &report,
        Severity::Error,
        IssueKind::InvalidId {
            id: "not valid".into(),
        },
    );
}

// ----- result values and sub-stages ------------------------------------------------

#[test]
fn result_values_must_be_positive_and_complete() {
    let mut c = base();
    for (i, v) in [1, 2, 3].iter().enumerate() {
        c["tables"][0]["results"][i]["value"] = json!(v);
    }
    let parsed = content(c.clone());
    assert_eq!(parsed.tables[0].results[2].value, Some(3));
    assert!(validate(&parsed).issues.is_empty());

    let mut zero = c.clone();
    zero["tables"][0]["results"][0]["value"] = json!(0);
    assert_has(
        &validate(&content(zero)),
        Severity::Error,
        IssueKind::InvalidValue { value: 0 },
    );
    let mut negative = c.clone();
    negative["tables"][0]["results"][1]["value"] = json!(-4);
    assert_has(
        &validate(&content(negative)),
        Severity::Error,
        IssueKind::InvalidValue { value: -4 },
    );
    let mut partial = c;
    partial["tables"][0]["results"][2]
        .as_object_mut()
        .unwrap()
        .remove("value");
    let report = validate(&content(partial));
    assert_has(&report, Severity::Error, IssueKind::MissingValue);
    assert_eq!(
        report.issues[0].location.result.as_ref().unwrap().as_str(),
        "high"
    );
}

#[test]
fn a_sub_stage_needs_a_stage_before_it() {
    let mut c = base();
    c["sequence"] = json!([{ "table": "table.a", "name": "A", "sub_stage": true }]);
    assert_has(
        &validate(&content(c.clone())),
        Severity::Error,
        IssueKind::SubStageWithoutParent,
    );
    c["tables"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "id": "table.b", "name": "B", "dice": "1d4",
                      "results": [{ "id": "x", "name": "X", "range": [1, 4] }] }));
    c["sequence"] = json!(["table.b", { "table": "table.a", "name": "A", "sub_stage": true }]);
    let parsed = content(c);
    assert!(parsed.sequence[1].sub_stage && !parsed.sequence[0].sub_stage);
    assert!(validate(&parsed).issues.is_empty());
    // Round trip keeps the flag.
    let again = ContentSet::from_json(&parsed.to_json_pretty()).unwrap();
    assert_eq!(again.sequence, parsed.sequence);
}

#[test]
fn the_authority_figures_stage_needs_its_tables() {
    let mut c = base();
    c["sequence"] = json!(["table.a", { "authority_figures": true, "name": "Authority Figures" }]);
    let report = validate(&content(c));
    for table in settlement_core::figures::required_tables() {
        assert_has(&report, Severity::Error, IssueKind::MissingTable { table });
    }
    let bundled = settlement_core::bundled::bundled_content().unwrap();
    assert!(!validate(&bundled).has_errors());
}

#[test]
fn naming_part_labels_must_match_the_components() {
    let mut c = base();
    c["naming_schemes"] = json!([{
        "id": "naming.person.x", "name": "X names", "subject": "person",
        "when": { "has_tag": "tag.x" }, "method_table": "table.a",
        "methods": [
            { "result": "low", "components": ["table.a"], "join": { "style": "compound" },
              "parts": [{ "id": "given", "label": "Given name" }, { "id": "family", "label": "Family name" }] },
            { "result": "mid", "components": ["table.a"], "join": { "style": "compound" } },
            { "result": "high", "components": ["table.a"], "join": { "style": "compound" },
              "parts": [{ "id": "given", "label": "Given name" }] }
        ]
    }]);
    let report = validate(&content(c));
    assert_has(
        &report,
        Severity::Error,
        IssueKind::NamingPartsLength {
            components: 1,
            parts: 2,
        },
    );
    assert_eq!(report.errors().count(), 1, "{report}");
}
