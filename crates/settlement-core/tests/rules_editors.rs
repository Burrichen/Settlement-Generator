//! Tags, settings and modifiers as the no-code editors use them: setting
//! kinds and their conditions, Bare Minimum Buildings as an ordinary
//! setting, the weighting / enable / subflow effects, plain-English
//! explanations, dependency inspection and safe deletion. Content here is
//! generic test data, not settlement content.

use std::sync::Arc;

use serde_json::{Value, json};
use settlement_core::content::{ContentSet, EffectDef, Tag};
use settlement_core::db::Database;
use settlement_core::dice::ScriptedFaces;
use settlement_core::engine::{GenerationOptions, Generator};
use settlement_core::framework::describe::SettingType;
use settlement_core::framework::{
    BARE_MINIMUM_BUILDINGS, SettingKind, SettingValue, SettingValues, StepOutcome, StepRecord,
};
use settlement_core::ids::{SettingId, TagId};
use settlement_core::service::AppService;
use settlement_core::session::{GenerationMode, GenerationSession, ViewOptions};
use settlement_core::validation::{IssueKind, Severity, validate};

fn content(extra: Value) -> ContentSet {
    let mut base = json!({
        "tables": [
            { "id": "table.pair", "name": "Pair", "dice": "1d4", "results": [
                { "id": "a", "name": "A", "range": [1, 2] },
                { "id": "b", "name": "B", "range": [3, 4], "tags": ["tag.mark"] } ] },
            { "id": "table.hidden", "name": "Hidden", "dice": "1d4", "results": [
                { "id": "seen", "name": "Seen", "range": [1, 2] },
                { "id": "secret", "name": "Secret", "range": [3, 4], "enabled": false } ] },
            { "id": "table.colour", "name": "Colour", "dice": "1d6", "results": [
                { "id": "red", "name": "Red", "range": [1, 3] },
                { "id": "blue", "name": "Blue", "range": [4, 6] } ] }
        ],
        "tags": [
            { "id": "tag.mark", "name": "Mark" },
            { "id": "tag.mod", "name": "Mod" }
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

fn with_effects(effects: Value) -> ContentSet {
    let mut c = content(json!({}));
    c.tags[1].effects = serde_json::from_value(effects).unwrap();
    let report = validate(&c);
    assert!(!report.has_errors(), "{report}");
    c
}

fn dice(values: &[u32]) -> ScriptedFaces {
    ScriptedFaces::new(values.iter().copied())
}

/// Rolls `table` with Mod active (first rolling a table that gives it).
fn roll_with_mod(c: &ContentSet, table: &str, faces: &[u32]) -> String {
    let mut c = c.clone();
    c.tables.push(
        serde_json::from_value(json!({ "id": "table.start", "name": "Start", "dice": "1d4",
            "results": [{ "id": "go", "name": "Go", "range": [1, 4], "tags": ["tag.mod"] }] }))
        .unwrap(),
    );
    let mut f = dice(
        &[1].into_iter()
            .chain(faces.iter().copied())
            .collect::<Vec<_>>(),
    );
    let log = Generator::new(&c)
        .unwrap()
        .run(&["table.start".into(), table.into()], &mut f)
        .unwrap();
    log.record_for(&table.into()).unwrap().result.name.clone()
}

fn errors(c: &ContentSet) -> Vec<IssueKind> {
    validate(c)
        .issues
        .into_iter()
        .filter(|i| i.severity == Severity::Error)
        .map(|i| i.kind)
        .collect()
}

fn warnings(c: &ContentSet) -> Vec<IssueKind> {
    validate(c)
        .issues
        .into_iter()
        .filter(|i| i.severity == Severity::Warning)
        .map(|i| i.kind)
        .collect()
}

// ----- effects ---------------------------------------------------------------------

#[test]
fn weighting_lays_the_table_out_again_on_its_own_dice() {
    // B three times as likely: A gets 1 face of the d4, B gets 3.
    let c = with_effects(json!([
        { "kind": "weight", "table": "table.pair", "result": "b", "percent": 300 }
    ]));
    assert_eq!(roll_with_mod(&c, "table.pair", &[1]), "A");
    assert_eq!(roll_with_mod(&c, "table.pair", &[2]), "B");
    // Without the tag, a 2 is still A.
    let mut f = dice(&[2]);
    let log = Generator::new(&c)
        .unwrap()
        .run(&["table.pair".into()], &mut f)
        .unwrap();
    assert_eq!(
        log.record_for(&"table.pair".into()).unwrap().result.name,
        "A"
    );
}

#[test]
fn a_tag_can_enable_a_disabled_result() {
    let c = with_effects(json!([
        { "kind": "enable_results", "table": "table.hidden", "results": ["secret"] }
    ]));
    assert_eq!(roll_with_mod(&c, "table.hidden", &[3]), "Secret");
    // Without it, Secret's faces are rolled again.
    let mut f = dice(&[3, 1]);
    let log = Generator::new(&c)
        .unwrap()
        .run(&["table.hidden".into()], &mut f)
        .unwrap();
    assert_eq!(
        log.record_for(&"table.hidden".into()).unwrap().result.name,
        "Seen"
    );
}

#[test]
fn new_effects_are_validated() {
    let bad = |effects: Value| {
        let mut c = content(json!({}));
        c.tags[1].effects = serde_json::from_value(effects).unwrap();
        errors(&c)
    };
    assert!(
        bad(json!([{ "kind": "weight", "table": "table.pair", "result": "b", "percent": 0 }]))
            .contains(&IssueKind::InvalidWeight { percent: 0 })
    );
    assert!(
        bad(json!([{ "kind": "enable_results", "table": "table.hidden", "results": ["ghost"] }]))
            .iter()
            .any(|k| matches!(k, IssueKind::BrokenResultReference { .. }))
    );
    assert!(
        bad(json!([{ "kind": "run_flow", "flow": "flow.none" }])).contains(
            &IssueKind::MissingFlow {
                flow: "flow.none".into()
            }
        )
    );
}

fn flow_session(extra: Value, steps: Value) -> GenerationSession {
    let mut extra = extra;
    extra["start"] = json!("flow.main");
    let mut flows = vec![json!({ "id": "flow.main", "name": "Main", "steps": steps })];
    flows.extend(extra["flows"].as_array().cloned().unwrap_or_default());
    extra["flows"] = Value::Array(flows);
    let c = content(extra);
    let report = validate(&c);
    assert!(!report.has_errors(), "{report}");
    GenerationSession::new(
        Arc::new(c),
        GenerationOptions::default(),
        GenerationMode::OneTable,
    )
    .unwrap()
}

fn record(s: &GenerationSession, stage: usize) -> StepRecord {
    s.view(stage, ViewOptions::default())
        .unwrap()
        .flow
        .and_then(|f| f.record)
        .expect("a flow record")
}

#[test]
fn a_tag_triggers_a_subflow_after_the_step_that_gave_it() {
    let extra = json!({
        "tables": [{ "id": "table.omen", "name": "Omen", "dice": "1d4", "results": [
            { "id": "none", "name": "None", "range": [1, 2] },
            { "id": "dark", "name": "Dark", "range": [3, 4], "tags": ["tag.go"] } ] }],
        "tags": [{ "id": "tag.go", "name": "Go", "effects": [{ "kind": "run_flow", "flow": "flow.extra" }] }],
        "flows": [{ "id": "flow.extra", "name": "Extra", "steps": [
            { "id": "colour", "kind": "roll", "table": "table.colour" } ] }]
    });
    let mut s = flow_session(
        extra,
        json!([{ "id": "twice", "kind": "repeat", "count": { "times": 2 }, "steps": [
            { "id": "omen", "kind": "roll", "table": "table.omen" } ] }]),
    );
    // Dark gives Go, which runs Extra right after; None gives nothing.
    s.roll(0, &mut dice(&[3, 5, 1])).unwrap();
    let r = record(&s, 0);
    let names: Vec<String> = r
        .table_rolls()
        .iter()
        .map(|t| t.result.name.clone())
        .collect();
    assert_eq!(names, ["Dark", "Blue", "None"]);
    let triggered = &r.children()[1];
    assert_eq!(triggered.label, "Extra (triggered by Go)");
    assert!(matches!(triggered.outcome, StepOutcome::Subflow { .. }));
}

#[test]
fn a_subflow_trigger_on_a_top_level_table_stage_is_warned() {
    let mut c = content(json!({
        "start": "flow.main",
        "flows": [
            { "id": "flow.main", "name": "Main", "steps": [
                { "id": "pair", "kind": "roll", "table": "table.pair", "name": "Pair" } ] },
            { "id": "flow.extra", "name": "Extra", "steps": [] }
        ]
    }));
    c.tags[0].effects = vec![EffectDef::active(
        settlement_core::content::Effect::RunFlow {
            flow: "flow.extra".into(),
        },
    )];
    assert!(warnings(&c).contains(&IssueKind::TriggerOutsideFlow {
        stage: "Pair".into()
    }));
}

// ----- settings --------------------------------------------------------------------

fn settings_content() -> Value {
    json!({ "settings": [
        { "id": "setting.size", "name": "Size", "kind": "number", "default": 3, "min": 1, "max": 10 },
        { "id": "setting.biomes", "name": "Biomes", "kind": "multi_select",
          "options": [{ "id": "coast", "name": "Coast" }, { "id": "forest", "name": "Forest" }],
          "default": ["forest"] },
        { "id": "setting.motto", "name": "Motto", "kind": "text", "default": "Onward" },
        { "id": "setting.later", "name": "Later", "kind": "toggle", "default": false, "status": "planned" }
    ] })
}

#[test]
fn every_kind_of_setting_drives_a_flow() {
    let steps = json!([
        { "id": "size", "kind": "read_setting", "setting": "setting.size" },
        { "id": "by", "kind": "branch", "cases": [
            { "when": { "all": [
                { "setting_between": { "setting": "setting.size", "min": 5 } },
                { "setting_has": { "setting": "setting.biomes", "value": "coast" } },
                { "setting_is": { "setting": "setting.motto", "value": "Onward" } },
                { "not": { "setting_on": "setting.later" } } ] },
              "steps": [{ "id": "yes", "kind": "roll", "table": "table.colour" }] } ] }
    ]);
    let values: SettingValues = [
        (SettingId::new("setting.size"), SettingValue::Number(7)),
        (
            SettingId::new("setting.biomes"),
            SettingValue::Many(vec!["coast".into()]),
        ),
        // A planned setting is never offered: its default is used.
        (SettingId::new("setting.later"), SettingValue::Toggle(true)),
    ]
    .into_iter()
    .collect();
    let mut s = flow_session(settings_content(), steps.clone()).with_settings(values);
    s.roll(0, &mut dice(&[])).unwrap();
    s.roll(1, &mut dice(&[2])).unwrap();
    let StepOutcome::ReadSetting { value, .. } = record(&s, 0).outcome else {
        panic!()
    };
    assert_eq!(value, SettingValue::Number(7));
    assert_eq!(record(&s, 1).table_rolls()[0].result.name, "Red");
    // With the defaults (size 3, forest only) the case does not hold.
    let mut s = flow_session(settings_content(), steps);
    s.roll(1, &mut dice(&[])).unwrap();
    assert!(record(&s, 1).table_rolls().is_empty());
}

#[test]
fn setting_definitions_and_tests_are_validated() {
    let mut c = content(settings_content());
    c.settings[0].kind = SettingKind::Number {
        default: 0,
        min: Some(5),
        max: Some(2),
    };
    c.settings[1].kind = SettingKind::MultiSelect {
        options: Vec::new(),
        default: Vec::new(),
    };
    let found = errors(&c);
    assert!(found.contains(&IssueKind::InvalidSettingRange), "{found:?}");
    assert!(
        found.contains(&IssueKind::SettingWithoutOptions),
        "{found:?}"
    );

    let mut c = content(settings_content());
    c.tables[0].eligibility = serde_json::from_value(json!({ "setting_on": "setting.size" })).ok();
    assert!(errors(&c).iter().any(|k| matches!(
        k,
        IssueKind::SettingKindMismatch { setting, .. } if setting == "setting.size"
    )));
}

#[test]
fn values_a_setting_cannot_take_are_refused() {
    let c = content(settings_content());
    let size = c.setting(&"setting.size".into()).unwrap();
    assert!(size.accepts(&SettingValue::Number(10)));
    assert!(!size.accepts(&SettingValue::Number(11)));
    assert!(!size.accepts(&SettingValue::Toggle(true)));
    let biomes = c.setting(&"setting.biomes".into()).unwrap();
    assert!(biomes.accepts(&SettingValue::Many(vec!["coast".into(), "forest".into()])));
    assert!(!biomes.accepts(&SettingValue::Many(vec!["desert".into()])));
    assert_eq!(
        biomes.describe_value(&SettingValue::Many(vec!["coast".into(), "forest".into()])),
        "Coast, Forest"
    );
    let motto = c.setting(&"setting.motto".into()).unwrap();
    assert!(!motto.accepts(&SettingValue::Choice("x".repeat(201))));
}

// ----- Bare Minimum Buildings ---------------------------------------------------------

#[test]
fn bare_minimum_buildings_is_an_ordinary_setting_with_its_old_choice_kept() {
    let mut service = AppService::new(Database::open_in_memory().unwrap());
    // The application setting chosen before it became a content setting.
    let mut settings = service.settings().unwrap();
    settings.bare_minimum_buildings = true;
    service.save_settings(&settings).unwrap();
    let view = service.content_settings().unwrap();
    let bmb = view
        .iter()
        .find(|v| v.setting.id.as_str() == BARE_MINIMUM_BUILDINGS)
        .unwrap();
    assert_eq!(bmb.value, SettingValue::Toggle(true));
    assert!(
        service
            .start_generation()
            .unwrap()
            .session
            .options()
            .bare_minimum_buildings
    );

    // Changing it through the generic setting reaches generation too.
    service
        .set_content_setting(&BARE_MINIMUM_BUILDINGS.into(), SettingValue::Toggle(false))
        .unwrap();
    assert!(!service.settings().unwrap().bare_minimum_buildings);
    assert!(
        !service
            .start_generation()
            .unwrap()
            .session
            .options()
            .bare_minimum_buildings
    );

    // The old ID still names it, and what uses it is listed.
    let detail = service
        .setting_detail(&"app.bareMinimumBuildings".into())
        .unwrap();
    assert_eq!(detail.setting.name, "Bare Minimum Buildings");
    assert!(
        detail
            .usages
            .iter()
            .any(|u| u.description.contains("Notable Locations")),
        "{:?}",
        detail.usages
    );
    // It cannot be deleted while the pipeline uses it.
    let err = service
        .delete_setting(&BARE_MINIMUM_BUILDINGS.into())
        .unwrap_err();
    assert!(err.contains("still used"), "{err}");
}

// ----- the editors' service ----------------------------------------------------------

#[test]
fn settings_are_created_checked_saved_and_deleted() {
    let mut service = AppService::new(Database::open_in_memory().unwrap());
    let mut setting = AppService::new_setting(SettingType::MultiSelect);
    setting.name = "Biomes".into();
    assert!(service.check_setting(&setting).unwrap().issues.is_empty());
    let saved = service.save_setting(setting.clone()).unwrap();
    assert_eq!(saved.setting.name, "Biomes");
    assert!(saved.usages.is_empty());
    // A broken draft is reported without being stored.
    let mut broken = setting.clone();
    broken.kind = SettingKind::MultiSelect {
        options: Vec::new(),
        default: Vec::new(),
    };
    let check = service.check_setting(&broken).unwrap();
    assert!(
        check
            .issues
            .iter()
            .any(|i| i.kind == IssueKind::SettingWithoutOptions)
    );
    assert!(service.delete_setting(&setting.id).is_ok());
    assert!(service.setting_detail(&setting.id).is_err());
}

#[test]
fn tags_explain_their_effects_and_cannot_be_deleted_while_used() {
    let mut service = AppService::new(Database::open_in_memory().unwrap());
    let mut tag: Tag = AppService::new_tag();
    tag.name = "Lucky".into();
    let insight = service.explain_tag(&tag).unwrap();
    assert!(insight.informational);
    tag.effects = serde_json::from_value(json!([
        { "kind": "plus", "table": "table.crime", "amount": 1 },
        { "kind": "cap_at_highest", "table": "table.crime" },
        { "kind": "shift", "table": "table.crime", "steps": 1, "status": "planned" }
    ]))
    .unwrap();
    let insight = service.explain_tag(&tag).unwrap();
    assert!(!insight.informational);
    assert!(
        insight.effects[0]
            .summary
            .starts_with("While Lucky is active")
    );
    assert!(insight.effects[2].planned);
    assert_eq!(insight.combined.len(), 1, "two active modifiers on Crime");
    assert_eq!(insight.combined[0].steps.first().unwrap(), "Roll 1d12");

    // A bundled tag that results give cannot be deleted.
    let used = service
        .overview()
        .unwrap()
        .tags
        .into_iter()
        .find(|t| {
            !service
                .tag_detail(&TagId::new(t.id.as_str()))
                .unwrap()
                .usages
                .is_empty()
        })
        .expect("a tag in use");
    let err = service.delete_tag(&used.id).unwrap_err();
    assert!(err.contains("still used"), "{err}");
}

#[test]
fn tag_usages_include_flows() {
    let mut c = content(json!({
        "start": "flow.main",
        "flows": [{ "id": "flow.main", "name": "Main", "steps": [
            { "id": "add", "kind": "add_tag", "tag": "tag.mod", "name": "Bless" },
            { "id": "drop", "kind": "remove_tag", "tag": "tag.mod" },
            { "id": "pick", "kind": "branch", "cases": [
                { "when": { "has_tag": "tag.mod" }, "steps": [] } ] } ] }]
    }));
    c.tags.truncate(2);
    let words: Vec<String> = settlement_core::dependencies::tag_usages(&c, &"tag.mod".into())
        .into_iter()
        .map(|u| u.description)
        .collect();
    assert_eq!(
        words,
        [
            "Checked by Pipeline stage “pick” › case 1",
            "Added by Pipeline stage “Bless”",
            "Removed by Pipeline stage “drop”",
        ]
    );
}
