//! Bundled defaults + user overrides. Fixtures are neutral test data.

use serde_json::json;
use settlement_core::content::{ContentSet, Effect, EffectDef, FaceRange, Table, TableResult, Tag};
use settlement_core::dice::ScriptedFaces;
use settlement_core::engine::GenerationError;
use settlement_core::ids::{ResultId, TableId, TagId};
use settlement_core::layers::{
    ContentLayers, EditError, Origin, TableOverride, UserOverrides, merge, new_user_id,
};
use settlement_core::validation::{IssueKind, Severity};

fn content(value: serde_json::Value) -> ContentSet {
    serde_json::from_value(value).expect("fixture deserialises")
}

fn t(id: &str) -> TableId {
    id.into()
}

fn r(id: &str) -> ResultId {
    id.into()
}

fn g(id: &str) -> TagId {
    id.into()
}

/// "Version 1" of the bundled defaults.
fn v1_json() -> serde_json::Value {
    json!({
        "tables": [
            { "id": "t.main", "name": "Main", "description": "Main table", "dice": "1d6",
              "results": [
                  { "id": "a", "name": "A", "range": [1, 3], "tags": ["g.one"] },
                  { "id": "b", "name": "B", "range": [4, 6] }
              ]},
            { "id": "t.side", "name": "Side", "dice": "1d4",
              "results": [ { "id": "x", "name": "X", "range": [1, 4] } ] }
        ],
        "tags": [
            { "id": "g.one", "name": "One",
              "effects": [{ "kind": "route_to", "table": "t.side" }] },
            { "id": "g.two", "name": "Two" }
        ]
    })
}

fn v1() -> ContentSet {
    content(v1_json())
}

fn layers() -> ContentLayers {
    ContentLayers::new(v1(), UserOverrides::default())
}

fn table<'a>(c: &'a ContentSet, id: &str) -> &'a Table {
    c.table(&t(id))
        .unwrap_or_else(|| panic!("table {id} missing"))
}

fn user_result(id: &str, range: Option<FaceRange>) -> TableResult {
    TableResult {
        id: id.into(),
        name: format!("Result {id}"),
        description: String::new(),
        range,
        tags: vec![],
        eligibility: None,
        enabled: true,
        generator: None,
        value: None,
        unique: false,
        fallback: None,
        label_variants: Vec::new(),
        approved_names: Default::default(),
    }
}

fn assert_clean(l: &ContentLayers) {
    let merged = l.merged();
    assert!(merged.report.issues.is_empty(), "{}", merged.report);
}

// ----- basic merging ---------------------------------------------------------

#[test]
fn without_overrides_the_effective_content_is_the_bundled_content() {
    let l = layers();
    let merged = l.merged();
    assert_eq!(merged.content, v1());
    assert!(merged.report.issues.is_empty(), "{}", merged.report);
    assert!(
        merged
            .origins
            .tables
            .values()
            .all(|o| *o == Origin::Bundled)
    );
    assert!(merged.origins.tags.values().all(|o| *o == Origin::Bundled));
    assert_eq!(
        merged.origins.results[&t("t.main")][&r("a")],
        Origin::Bundled
    );
}

// ----- renaming --------------------------------------------------------------

#[test]
fn renaming_built_in_tables_and_tags_keeps_references_working() {
    let mut l = layers();
    l.rename_table(&t("t.side"), "Renamed side").unwrap();
    l.rename_tag(&g("g.one"), "Renamed one").unwrap();
    l.rename_result(&t("t.main"), &r("a"), "Renamed A").unwrap();
    assert_clean(&l);

    let merged = l.merged();
    assert_eq!(table(&merged.content, "t.side").name, "Renamed side");
    assert_eq!(merged.origins.tables[&t("t.side")], Origin::Modified);
    assert_eq!(merged.origins.tags[&g("g.one")], Origin::Modified);

    // The tag still routes to the renamed table by ID.
    let log = merged
        .generator()
        .unwrap()
        .run(&[t("t.main")], &mut ScriptedFaces::new([1, 2]))
        .unwrap();
    let rolls: Vec<_> = log.table_rolls().collect();
    assert_eq!(rolls[0].result.name, "Renamed A");
    assert_eq!(rolls[0].tags[0].name, "Renamed one");
    assert_eq!(rolls[1].table.name, "Renamed side");

    // Bundled resources are untouched.
    assert_eq!(l.bundled(), &v1());
}

#[test]
fn setting_a_field_back_to_its_default_removes_the_override() {
    let mut l = layers();
    l.rename_table(&t("t.main"), "Other").unwrap();
    l.rename_table(&t("t.main"), "Main").unwrap();
    l.rename_tag(&g("g.two"), "X").unwrap();
    l.rename_tag(&g("g.two"), "Two").unwrap();
    l.set_result_enabled(&t("t.main"), &r("b"), false).unwrap();
    l.set_result_enabled(&t("t.main"), &r("b"), true).unwrap();
    assert!(l.overrides().is_empty(), "{:?}", l.overrides());
    assert_eq!(l.merged().origins.tables[&t("t.main")], Origin::Bundled);
}

// ----- ranges and dice -------------------------------------------------------

#[test]
fn altering_dice_and_ranges_of_built_in_tables() {
    let mut l = layers();
    l.set_table_dice(&t("t.main"), "1d8").unwrap();
    l.set_result_range(&t("t.main"), &r("b"), Some(FaceRange::new(4, 8)))
        .unwrap();
    assert_clean(&l);
    let merged = l.merged();
    let main = table(&merged.content, "t.main");
    assert_eq!(main.dice.as_deref(), Some("1d8"));
    assert_eq!(
        main.result(&r("b")).unwrap().range,
        Some(FaceRange::new(4, 8))
    );
    assert_eq!(
        merged.origins.results[&t("t.main")][&r("b")],
        Origin::Modified
    );
    assert_eq!(
        merged.origins.results[&t("t.main")][&r("a")],
        Origin::Bundled
    );

    let log = merged
        .generator()
        .unwrap()
        .run(&[t("t.main")], &mut ScriptedFaces::new([8]))
        .unwrap();
    assert_eq!(log.table_rolls().next().unwrap().result.id, r("b"));
}

#[test]
fn invalid_customisations_produce_validation_errors_not_crashes() {
    let mut l = layers();
    // Changing the dice without changing the ranges leaves 7-8 uncovered.
    l.set_table_dice(&t("t.main"), "1d8").unwrap();
    let merged = l.merged();
    assert!(merged.report.has_errors());
    assert!(merged.report.errors().any(|i| matches!(
        &i.kind,
        IssueKind::UncoveredOutcomes { outcomes, .. } if outcomes == &[FaceRange::new(7, 8)]
    )));
    let Err(GenerationError::InvalidContent(report)) = merged.generator() else {
        panic!("generation must refuse invalid content");
    };
    assert!(
        report
            .to_string()
            .contains("table `t.main`: no result for 7-8 on 1d8")
    );

    // Nonsense dice are stored, then reported clearly.
    l.set_table_dice(&t("t.main"), "banana").unwrap();
    let merged = l.merged();
    assert!(
        merged
            .report
            .errors()
            .any(|i| matches!(i.kind, IssueKind::InvalidDice { .. }))
    );
    assert!(merged.generator().is_err());
}

// ----- tag effects -----------------------------------------------------------

#[test]
fn changing_tag_effects() {
    let mut l = layers();
    l.set_tag_effects(
        &g("g.one"),
        vec![EffectDef::active(Effect::AddTag { tag: g("g.two") })],
    )
    .unwrap();
    assert_clean(&l);
    let merged = l.merged();
    let log = merged
        .generator()
        .unwrap()
        .run(&[t("t.main")], &mut ScriptedFaces::new([2]))
        .unwrap();
    // No longer routes to t.side; adds g.two instead.
    assert_eq!(log.table_rolls().count(), 1);
    assert!(log.has_tag(&g("g.two")));
}

// ----- adding ----------------------------------------------------------------

#[test]
fn adding_tables_tags_and_results() {
    let mut l = layers();
    let tag_id = new_user_id("tag");
    l.add_tag(Tag {
        id: tag_id.as_str().into(),
        name: "Mine".into(),
        description: String::new(),
        effects: vec![],
        enabled: true,
    })
    .unwrap();

    let table_id = new_user_id("table");
    let mut res = user_result("user.r1", Some(FaceRange::new(1, 4)));
    res.tags = vec![tag_id.as_str().into()];
    l.add_table(Table {
        id: table_id.as_str().into(),
        name: "My table".into(),
        description: String::new(),
        dice: Some("1d4".into()),
        active: true,
        eligibility: None,
        results: vec![res],
        variants: vec![],
        selection: Default::default(),
        status: Default::default(),
        status_note: String::new(),
        ordered: false,
        follow_ups: vec![],
    })
    .unwrap();

    // A result added to a built-in table (with ranges adjusted to fit).
    l.set_table_dice(&t("t.side"), "1d6").unwrap();
    l.add_result(
        &t("t.side"),
        user_result("user.extra", Some(FaceRange::new(5, 6))),
    )
    .unwrap();
    assert_clean(&l);

    let merged = l.merged();
    assert_eq!(
        merged.origins.tables[&table_id.as_str().into()],
        Origin::Custom
    );
    assert_eq!(merged.origins.tags[&tag_id.as_str().into()], Origin::Custom);
    assert_eq!(
        merged.origins.results[&t("t.side")][&r("user.extra")],
        Origin::Custom
    );
    let side = table(&merged.content, "t.side");
    assert_eq!(side.results.last().unwrap().id, r("user.extra"));

    // Custom tables can be edited with the same operations.
    l.rename_table(&table_id.as_str().into(), "Renamed mine")
        .unwrap();
    assert_eq!(
        l.merged()
            .content
            .table(&table_id.as_str().into())
            .unwrap()
            .name,
        "Renamed mine"
    );
}

#[test]
fn user_items_must_use_the_user_namespace_and_unique_ids() {
    let mut l = layers();
    let tag = Tag {
        id: "g.mine".into(),
        name: "Mine".into(),
        description: String::new(),
        effects: vec![],
        enabled: true,
    };
    assert_eq!(l.add_tag(tag), Err(EditError::NotUserId("g.mine".into())));
    assert_eq!(
        l.add_result(&t("t.main"), user_result("plain", None)),
        Err(EditError::NotUserId("plain".into()))
    );
    l.add_result(&t("t.main"), user_result("user.dup", None))
        .unwrap();
    assert_eq!(
        l.add_result(&t("t.main"), user_result("user.dup", None)),
        Err(EditError::IdInUse("user.dup".into()))
    );
}

#[test]
fn hand_edited_overrides_with_bad_ids_are_reported() {
    let overrides: UserOverrides = serde_json::from_value(json!({
        "tables": {
            "t.main": { "action": "custom", "id": "t.main", "name": "Clash", "dice": "1d4",
                        "results": [{ "id": "r", "name": "R", "range": [1, 4] }] },
            "not-user": { "action": "custom", "id": "not-user", "name": "N", "dice": "1d4",
                          "results": [{ "id": "r", "name": "R", "range": [1, 4] }] }
        }
    }))
    .unwrap();
    let merged = merge(&v1(), &overrides);
    let kinds: Vec<_> = merged.report.errors().map(|i| &i.kind).collect();
    assert!(
        kinds
            .iter()
            .any(|k| matches!(k, IssueKind::DuplicateId { id, .. } if id == "t.main"))
    );
    assert!(
        kinds
            .iter()
            .any(|k| matches!(k, IssueKind::NotUserId { id } if id == "not-user"))
    );
    // The default is kept when a custom table claims its ID.
    assert_eq!(table(&merged.content, "t.main").name, "Main");
}

// ----- deleting and disabling --------------------------------------------------

#[test]
fn deleting_built_in_items_is_recorded_in_the_override_layer_only() {
    let mut l = layers();
    l.delete_tag(&g("g.two")).unwrap();
    l.delete_result(&t("t.main"), &r("b")).unwrap();
    l.set_result_range(&t("t.main"), &r("a"), Some(FaceRange::new(1, 6)))
        .unwrap();
    assert_clean(&l);

    let merged = l.merged();
    assert!(merged.content.tag(&g("g.two")).is_none());
    assert!(table(&merged.content, "t.main").result(&r("b")).is_none());
    assert!(merged.origins.deleted_tags.contains(&g("g.two")));
    assert!(merged.origins.deleted_results[&t("t.main")].contains(&r("b")));
    // Bundled content still has everything.
    assert_eq!(l.bundled(), &v1());

    // Editing a deleted item is refused until it is restored.
    assert_eq!(
        l.rename_tag(&g("g.two"), "x"),
        Err(EditError::Deleted("g.two".into()))
    );
}

#[test]
fn deleting_a_referenced_built_in_gives_a_clear_validation_error() {
    let mut l = layers();
    l.delete_table(&t("t.side")).unwrap();
    let merged = l.merged();
    assert!(merged.origins.deleted_tables.contains(&t("t.side")));
    let issue = merged.report.errors().next().expect("an error");
    assert_eq!(issue.kind, IssueKind::MissingTable { table: t("t.side") });
    assert_eq!(
        issue.message,
        "tag `g.one` › effect #1: references missing table `t.side`"
    );
    assert!(merged.generator().is_err());
}

#[test]
fn disabling_built_in_items() {
    let mut l = layers();
    l.set_tag_enabled(&g("g.one"), false).unwrap();
    l.set_table_active(&t("t.side"), false).unwrap();
    l.set_result_enabled(&t("t.main"), &r("a"), false).unwrap();
    assert_clean(&l);
    let merged = l.merged();
    assert!(!merged.content.tag(&g("g.one")).unwrap().enabled);
    let log = merged
        .generator()
        .unwrap()
        .run(&[t("t.main"), t("t.side")], &mut ScriptedFaces::new([1, 5]))
        .unwrap();
    // "a" is disabled → rerolled to "b"; t.side is inactive → skipped.
    assert_eq!(log.table_rolls().count(), 1);
    assert_eq!(log.table_rolls().next().unwrap().result.id, r("b"));
}

#[test]
fn deleting_custom_items_removes_them() {
    let mut l = layers();
    let id: TableId = new_user_id("table").as_str().into();
    l.add_table(Table {
        id: id.clone(),
        name: "Mine".into(),
        description: String::new(),
        dice: Some("1d4".into()),
        active: true,
        eligibility: None,
        results: vec![
            user_result("user.a", Some(FaceRange::new(1, 2))),
            user_result("user.b", Some(FaceRange::new(3, 4))),
        ],
        variants: vec![],
        selection: Default::default(),
        status: Default::default(),
        status_note: String::new(),
        ordered: false,
        follow_ups: vec![],
    })
    .unwrap();
    l.delete_result(&id, &r("user.b")).unwrap();
    assert_eq!(l.merged().content.table(&id).unwrap().results.len(), 1);

    l.add_result(&t("t.main"), user_result("user.c", None))
        .unwrap();
    l.delete_result(&t("t.main"), &r("user.c")).unwrap();

    l.delete_table(&id).unwrap();
    assert!(l.overrides().is_empty(), "{:?}", l.overrides());
    assert_eq!(l.delete_table(&id), Err(EditError::UnknownTable(id)));
}

// ----- resetting -------------------------------------------------------------

#[test]
fn resetting_individual_items() {
    let mut l = layers();
    l.rename_table(&t("t.main"), "Changed").unwrap();
    l.rename_result(&t("t.main"), &r("a"), "Changed A").unwrap();
    l.rename_result(&t("t.main"), &r("b"), "Changed B").unwrap();
    l.delete_table(&t("t.side")).unwrap();
    l.set_tag_effects(&g("g.one"), vec![]).unwrap();

    l.reset_result(&t("t.main"), &r("a")).unwrap();
    let merged = l.merged();
    let main = table(&merged.content, "t.main");
    assert_eq!(main.result(&r("a")).unwrap().name, "A");
    assert_eq!(main.result(&r("b")).unwrap().name, "Changed B");
    assert_eq!(main.name, "Changed");

    l.reset_table(&t("t.main")).unwrap();
    l.reset_table(&t("t.side")).unwrap();
    l.reset_tag(&g("g.one")).unwrap();
    assert!(l.overrides().is_empty());
    assert_eq!(l.merged().content, v1());
}

#[test]
fn reset_of_a_deleted_result_restores_it() {
    let mut l = layers();
    l.delete_result(&t("t.main"), &r("b")).unwrap();
    l.reset_result(&t("t.main"), &r("b")).unwrap();
    assert!(l.overrides().is_empty());
}

#[test]
fn custom_items_have_no_default_to_reset_to() {
    let mut l = layers();
    let id = new_user_id("tag");
    l.add_tag(Tag {
        id: id.as_str().into(),
        name: "Mine".into(),
        description: String::new(),
        effects: vec![],
        enabled: true,
    })
    .unwrap();
    assert_eq!(
        l.reset_tag(&id.as_str().into()),
        Err(EditError::NotBundled(id))
    );
}

#[test]
fn resetting_everything() {
    let mut l = layers();
    l.rename_table(&t("t.main"), "Changed").unwrap();
    l.delete_tag(&g("g.two")).unwrap();
    l.add_result(&t("t.main"), user_result("user.x", None))
        .unwrap();
    l.reset_all();
    assert!(l.overrides().is_empty());
    assert_eq!(l.merged().content, v1());
}

// ----- application updates ---------------------------------------------------

/// "Version 2" of the defaults: several defaults changed, one added, one
/// removed.
fn v2() -> ContentSet {
    let mut c = v1_json();
    // t.main: name and description changed, a result added.
    c["tables"][0]["name"] = json!("Main v2");
    c["tables"][0]["description"] = json!("Main table, revised");
    c["tables"][0]["dice"] = json!("1d8");
    c["tables"][0]["results"][1]["range"] = json!([4, 6]);
    c["tables"][0]["results"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "id": "c", "name": "C", "range": [7, 8] }));
    // t.side removed; t.new added.
    c["tables"].as_array_mut().unwrap().remove(1);
    c["tables"].as_array_mut().unwrap().push(json!({
        "id": "t.new", "name": "New", "dice": "1d4",
        "results": [{ "id": "n", "name": "N", "range": [1, 4] }]
    }));
    // g.one no longer routes (t.side is gone); g.two renamed; g.three added.
    c["tags"][0]["effects"] = json!([]);
    c["tags"][1]["name"] = json!("Two v2");
    c["tags"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "id": "g.three", "name": "Three" }));
    content(c)
}

/// Customisations made while v1 was installed.
fn customised_on_v1() -> UserOverrides {
    let mut l = layers();
    l.rename_table(&t("t.main"), "My main").unwrap();
    l.rename_result(&t("t.main"), &r("a"), "My A").unwrap();
    l.set_result_tags(&t("t.main"), &r("a"), vec![]).unwrap();
    l.add_result(&t("t.main"), user_result("user.mine", None))
        .unwrap();
    l.rename_table(&t("t.side"), "My side").unwrap();
    l.rename_tag(&g("g.two"), "My two").unwrap();
    l.delete_tag(&g("g.one")).unwrap();
    l.into_overrides()
}

#[test]
fn updated_defaults_keep_existing_overrides_intact() {
    let overrides = customised_on_v1();
    let before = overrides.clone();
    let l = ContentLayers::new(v2(), overrides);
    let merged = l.merged();
    let main = table(&merged.content, "t.main");

    // The user's own changes win.
    assert_eq!(main.name, "My main");
    assert_eq!(main.result(&r("a")).unwrap().name, "My A");
    assert!(main.result(&r("user.mine")).is_some());
    assert_eq!(merged.content.tag(&g("g.two")).unwrap().name, "My two");
    assert!(merged.content.tag(&g("g.one")).is_none());

    // The override layer itself is untouched by the update.
    assert_eq!(l.overrides(), &before);
}

#[test]
fn updated_defaults_flow_through_where_not_customised() {
    let l = ContentLayers::new(v2(), customised_on_v1());
    let merged = l.merged();
    let main = table(&merged.content, "t.main");
    // Fields the user never touched take the new defaults.
    assert_eq!(main.description, "Main table, revised");
    assert_eq!(main.dice.as_deref(), Some("1d8"));
    // New results inside a customised table appear, before user additions.
    let ids: Vec<_> = main.results.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids, ["a", "b", "c", "user.mine"]);
    // New untouched defaults are available.
    assert!(merged.content.table(&t("t.new")).is_some());
    assert!(merged.content.tag(&g("g.three")).is_some());
    assert_eq!(merged.origins.tables[&t("t.new")], Origin::Bundled);
}

#[test]
fn updates_are_reported_as_warnings_not_errors() {
    let merged = ContentLayers::new(v2(), customised_on_v1()).merged();
    assert!(!merged.report.has_errors(), "{}", merged.report);
    let warnings: Vec<_> = merged
        .report
        .warnings()
        .map(|i| (i.location.to_string(), i.kind.clone()))
        .collect();
    assert!(warnings.contains(&("table `t.main`".into(), IssueKind::DefaultChanged)));
    assert!(warnings.contains(&("tag `g.two`".into(), IssueKind::DefaultChanged)));
    // The customised t.side was removed by the update; its override is ignored.
    assert!(warnings.contains(&("table `t.side`".into(), IssueKind::OverrideTargetMissing)));
    // Result "a" did not change between versions, so no warning for it.
    assert!(!warnings.iter().any(|(loc, _)| loc.contains("result `a`")));
    assert!(
        merged
            .report
            .issues
            .iter()
            .all(|i| i.severity == Severity::Warning)
    );
}

#[test]
fn reset_after_an_update_uses_the_installed_defaults() {
    let mut l = ContentLayers::new(v2(), customised_on_v1());
    l.reset_table(&t("t.main")).unwrap();
    l.reset_tag(&g("g.two")).unwrap();
    let merged = l.merged();
    assert_eq!(table(&merged.content, "t.main").name, "Main v2");
    assert_eq!(merged.content.tag(&g("g.two")).unwrap().name, "Two v2");

    l.reset_all();
    assert_eq!(l.merged().content, v2());
}

#[test]
fn reset_of_a_default_removed_by_an_update_is_refused() {
    let mut l = ContentLayers::new(v2(), customised_on_v1());
    assert_eq!(
        l.reset_table(&t("t.side")),
        Err(EditError::NotBundled("t.side".into()))
    );
}

// ----- serialisation ---------------------------------------------------------

#[test]
fn overrides_round_trip_through_json() {
    let mut l = layers();
    l.rename_table(&t("t.main"), "Changed").unwrap();
    l.set_table_eligibility(&t("t.main"), None).unwrap();
    l.set_result_range(&t("t.main"), &r("b"), None).unwrap();
    l.delete_result(&t("t.main"), &r("a")).unwrap();
    l.set_tag_effects(
        &g("g.two"),
        vec![EffectDef::planned(
            Effect::RouteTo {
                table: t("t.future"),
                variant: None,
            },
            "later",
        )],
    )
    .unwrap();
    let id = new_user_id("tag");
    l.add_tag(Tag {
        id: id.as_str().into(),
        name: "Mine".into(),
        description: String::new(),
        effects: vec![EffectDef::active(Effect::AddTag { tag: g("g.two") })],
        enabled: true,
    })
    .unwrap();

    let json = serde_json::to_string_pretty(l.overrides()).unwrap();
    let back: UserOverrides = serde_json::from_str(&json).unwrap();
    assert_eq!(&back, l.overrides());

    // An explicit "no range" survives as `null`, distinct from "unchanged".
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        value["tables"]["t.main"]["results"]["b"]["range"],
        serde_json::Value::Null
    );
    assert!(
        value["tables"]["t.main"]["results"]["b"]
            .as_object()
            .unwrap()
            .contains_key("range")
    );
    assert!(matches!(
        back.tables[&t("t.main")],
        TableOverride::Modify(_)
    ));
}

#[test]
fn user_ids_are_unique_and_valid() {
    let a = new_user_id("table");
    let b = new_user_id("table");
    assert_ne!(a, b);
    assert!(TableId::from(a.as_str()).is_user());
    assert!(TableId::from(a.as_str()).is_valid());
}

// ----- whole-item edits from the editors -----------------------------------------

use settlement_core::layers::CheckedEditError;

#[test]
fn putting_an_unchanged_table_or_tag_records_nothing() {
    let mut l = layers();
    let table = v1().table(&t("t.main")).unwrap().clone();
    l.put_table(table).unwrap();
    let tag = v1().tag(&g("g.one")).unwrap().clone();
    l.put_tag(tag).unwrap();
    assert!(l.overrides().is_empty(), "{:?}", l.overrides());
}

#[test]
fn putting_a_bundled_table_records_only_the_differences() {
    let mut l = layers();
    let mut table = v1().table(&t("t.main")).unwrap().clone();
    table.name = "Edited".into();
    table.dice = Some("1d8".into());
    table.results[1].range = Some(FaceRange::new(4, 8));
    table.results[1].name = "B edited".into();
    l.put_table(table.clone()).unwrap();
    assert_clean(&l);
    assert_eq!(table_of(&l, "t.main"), table);

    let Some(TableOverride::Modify(patch)) = l.overrides().tables.get(&t("t.main")) else {
        panic!("expected a patch");
    };
    assert!(patch.description.is_none(), "unchanged fields stay default");
    assert_eq!(patch.results.len(), 1, "only result b changed");
}

#[test]
fn a_bundled_scale_stays_ordered_when_customised() {
    let mut defaults = v1_json();
    defaults["tables"][0]["ordered"] = json!(true);
    let mut l = ContentLayers::new(content(defaults), UserOverrides::default());
    // An editor draft that leaves the flag out (or clears it) cannot unset it.
    let mut table = table_of(&l, "t.main");
    table.ordered = false;
    table.results[0].name = "A edited".into();
    l.put_table(table).unwrap();
    assert_clean(&l);
    let merged = table_of(&l, "t.main");
    assert!(merged.ordered);
    assert_eq!(merged.results[0].name, "A edited");

    // Moving faces out of scale order is allowed, with a warning.
    let mut table = merged;
    table.results[0].range = Some(FaceRange::new(4, 6));
    table.results[1].range = Some(FaceRange::new(1, 3));
    l.apply_checked(|l| l.put_table(table)).unwrap();
    let report = l.merged().report;
    assert!(!report.has_errors(), "{report}");
    assert!(
        report
            .warnings()
            .any(|w| w.kind == IssueKind::ScaleOutOfOrder),
        "{report}"
    );
}

#[test]
fn putting_a_table_can_remove_add_and_vary_results() {
    let mut l = layers();
    let mut table = v1().table(&t("t.main")).unwrap().clone();
    table.results.remove(1); // remove b
    table.results[0].range = Some(FaceRange::new(1, 4));
    let mut extra = user_result("user.r.new", Some(FaceRange::new(5, 6)));
    extra.tags = vec![g("g.two")];
    table.results.push(extra);
    table.variants = vec![settlement_core::content::TableVariant {
        id: "user.v.1".into(),
        name: "Two-tagged".into(),
        description: String::new(),
        when: Some(settlement_core::content::Condition::HasTag(g("g.two"))),
        dice: Some("1d6".into()),
        ranges: None,
    }];
    l.put_table(table.clone()).unwrap();
    assert_clean(&l);
    let merged = table_of(&l, "t.main");
    let ids: Vec<_> = merged.results.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids, ["a", "user.r.new"]);
    assert_eq!(merged.variants.len(), 1);
    assert!(l.merged().origins.deleted_results[&t("t.main")].contains(&r("b")));

    // Bundled results cannot be re-added under a non-user ID.
    let mut bad = merged.clone();
    bad.results.push(user_result("plain", None));
    assert_eq!(l.put_table(bad), Err(EditError::NotUserId("plain".into())));
}

#[test]
fn checked_edits_refuse_malformed_tables() {
    let mut l = layers();
    let mut table = v1().table(&t("t.main")).unwrap().clone();
    table.dice = Some("1d20".into()); // results still only cover 1-6
    let before = l.overrides().clone();
    let err = l.apply_checked(|l| l.put_table(table)).unwrap_err();
    let CheckedEditError::Invalid(issues) = err else {
        panic!("expected validation errors");
    };
    assert!(
        issues
            .iter()
            .any(|i| i.message.contains("no result for 7-20 on 1d20"))
    );
    assert_eq!(l.overrides(), &before, "nothing changed");
}

#[test]
fn checked_edits_refuse_deleting_a_referenced_tag_but_allow_valid_edits() {
    let mut l = layers();
    // g.one is emitted by t.main › a: deleting it would break that reference.
    let err = l.apply_checked(|l| l.delete_tag(&g("g.one"))).unwrap_err();
    assert!(matches!(err, CheckedEditError::Invalid(_)));
    assert!(l.overrides().is_empty());

    // A rename is fine.
    let mut tag = v1().tag(&g("g.one")).unwrap().clone();
    tag.name = "Renamed".into();
    l.apply_checked(|l| l.put_tag(tag)).unwrap();
    assert_clean(&l);
}

#[test]
fn checked_edits_refuse_invalid_effects() {
    let mut l = layers();
    let mut tag = v1().tag(&g("g.two")).unwrap().clone();
    tag.effects = vec![EffectDef::active(Effect::RouteTo {
        table: t("t.nowhere"),
        variant: None,
    })];
    assert!(l.apply_checked(|l| l.put_tag(tag.clone())).is_err());
    // The same effect is accepted as a planned intention.
    tag.effects[0].status = settlement_core::content::EffectStatus::Planned;
    l.apply_checked(|l| l.put_tag(tag)).unwrap();
}

#[test]
fn putting_a_new_custom_table_creates_then_updates_it() {
    let mut l = layers();
    let mut table = Table {
        id: "user.table.x".into(),
        name: "Mine".into(),
        description: String::new(),
        dice: Some("1d4".into()),
        selection: Default::default(),
        status: Default::default(),
        status_note: String::new(),
        ordered: false,
        active: true,
        eligibility: None,
        results: vec![user_result("user.a", Some(FaceRange::new(1, 4)))],
        variants: vec![],
        follow_ups: vec![],
    };
    l.apply_checked(|l| l.put_table(table.clone())).unwrap();
    assert_eq!(
        l.merged().origins.tables[&t("user.table.x")],
        Origin::Custom
    );
    table.name = "Mine, renamed".into();
    l.apply_checked(|l| l.put_table(table.clone())).unwrap();
    assert_eq!(table_of(&l, "user.table.x").name, "Mine, renamed");
}

fn table_of(l: &ContentLayers, id: &str) -> Table {
    l.merged().content.table(&t(id)).unwrap().clone()
}
